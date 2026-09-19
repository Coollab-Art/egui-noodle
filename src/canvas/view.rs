//! Where the user is looking, and the input that moves it: panning, zooming,
//! and the edge of the panel pulling the view along with a drag.

use super::{Canvas, CanvasStyle};
use egui::{PointerButton, Pos2, Rangef, Rect, Response, Ui, Vec2, emath::TSTransform};

/// Where the user is looking. Stored as a graph-space centre and a zoom,
/// never as a screen-space transform, so it means the same thing whatever
/// panel it is drawn into: moving or resizing the panel keeps the same graph
/// point at the centre and the same zoom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewState {
    /// The graph-space point at the centre of the panel.
    pub center: Pos2,
    /// Screen pixels per graph unit.
    pub zoom: f32,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            center: Pos2::ZERO,
            zoom: 1.0,
        }
    }
}

impl ViewState {
    /// Graph space to screen space, for a canvas drawn into `panel`.
    pub fn to_global(&self, panel: Rect) -> TSTransform {
        TSTransform::from_translation(panel.center().to_vec2())
            * TSTransform::from_scaling(self.zoom)
            * TSTransform::from_translation(-self.center.to_vec2())
    }

    /// Moves the view so that what was under the pointer follows a drag of
    /// `screen_delta`.
    pub fn pan_by_screen(&mut self, screen_delta: Vec2) {
        self.center -= screen_delta / self.zoom;
    }

    /// Multiplies the zoom by `factor`, clamped to `range`, keeping the graph
    /// point under `screen_pointer` exactly where it is.
    pub fn zoom_about(&mut self, screen_pointer: Pos2, panel: Rect, factor: f32, range: Rangef) {
        let anchor = self.to_global(panel).inverse() * screen_pointer;
        self.zoom = range.clamp(self.zoom * factor);
        self.center = anchor - (screen_pointer - panel.center()) / self.zoom;
    }
}

impl Canvas {
    /// Pan and zoom from the background's input. Middle or secondary drag
    /// pans and leaves the primary button free for selection; a primary drag
    /// near the panel's edge pans too, so a node can be carried further than
    /// the panel shows; an unmodified wheel zooms around the pointer; a
    /// modified wheel (shift or alt, which egui turns into horizontal or
    /// vertical scroll) pans; ctrl and pinch zoom through egui's own
    /// `zoom_delta`.
    pub(super) fn navigate(
        &mut self,
        ui: &Ui,
        background: &Response,
        panel_rect: Rect,
        over_canvas: bool,
        style: &CanvasStyle,
    ) {
        if background.dragged_by(PointerButton::Middle)
            || background.dragged_by(PointerButton::Secondary)
        {
            // The response lives on the transformed layer, so its delta is in
            // graph units.
            self.view
                .pan_by_screen(background.drag_delta() * self.view.zoom);
        }

        let Some(pointer) = ui.input(|input| input.pointer.latest_pos()) else {
            return;
        };
        if self.gesture.is_drag() {
            let depth = edge_depth(panel_rect, pointer, style.edge_pan_margin);
            if depth != Vec2::ZERO {
                let delta_time = ui.input(|input| input.stable_dt);
                self.view
                    .pan_by_screen(-depth * style.edge_pan_speed * delta_time);
                ui.ctx().request_repaint();
            }
        }

        if !over_canvas {
            return;
        }
        let (mut zoom_factor, mut scroll, modifiers) = ui.input(|input| {
            (
                input.zoom_delta(),
                input.smooth_scroll_delta(),
                input.modifiers,
            )
        });
        if modifiers.is_none() {
            // The curve egui's `InputState` applies to a modified wheel, so an
            // unmodified one zooms at the same rate.
            let speed = ui
                .ctx()
                .options(|options| options.input_options.scroll_zoom_speed);
            zoom_factor *= (speed * (scroll.x + scroll.y)).exp();
            scroll = Vec2::ZERO;
        }
        if zoom_factor != 1.0 {
            self.view
                .zoom_about(pointer, panel_rect, zoom_factor, style.zoom_range);
        }
        if scroll != Vec2::ZERO {
            self.view.pan_by_screen(scroll);
        }
    }
}

/// How deep into the panel's edge band the pointer is, per axis, from -1 (at
/// or past the left or top edge) to 1 (right or bottom). Zero away from the
/// edges. Carrying the drag off the panel saturates rather than stops, so the
/// view keeps moving while the pointer is held outside.
fn edge_depth(panel: Rect, pointer: Pos2, margin: f32) -> Vec2 {
    let depth = |low: f32, high: f32, at: f32| {
        if at < low + margin {
            -((low + margin - at) / margin).min(1.0)
        } else if at > high - margin {
            ((at - (high - margin)) / margin).min(1.0)
        } else {
            0.0
        }
    };
    Vec2::new(
        depth(panel.left(), panel.right(), pointer.x),
        depth(panel.top(), panel.bottom(), pointer.y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn assert_close(actual: Pos2, expected: Pos2) {
        assert!(
            actual.distance(expected) < 1e-3,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn the_centre_lands_on_the_middle_of_the_panel() {
        let view = ViewState {
            center: pos2(100.0, 50.0),
            zoom: 2.0,
        };
        let panel = Rect::from_min_size(pos2(300.0, 200.0), vec2(400.0, 300.0));
        assert_close(view.to_global(panel) * view.center, panel.center());
    }

    /// The reason the view is not stored as a screen-space transform.
    #[test]
    fn resizing_the_panel_keeps_zoom_and_centre() {
        let view = ViewState {
            center: pos2(100.0, 50.0),
            zoom: 1.5,
        };
        let small = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));
        let large = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 300.0));
        assert_eq!(view.to_global(small).scaling, view.to_global(large).scaling);
        assert_close(view.to_global(large) * view.center, large.center());
    }

    #[test]
    fn panning_moves_the_graph_by_the_screen_delta() {
        let mut view = ViewState {
            center: Pos2::ZERO,
            zoom: 2.0,
        };
        let panel = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0));
        let before = view.to_global(panel) * pos2(10.0, 10.0);
        view.pan_by_screen(vec2(30.0, -20.0));
        let after = view.to_global(panel) * pos2(10.0, 10.0);
        assert_close(after, before + vec2(30.0, -20.0));
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer_still() {
        let mut view = ViewState {
            center: pos2(20.0, 30.0),
            zoom: 1.0,
        };
        let panel = Rect::from_min_size(pos2(100.0, 100.0), vec2(400.0, 300.0));
        let pointer = pos2(150.0, 320.0);
        let under_pointer = view.to_global(panel).inverse() * pointer;

        view.zoom_about(pointer, panel, 2.5, Rangef::new(0.1, 10.0));

        assert_eq!(view.zoom, 2.5);
        assert_close(view.to_global(panel) * under_pointer, pointer);
    }

    #[test]
    fn a_clamped_zoom_still_keeps_the_pointer_still() {
        let mut view = ViewState {
            center: Pos2::ZERO,
            zoom: 2.0,
        };
        let panel = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0));
        let pointer = pos2(50.0, 50.0);
        let under_pointer = view.to_global(panel).inverse() * pointer;

        view.zoom_about(pointer, panel, 100.0, Rangef::new(0.1, 3.0));

        assert_eq!(view.zoom, 3.0);
        assert_close(view.to_global(panel) * under_pointer, pointer);
    }

    #[test]
    fn the_edge_band_ramps_from_nothing_to_full_and_saturates_outside() {
        let panel = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 300.0));
        assert_eq!(edge_depth(panel, pos2(200.0, 150.0), 40.0), Vec2::ZERO);
        assert_eq!(
            edge_depth(panel, pos2(380.0, 150.0), 40.0),
            Vec2::new(0.5, 0.0)
        );
        assert_eq!(
            edge_depth(panel, pos2(400.0, 150.0), 40.0),
            Vec2::new(1.0, 0.0)
        );
        assert_eq!(
            edge_depth(panel, pos2(200.0, 10.0), 40.0),
            Vec2::new(0.0, -0.75)
        );
        // Carried off the panel: full speed, not stopped.
        assert_eq!(
            edge_depth(panel, pos2(500.0, 150.0), 40.0),
            Vec2::new(1.0, 0.0)
        );
        assert_eq!(
            edge_depth(panel, pos2(200.0, -20.0), 40.0),
            Vec2::new(0.0, -1.0)
        );
    }
}
