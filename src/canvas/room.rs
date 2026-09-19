//! Sliding the nodes on either side of a splice out of the way, so an
//! inserted node overlaps neither the node it was inserted after nor the one
//! it was inserted before.
//!
//! The slide lives entirely in the view: the nodes keep their authored
//! positions throughout and the offset is added at draw time, so the
//! in-between frames never reach the graph. Only the settled result is
//! reported, as one `NodesMoved`.

use super::{Canvas, CanvasEvent, CanvasStyle, NodeMove};
use crate::{Graph, NodeId, Wire};
use egui::{Pos2, Vec2};
use std::collections::{BTreeMap, BTreeSet};

/// The slide-apart after a node is spliced into a wire, from `make_room` until
/// the nodes have settled.
pub(super) enum Room {
    /// The inserted node has not been drawn yet, so its size is unknown.
    Waiting { inserted: NodeId, wire: Wire },
    Sliding {
        /// Where each sliding node is in the graph, and how far it goes.
        slides: BTreeMap<NodeId, (Pos2, Vec2)>,
        /// How far along every slide is, from 0 to 1.
        progress: f32,
        /// The longest slide, to know when they have all visibly arrived.
        longest: f32,
    },
}

/// The slide covers this fraction of the remaining distance in this many
/// seconds.
const SLIDE_REACH: (f32, f32) = (0.9, 0.2);

impl Canvas {
    /// Slides the nodes downstream of `wire`'s target rightwards, and the
    /// nodes upstream of its source leftwards, until `inserted` overhangs
    /// neither, animated. Call it after splicing a node into `wire`. Takes
    /// effect once the inserted node has been drawn and measured - so it is
    /// fine to call the frame the node is created. The moves are reported as
    /// one `NodesMoved` when the slide settles.
    pub fn make_room(&mut self, inserted: NodeId, wire: Wire) {
        self.room = Some(Room::Waiting { inserted, wire });
    }

    /// Turns a waiting `make_room` into a slide once the inserted node has a
    /// rect to measure the overhangs from. Runs after this frame's layout, so
    /// a node created this frame is ready next frame.
    pub(super) fn resolve_waiting_room<N>(&mut self, graph: &Graph<N>, style: &CanvasStyle) {
        let Some(Room::Waiting { inserted, wire }) = self.room else {
            return;
        };
        let (upstream, downstream) = (wire.from.node, wire.to.node);
        if !graph.contains(inserted) || !graph.contains(upstream) || !graph.contains(downstream) {
            self.room = None;
            return;
        }
        let (Some(inserted_layout), Some(upstream_layout), Some(downstream_layout)) = (
            self.layout.nodes.get(&inserted),
            self.layout.nodes.get(&upstream),
            self.layout.nodes.get(&downstream),
        ) else {
            return; // Not drawn yet; try again next frame.
        };
        let right_overhang =
            inserted_layout.rect.right() + style.auto_offset_margin - downstream_layout.rect.left();
        let left_overhang =
            upstream_layout.rect.right() + style.auto_offset_margin - inserted_layout.rect.left();

        let mut slides: BTreeMap<NodeId, (Pos2, Vec2)> = BTreeMap::new();
        let mut side = |cone: Vec<NodeId>, root: NodeId, offset: Vec2| {
            let mut cone: BTreeSet<NodeId> = cone.into_iter().collect();
            cone.insert(root);
            cone.remove(&inserted);
            for id in cone {
                // A node on both sides (the wire closed a cycle) goes with
                // the side handled first, downstream.
                if let Some(node) = graph.node(id)
                    && !slides.contains_key(&id)
                {
                    slides.insert(id, (node.pos, offset));
                }
            }
        };
        if right_overhang > 0.0 {
            side(
                graph.downstream_nodes(downstream),
                downstream,
                Vec2::new(right_overhang, 0.0),
            );
        }
        if left_overhang > 0.0 {
            side(
                graph.upstream_nodes(upstream),
                upstream,
                Vec2::new(-left_overhang, 0.0),
            );
        }
        self.room = (!slides.is_empty()).then(|| Room::Sliding {
            longest: slides
                .values()
                .map(|(_, offset)| offset.length())
                .fold(0.0, f32::max),
            slides,
            progress: 0.0,
        });
    }

    /// Eases the sliding nodes toward their targets; when they arrive, reports
    /// the moves and stops. The nodes keep being drawn where they arrived
    /// until the graph catches up.
    pub(super) fn advance_slide<N>(
        &mut self,
        ctx: &egui::Context,
        graph: &Graph<N>,
        events: &mut Vec<CanvasEvent>,
    ) {
        let Some(Room::Sliding {
            slides,
            progress,
            longest,
        }) = &mut self.room
        else {
            return;
        };
        let delta_time = ctx.input(|input| input.stable_dt);
        *progress += (1.0 - *progress)
            * egui::emath::exponential_smooth_factor(SLIDE_REACH.0, SLIDE_REACH.1, delta_time);
        if (1.0 - *progress) * *longest > 0.5 {
            ctx.request_repaint();
            return;
        }
        let moves: Vec<NodeMove> = slides
            .iter()
            .filter(|(id, _)| graph.contains(**id))
            .map(|(id, (from, offset))| NodeMove {
                id: *id,
                from: *from,
                to: *from + *offset,
            })
            .collect();
        if !moves.is_empty() {
            self.settled
                .extend(moves.iter().map(|node_move| (node_move.id, node_move.to)));
            events.push(CanvasEvent::NodesMoved { moves });
        }
        self.room = None;
    }

    pub(super) fn sliding_offset(&self, id: NodeId) -> Vec2 {
        match &self.room {
            Some(Room::Sliding {
                slides, progress, ..
            }) => slides
                .get(&id)
                .map_or(Vec2::ZERO, |(_, offset)| *offset * *progress),
            _ => Vec2::ZERO,
        }
    }
}
