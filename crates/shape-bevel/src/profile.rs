//! The edge profile: a superellipse quarter in a unit square, mapped into
//! the parallelogram of each edge end (`docs/research.md` §7.1).
//!
//! In square coordinates the profile runs from `(1, 0)` to `(0, 1)` on
//! `|x|^p + |y|^p = 1`. The author's `profile` value is a quarter of the
//! exponent: 0.5 is a circle (p = 2), 0.25 a straight line (p = 1).
//! Samples are spaced at equal chord length in square space and are
//! symmetric: sample `n - k` is sample `k` with x and y swapped.

use shape_core::{DVec3, trig};

/// Table resolution for the arc-length search.
const TABLE: usize = 2048;

/// Points on the quarter superellipse of exponent `p`, `n + 1` of them.
pub(crate) fn samples(p: f64, n: u32) -> Vec<(f64, f64)> {
    let n = n.max(1) as usize;
    let at = |theta: f64| -> (f64, f64) {
        let (s, c) = trig::sin_cos(theta);
        let e = 2.0 / p;
        (trig::pow(c.max(0.0), e), trig::pow(s.max(0.0), e))
    };
    // Cumulative chord length over a fine table in θ.
    let mut thetas = Vec::with_capacity(TABLE + 1);
    let mut lens = Vec::with_capacity(TABLE + 1);
    let mut prev = (1.0, 0.0);
    let mut acc = 0.0;
    for i in 0..=TABLE {
        let theta = trig::PI / 2.0 * (i as f64 / TABLE as f64);
        let q = if i == 0 {
            (1.0, 0.0)
        } else if i == TABLE {
            (0.0, 1.0)
        } else {
            at(theta)
        };
        let (dx, dy) = (q.0 - prev.0, q.1 - prev.1);
        acc += (dx * dx + dy * dy).sqrt();
        thetas.push(theta);
        lens.push(acc);
        prev = q;
    }
    let total = acc;
    let mut out = vec![(0.0, 0.0); n + 1];
    out[0] = (1.0, 0.0);
    out[n] = (0.0, 1.0);
    let mut j = 0;
    for (k, slot) in out.iter_mut().enumerate().take(n / 2 + 1).skip(1) {
        let target = total * (k as f64 / n as f64);
        while j + 1 < TABLE && lens[j + 1] < target {
            j += 1;
        }
        let span = lens[j + 1] - lens[j];
        let f = if span > 0.0 {
            (target - lens[j]) / span
        } else {
            0.0
        };
        let theta = thetas[j] + (thetas[j + 1] - thetas[j]) * f;
        *slot = at(theta);
    }
    // Exactly symmetric, so a chain reads the same from either end and the
    // middle sample of an even count has x = y.
    for k in 0..=n / 2 {
        out[n - k] = (out[k].1, out[k].0);
    }
    if n % 2 == 0 {
        let m = (out[n / 2].0 + out[n / 2].1) / 2.0;
        out[n / 2] = (m, m);
    }
    out
}

/// The profile from `a` to `b` around the corner `c`: the image of the
/// square samples under the map sending (1, 0) to `a`, (0, 1) to `b` and
/// (1, 1) to `c`.
pub(crate) fn chain(a: DVec3, b: DVec3, c: DVec3, s: &[(f64, f64)]) -> Vec<DVec3> {
    let o = a + b - c;
    let n = s.len() - 1;
    s.iter()
        .enumerate()
        .map(|(k, &(x, y))| {
            if k == 0 {
                a
            } else if k == n {
                b
            } else {
                o + (a - o) * x + (b - o) * y
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_half_profile_is_a_circle_with_even_chords() {
        let s = samples(2.0, 6);
        for &(x, y) in &s {
            assert!((x * x + y * y - 1.0).abs() < 1e-12, "{x} {y}");
        }
        let d: Vec<f64> = s
            .windows(2)
            .map(|w| {
                let (dx, dy) = (w[1].0 - w[0].0, w[1].1 - w[0].1);
                (dx * dx + dy * dy).sqrt()
            })
            .collect();
        for x in &d {
            assert!((x - d[0]).abs() < 1e-5, "{d:?}");
        }
    }

    #[test]
    fn a_quarter_profile_is_straight() {
        for &(x, y) in &samples(1.0, 5) {
            assert!((x + y - 1.0).abs() < 1e-12);
        }
    }
}
