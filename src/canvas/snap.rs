//! Aligning a dragged group of nodes with what is around it: the same edge
//! of a nearby node, or a wire that would become a straight line.

use super::{Canvas, CanvasStyle};
use crate::{Graph, NodeId};
use egui::{Pos2, Rangef, Rect, Vec2};
use std::collections::BTreeMap;

/// One thing the dragged group could line up with, on one axis.
pub(super) struct Alignment {
    /// The group's own line, where it is now.
    pub moving: f32,
    /// The line to land on.
    pub target: f32,
    /// The extent of the guide to draw along the other axis, when the
    /// alignment deserves one. An edge alignment spans both nodes; a wire
    /// straightening is its own evidence, so it draws none.
    pub guide_span: Option<Rangef>,
}

/// A snap guide: a line at `at` on its axis, from `span.min` to `span.max`
/// on the other.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Guide {
    pub at: f32,
    pub span: Rangef,
}

#[derive(Default)]
pub(super) struct Snap {
    /// How far to shift the dragged group.
    pub delta: Vec2,
    pub guide_x: Option<Guide>,
    pub guide_y: Option<Guide>,
}

/// Picks, on each axis independently, the alignment whose two lines are
/// nearest - within `threshold` - and shifts the group onto it.
pub(super) fn snap(
    x: impl IntoIterator<Item = Alignment>,
    y: impl IntoIterator<Item = Alignment>,
    threshold: f32,
) -> Snap {
    let (delta_x, guide_x) = snap_axis(x, threshold);
    let (delta_y, guide_y) = snap_axis(y, threshold);
    Snap {
        delta: Vec2::new(delta_x, delta_y),
        guide_x,
        guide_y,
    }
}

fn snap_axis(
    alignments: impl IntoIterator<Item = Alignment>,
    threshold: f32,
) -> (f32, Option<Guide>) {
    let best = alignments
        .into_iter()
        .filter(|alignment| (alignment.target - alignment.moving).abs() <= threshold)
        .min_by(|a, b| {
            (a.target - a.moving)
                .abs()
                .total_cmp(&(b.target - b.moving).abs())
        });
    match best {
        Some(alignment) => (
            alignment.target - alignment.moving,
            alignment.guide_span.map(|span| Guide {
                at: alignment.target,
                span,
            }),
        ),
        None => (0.0, None),
    }
}

impl Canvas {
    /// Snaps the dragged group as a whole: to the grid when that is on, then
    /// to the same edge of a nearby node (left to left, top to top, ...) or
    /// to a straight run for a wire between the group and a node outside it.
    /// Reads where everything was drawn last frame.
    pub(super) fn snap_targets<N>(
        &self,
        graph: &Graph<N>,
        targets: &BTreeMap<NodeId, Pos2>,
        style: &CanvasStyle,
    ) -> Snap {
        let group = targets
            .iter()
            .filter_map(|(id, pos)| {
                let size = self.layout.nodes.get(id)?.rect.size();
                Some(Rect::from_min_size(*pos, size))
            })
            .reduce(|a, b| a.union(b));
        let Some(mut group) = group else {
            return Snap::default();
        };
        let mut grid_delta = Vec2::ZERO;
        if style.snap_to_grid && style.grid_spacing > 0.0 {
            let snapped_min = (group.min / style.grid_spacing).round() * style.grid_spacing;
            grid_delta = snapped_min - group.min;
            group = group.translate(grid_delta);
        }

        let (mut x, mut y) = self.edge_alignments(group, targets, style);
        y.extend(self.straight_wire_alignments(graph, targets, grid_delta.y));

        let mut snapped = snap(x.drain(..), y, style.snap_distance / self.view.zoom);
        snapped.delta += grid_delta;
        snapped
    }

    /// Lining each edge of the dragged group up with the same edge of a node
    /// near it, per axis. Only nodes in view and within the search distance
    /// are offered, so a busy graph does not align with everything.
    fn edge_alignments(
        &self,
        group: Rect,
        targets: &BTreeMap<NodeId, Pos2>,
        style: &CanvasStyle,
    ) -> (Vec<Alignment>, Vec<Alignment>) {
        let search = group.expand(style.snap_search_distance / self.view.zoom);
        let nearby = self
            .layout
            .nodes
            .iter()
            .filter(|(id, _)| !targets.contains_key(id))
            .map(|(_, node)| node.rect)
            .filter(|rect| rect.intersects(self.layout.viewport) && rect.intersects(search));

        let mut x = Vec::new();
        let mut y = Vec::new();
        for other in nearby {
            // The guide spans both nodes, so it reads as joining the two.
            let span_y = Some(Rangef::new(
                group.top().min(other.top()),
                group.bottom().max(other.bottom()),
            ));
            let span_x = Some(Rangef::new(
                group.left().min(other.left()),
                group.right().max(other.right()),
            ));
            for (moving, target) in [(group.left(), other.left()), (group.right(), other.right())] {
                x.push(Alignment {
                    moving,
                    target,
                    guide_span: span_y,
                });
            }
            for (moving, target) in [(group.top(), other.top()), (group.bottom(), other.bottom())] {
                y.push(Alignment {
                    moving,
                    target,
                    guide_span: span_x,
                });
            }
        }
        (x, y)
    }

    /// Lining a dragged node up so that a wire between it and a node standing
    /// still comes out straight, which happens when the wire's two pins share
    /// a row. `grid_offset` is how far the grid snap already moved the group
    /// on this axis.
    fn straight_wire_alignments<N>(
        &self,
        graph: &Graph<N>,
        targets: &BTreeMap<NodeId, Pos2>,
        grid_offset: f32,
    ) -> Vec<Alignment> {
        // Where a pin of a dragged node is being drawn, rather than where it
        // was drawn last frame.
        let dragged_pin_y = |pin_y: f32, node: NodeId| {
            let layout = self.layout.nodes.get(&node)?;
            let target = targets.get(&node)?;
            Some(pin_y - layout.rect.top() + target.y + grid_offset)
        };
        graph
            .wires()
            .iter()
            .filter_map(|wire| {
                let (from, to) = (self.layout.output(wire.from)?, self.layout.input(wire.to)?);
                let (from_y, to_y) = (from.rect.center().y, to.rect.center().y);
                let (moving, target) = match (
                    targets.contains_key(&wire.from.node),
                    targets.contains_key(&wire.to.node),
                ) {
                    (true, false) => (dragged_pin_y(from_y, wire.from.node)?, to_y),
                    (false, true) => (dragged_pin_y(to_y, wire.to.node)?, from_y),
                    _ => return None,
                };
                Some(Alignment {
                    moving,
                    target,
                    guide_span: None,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(moving: f32, target: f32) -> Alignment {
        Alignment {
            moving,
            target,
            guide_span: Some(Rangef::new(0.0, 10.0)),
        }
    }

    #[test]
    fn the_nearest_alignment_within_the_threshold_wins() {
        let snap = snap(
            [edge(103.0, 100.0), edge(103.0, 105.0), edge(103.0, 50.0)],
            [],
            8.0,
        );
        assert_eq!(snap.delta.x, 2.0);
        assert_eq!(snap.guide_x.map(|guide| guide.at), Some(105.0));
        assert_eq!(snap.delta.y, 0.0, "nothing offered on y");
        assert_eq!(snap.guide_y, None);
    }

    #[test]
    fn nothing_snaps_beyond_the_threshold() {
        let snap = snap([edge(200.0, 180.0)], [], 8.0);
        assert_eq!(snap.delta, Vec2::ZERO);
        assert_eq!(snap.guide_x, None);
    }

    #[test]
    fn an_alignment_without_a_span_snaps_but_draws_no_guide() {
        let snap = snap(
            [],
            [Alignment {
                moving: 40.0,
                target: 43.0,
                guide_span: None,
            }],
            8.0,
        );
        assert_eq!(snap.delta.y, 3.0);
        assert_eq!(snap.guide_y, None);
    }
}
