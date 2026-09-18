use super::WireRouting;
use egui::{Color32, Margin, Rangef, Stroke, Vec2};

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

    pub zoom_range: Rangef,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinShape {
    Circle,
    Square,
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

            zoom_range: Rangef::new(0.1, 3.0),
        }
    }
}
