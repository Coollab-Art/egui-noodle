//! What is painted over every node: the box-selection rectangle, snap guides,
//! the `+` on a hovered wire, the wire being dragged, and the cutting stroke.
//! What belongs to one node - its selection outline and its pins - is painted
//! with that node instead (`node_ui::draw_marks`), so a node in front covers
//! it; and a wire's highlight is painted with the wires, behind the nodes, as
//! `wire_highlight` says.

use super::{
    CanvasStyle, GraphLayout, NodeLayout, Selection, SelectionColor, WireEndpoints,
    gestures::{Gesture, Hovered, InsertTarget, can_connect},
    node_ui::{self, highlight},
    round_corners, route_wire, screen_stroke,
};
use crate::{AnyPin, NodeId};
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2, vec2};

pub(super) struct OverlayInput<'a> {
    pub layout: &'a GraphLayout,
    pub gesture: &'a Gesture,
    /// The `+` on the hovered wire, and whether the pointer is on it.
    pub insert_button: Option<(Rect, bool)>,
    pub zoom: f32,
}

pub(super) fn draw_overlay(painter: &Painter, input: OverlayInput<'_>, style: &CanvasStyle) {
    let OverlayInput {
        layout,
        gesture,
        insert_button,
        zoom,
    } = input;

    if let Some((rect, pointer_on_it)) = insert_button {
        draw_insert_button(painter, rect, pointer_on_it, style, zoom);
    }

    match gesture {
        Gesture::Idle => {}
        Gesture::BoxSelecting { start, current, .. } => {
            let rect = Rect::from_two_pos(*start, *current);
            painter.rect_filled(rect, 0.0, style.box_select_fill);
            painter.rect_stroke(
                rect,
                0.0,
                screen_stroke(style.box_select_stroke, zoom),
                StrokeKind::Middle,
            );
        }
        Gesture::DraggingNodes {
            guide_x, guide_y, ..
        } => {
            let stroke = screen_stroke(style.snap_guide_stroke, zoom);
            if let Some(guide) = guide_x {
                painter.line_segment(
                    [
                        pos2(guide.at, guide.span.min),
                        pos2(guide.at, guide.span.max),
                    ],
                    stroke,
                );
            }
            if let Some(guide) = guide_y {
                painter.line_segment(
                    [
                        pos2(guide.span.min, guide.at),
                        pos2(guide.span.max, guide.at),
                    ],
                    stroke,
                );
            }
        }
        Gesture::Cutting { stroke, .. } => {
            if stroke.len() >= 2 {
                painter.add(Shape::line(
                    stroke.clone(),
                    screen_stroke(style.wire_cut_stroke, zoom),
                ));
            }
        }
        Gesture::DraggingWire { origin, current } => {
            let color = layout.pin_color(*origin).unwrap_or(Color32::GRAY);
            // Over a pin it can land on, the preview is the wire as it will
            // be; elsewhere its loose end is the pointer.
            let target = layout
                .pin_at(*current, style.pin_hit_expansion)
                .filter(|target| compatible(*origin, *target));
            if let Some(ends) = drag_ends(layout, *origin, target, *current) {
                let route = route_wire(&ends, &style.wire_routing, 0.0);
                painter.add(Shape::line(
                    round_corners(
                        &route,
                        style.wire_routing.corner_radius,
                        style.wire_tolerance / zoom,
                    ),
                    Stroke::new(style.wire_width, color),
                ));
            }
            if let Some(target) = target {
                draw_pin_highlight(painter, layout, target, style);
            }
        }
    }
}

/// The outline colour for a node that is selected or about to be cut.
pub(super) fn node_outline(
    id: NodeId,
    node: &NodeLayout,
    selection: &Selection,
    gesture: &Gesture,
    style: &CanvasStyle,
) -> Option<Color32> {
    if gesture.cut_nodes().contains(&id) {
        return Some(style.wire_cut_stroke.color);
    }
    selection
        .contains_node(id)
        .then_some(match style.selection_color {
            SelectionColor::Fixed(color) => color,
            SelectionColor::Own => node.header_color,
        })
}

/// How a wire is marked out from the others.
pub(super) enum WireHighlight {
    /// A wider stroke *behind* the wire, so its own colour still reads down
    /// the middle. What a selected wire gets, to match a selected node's halo.
    Outline(Color32),
    /// A wider stroke *over* the wire, replacing its colour.
    Restroke(Color32),
}

/// How to mark a wire out, if at all: the cut colour over a wire about to be
/// cut or one a dragged node cannot go into, a lighter shade of its own over
/// the hovered wire or the one a dragged node would be spliced into, and the
/// selection colour as an outline behind a selected wire.
pub(super) fn wire_highlight(
    drawn: &super::WireLayout,
    selection: &Selection,
    gesture: &Gesture,
    hovered: Hovered,
    style: &CanvasStyle,
) -> Option<WireHighlight> {
    let wire = drawn.wire;
    match gesture {
        Gesture::Cutting { wires, .. } if wires.contains(&wire) => {
            return Some(WireHighlight::Restroke(style.wire_cut_stroke.color));
        }
        Gesture::DraggingNodes {
            insert_target:
                Some(InsertTarget {
                    wire: target,
                    valid,
                }),
            ..
        } if *target == wire => {
            return Some(WireHighlight::Restroke(if *valid {
                highlight(drawn.color)
            } else {
                style.wire_cut_stroke.color
            }));
        }
        _ => {}
    }
    if hovered.wire == Some(wire) {
        return Some(WireHighlight::Restroke(highlight(drawn.color)));
    }
    let color = match style.selection_color {
        SelectionColor::Fixed(color) => color,
        SelectionColor::Own => highlight(drawn.color),
    };
    selection
        .contains_wire(wire)
        .then_some(WireHighlight::Outline(color))
}

fn compatible(origin: AnyPin, target: AnyPin) -> bool {
    match (origin, target) {
        (AnyPin::Out(from), AnyPin::In(to)) | (AnyPin::In(to), AnyPin::Out(from)) => {
            can_connect(from, to)
        }
        _ => false,
    }
}

/// The wire being dragged from `origin`, ready to route: between two drawn
/// pins when it is over `target`, to a loose end at the pointer otherwise.
fn drag_ends(
    layout: &GraphLayout,
    origin: AnyPin,
    target: Option<AnyPin>,
    pointer: Pos2,
) -> Option<WireEndpoints> {
    match (origin, target) {
        (AnyPin::Out(from), Some(AnyPin::In(to))) | (AnyPin::In(to), Some(AnyPin::Out(from))) => {
            Some(WireEndpoints {
                from: layout.output(from)?.rect.center(),
                to: layout.input(to)?.rect.center(),
                from_node: layout.nodes.get(&from.node)?.rect,
                to_node: layout.nodes.get(&to.node)?.rect,
            })
        }
        (AnyPin::Out(from), _) => Some(WireEndpoints {
            from: layout.output(from)?.rect.center(),
            to: pointer,
            from_node: layout.nodes.get(&from.node)?.rect,
            to_node: Rect::from_pos(pointer),
        }),
        (AnyPin::In(to), _) => Some(WireEndpoints {
            from: pointer,
            to: layout.input(to)?.rect.center(),
            from_node: Rect::from_pos(pointer),
            to_node: layout.nodes.get(&to.node)?.rect,
        }),
    }
}

fn draw_insert_button(
    painter: &Painter,
    rect: Rect,
    pointer_on_it: bool,
    style: &CanvasStyle,
    zoom: f32,
) {
    let fill = if pointer_on_it {
        highlight(style.insert_button_fill)
    } else {
        style.insert_button_fill
    };
    painter.circle_filled(rect.center(), rect.width() / 2.0, fill);
    let arm = rect.width() * 0.28;
    let stroke = Stroke::new((rect.width() * 0.12).max(1.0 / zoom), Color32::WHITE);
    painter.line_segment(
        [
            rect.center() - vec2(arm, 0.0),
            rect.center() + vec2(arm, 0.0),
        ],
        stroke,
    );
    painter.line_segment(
        [
            rect.center() - vec2(0.0, arm),
            rect.center() + vec2(0.0, arm),
        ],
        stroke,
    );
}

/// The pin a dragged wire would land on. Over every node, unlike a node's own
/// pins: it is feedback for the drag, not part of the node.
fn draw_pin_highlight(painter: &Painter, layout: &GraphLayout, pin: AnyPin, style: &CanvasStyle) {
    if let (Some(rect), Some(color)) = (layout.pin_rect(pin), layout.pin_color(pin)) {
        node_ui::draw_pin(
            painter,
            style.pin_shape,
            rect.expand(style.pin_size * 0.35),
            highlight(color),
            style.pin_stroke,
        );
    }
}
