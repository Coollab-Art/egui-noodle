use egui::{Painter, Rect, Stroke, pos2};

/// Grid lines over the visible part of graph space. `zoom` keeps the stroke
/// one screen pixel wide whatever the view.
pub(super) fn draw_grid(
    painter: &Painter,
    viewport: Rect,
    spacing: f32,
    stroke: Stroke,
    zoom: f32,
) {
    if spacing <= 0.0 || !viewport.is_finite() {
        return;
    }
    let stroke = Stroke::new(stroke.width / zoom, stroke.color);

    let first_column = (viewport.left() / spacing).floor() as i64;
    let last_column = (viewport.right() / spacing).ceil() as i64;
    for column in first_column..=last_column {
        let x = column as f32 * spacing;
        painter.line_segment(
            [pos2(x, viewport.top()), pos2(x, viewport.bottom())],
            stroke,
        );
    }

    let first_row = (viewport.top() / spacing).floor() as i64;
    let last_row = (viewport.bottom() / spacing).ceil() as i64;
    for row in first_row..=last_row {
        let y = row as f32 * spacing;
        painter.line_segment(
            [pos2(viewport.left(), y), pos2(viewport.right(), y)],
            stroke,
        );
    }
}
