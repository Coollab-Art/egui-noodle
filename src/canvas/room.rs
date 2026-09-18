//! Sliding the nodes downstream of a splice out of the way, so an inserted
//! node does not overlap the one it was inserted before.
//!
//! The slide lives entirely in the view: the nodes keep their authored
//! positions throughout and the offset is added at draw time, so the
//! in-between frames never reach the graph. Only the settled result is
//! reported, as one `NodesMoved`.

use super::{Canvas, CanvasEvent, CanvasStyle, NodeMove};
use crate::{Graph, NodeId};
use egui::{Pos2, Vec2};
use std::collections::{BTreeMap, BTreeSet};

/// The slide-apart after a node is spliced into a wire, from `make_room` until
/// the nodes have settled.
pub(super) enum Room {
    /// The inserted node has not been drawn yet, so its size is unknown.
    Waiting {
        inserted: NodeId,
        downstream: NodeId,
    },
    Sliding {
        /// Where each sliding node is in the graph.
        start: BTreeMap<NodeId, Pos2>,
        target: Vec2,
        current: Vec2,
    },
}

/// The slide covers this fraction of the remaining distance in this many
/// seconds.
const SLIDE_REACH: (f32, f32) = (0.9, 0.2);

impl Canvas {
    /// Slides the nodes downstream of `downstream` rightwards until `inserted`
    /// no longer overhangs it, animated. Call it after splicing a node into a
    /// wire, with the wire's target as `downstream`. Takes effect once the
    /// inserted node has been drawn and measured - so it is fine to call the
    /// frame the node is created. The moves are reported as one `NodesMoved`
    /// when the slide settles.
    pub fn make_room(&mut self, inserted: NodeId, downstream: NodeId) {
        self.room = Some(Room::Waiting {
            inserted,
            downstream,
        });
    }

    /// Turns a waiting `make_room` into a slide once the inserted node has a
    /// rect to measure the overhang from. Runs after this frame's layout, so a
    /// node created this frame is ready next frame.
    pub(super) fn resolve_waiting_room<N>(&mut self, graph: &Graph<N>, style: &CanvasStyle) {
        let Some(Room::Waiting {
            inserted,
            downstream,
        }) = self.room
        else {
            return;
        };
        if !graph.contains(inserted) || !graph.contains(downstream) {
            self.room = None;
            return;
        }
        let (Some(inserted_layout), Some(downstream_layout)) = (
            self.layout.nodes.get(&inserted),
            self.layout.nodes.get(&downstream),
        ) else {
            return; // Not drawn yet; try again next frame.
        };
        let overhang =
            inserted_layout.rect.right() + style.auto_offset_margin - downstream_layout.rect.left();
        let mut cone: BTreeSet<NodeId> = graph.downstream_nodes(downstream).into_iter().collect();
        cone.insert(downstream);
        cone.remove(&inserted);
        self.room = (overhang > 0.0 && !cone.is_empty()).then(|| Room::Sliding {
            start: cone
                .iter()
                .filter_map(|id| Some((*id, graph.node(*id)?.pos)))
                .collect(),
            target: Vec2::new(overhang, 0.0),
            current: Vec2::ZERO,
        });
    }

    /// Eases the sliding nodes toward their target; when they arrive, reports
    /// the moves and stops.
    pub(super) fn advance_slide<N>(
        &mut self,
        ctx: &egui::Context,
        graph: &Graph<N>,
        events: &mut Vec<CanvasEvent>,
    ) {
        let Some(Room::Sliding {
            start,
            target,
            current,
        }) = &mut self.room
        else {
            return;
        };
        let delta_time = ctx.input(|input| input.stable_dt);
        *current += (*target - *current)
            * egui::emath::exponential_smooth_factor(SLIDE_REACH.0, SLIDE_REACH.1, delta_time);
        if (*target - *current).length() > 0.5 {
            ctx.request_repaint();
            return;
        }
        let moves: Vec<NodeMove> = start
            .iter()
            .filter(|(id, _)| graph.contains(**id))
            .map(|(id, from)| NodeMove {
                id: *id,
                from: *from,
                to: *from + *target,
            })
            .collect();
        if !moves.is_empty() {
            events.push(CanvasEvent::NodesMoved { moves });
        }
        self.room = None;
    }

    pub(super) fn sliding_offset(&self, id: NodeId) -> Vec2 {
        match &self.room {
            Some(Room::Sliding { start, current, .. }) if start.contains_key(&id) => *current,
            _ => Vec2::ZERO,
        }
    }
}
