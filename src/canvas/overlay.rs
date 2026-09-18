//! What is painted over the nodes: selection outlines, the box-selection
//! rectangle, snap guides, hover highlights, and the wire being dragged.

use super::{
    CanvasStyle, GraphLayout, WireEndpoints, gestures::Gesture, gestures::Hovered, node_ui,
    round_corners, route_wire,
};
use crate::{AnyPin, NodeId};
use egui::{Color32, CornerRadius, Painter, Rect, Shape, Stroke, StrokeKind, pos2};
use std::collections::BTreeSet;

pub(super) fn draw_overlay(
    painter: &Painter,
    layout: &GraphLayout,
    selection: &BTreeSet<NodeId>,
    gesture: &Gesture,
    hovered: Hovered,
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

    if let Some(wire) = hovered.wire
        && let Some(drawn) = layout.wires.iter().find(|drawn| drawn.wire == wire)
    {
        painter.add(Shape::line(
            drawn.polyline.clone(),
            Stroke::new(style.wire_width * 1.5, highlight(drawn.color)),
        ));
    }
    if let Some(pin) = hovered.pin {
        draw_pin_highlight(painter, layout, pin, style);
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
        Gesture::DraggingWire {
            origin,
            detached,
            current,
        } => {
            let color = layout.pin_color(*origin).unwrap_or(Color32::GRAY);
            let loose_end = Rect::from_center_size(*current, egui::Vec2::ZERO);
            let tolerance = style.wire_tolerance / zoom;
            let draw = |ends: WireEndpoints| {
                let route = route_wire(&ends, &style.wire_routing, 0.0);
                painter.add(Shape::line(
                    round_corners(&route, style.wire_routing.corner_radius, tolerance),
                    Stroke::new(style.wire_width, color),
                ));
            };
            if detached.is_empty() {
                match origin {
                    AnyPin::Out(from) => {
                        if let (Some(pin), Some(node)) =
                            (layout.output(*from), layout.nodes.get(&from.node))
                        {
                            draw(WireEndpoints {
                                from: pin.rect.center(),
                                to: *current,
                                from_node: node.rect,
                                to_node: loose_end,
                            });
                        }
                    }
                    AnyPin::In(to) => {
                        if let (Some(pin), Some(node)) =
                            (layout.input(*to), layout.nodes.get(&to.node))
                        {
                            draw(WireEndpoints {
                                from: *current,
                                to: pin.rect.center(),
                                from_node: loose_end,
                                to_node: node.rect,
                            });
                        }
                    }
                }
            } else {
                for wire in detached {
                    if let (Some(pin), Some(node)) =
                        (layout.output(wire.from), layout.nodes.get(&wire.from.node))
                    {
                        draw(WireEndpoints {
                            from: pin.rect.center(),
                            to: *current,
                            from_node: node.rect,
                            to_node: loose_end,
                        });
                    }
                }
            }
            // The pin this would land on, if any.
            if let Some(target) = layout.pin_at(*current, style.pin_hit_expansion) {
                draw_pin_highlight(painter, layout, target, style);
            }
        }
    }
}

fn draw_pin_highlight(painter: &Painter, layout: &GraphLayout, pin: AnyPin, style: &CanvasStyle) {
    if let (Some(rect), Some(color)) = (layout.pin_rect(pin), layout.pin_color(pin)) {
        node_ui::draw_pin(
            painter,
            style,
            rect.expand(style.pin_size * 0.35),
            highlight(color),
        );
    }
}

fn highlight(color: Color32) -> Color32 {
    color.lerp_to_gamma(Color32::WHITE, 0.35)
}

/// A stroke `zoom`-adjusted to keep its width in screen pixels.
fn screen_width(stroke: Stroke, zoom: f32) -> Stroke {
    Stroke::new(stroke.width / zoom, stroke.color)
}
