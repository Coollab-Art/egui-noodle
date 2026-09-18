//! Aligning a dragged group of nodes with the nodes around it.

use egui::{Rect, Vec2};

pub(super) struct Snap {
    /// How far to shift the dragged group.
    pub delta: Vec2,
    /// The graph-space x and y lines the group snapped to, for drawing guides.
    pub guide_x: Option<f32>,
    pub guide_y: Option<f32>,
}

/// Shifts `moving` so that one of its edges or its centre lines up with an
/// edge or centre of one of `others`, when one is within `threshold` on that
/// axis. Each axis snaps independently, to its nearest candidate.
pub(super) fn snap_to_nodes(
    moving: Rect,
    others: impl Iterator<Item = Rect>,
    threshold: f32,
) -> Snap {
    let mut best_x: Option<(f32, f32)> = None; // (|delta|, candidate line)
    let mut best_y: Option<(f32, f32)> = None;
    for other in others {
        consider_axis(
            [moving.left(), moving.center().x, moving.right()],
            [other.left(), other.center().x, other.right()],
            threshold,
            &mut best_x,
        );
        consider_axis(
            [moving.top(), moving.center().y, moving.bottom()],
            [other.top(), other.center().y, other.bottom()],
            threshold,
            &mut best_y,
        );
    }
    let delta_for = |best: Option<(f32, f32)>, moving_lines: [f32; 3]| {
        best.map(|(_, line)| {
            // Which of our lines snapped: the one nearest the candidate.
            let nearest = moving_lines
                .iter()
                .copied()
                .min_by(|a, b| (a - line).abs().total_cmp(&(b - line).abs()))
                .unwrap_or(line);
            line - nearest
        })
    };
    Snap {
        delta: Vec2::new(
            delta_for(best_x, [moving.left(), moving.center().x, moving.right()]).unwrap_or(0.0),
            delta_for(best_y, [moving.top(), moving.center().y, moving.bottom()]).unwrap_or(0.0),
        ),
        guide_x: best_x.map(|(_, line)| line),
        guide_y: best_y.map(|(_, line)| line),
    }
}

fn consider_axis(
    moving: [f32; 3],
    candidates: [f32; 3],
    threshold: f32,
    best: &mut Option<(f32, f32)>,
) {
    for line in moving {
        for candidate in candidates {
            let distance = (candidate - line).abs();
            if distance <= threshold
                && best.is_none_or(|(best_distance, _)| distance < best_distance)
            {
                *best = Some((distance, candidate));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(pos2(x, y), vec2(w, h))
    }

    #[test]
    fn a_left_edge_snaps_flush_with_a_nearby_left_edge() {
        let snap = snap_to_nodes(
            rect(103.0, 300.0, 50.0, 50.0),
            [rect(100.0, 0.0, 80.0, 40.0)].into_iter(),
            8.0,
        );
        assert_eq!(snap.delta.x, -3.0);
        assert_eq!(snap.guide_x, Some(100.0));
        assert_eq!(snap.delta.y, 0.0, "nothing near on y");
        assert_eq!(snap.guide_y, None);
    }

    #[test]
    fn nothing_snaps_beyond_the_threshold() {
        // Nearest lines: left 200 vs right 180, twenty apart.
        let snap = snap_to_nodes(
            rect(200.0, 300.0, 50.0, 50.0),
            [rect(100.0, 0.0, 80.0, 40.0)].into_iter(),
            8.0,
        );
        assert_eq!(snap.delta, Vec2::ZERO);
        assert_eq!(snap.guide_x, None);
    }

    #[test]
    fn the_nearest_candidate_wins() {
        let snap = snap_to_nodes(
            rect(103.0, 300.0, 50.0, 50.0),
            [rect(100.0, 0.0, 80.0, 40.0), rect(105.0, 0.0, 80.0, 40.0)].into_iter(),
            8.0,
        );
        assert_eq!(snap.guide_x, Some(105.0));
        assert_eq!(snap.delta.x, 2.0);
    }

    #[test]
    fn centres_snap_to_centres() {
        // Different heights, so only the centres come close: moving lines at
        // y = 3, 23, 43 against 0, 25, 50 - the centres are two apart, the
        // tops three, the bottoms seven.
        let snap = snap_to_nodes(
            rect(500.0, 3.0, 50.0, 40.0),
            [rect(0.0, 0.0, 80.0, 50.0)].into_iter(),
            8.0,
        );
        assert_eq!(snap.delta.y, 2.0);
        assert_eq!(snap.guide_y, Some(25.0));
    }
}
