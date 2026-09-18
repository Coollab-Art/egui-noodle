use super::{CanvasStyle, NodeContent, PinLayout, PinShape};
use crate::{InputId, NodeId, OutputId};
use egui::{
    Align, Color32, CornerRadius, Frame, Layout, Rect, Shape, Stroke, TextWrapMode, Ui, UiBuilder,
    Vec2, pos2,
};

/// A node's geometry once drawn. Pin rects are where the pins were painted.
pub(super) struct DrawnNode {
    pub rect: Rect,
    pub header_rect: Rect,
    pub inputs: Vec<PinLayout<InputId>>,
    pub outputs: Vec<PinLayout<OutputId>>,
}

/// Lays out and paints one node into `canvas_ui` (graph space), with its
/// top-left at `pos`.
///
/// `known_size` is what the node measured last time; the frame it first
/// appears it is laid out against `style.default_node_size` instead, as egui's
/// own `Area` does. The header fill spans the node's full width, which is not
/// known until the content is laid out, so it is painted into a slot reserved
/// beforehand.
pub(super) fn draw_node<C: NodeContent>(
    canvas_ui: &mut Ui,
    style: &CanvasStyle,
    content: &mut C,
    id: NodeId,
    payload: &mut C::Node,
    pos: egui::Pos2,
    known_size: Option<Vec2>,
) -> DrawnNode {
    let inputs = content.inputs(payload);
    let outputs = content.outputs(payload);

    let max_rect = Rect::from_min_size(pos, known_size.unwrap_or(style.default_node_size));
    let mut builder = UiBuilder::new().max_rect(max_rect).id_salt(id);
    if known_size.is_none() {
        builder = builder.sizing_pass();
    }
    let mut node_ui = canvas_ui.new_child(builder);
    // A label that wraps at the previous frame's width could never widen the
    // node; extending lets the node grow to its content.
    node_ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);

    let header_fill_slot = node_ui.painter().add(Shape::Noop);
    let mut header_rect = Rect::NOTHING;
    let mut input_rows: Vec<Rect> = Vec::with_capacity(inputs.len());
    let mut output_rows: Vec<Rect> = Vec::with_capacity(outputs.len());

    let rounding = CornerRadius::same(style.node_rounding.round() as u8);
    let frame = Frame::NONE
        .fill(style.node_fill)
        .stroke(style.node_stroke)
        .corner_radius(rounding)
        .show(&mut node_ui, |ui| {
            header_rect = Frame::NONE
                .inner_margin(style.header_padding)
                .show(ui, |ui| content.header(ui, id, payload))
                .response
                .rect;

            Frame::NONE.inner_margin(style.node_padding).show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        for input in &inputs {
                            let row = ui.horizontal(|ui| content.input_row(ui, input));
                            input_rows.push(row.response.rect);
                        }
                    });
                    content.body(ui, id, payload);
                    ui.with_layout(Layout::top_down(Align::Max), |ui| {
                        for output in &outputs {
                            let row = ui.horizontal(|ui| content.output_row(ui, output));
                            output_rows.push(row.response.rect);
                        }
                    });
                });
            });
        });
    let rect = frame.response.rect;

    // Now that the node's width is known, the header fill can span it.
    let header_band = Rect::from_min_max(rect.min, pos2(rect.max.x, header_rect.max.y));
    node_ui.painter().set(
        header_fill_slot,
        Shape::rect_filled(
            header_band,
            CornerRadius {
                nw: rounding.nw,
                ne: rounding.ne,
                sw: 0,
                se: 0,
            },
            style.header_fill,
        ),
    );

    let painter = canvas_ui.painter();
    let pin_size = Vec2::splat(style.pin_size);
    let inputs = inputs
        .iter()
        .zip(&input_rows)
        .map(|(input, row)| {
            let pin_rect = Rect::from_center_size(pos2(rect.left(), row.center().y), pin_size);
            draw_pin(painter, style, pin_rect, input.color);
            PinLayout {
                id: input.id,
                rect: pin_rect,
                color: input.color,
            }
        })
        .collect();
    let outputs = outputs
        .iter()
        .zip(&output_rows)
        .map(|(output, row)| {
            let pin_rect = Rect::from_center_size(pos2(rect.right(), row.center().y), pin_size);
            draw_pin(painter, style, pin_rect, output.color);
            PinLayout {
                id: output.id,
                rect: pin_rect,
                color: output.color,
            }
        })
        .collect();

    DrawnNode {
        rect,
        header_rect: header_band,
        inputs,
        outputs,
    }
}

pub(super) fn draw_pin(painter: &egui::Painter, style: &CanvasStyle, rect: Rect, fill: Color32) {
    let stroke: Stroke = style.pin_stroke;
    match style.pin_shape {
        PinShape::Circle => {
            painter.circle(rect.center(), rect.width() / 2.0, fill, stroke);
        }
        PinShape::Square => {
            painter.rect(rect, 0.0, fill, stroke, egui::StrokeKind::Middle);
        }
    }
}
