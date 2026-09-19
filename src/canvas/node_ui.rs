use super::{CanvasStyle, InputPinLayout, NodeContent, NodeLayout, OutputPinLayout, PinShape};
use crate::{AnyPin, NodeId};
use egui::{
    Align, CornerRadius, Frame, Id, Layout, Margin, Rect, Shape, Stroke, StrokeKind, Style,
    TextWrapMode, Ui, UiBuilder, Vec2, emath::TSTransform, epaint::RectShape, pos2,
};

/// Lays out and paints one node into `nodes_ui`, with its top-left at `pos`
/// (graph space), and returns where everything landed, in graph space.
///
/// `nodes_ui` is in graph space scaled by `scale` - see `content_scale` - so
/// text is laid out at the size it is shown at and stays crisp. Every graph
/// length going in is multiplied by `scale`, every rect coming out divided.
///
/// The node's background and its header fill span the node's full width, which
/// is not known until the content is laid out, so both are painted into slots
/// reserved beforehand. The node's marks - its selection outline and its pins -
/// are painted by `draw_marks` once this has returned the geometry they need.
pub(super) fn draw_node<C: NodeContent>(
    nodes_ui: &mut Ui,
    style: &CanvasStyle,
    scale: f32,
    content: &mut C,
    payload: &mut C::Node,
    placement: NodePlacement,
) -> NodeLayout {
    let NodePlacement {
        id,
        pos,
        known_size,
    } = placement;
    let inputs = content.inputs(payload);
    let outputs = content.outputs(payload);
    let splice_pins = content.splice_pins(payload);
    let header_fill = content.header_color(payload).unwrap_or(style.header_fill);

    let to_scaled = TSTransform::from_scaling(scale);
    let max_rect =
        to_scaled * Rect::from_min_size(pos, known_size.unwrap_or(style.default_node_size));
    let mut builder = UiBuilder::new().max_rect(max_rect).id_salt(id);
    if known_size.is_none() {
        builder = builder.sizing_pass();
    }
    let mut node_ui = nodes_ui.new_child(builder);
    scale_style(node_ui.style_mut(), scale);
    // A label that wraps at the previous frame's width could never widen the
    // node; extending lets the node grow to its content.
    node_ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);

    // Background first, header over it, and the content - laid out below, so
    // painted later - over both. The background cannot be a `Frame` fill: a
    // `Frame` reserves its own slot when it is shown, which is after these
    // two, so it would cover the header.
    let background_slot = node_ui.painter().add(Shape::Noop);
    let header_slot = node_ui.painter().add(Shape::Noop);
    let mut header_bottom = max_rect.top();
    let mut input_rows: Vec<Rect> = Vec::with_capacity(inputs.len());
    let mut output_rows: Vec<Rect> = Vec::with_capacity(outputs.len());

    let rounding = scaled_corner_radius(style, scale);
    let frame = Frame::NONE.show(&mut node_ui, |ui| {
        header_bottom = Frame::NONE
            .inner_margin(scale_margin(style.header_padding, scale))
            .show(ui, |ui| content.header(ui, id, payload))
            .response
            .rect
            .bottom();

        Frame::NONE
            .inner_margin(scale_margin(style.node_padding, scale))
            .show(ui, |ui| {
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
    let scaled_rect = frame.response.rect;

    // Now that the node's width is known, both fills can span it.
    node_ui.painter().set(
        background_slot,
        Shape::Rect(RectShape::new(
            scaled_rect,
            rounding,
            style.node_fill,
            Stroke::new(style.node_stroke.width * scale, style.node_stroke.color),
            StrokeKind::Middle,
        )),
    );
    node_ui.painter().set(
        header_slot,
        Shape::rect_filled(
            Rect::from_min_max(scaled_rect.min, pos2(scaled_rect.max.x, header_bottom)),
            CornerRadius {
                nw: rounding.nw,
                ne: rounding.ne,
                sw: 0,
                se: 0,
            },
            header_fill,
        ),
    );

    let to_graph = to_scaled.inverse();
    let rect = to_graph * scaled_rect;
    let pin_size = Vec2::splat(style.pin_size);
    let inputs = inputs
        .iter()
        .zip(&input_rows)
        .map(|(input, row)| InputPinLayout {
            id: input.id,
            rect: Rect::from_center_size(pos2(rect.left(), (to_graph * row.center()).y), pin_size),
            color: input.color,
            policy: input.policy,
        })
        .collect();
    let outputs = outputs
        .iter()
        .zip(&output_rows)
        .map(|(output, row)| OutputPinLayout {
            id: output.id,
            rect: Rect::from_center_size(pos2(rect.right(), (to_graph * row.center()).y), pin_size),
            color: output.color,
        })
        .collect();

    NodeLayout {
        rect,
        header_color: header_fill,
        inputs,
        outputs,
        splice_pins,
        culled: false,
    }
}

/// What marks one node out from the rest.
pub(super) struct NodeMarks {
    /// The outline around it: selected, or about to be cut.
    pub outline: Option<egui::Color32>,
    /// The pin the pointer is on, wherever it is - only this node's own is
    /// drawn.
    pub hovered_pin: Option<AnyPin>,
}

/// Paints one node's outline and its pins, in the same pass as the node so
/// that a node drawn later covers them - they belong to it, not to the canvas.
/// The outline goes under the pins, so a selected node's pins stay whole. Why
/// not on the overlay: `5-Crisp Text.md`.
///
/// `layout` is in graph units; `scale` puts it in the units the nodes layer is
/// drawn in.
pub(super) fn draw_marks(
    painter: &egui::Painter,
    style: &CanvasStyle,
    scale: f32,
    id: NodeId,
    layout: &NodeLayout,
    marks: &NodeMarks,
) {
    let to_scaled = TSTransform::from_scaling(scale);
    if let Some(color) = marks.outline {
        // On the node's own rect with the stroke centred on its edge: the
        // inner half covers the node's anti-aliased edge pixels, the outer
        // half is the visible halo, and there is no second shape to leave a
        // gap at the rounded corners.
        painter.rect_stroke(
            to_scaled * layout.rect,
            scaled_corner_radius(style, scale),
            Stroke::new(style.selection_width * scale, color),
            StrokeKind::Middle,
        );
    }
    let pin_stroke = Stroke::new(style.pin_stroke.width * scale, style.pin_stroke.color);
    for (pin, rect) in layout.pins(id) {
        let Some(color) = layout.pin_color(pin) else {
            continue;
        };
        let (rect, color) = if marks.hovered_pin == Some(pin) {
            (rect.expand(style.pin_size * 0.35), highlight(color))
        } else {
            (rect, color)
        };
        draw_pin(
            painter,
            style.pin_shape,
            to_scaled * rect,
            color,
            pin_stroke,
        );
    }
}

pub(super) fn draw_pin(
    painter: &egui::Painter,
    shape: PinShape,
    rect: Rect,
    fill: egui::Color32,
    stroke: Stroke,
) {
    match shape {
        PinShape::Circle => painter.circle(rect.center(), rect.width() / 2.0, fill, stroke),
        PinShape::Square => painter.rect(rect, 0.0, fill, stroke, StrokeKind::Middle),
    };
}

/// A lighter shade of a colour, for what the pointer is on.
pub(super) fn highlight(color: egui::Color32) -> egui::Color32 {
    color.lerp_to_gamma(egui::Color32::WHITE, 0.35)
}

/// Where one node goes this frame.
pub(super) struct NodePlacement {
    pub id: NodeId,
    /// Graph space, the node's top-left corner.
    pub pos: egui::Pos2,
    /// What the node measured last time. The frame it first appears it is
    /// laid out against `style.default_node_size` instead, as egui's own
    /// `Area` does.
    pub known_size: Option<Vec2>,
}

/// The factor between graph units and the units the `Ui` handed to
/// `NodeContent` is laid out in - the zoom, when zoomed in past 1, so that
/// text is rasterised at the size it is shown at. Multiply a hard-coded
/// graph-space size by it before handing it to a widget. See
/// `5-Crisp Text.md`.
pub fn content_scale(ui: &Ui) -> f32 {
    ui.ctx()
        .data(|data| data.get_temp(content_scale_key(ui.layer_id())))
        .unwrap_or(1.0)
}

pub(super) fn content_scale_key(layer: egui::LayerId) -> Id {
    layer.id.with("egui_noodle_content_scale")
}

pub(super) fn scaled_corner_radius(style: &CanvasStyle, scale: f32) -> CornerRadius {
    CornerRadius::same((style.node_rounding * scale).round() as u8)
}

fn scale_margin(margin: Margin, scale: f32) -> Margin {
    let scaled = |side: i8| (side as f32 * scale).round().clamp(0.0, i8::MAX as f32) as i8;
    Margin {
        left: scaled(margin.left),
        right: scaled(margin.right),
        top: scaled(margin.top),
        bottom: scaled(margin.bottom),
    }
}

/// Scales every length in the style that node content lays out with. egui
/// offers nothing for this itself; what is covered is what ordinary widgets
/// read.
fn scale_style(style: &mut Style, scale: f32) {
    if scale == 1.0 {
        return;
    }
    for font in style.text_styles.values_mut() {
        font.size *= scale;
    }
    let spacing = &mut style.spacing;
    spacing.item_spacing *= scale;
    spacing.button_padding *= scale;
    spacing.menu_margin = scale_margin(spacing.menu_margin, scale);
    spacing.window_margin = scale_margin(spacing.window_margin, scale);
    spacing.indent *= scale;
    spacing.interact_size *= scale;
    spacing.slider_width *= scale;
    spacing.slider_rail_height *= scale;
    spacing.combo_width *= scale;
    spacing.text_edit_width *= scale;
    spacing.icon_width *= scale;
    spacing.icon_width_inner *= scale;
    spacing.icon_spacing *= scale;
    spacing.combo_height *= scale;
    let widgets = &mut style.visuals.widgets;
    for visuals in [
        &mut widgets.noninteractive,
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        visuals.bg_stroke.width *= scale;
        visuals.fg_stroke.width *= scale;
        visuals.expansion *= scale;
        visuals.corner_radius = scale_corner_radius(visuals.corner_radius, scale);
    }
    style.visuals.selection.stroke.width *= scale;
}

fn scale_corner_radius(radius: CornerRadius, scale: f32) -> CornerRadius {
    let scaled = |corner: u8| (corner as f32 * scale).round().min(u8::MAX as f32) as u8;
    CornerRadius {
        nw: scaled(radius.nw),
        ne: scaled(radius.ne),
        sw: scaled(radius.sw),
        se: scaled(radius.se),
    }
}
