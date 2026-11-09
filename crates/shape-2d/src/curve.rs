//! Curve flattening: how many segments a curve gets, and arcs.
//!
//! The rule (sand3d's, `docs/research.md` §1.4): enough segments that the
//! chord deviates from the arc by at most [`CHORD_TOLERANCE`] millimetres,
//! clamped to [`MIN_PER_TURN`]..=[`MAX_PER_TURN`] per full turn, scaled by
//! the fraction of a turn swept. It depends only on the radius and angle.

use shape_core::{DVec2, trig};

/// Maximum chord deviation, in millimetres.
pub const CHORD_TOLERANCE: f64 = 0.05;
/// Fewest segments in a full turn.
pub const MIN_PER_TURN: u32 = 24;
/// Most segments in a full turn.
pub const MAX_PER_TURN: u32 = 64;

/// Segments for an arc of `radius` sweeping `angle` radians (at least 1).
pub fn segments(radius: f64, angle: f64) -> u32 {
    let per_turn = if radius <= CHORD_TOLERANCE {
        MIN_PER_TURN
    } else {
        // The angle one segment may span: 2·acos(1 − tol/r).
        let step = 2.0 * trig::acos(1.0 - CHORD_TOLERANCE / radius);
        let n = (trig::TAU / step).ceil();
        (n as u32).clamp(MIN_PER_TURN, MAX_PER_TURN)
    };
    let fraction = (angle.abs() / trig::TAU).min(1.0);
    ((per_turn as f64 * fraction).ceil() as u32).max(1)
}

/// Points on an arc around `center` from `start` sweeping `sweep` radians
/// (positive counter-clockwise), with both ends, `n + 1` points.
pub fn arc(center: DVec2, radius: f64, start: f64, sweep: f64, n: u32) -> Vec<DVec2> {
    (0..=n)
        .map(|i| {
            let a = start + sweep * (i as f64 / n as f64);
            let (s, c) = trig::sin_cos(a);
            center + DVec2::new(radius * c, radius * s)
        })
        .collect()
}

/// A cubic Bézier from `p0` to `p3`, flattened to `n` segments; the first
/// point is left out (it ends the previous piece).
pub fn cubic(p0: DVec2, p1: DVec2, p2: DVec2, p3: DVec2, n: u32) -> Vec<DVec2> {
    (1..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t)
        })
        .collect()
}

/// A quadratic Bézier, flattened like [`cubic`].
pub fn quadratic(p0: DVec2, p1: DVec2, p2: DVec2, n: u32) -> Vec<DVec2> {
    (1..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            p0 * (u * u) + p1 * (2.0 * u * t) + p2 * (t * t)
        })
        .collect()
}

/// Segments for a Bézier with the given control polygon: the arc rule for a
/// circle whose quarter has the polygon's length, at least 4.
pub fn bezier_segments(control: &[DVec2]) -> u32 {
    let length: f64 = control.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    segments(length * 2.0 / trig::PI, trig::PI / 2.0).max(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_counts_follow_the_rule() {
        // Small circles get the minimum, big ones the maximum.
        assert_eq!(segments(0.5, trig::TAU), MIN_PER_TURN);
        assert_eq!(segments(1000.0, trig::TAU), MAX_PER_TURN);
        // Between: 2π / (2·acos(1 − 0.05/10)) = 31.4… → 32.
        assert_eq!(segments(10.0, trig::TAU), 32);
        assert_eq!(segments(10.0, trig::PI / 2.0), 8);
    }
}
