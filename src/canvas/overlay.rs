//! What is painted over the nodes: selection outlines, the box-selection
//! rectangle, and snap guides.

use super::{CanvasStyle, GraphLayout, gestures::Gesture};
use crate::NodeId;
use egui::{CornerRadius, Painter, Rect, Stroke, StrokeKind, pos2};
use std::collections::BTreeSet;

pub(super) fn draw_overlay(
    painter: &Painter,
    layout: &GraphLayout,
    selection: &BTreeSet<NodeId>,
    gesture: &Gesture,
    style: &CanvasStyle,
    zoom: f32,
) {
    // Drawn on the node's own rect with the stroke centred on its edge: the
    // inner half covers the node's anti-aliased edge pixels, the outer half is
    // the visible halo, and there is no second shape to leave a gap at the
    // rounded corners.
    let rounding = CornerRadius::same(style.node_rounding.round() as u8);
    for id in selection {
        if let Some(node) = layout.nodes.get(id) {
            painter.rect_stroke(
                node.rect,
                rounding,
                style.selection_stroke,
                StrokeKind::Middle,
            );
        }
    }

    match gesture {
        Gesture::Idle => {}
        Gesture::BoxSelecting { start, current } => {
            let rect = Rect::from_two_pos(*start, *current);
            painter.rect_filled(rect, 0.0, style.box_select_fill);
            painter.rect_stroke(
                rect,
                0.0,
                screen_width(style.box_select_stroke, zoom),
                StrokeKind::Middle,
            );
        }
        Gesture::DraggingNodes {
            guide_x, guide_y, ..
        } => {
            let stroke = screen_width(style.snap_guide_stroke, zoom);
            let viewport = layout.viewport;
            if let Some(x) = guide_x {
                painter.line_segment(
                    [pos2(*x, viewport.top()), pos2(*x, viewport.bottom())],
                    stroke,
                );
            }
            if let Some(y) = guide_y {
                painter.line_segment(
                    [pos2(viewport.left(), *y), pos2(viewport.right(), *y)],
                    stroke,
                );
            }
        }
    }
}

/// A stroke `zoom`-adjusted to keep its width in screen pixels.
fn screen_width(stroke: Stroke, zoom: f32) -> Stroke {
    Stroke::new(stroke.width / zoom, stroke.color)
}
