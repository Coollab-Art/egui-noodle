use egui::{Pos2, Rangef, Rect, Vec2, emath::TSTransform};

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
}
