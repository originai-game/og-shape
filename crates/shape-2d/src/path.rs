//! A path builder for outlines made of lines and curves.

use shape_core::{DVec2, trig};

use crate::curve::{arc, bezier_segments, cubic, quadratic, segments};
use crate::{Contour, Profile, ProfileError};

/// An outline built piece by piece, closed by [`Path::close`].
///
/// Where two pieces meet the point is a corner, except where the next
/// piece starts tangent to the last ([`Path::tangent_arc_to`]) or inside
/// one smooth curve ([`Path::spline_through`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    points: Vec<DVec2>,
    sharp: Vec<bool>,
    /// The direction the path arrives at its last point.
    heading: Option<DVec2>,
}

impl Path {
    /// A path starting at `(x, y)`.
    pub fn new(x: f64, y: f64) -> Path {
        Path {
            points: vec![DVec2::new(x, y)],
            sharp: vec![true],
            heading: None,
        }
    }

    fn last(&self) -> DVec2 {
        *self.points.last().unwrap_or(&DVec2::ZERO)
    }

    fn extend(&mut self, pts: impl IntoIterator<Item = DVec2>, start_smooth: bool) {
        if start_smooth && let Some(s) = self.sharp.last_mut() {
            *s = false;
        }
        let before = self.points.len();
        self.points.extend(pts);
        self.sharp.resize(before, true);
        self.sharp.resize(self.points.len(), false);
        if let Some(s) = self.sharp.last_mut() {
            *s = true;
        }
        let n = self.points.len();
        if n >= 2 {
            self.heading = (self.points[n - 1] - self.points[n - 2]).try_normalize();
        }
    }

    /// A straight line to `(x, y)`.
    pub fn line_to(mut self, x: f64, y: f64) -> Path {
        self.extend([DVec2::new(x, y)], false);
        self
    }

    /// A circular arc through `(tx, ty)` ending at `(x, y)`. Collinear
    /// points make a straight line.
    pub fn arc_to(mut self, tx: f64, ty: f64, x: f64, y: f64) -> Path {
        let (a, b, c) = (self.last(), DVec2::new(tx, ty), DVec2::new(x, y));
        match circumcircle(a, b, c) {
            None => self.extend([c], false),
            Some(center) => {
                let r = (a - center).length();
                let ang = |p: DVec2| trig::atan2(p.y - center.y, p.x - center.x);
                let (a0, a1) = (ang(a), ang(c));
                let ccw = (b - a).perp_dot(c - b) > 0.0;
                let norm = |mut s: f64| {
                    while ccw && s < 0.0 {
                        s += trig::TAU;
                    }
                    while !ccw && s > 0.0 {
                        s -= trig::TAU;
                    }
                    s
                };
                let sweep = norm(a1 - a0);
                let n = segments(r, sweep);
                let mut pts = arc(center, r, a0, sweep, n);
                pts.remove(0);
                if let Some(l) = pts.last_mut() {
                    *l = c;
                }
                self.extend(pts, false);
            }
        }
        self
    }

    /// A circular arc to `(x, y)` that leaves tangent to the path so far.
    /// Without a previous direction it is a straight line.
    pub fn tangent_arc_to(self, x: f64, y: f64) -> Path {
        let (a, c) = (self.last(), DVec2::new(x, y));
        let Some(t) = self.heading else {
            return self.line_to(x, y);
        };
        // The centre is on the normal at `a`, equidistant from `a` and `c`.
        let n = t.perp();
        let d = c - a;
        let denom = 2.0 * n.dot(d);
        if denom == 0.0 {
            return self.line_to(x, y);
        }
        let k = d.dot(d) / denom;
        let center = a + n * k;
        let r = k.abs();
        let a0 = trig::atan2(a.y - center.y, a.x - center.x);
        let a1 = trig::atan2(c.y - center.y, c.x - center.x);
        let ccw = k > 0.0;
        let mut sweep = a1 - a0;
        if ccw && sweep < 0.0 {
            sweep += trig::TAU;
        }
        if !ccw && sweep > 0.0 {
            sweep -= trig::TAU;
        }
        let mut pts = arc(center, r, a0, sweep, segments(r, sweep));
        pts.remove(0);
        if let Some(l) = pts.last_mut() {
            *l = c;
        }
        let mut s = self;
        s.extend(pts, true);
        s
    }

    /// A quadratic Bézier with control `(cx, cy)` to `(x, y)`.
    pub fn quadratic_to(mut self, cx: f64, cy: f64, x: f64, y: f64) -> Path {
        let (a, b, c) = (self.last(), DVec2::new(cx, cy), DVec2::new(x, y));
        let n = bezier_segments(&[a, b, c]);
        self.extend(quadratic(a, b, c, n), false);
        self
    }

    /// A cubic Bézier with controls `c1`, `c2` to `end`.
    pub fn cubic_to(mut self, c1: DVec2, c2: DVec2, end: DVec2) -> Path {
        let a = self.last();
        let n = bezier_segments(&[a, c1, c2, end]);
        self.extend(cubic(a, c1, c2, end, n), false);
        self
    }

    /// A smooth curve (Catmull-Rom, as cubic Béziers) through `points`.
    pub fn spline_through(mut self, points: &[DVec2]) -> Path {
        let mut all = vec![self.last()];
        all.extend_from_slice(points);
        let m = all.len();
        let mut pts = Vec::new();
        for i in 0..m - 1 {
            let p0 = all[i.saturating_sub(1)];
            let (p1, p2) = (all[i], all[i + 1]);
            let p3 = all[(i + 2).min(m - 1)];
            let c1 = p1 + (p2 - p0) / 6.0;
            let c2 = p2 - (p3 - p1) / 6.0;
            pts.extend(cubic(p1, c1, c2, p2, bezier_segments(&[p1, c1, c2, p2])));
        }
        self.extend(pts, false);
        self
    }

    /// The points so far, every curve flattened: an open polyline, as
    /// [`crate::stroke`] takes.
    pub fn points(&self) -> &[DVec2] {
        &self.points
    }

    /// The direction the path arrives at its last point; `None` before
    /// its first piece.
    pub fn heading(&self) -> Option<DVec2> {
        self.heading
    }

    /// Closes the outline into a profile. A last point equal to the first
    /// is dropped.
    pub fn close(mut self) -> Result<Profile, ProfileError> {
        if self.points.len() > 1 && self.points.first() == self.points.last() {
            self.points.pop();
            self.sharp.pop();
        }
        if self.points.len() < 3 {
            return Err(ProfileError::TooFewPoints(self.points.len()));
        }
        if self.points.iter().any(|p| !p.is_finite()) {
            return Err(ProfileError::NotFinite("points"));
        }
        let c = Contour {
            points: self.points,
            sharp: self.sharp,
        };
        if c.signed_area2() == 0.0 {
            return Err(ProfileError::Empty);
        }
        Ok(Profile::from_contour(c))
    }
}

/// The centre of the circle through three points, `None` if collinear.
fn circumcircle(a: DVec2, b: DVec2, c: DVec2) -> Option<DVec2> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d == 0.0 {
        return None;
    }
    let (a2, b2, c2) = (a.length_squared(), b.length_squared(), c.length_squared());
    Some(DVec2::new(
        (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d,
        (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_d_shape_has_two_corners_and_its_area() {
        // A half disc of radius 5: a line and a three-point arc.
        let d = Path::new(0.0, -5.0)
            .arc_to(5.0, 0.0, 0.0, 5.0)
            .line_to(0.0, -5.0)
            .close()
            .unwrap();
        let c = &d.regions[0].outer;
        assert_eq!(c.sharp.iter().filter(|s| **s).count(), 2);
        let want = trig::PI * 25.0 / 2.0;
        assert!((d.area() - want).abs() / want < 1.5e-2, "{}", d.area());
    }

    #[test]
    fn a_tangent_arc_leaves_no_corner() {
        let p = Path::new(0.0, 0.0)
            .line_to(10.0, 0.0)
            .tangent_arc_to(10.0, 10.0)
            .line_to(0.0, 10.0)
            .close()
            .unwrap();
        let c = &p.regions[0].outer;
        let i = c
            .points
            .iter()
            .position(|q| *q == DVec2::new(10.0, 0.0))
            .unwrap();
        assert!(!c.sharp[i]);
    }
}
