use super::WireRouting;
use egui::{Color32, CornerRadius, Margin, Rangef, Stroke, Vec2};

/// Everything about how a canvas looks. Plain fields, so an application
/// builds one from its own theme.
pub struct CanvasStyle {
    pub background: Color32,
    pub grid_spacing: f32,
    /// Width in screen pixels, whatever the zoom.
    pub grid_stroke: Stroke,

    pub node_fill: Color32,
    pub node_stroke: Stroke,
    pub node_rounding: f32,
    /// Spans the node's full width; its top corners follow `node_rounding`.
    pub header_fill: Color32,
    pub header_padding: Margin,
    /// Around the pin rows and body, below the header.
    pub node_padding: Margin,
    /// What a node is laid out against on the frame it first appears, before
    /// its size is known. Content that fills the available width fills this.
    pub default_node_size: Vec2,

    pub pin_shape: PinShape,
    pub pin_size: f32,
    /// Grows the grab area past the drawn pin: grabbing a wire should not be
    /// a precision task.
    pub pin_hit_expansion: f32,
    pub pin_stroke: Stroke,

    pub wire_width: f32,
    pub wire_routing: WireRouting,
    /// Screen pixels a wire's corner arcs may deviate from the true arc.
    pub wire_tolerance: f32,
    /// Graph units either side of a wire within which it counts as hovered.
    pub wire_hit_slack: f32,
    /// The cutting stroke, in screen pixels; its colour also marks the wires
    /// about to be cut.
    pub wire_cut_stroke: Stroke,

    pub zoom_range: Rangef,

    /// Drawn on a selected node's own rect, centred on its edge.
    pub selection_stroke: Stroke,
    /// Screen pixels, whatever the zoom.
    pub box_select_stroke: Stroke,
    pub box_select_fill: Color32,

    /// Screen pixels within which a dragged node aligns with another.
    pub snap_distance: f32,
    pub snap_to_grid: bool,
    /// Screen pixels, whatever the zoom.
    pub snap_guide_stroke: Stroke,

    /// The `+` shown on a hovered wire, in graph units.
    pub insert_button_size: f32,
    pub insert_button_fill: Color32,
    /// Graph units kept clear on either side of a node spliced into a wire,
    /// when the nodes downstream slide apart to make room.
    pub auto_offset_margin: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinShape {
    Circle,
    Square,
}

impl CanvasStyle {
    /// The node frame's corners; the selection halo uses the same so the two
    /// coincide exactly.
    pub(super) fn corner_radius(&self) -> CornerRadius {
        CornerRadius::same(self.node_rounding.round() as u8)
    }
}

/// A stroke whose width is given in screen pixels, made `zoom`-independent
/// for drawing in graph space.
pub(super) fn screen_stroke(stroke: Stroke, zoom: f32) -> Stroke {
    Stroke::new(stroke.width / zoom, stroke.color)
}

impl Default for CanvasStyle {
    fn default() -> Self {
        Self {
            background: Color32::from_gray(28),
            grid_spacing: 50.0,
            grid_stroke: Stroke::new(1.0, Color32::from_gray(44)),

            node_fill: Color32::from_gray(60),
            node_stroke: Stroke::NONE,
            node_rounding: 4.0,
            header_fill: Color32::from_gray(80),
            header_padding: Margin::symmetric(6, 4),
            node_padding: Margin::same(6),
            default_node_size: Vec2::new(150.0, 60.0),

            pin_shape: PinShape::Circle,
            pin_size: 8.0,
            pin_hit_expansion: 6.0,
            pin_stroke: Stroke::NONE,

            wire_width: 2.0,
            wire_routing: WireRouting::default(),
            wire_tolerance: 0.3,
            wire_hit_slack: 6.0,
            wire_cut_stroke: Stroke::new(2.0, Color32::from_rgb(230, 70, 60)),

            zoom_range: Rangef::new(0.1, 3.0),

            selection_stroke: Stroke::new(3.0, Color32::from_gray(220)),
            box_select_stroke: Stroke::new(1.0, Color32::from_gray(200)),
            box_select_fill: Color32::from_rgba_unmultiplied(200, 200, 200, 30),

            snap_distance: 8.0,
            snap_to_grid: false,
            snap_guide_stroke: Stroke::new(1.0, Color32::from_rgb(255, 160, 60)),

            insert_button_size: 14.0,
            insert_button_fill: Color32::from_gray(90),
            auto_offset_margin: 24.0,
        }
    }
}
