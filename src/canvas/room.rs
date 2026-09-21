//! Sliding the nodes on either side of a splice out of the way, so an
//! inserted node overlaps neither the node it was inserted after nor the one
//! it was inserted before.
//!
//! `make_room` works out the final positions at once and hands them back for
//! the application to apply - they are model changes, and belong in the same
//! undo entry as the insert that caused them. What stays in the view is only
//! the animation: each slid node is drawn short of its new position by an
//! offset that eases to zero, so the graph is right immediately and the eye
//! sees a slide. Why synchronous: `6-Application Owns Identity.md`.

use super::{Canvas, CanvasStyle, NodeMove};
use crate::{Graph, NodeIdentifier, Wire};
use egui::{Rect, Vec2};
use std::collections::{BTreeMap, BTreeSet};

/// The slide-apart animation after a splice.
pub(super) struct Room<I> {
    /// How far each node was moved. It is drawn that far short of its new
    /// position, times how much of the slide is left.
    offsets: BTreeMap<I, Vec2>,
    /// From 0 to 1.
    progress: f32,
    /// The longest offset, to know when they have all visibly arrived.
    longest: f32,
}

/// The slide covers this fraction of the remaining distance in this many
/// seconds.
const SLIDE_REACH: (f32, f32) = (0.9, 0.2);

impl<I: NodeIdentifier> Canvas<I> {
    /// The moves that slide the nodes downstream of `wire`'s target rightwards
    /// and the nodes upstream of its source leftwards until a node occupying
    /// `inserted` (graph space) overhangs neither. Call it before splicing the
    /// node into `wire` and apply the moves along with the splice; the canvas
    /// then animates the nodes from where they were to where they are.
    ///
    /// Reads where the neighbours were drawn last frame, so a neighbour that
    /// has never been drawn does not move. Measure `inserted` with
    /// [`Canvas::measure_node`].
    pub fn make_room<N>(
        &mut self,
        graph: &Graph<N, I>,
        wire: Wire<I>,
        inserted: Rect,
        style: &CanvasStyle,
    ) -> Vec<NodeMove<I>> {
        let (upstream, downstream) = (wire.from.node, wire.to.node);
        let (Some(upstream_layout), Some(downstream_layout)) = (
            self.layout.nodes.get(&upstream),
            self.layout.nodes.get(&downstream),
        ) else {
            return Vec::new();
        };
        let right_overhang =
            inserted.right() + style.auto_offset_margin - downstream_layout.rect.left();
        let left_overhang =
            upstream_layout.rect.right() + style.auto_offset_margin - inserted.left();

        let mut offsets: BTreeMap<I, Vec2> = BTreeMap::new();
        let mut side = |cone: Vec<I>, root: I, offset: Vec2| {
            let mut cone: BTreeSet<I> = cone.into_iter().collect();
            cone.insert(root);
            for id in cone {
                // A node on both sides (the wire closed a cycle) goes with
                // the side handled first, downstream.
                if graph.contains(id) && !offsets.contains_key(&id) {
                    offsets.insert(id, offset);
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
        let moves = offsets
            .iter()
            .filter_map(|(id, offset)| {
                Some(NodeMove {
                    id: *id,
                    to: graph.node(*id)?.pos + *offset,
                })
            })
            .collect();
        if !offsets.is_empty() {
            self.room = Some(Room {
                longest: offsets
                    .values()
                    .map(|offset| offset.length())
                    .fold(0.0, f32::max),
                offsets,
                progress: 0.0,
            });
        }
        moves
    }

    /// Eases the slid nodes toward where the graph now has them; done when
    /// they have visibly arrived.
    pub(super) fn advance_slide(&mut self, ctx: &egui::Context) {
        let Some(Room {
            progress, longest, ..
        }) = &mut self.room
        else {
            return;
        };
        let delta_time = ctx.input(|input| input.stable_dt);
        *progress += (1.0 - *progress)
            * egui::emath::exponential_smooth_factor(SLIDE_REACH.0, SLIDE_REACH.1, delta_time);
        if (1.0 - *progress) * *longest > 0.5 {
            ctx.request_repaint();
        } else {
            self.room = None;
        }
    }

    /// How far short of its graph position a sliding node is drawn.
    pub(super) fn sliding_offset(&self, id: I) -> Vec2 {
        match &self.room {
            Some(Room {
                offsets, progress, ..
            }) => offsets
                .get(&id)
                .map_or(Vec2::ZERO, |offset| -*offset * (1.0 - *progress)),
            None => Vec2::ZERO,
        }
    }
}

/// Run through a real `egui::Context`: `make_room` reads where the neighbours
/// were drawn, so the graph has to be shown once first.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        InPin, InputId, InputSpec, NodeContent, OutPin, OutputId, OutputSpec, SequentialNodeId,
    };
    use egui::{Color32, pos2, vec2};

    struct Content;

    impl NodeContent for Content {
        type Node = ();
        type Id = SequentialNodeId;

        fn inputs(&mut self, _node: &()) -> Vec<InputSpec> {
            vec![InputSpec {
                id: InputId(0),
                label: "in".to_owned(),
                color: Color32::RED,
                policy: crate::ConnectionPolicy::Single,
            }]
        }

        fn outputs(&mut self, _node: &()) -> Vec<OutputSpec> {
            vec![OutputSpec {
                id: OutputId(0),
                label: "out".to_owned(),
                color: Color32::GREEN,
            }]
        }

        fn title(&mut self, _node: &()) -> String {
            "node".to_owned()
        }
    }

    fn wire(from: SequentialNodeId, to: SequentialNodeId) -> Wire<SequentialNodeId> {
        Wire {
            from: OutPin {
                node: from,
                output: OutputId(0),
            },
            to: InPin {
                node: to,
                input: InputId(0),
            },
        }
    }

    /// Two wired nodes close together, drawn a few frames so their sizes
    /// have settled, and a canvas that has seen them.
    fn shown_pair() -> (
        egui::Context,
        Canvas<SequentialNodeId>,
        Graph<(), SequentialNodeId>,
        CanvasStyle,
    ) {
        let ctx = egui::Context::default();
        let style = CanvasStyle::default();
        let mut canvas = Canvas::new();
        let mut graph = Graph::new();
        let (a, c) = (SequentialNodeId(0), SequentialNodeId(1));
        graph.add_node(a, (), pos2(0.0, 0.0)).unwrap();
        graph.add_node(c, (), pos2(200.0, 0.0)).unwrap();
        graph.connect(wire(a, c)).unwrap();
        for _ in 0..4 {
            ctx.run_ui(Default::default(), |ui| {
                canvas.show(ui, &graph, &mut Content, &style);
            })
            .drop_without_applying_deltas();
        }
        (ctx, canvas, graph, style)
    }

    #[test]
    fn making_room_pushes_the_downstream_node_clear_of_the_inserted_one() {
        let (_ctx, mut canvas, mut graph, style) = shown_pair();
        let (a, c) = (SequentialNodeId(0), SequentialNodeId(1));
        let a_right = canvas.layout().nodes[&a].rect.right();
        // Wide enough to overhang `c` on the right and `a` on the left.
        let inserted = egui::Rect::from_min_size(pos2(a_right - 20.0, 0.0), vec2(300.0, 60.0));

        let moves = canvas.make_room(&graph, wire(a, c), inserted, &style);
        for node_move in &moves {
            graph.set_pos(node_move.id, node_move.to);
        }

        let c_pos = graph.node(c).unwrap().pos;
        assert!(
            c_pos.x >= inserted.right() + style.auto_offset_margin,
            "c moved to {c_pos:?} but the inserted node ends at {}",
            inserted.right()
        );
        let a_pos = graph.node(a).unwrap().pos;
        let a_width = canvas.layout().nodes[&a].rect.width();
        assert!(
            a_pos.x + a_width + style.auto_offset_margin <= inserted.left(),
            "a moved to {a_pos:?} but still overhangs the inserted node at {}",
            inserted.left()
        );
        // The graph holds the final positions; the view lags behind and eases in.
        assert_ne!(canvas.sliding_offset(c), Vec2::ZERO);
    }

    #[test]
    fn nothing_moves_when_the_inserted_node_fits() {
        let (_ctx, mut canvas, graph, style) = shown_pair();
        let (a, c) = (SequentialNodeId(0), SequentialNodeId(1));
        let a_right = canvas.layout().nodes[&a].rect.right();
        let c_left = canvas.layout().nodes[&c].rect.left();
        let margin = style.auto_offset_margin;
        let inserted =
            egui::Rect::from_min_max(pos2(a_right + margin, 0.0), pos2(c_left - margin, 20.0));

        assert!(
            canvas
                .make_room(&graph, wire(a, c), inserted, &style)
                .is_empty()
        );
        assert_eq!(canvas.sliding_offset(c), Vec2::ZERO);
    }

    /// `make_room` places a node by the size `measure_node` gave, so the two
    /// must agree with what the node is then drawn at.
    #[test]
    fn a_measured_node_is_drawn_at_the_measured_size() {
        let (ctx, mut canvas, mut graph, style) = shown_pair();
        let id = SequentialNodeId(2);
        let mut measured = Vec2::ZERO;
        ctx.run_ui(Default::default(), |ui| {
            measured = canvas.measure_node(ui, &mut Content, id, &(), &style);
        })
        .drop_without_applying_deltas();
        assert!(measured.x > 0.0 && measured.y > 0.0);

        graph.add_node(id, (), pos2(0.0, 300.0)).unwrap();
        for _ in 0..4 {
            ctx.run_ui(Default::default(), |ui| {
                canvas.show(ui, &graph, &mut Content, &style);
            })
            .drop_without_applying_deltas();
        }
        let drawn = canvas.layout().nodes[&id].rect.size();
        assert!(
            (drawn - measured).length() < 0.5,
            "measured {measured:?}, drawn {drawn:?}"
        );
    }
}
