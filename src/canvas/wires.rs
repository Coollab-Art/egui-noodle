//! Wire geometry: routing a wire between two pins, rounding its corners, and
//! the hit-testing that runs against the exact polyline that was drawn.
//!
//! Wires are orthogonal - horizontal, vertical, horizontal - with rounded
//! corners. One polyline serves drawing, hit-testing, box-selecting and
//! cutting alike, so what the user grabs is by construction what they see.
//! See `3-Orthogonal Wires.md`.

use egui::{Pos2, Rangef, Rect, Vec2, pos2};

/// The geometry knobs shared by every wire on a canvas.
pub struct WireRouting {
    /// How far a wire runs straight out of its source pin, and straight into
    /// its target, before it may turn. The route is computed between the ends
    /// of those two stubs rather than between the pins - except between two
    /// nodes that sit side by side, which have nothing to keep the wire clear
    /// of and connect straight across.
    pub stub: f32,
    pub corner_radius: f32,
    /// How far above or below the two nodes a backward wire's return run
    /// sits, when it has to clear them both.
    pub backward_clearance: f32,
}

impl Default for WireRouting {
    fn default() -> Self {
        Self {
            stub: 24.0,
            corner_radius: 8.0,
            backward_clearance: 24.0,
        }
    }
}

/// Where a wire starts and ends, and the nodes it must route around.
///
/// A wire being dragged has no node on its loose end yet: pass a zero-sized
/// rect at the pointer.
pub struct WireEndpoints {
    /// The output pin's centre. The wire leaves it heading right.
    pub from: Pos2,
    /// The input pin's centre. The wire arrives at it heading right.
    pub to: Pos2,
    pub from_node: Rect,
    pub to_node: Rect,
}

/// The corner points of a wire's path, before rounding. Graph space.
///
/// A wire turns twice - out of the source, down or up mid-gap, and into the
/// target - whenever it can run straight across. It turns four times, looping
/// back past both nodes, only when it cannot: a target to the left of its
/// source, or one below or above it and too close to leave room for the two
/// stubs. `lane_offset` shifts the first vertical run so sibling wires leaving
/// one node do not share it.
pub fn route_wire(ends: &WireEndpoints, routing: &WireRouting, lane_offset: f32) -> Vec<Pos2> {
    let (from, to) = (ends.from, ends.to);
    let (out_x, in_x) = (from.x + routing.stub, to.x - routing.stub);
    let gap = vertical_gap(ends.from_node, ends.to_node);

    // Straight across when the two stubs leave room between them - or, even
    // when they do not, when the nodes sit side by side. Squeezing the
    // vertical run between two close pins reads fine there; it is a target
    // below or above its source that needs the stubs, or the wire runs down
    // through both pins and reads as one straight line touching them.
    let side_by_side = to.x > from.x && gap.is_none();
    if in_x > out_x || side_by_side {
        if from.y == to.y {
            return vec![from, to];
        }
        // The vertical sits mid-gap, so the two horizontal runs read as equal
        // stubs out of the source and into the target.
        let lane = if in_x > out_x {
            Rangef::new(out_x, in_x)
        } else {
            Rangef::new(from.x, to.x)
        };
        let vertical_x = lane.clamp((from.x + to.x) / 2.0 + lane_offset);
        return vec![from, pos2(vertical_x, from.y), pos2(vertical_x, to.y), to];
    }

    // Out of the source, back past both nodes, and in from the left of the
    // target. The return run goes between the two nodes whenever they leave a
    // gap - that lane crosses neither of them and keeps the wire in the space
    // the eye already reads as between them - and otherwise over or under
    // both, on the side nearer the pins.
    let out_x = out_x + lane_offset;
    let return_y = match gap {
        Some(gap) => gap.center(),
        None => {
            let above = ends.from_node.top().min(ends.to_node.top()) - routing.backward_clearance;
            let below =
                ends.from_node.bottom().max(ends.to_node.bottom()) + routing.backward_clearance;
            let middle_y = (from.y + to.y) / 2.0;
            if middle_y - above <= below - middle_y {
                above
            } else {
                below
            }
        }
    };
    vec![
        from,
        pos2(out_x, from.y),
        pos2(out_x, return_y),
        pos2(in_x, return_y),
        pos2(in_x, to.y),
        to,
    ]
}

/// The clear vertical space between two rects, or `None` when they overlap on
/// that axis.
fn vertical_gap(a: Rect, b: Rect) -> Option<Rangef> {
    if a.top() > b.bottom() {
        Some(Rangef::new(b.bottom(), a.top()))
    } else if b.top() > a.bottom() {
        Some(Rangef::new(a.bottom(), b.top()))
    } else {
        None
    }
}

/// Replaces every interior corner of `route` with an arc of the given radius,
/// sampled finely enough that no chord strays more than `tolerance` from the
/// true arc. The radius shrinks at a corner whose adjacent segments are too
/// short to fit it. Endpoints are preserved.
pub fn round_corners(route: &[Pos2], radius: f32, tolerance: f32) -> Vec<Pos2> {
    use std::f32::consts::{PI, TAU};

    if route.len() < 3 || radius <= 0.0 {
        return route.to_vec();
    }

    let mut rounded = Vec::with_capacity(route.len() * 4);
    rounded.push(route[0]);
    for window in route.windows(3) {
        let (previous, corner, next) = (window[0], window[1], window[2]);
        let incoming = corner - previous;
        let outgoing = next - corner;
        let (incoming_length, outgoing_length) = (incoming.length(), outgoing.length());
        if incoming_length == 0.0 || outgoing_length == 0.0 {
            continue; // A duplicated point, not a corner.
        }
        let incoming_direction = incoming / incoming_length;
        let outgoing_direction = outgoing / outgoing_length;
        let cross = incoming_direction.x * outgoing_direction.y
            - incoming_direction.y * outgoing_direction.x;
        if cross.abs() < 1e-4 {
            rounded.push(corner); // Straight through, or a U-turn: nothing to round.
            continue;
        }

        // The arc starts and ends `tangent` away from the corner along each
        // segment. For a turn of angle theta, the arc tangent to both segments
        // there has radius tangent / tan(theta / 2) - the plain radius for a
        // right angle.
        let tangent = radius.min(incoming_length / 2.0).min(outgoing_length / 2.0);
        let arc_start = corner - incoming_direction * tangent;
        let arc_end = corner + outgoing_direction * tangent;
        let half_turn = incoming_direction
            .dot(outgoing_direction)
            .clamp(-1.0, 1.0)
            .acos()
            / 2.0;
        let arc_radius = tangent / half_turn.tan();
        // The centre is perpendicular to the incoming run, on the side we turn to.
        let toward_center = Vec2::new(-incoming_direction.y, incoming_direction.x) * cross.signum();
        let center = arc_start + toward_center * arc_radius;

        let start_angle = (arc_start - center).angle();
        let mut sweep = (arc_end - center).angle() - start_angle;
        if sweep > PI {
            sweep -= TAU;
        } else if sweep < -PI {
            sweep += TAU;
        }
        // A chord over `step` radians sits radius * (1 - cos(step / 2)) inside
        // the true arc; keep that within tolerance.
        let max_step = if tolerance >= arc_radius {
            sweep.abs()
        } else {
            2.0 * (1.0 - tolerance / arc_radius).acos()
        };
        let segments = (sweep.abs() / max_step).ceil().max(1.0) as usize;
        for i in 0..=segments {
            let angle = start_angle + sweep * (i as f32 / segments as f32);
            rounded.push(center + Vec2::angled(angle) * arc_radius);
        }
    }
    rounded.push(route[route.len() - 1]);
    rounded
}

/// Shortest distance from `point` to any segment of the polyline.
pub fn distance_to_polyline(polyline: &[Pos2], point: Pos2) -> f32 {
    match polyline {
        [] => f32::INFINITY,
        [only] => point.distance(*only),
        _ => polyline
            .windows(2)
            .map(|segment| distance_to_segment(segment[0], segment[1], point))
            .fold(f32::INFINITY, f32::min),
    }
}

fn distance_to_segment(a: Pos2, b: Pos2, point: Pos2) -> f32 {
    let ab = b - a;
    let length_squared = ab.length_sq();
    if length_squared == 0.0 {
        return point.distance(a);
    }
    let t = ((point - a).dot(ab) / length_squared).clamp(0.0, 1.0);
    point.distance(a + ab * t)
}

/// Whether the segment `a`-`b` touches or crosses any segment of the polyline.
pub fn polyline_crosses_segment(polyline: &[Pos2], a: Pos2, b: Pos2) -> bool {
    polyline
        .windows(2)
        .any(|segment| segments_intersect(segment[0], segment[1], a, b))
}

/// Whether any part of the polyline lies in or crosses `rect`.
pub fn polyline_intersects_rect(polyline: &[Pos2], rect: Rect) -> bool {
    match polyline {
        [] => false,
        [only] => rect.contains(*only),
        _ => polyline
            .windows(2)
            .any(|segment| segment_intersects_rect(segment[0], segment[1], rect)),
    }
}

/// Whether the segment `a`-`b` has a point inside `rect` or on its edge.
pub fn segment_intersects_rect(a: Pos2, b: Pos2, rect: Rect) -> bool {
    if rect.contains(a) || rect.contains(b) {
        return true;
    }
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    (0..4).any(|i| segments_intersect(a, b, corners[i], corners[(i + 1) % 4]))
}

/// Proper crossings and touching endpoints alike.
fn segments_intersect(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> bool {
    let side_of_cd_a = orientation(c, d, a);
    let side_of_cd_b = orientation(c, d, b);
    let side_of_ab_c = orientation(a, b, c);
    let side_of_ab_d = orientation(a, b, d);

    let straddles =
        |first: f32, second: f32| (first > 0.0 && second < 0.0) || (first < 0.0 && second > 0.0);
    if straddles(side_of_cd_a, side_of_cd_b) && straddles(side_of_ab_c, side_of_ab_d) {
        return true;
    }
    // Collinear cases: an endpoint of one lies on the other.
    (side_of_cd_a == 0.0 && Rect::from_two_pos(c, d).contains(a))
        || (side_of_cd_b == 0.0 && Rect::from_two_pos(c, d).contains(b))
        || (side_of_ab_c == 0.0 && Rect::from_two_pos(a, b).contains(c))
        || (side_of_ab_d == 0.0 && Rect::from_two_pos(a, b).contains(d))
}

/// Which side of the line through `a` and `b` the point `c` lies on: positive,
/// negative, or zero for collinear.
fn orientation(a: Pos2, b: Pos2, c: Pos2) -> f32 {
    let ab = b - a;
    let ac = c - a;
    ab.x * ac.y - ab.y * ac.x
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    const ROUTING: WireRouting = WireRouting {
        stub: 20.0,
        corner_radius: 8.0,
        backward_clearance: 30.0,
    };

    fn forward_ends(from: Pos2, to: Pos2) -> WireEndpoints {
        WireEndpoints {
            from,
            to,
            from_node: Rect::from_min_max(from - vec2(100.0, 20.0), from + vec2(0.0, 20.0)),
            to_node: Rect::from_min_max(to - vec2(0.0, 20.0), to + vec2(100.0, 20.0)),
        }
    }

    fn assert_close(actual: f32, expected: f32, tolerance: f32) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected} +- {tolerance}, got {actual}"
        );
    }

    fn all_finite(points: &[Pos2]) -> bool {
        points.iter().all(|p| p.x.is_finite() && p.y.is_finite())
    }

    #[test]
    fn a_forward_wire_is_horizontal_then_vertical_then_horizontal() {
        let route = route_wire(
            &forward_ends(pos2(0.0, 0.0), pos2(200.0, 100.0)),
            &ROUTING,
            0.0,
        );
        assert_eq!(route.len(), 4);
        assert_eq!(route[0], pos2(0.0, 0.0));
        assert_eq!(route[3], pos2(200.0, 100.0));
        assert_eq!(route[0].y, route[1].y, "first run is horizontal");
        assert_eq!(route[1].x, route[2].x, "middle run is vertical");
        assert_eq!(route[2].y, route[3].y, "last run is horizontal");
    }

    #[test]
    fn a_forward_wire_turns_mid_gap() {
        let route = route_wire(
            &forward_ends(pos2(0.0, 0.0), pos2(200.0, 100.0)),
            &ROUTING,
            0.0,
        );
        assert_eq!(route[1].x, 100.0);
    }

    #[test]
    fn a_wire_between_aligned_pins_is_a_straight_line() {
        let route = route_wire(
            &forward_ends(pos2(0.0, 50.0), pos2(200.0, 50.0)),
            &ROUTING,
            0.0,
        );
        assert_eq!(route, vec![pos2(0.0, 50.0), pos2(200.0, 50.0)]);
    }

    #[test]
    fn a_lane_offset_shifts_the_vertical_run_but_never_into_a_stub() {
        let ends = forward_ends(pos2(0.0, 0.0), pos2(200.0, 100.0));
        let base = route_wire(&ends, &ROUTING, 0.0);
        let shifted = route_wire(&ends, &ROUTING, 12.0);
        assert_eq!(shifted[1].x, base[1].x + 12.0);
        let overshooting = route_wire(&ends, &ROUTING, 500.0);
        assert_eq!(overshooting[1].x, 200.0 - ROUTING.stub);
        assert!(all_finite(&overshooting));
    }

    /// Two nodes side by side, the target barely to the right of the source:
    /// no room for the two stubs, but nothing to loop around either, so the
    /// wire turns once each way and goes straight across.
    #[test]
    fn a_target_beside_its_source_is_routed_straight_across() {
        let from_node = Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 100.0));
        let to_node = Rect::from_min_max(pos2(210.0, 40.0), pos2(400.0, 140.0));
        let ends = WireEndpoints {
            from: pos2(200.0, 20.0),
            to: pos2(210.0, 60.0),
            from_node,
            to_node,
        };

        let route = route_wire(&ends, &ROUTING, 0.0);

        assert_eq!(route.len(), 4);
        assert_eq!(route[1].x, 205.0, "the vertical sits between the two pins");
        assert_eq!(route[1].y, 20.0);
        assert_eq!(route[2].y, 60.0);
    }

    /// A target barely to the right of its source but *below* it: there is no
    /// room between the two stubs, so the wire takes the four-turn route
    /// rather than collapsing into one vertical run straight through both
    /// pins.
    #[test]
    fn a_target_within_a_stub_below_its_source_is_not_routed_straight_down() {
        let from_node = Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 100.0));
        let to_node = Rect::from_min_max(pos2(204.0, 200.0), pos2(400.0, 300.0));
        let ends = WireEndpoints {
            from: pos2(200.0, 50.0),
            to: pos2(204.0, 250.0),
            from_node,
            to_node,
        };

        let route = route_wire(&ends, &ROUTING, 0.0);

        assert_eq!(route.len(), 6);
        assert_eq!(route[1].x, 220.0, "leaves by a stub");
        assert_eq!(route[4].x, 184.0, "arrives from a stub out");
        assert_eq!(route[2].y, 150.0, "returning between the two nodes");
    }

    /// Nodes one above the other, as a wire feeding a node up and to the
    /// left finds them: the return run takes the gap between them rather than
    /// going around the outside of both.
    #[test]
    fn a_backward_wire_returns_between_nodes_that_leave_a_gap() {
        let from_node = Rect::from_min_max(pos2(0.0, 200.0), pos2(200.0, 300.0));
        let to_node = Rect::from_min_max(pos2(-100.0, 0.0), pos2(100.0, 100.0));
        let ends = WireEndpoints {
            from: pos2(200.0, 250.0),
            to: pos2(-100.0, 50.0),
            from_node,
            to_node,
        };

        let route = route_wire(&ends, &ROUTING, 0.0);

        assert_eq!(route.len(), 6);
        let return_run_y = route[2].y;
        assert_eq!(route[3].y, return_run_y);
        assert_eq!(return_run_y, 150.0, "midway between the two nodes");
        for point in &route[1..5] {
            assert!(
                !from_node.contains(*point) && !to_node.contains(*point),
                "{point:?} is inside a node"
            );
        }
    }

    /// Target left of the source and level with it: no gap to return
    /// through, so the wire goes around both nodes.
    #[test]
    fn a_backward_wire_detours_around_both_nodes() {
        let from_node = Rect::from_min_max(pos2(100.0, 0.0), pos2(200.0, 40.0));
        let to_node = Rect::from_min_max(pos2(-200.0, 0.0), pos2(-100.0, 40.0));
        let ends = WireEndpoints {
            from: pos2(200.0, 20.0),
            to: pos2(-200.0, 20.0),
            from_node,
            to_node,
        };
        let route = route_wire(&ends, &ROUTING, 0.0);

        assert_eq!(route.len(), 6);
        assert_eq!(route[0], ends.from);
        assert_eq!(route[5], ends.to);
        assert_eq!(route[1].x, 220.0, "leaves the source by one stub");
        assert_eq!(
            route[4].x, -220.0,
            "arrives at the target from one stub out"
        );
        // The return run clears both nodes.
        let return_run_y = route[2].y;
        assert_eq!(route[3].y, return_run_y);
        assert!(
            return_run_y <= 0.0 - ROUTING.backward_clearance
                || return_run_y >= 40.0 + ROUTING.backward_clearance
        );
        for point in &route[1..5] {
            assert!(
                !from_node.contains(*point) && !to_node.contains(*point),
                "{point:?} is inside a node"
            );
        }
    }

    #[test]
    fn a_backward_wire_detours_on_the_nearer_side() {
        let from_node = Rect::from_min_max(pos2(100.0, 0.0), pos2(200.0, 40.0));
        let to_node = Rect::from_min_max(pos2(-200.0, 0.0), pos2(-100.0, 40.0));

        let low_pins = WireEndpoints {
            from: pos2(200.0, 35.0),
            to: pos2(-200.0, 35.0),
            from_node,
            to_node,
        };
        assert!(
            route_wire(&low_pins, &ROUTING, 0.0)[2].y > 40.0,
            "pins near the bottom go under"
        );

        let high_pins = WireEndpoints {
            from: pos2(200.0, 5.0),
            to: pos2(-200.0, 5.0),
            from_node,
            to_node,
        };
        assert!(
            route_wire(&high_pins, &ROUTING, 0.0)[2].y < 0.0,
            "pins near the top go over"
        );
    }

    #[test]
    fn a_zero_radius_leaves_the_route_untouched() {
        let route = vec![pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0)];
        assert_eq!(round_corners(&route, 0.0, 0.1), route);
    }

    #[test]
    fn rounding_preserves_the_endpoints() {
        let route = vec![
            pos2(0.0, 0.0),
            pos2(10.0, 0.0),
            pos2(10.0, 10.0),
            pos2(30.0, 10.0),
        ];
        let rounded = round_corners(&route, 4.0, 0.1);
        assert_eq!(rounded.first(), route.first());
        assert_eq!(rounded.last(), route.last());
    }

    /// An L with radius 5 turns into a quarter circle centred at (5, 5).
    #[test]
    fn a_rounded_corner_is_an_arc_of_the_requested_radius() {
        let route = vec![pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0)];
        let rounded = round_corners(&route, 5.0, 0.01);
        let center = pos2(5.0, 5.0);
        let arc = &rounded[1..rounded.len() - 1];
        assert!(
            arc.len() >= 3,
            "an arc has interior samples, got {}",
            arc.len()
        );
        for point in arc {
            assert_close(point.distance(center), 5.0, 1e-3);
        }
    }

    /// The chords never stray from the true arc by more than the tolerance -
    /// at a coarse tolerance (zoomed far out) and a fine one (zoomed far in).
    #[test]
    fn arc_chords_stay_within_tolerance() {
        let route = vec![pos2(0.0, 0.0), pos2(100.0, 0.0), pos2(100.0, 100.0)];
        let radius = 40.0;
        let center = pos2(60.0, 40.0);
        for tolerance in [5.0, 0.05] {
            let rounded = round_corners(&route, radius, tolerance);
            let arc = &rounded[1..rounded.len() - 1];
            for chord in arc.windows(2) {
                let midpoint = chord[0] + (chord[1] - chord[0]) / 2.0;
                let sagitta = radius - midpoint.distance(center);
                assert!(
                    sagitta <= tolerance + 1e-4,
                    "sagitta {sagitta} exceeds tolerance {tolerance}"
                );
            }
        }
        let coarse = round_corners(&route, radius, 5.0).len();
        let fine = round_corners(&route, radius, 0.05).len();
        assert!(
            fine > coarse,
            "a finer tolerance takes more samples ({fine} vs {coarse})"
        );
    }

    #[test]
    fn a_corner_between_short_segments_shrinks_its_radius_to_fit() {
        let route = vec![
            pos2(0.0, 0.0),
            pos2(4.0, 0.0),
            pos2(4.0, 4.0),
            pos2(40.0, 4.0),
        ];
        let rounded = round_corners(&route, 8.0, 0.1);
        assert!(all_finite(&rounded));
        assert_eq!(rounded.first(), route.first());
        assert_eq!(rounded.last(), route.last());
        let bounds = Rect::from_min_max(pos2(0.0, 0.0), pos2(40.0, 4.0));
        for point in &rounded {
            assert!(
                bounds.expand(1e-3).contains(*point),
                "{point:?} left the route's bounds"
            );
        }
    }

    #[test]
    fn distance_is_zero_on_the_wire_and_grows_away_from_it() {
        let polyline = vec![pos2(0.0, 0.0), pos2(100.0, 0.0), pos2(100.0, 100.0)];
        assert_close(distance_to_polyline(&polyline, pos2(50.0, 0.0)), 0.0, 1e-5);
        assert_close(
            distance_to_polyline(&polyline, pos2(100.0, 60.0)),
            0.0,
            1e-5,
        );
        assert_close(
            distance_to_polyline(&polyline, pos2(50.0, 20.0)),
            20.0,
            1e-5,
        );
        assert_close(
            distance_to_polyline(&polyline, pos2(120.0, 60.0)),
            20.0,
            1e-5,
        );
    }

    /// Why hit-testing runs on the rounded polyline and not a corner-only
    /// shortcut: at a large radius the two disagree by a visible margin.
    #[test]
    fn hit_testing_agrees_with_the_drawn_shape_at_a_large_radius() {
        let route = vec![pos2(0.0, 0.0), pos2(100.0, 0.0), pos2(100.0, 100.0)];
        let radius = 40.0;
        let rounded = round_corners(&route, radius, 0.1);
        // The arc's midpoint, well inside the sharp corner.
        let center = pos2(60.0, 40.0);
        let arc_midpoint = center + Vec2::angled(-std::f32::consts::FRAC_PI_4) * radius;
        assert!(distance_to_polyline(&rounded, arc_midpoint) < 0.2);
        // The arc's midpoint sits radius * (1 - 1/sqrt(2)), about 0.29 radius,
        // inside the sharp corner.
        assert!(distance_to_polyline(&route, arc_midpoint) > radius * 0.25);
    }

    #[test]
    fn a_polyline_meets_a_rect_by_entering_it_or_crossing_it() {
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0));
        // A vertex inside.
        assert!(polyline_intersects_rect(
            &[pos2(-50.0, 50.0), pos2(50.0, 50.0)],
            rect
        ));
        // Straight through, both ends outside.
        assert!(polyline_intersects_rect(
            &[pos2(-50.0, 50.0), pos2(150.0, 50.0)],
            rect
        ));
        // Passing beside.
        assert!(!polyline_intersects_rect(
            &[pos2(-50.0, 150.0), pos2(150.0, 150.0)],
            rect
        ));
        // Diagonal clipping one corner, both ends outside.
        assert!(segment_intersects_rect(
            pos2(-10.0, 50.0),
            pos2(50.0, -10.0),
            rect
        ));
        assert!(!polyline_intersects_rect(&[], rect));
    }

    #[test]
    fn a_stroke_across_the_wire_crosses_it_and_one_beside_it_does_not() {
        let polyline = vec![pos2(0.0, 0.0), pos2(100.0, 0.0), pos2(100.0, 100.0)];
        assert!(polyline_crosses_segment(
            &polyline,
            pos2(90.0, 50.0),
            pos2(110.0, 50.0)
        ));
        assert!(polyline_crosses_segment(
            &polyline,
            pos2(50.0, -10.0),
            pos2(50.0, 10.0)
        ));
        assert!(!polyline_crosses_segment(
            &polyline,
            pos2(0.0, 50.0),
            pos2(50.0, 50.0)
        ));
        // Parallel to the first run and stopping short of the vertical one.
        assert!(!polyline_crosses_segment(
            &polyline,
            pos2(0.0, 10.0),
            pos2(90.0, 10.0)
        ));
    }
}
