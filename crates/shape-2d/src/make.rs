//! Profile constructors. Each takes a zero-value desc struct; the shapes
//! are centred on the origin unless their points say otherwise.

use shape_core::{DVec2, trig};

use crate::curve::{arc, segments};
use crate::{Contour, Profile, ProfileError};

fn positive(v: f64, name: &'static str) -> Result<f64, ProfileError> {
    if !v.is_finite() {
        Err(ProfileError::NotFinite(name))
    } else if v <= 0.0 {
        Err(ProfileError::NotPositive(name))
    } else {
        Ok(v)
    }
}

/// A rectangle, optionally with rounded corners.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    /// Size along x.
    pub width: f64,
    /// Size along y.
    pub height: f64,
    /// Corner radius; 0 for sharp corners. At most half the smaller side.
    pub radius: f64,
}

/// A rectangle centred on the origin.
pub fn rect(d: &Rect) -> Result<Profile, ProfileError> {
    let (w, h) = (
        positive(d.width, "width")? / 2.0,
        positive(d.height, "height")? / 2.0,
    );
    if d.radius == 0.0 {
        let p = vec![
            DVec2::new(-w, -h),
            DVec2::new(w, -h),
            DVec2::new(w, h),
            DVec2::new(-w, h),
        ];
        return Ok(Profile::from_contour(Contour::corners(p)));
    }
    let r = positive(d.radius, "radius")?;
    if r > w.min(h) {
        return Err(ProfileError::RadiusTooLarge(0));
    }
    let n = segments(r, trig::PI / 2.0);
    let mut points = Vec::new();
    let corners = [
        (w - r, -h + r, -trig::PI / 2.0),
        (w - r, h - r, 0.0),
        (-w + r, h - r, trig::PI / 2.0),
        (-w + r, -h + r, trig::PI),
    ];
    for (cx, cy, start) in corners {
        let mut a = arc(DVec2::new(cx, cy), r, start, trig::PI / 2.0, n);
        // Where two quarter arcs meet with no straight between, keep one.
        if let (Some(last), Some(first)) = (points.last(), a.first())
            && *last == *first
        {
            a.remove(0);
        }
        points.extend(a);
    }
    if points.first() == points.last() {
        points.pop();
    }
    Ok(Profile::from_contour(Contour::smooth(points)))
}

/// A circle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Circle {
    /// The radius.
    pub radius: f64,
    /// Segments; 0 for the chord rule of [`crate::curve::segments`].
    pub segments: u32,
}

/// A circle centred on the origin, its first point on +x.
pub fn circle(d: &Circle) -> Result<Profile, ProfileError> {
    let r = positive(d.radius, "radius")?;
    let n = if d.segments >= 3 {
        d.segments
    } else {
        segments(r, trig::TAU)
    };
    let mut p = arc(DVec2::ZERO, r, 0.0, trig::TAU, n);
    p.pop();
    Ok(Profile::from_contour(Contour::smooth(p)))
}

/// An ellipse.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ellipse {
    /// Semi-axis along x.
    pub rx: f64,
    /// Semi-axis along y.
    pub ry: f64,
    /// Segments; 0 for the chord rule at the larger radius.
    pub segments: u32,
}

/// An ellipse centred on the origin.
pub fn ellipse(d: &Ellipse) -> Result<Profile, ProfileError> {
    let (rx, ry) = (positive(d.rx, "rx")?, positive(d.ry, "ry")?);
    let n = if d.segments >= 3 {
        d.segments
    } else {
        segments(rx.max(ry), trig::TAU)
    };
    let p = (0..n)
        .map(|i| {
            let (s, c) = trig::sin_cos(trig::TAU * (i as f64 / n as f64));
            DVec2::new(rx * c, ry * s)
        })
        .collect();
    Ok(Profile::from_contour(Contour::smooth(p)))
}

/// A slot: a rectangle with semicircular ends, along x.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Slot {
    /// Overall length, end to end; at least `width`.
    pub length: f64,
    /// Width (the ends' diameter).
    pub width: f64,
}

/// A slot centred on the origin.
pub fn slot(d: &Slot) -> Result<Profile, ProfileError> {
    let w = positive(d.width, "width")?;
    let l = positive(d.length, "length")?;
    if l < w {
        return Err(ProfileError::RadiusTooLarge(0));
    }
    let r = w / 2.0;
    let half = (l - w) / 2.0;
    if half == 0.0 {
        return circle(&Circle {
            radius: r,
            segments: 0,
        });
    }
    let n = segments(r, trig::PI);
    let mut p = arc(DVec2::new(half, 0.0), r, -trig::PI / 2.0, trig::PI, n);
    p.extend(arc(DVec2::new(-half, 0.0), r, trig::PI / 2.0, trig::PI, n));
    Ok(Profile::from_contour(Contour::smooth(p)))
}

/// A regular polygon.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Polygon {
    /// Number of sides, at least 3.
    pub sides: u32,
    /// Distance from the centre to each corner.
    pub radius: f64,
    /// Distance across flats (twice the centre-to-side distance), a nut's
    /// or a hex bar's size; 0 gives the size by `radius` instead. Give
    /// one of the two.
    pub flats: f64,
}

/// A regular polygon centred on the origin, a corner on +x.
pub fn polygon(d: &Polygon) -> Result<Profile, ProfileError> {
    if d.sides < 3 {
        return Err(ProfileError::TooFewPoints(d.sides as usize));
    }
    let r = if d.flats == 0.0 {
        positive(d.radius, "radius")?
    } else {
        if d.radius != 0.0 {
            return Err(ProfileError::Conflict("radius and flats"));
        }
        // The apothem is flats / 2 = r cos(π / n).
        positive(d.flats, "flats")? / 2.0 / trig::cos(trig::PI / d.sides as f64)
    };
    let mut p = arc(DVec2::ZERO, r, 0.0, trig::TAU, d.sides);
    p.pop();
    Ok(Profile::from_contour(Contour::corners(p)))
}

/// A point of a [`Poly`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PolyPoint {
    /// x.
    pub x: f64,
    /// y.
    pub y: f64,
    /// Corner radius; 0 for a sharp corner.
    pub radius: f64,
    /// The edge from this point to the next is a circular arc whose
    /// middle stands `bulge` away from the straight edge (the sagitta):
    /// positive bows out of the outline, negative into it; 0 is straight.
    /// A bulge of half the edge's length is a half circle.
    pub bulge: f64,
}

/// A polygon through given points, each corner sharp or rounded.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Poly {
    /// The corners, in order (either winding).
    pub points: Vec<PolyPoint>,
}

/// A polygon with per-corner fillets. A fillet is tangent to both edges;
/// two fillets on one edge must fit along it.
pub fn poly(d: &Poly) -> Result<Profile, ProfileError> {
    let n = d.points.len();
    if n < 3 {
        return Err(ProfileError::TooFewPoints(n));
    }
    for p in &d.points {
        if !(p.x.is_finite() && p.y.is_finite() && p.radius.is_finite() && p.bulge.is_finite()) {
            return Err(ProfileError::NotFinite("points"));
        }
    }
    // Wind counter-clockwise first, so every convex corner turns left.
    // `bulge[i]` belongs to the edge from point i to point i + 1; on the
    // reversed list that edge runs from n − 2 − i to n − 1 − i.
    let mut pts = d.points.clone();
    let mut bulge: Vec<f64> = pts.iter().map(|p| p.bulge).collect();
    let area2: f64 = (0..n)
        .map(|i| pts[i].x * pts[(i + 1) % n].y - pts[i].y * pts[(i + 1) % n].x)
        .sum();
    if area2 < 0.0 {
        pts.reverse();
        bulge = (0..n)
            .map(|k| d.points[(2 * n - 2 - k) % n].bulge)
            .collect();
    }
    let at = |i: usize| DVec2::new(pts[i % n].x, pts[i % n].y);
    // The distance from each corner to its tangent points.
    let mut trim = vec![0.0; n];
    for i in 0..n {
        let r = pts[i].radius;
        if r <= 0.0 {
            continue;
        }
        if bulge[i] != 0.0 || bulge[(i + n - 1) % n] != 0.0 {
            return Err(ProfileError::ArcCorner(i));
        }
        let (prev, p, next) = (at(i + n - 1), at(i), at(i + 1));
        let (u, v) = (
            (prev - p).normalize_or_zero(),
            (next - p).normalize_or_zero(),
        );
        let cos = u.dot(v).clamp(-1.0, 1.0);
        let half = trig::acos(cos) / 2.0;
        if half <= 0.0 || half >= trig::PI / 2.0 {
            return Err(ProfileError::RadiusTooLarge(i));
        }
        trim[i] = r / trig::tan(half);
    }
    for i in 0..n {
        let edge = (at(i + 1) - at(i)).length();
        if trim[i] + trim[(i + 1) % n] > edge {
            return Err(ProfileError::RadiusTooLarge(i));
        }
    }
    let mut c = Contour::default();
    for i in 0..n {
        let (prev, p, next) = (at(i + n - 1), at(i), at(i + 1));
        if trim[i] == 0.0 {
            c.points.push(p);
            c.sharp.push(true);
            if bulge[i] != 0.0 {
                arc_edge(&mut c, p, next, bulge[i])?;
            }
            continue;
        }
        let (u, v) = ((prev - p).normalize(), (next - p).normalize());
        let (t0, t1) = (p + u * trim[i], p + v * trim[i]);
        let r = pts[i].radius;
        // The centre lies along the bisector, r / sin(half) from the corner.
        let bis = (u + v).normalize();
        let half = trig::acos(u.dot(v).clamp(-1.0, 1.0)) / 2.0;
        let center = p + bis * (r / trig::sin(half));
        let a0 = trig::atan2(t0.y - center.y, t0.x - center.x);
        let a1 = trig::atan2(t1.y - center.y, t1.x - center.x);
        // A left turn (convex corner) sweeps counter-clockwise.
        let left = (p - prev).perp_dot(next - p) > 0.0;
        let mut sweep = a1 - a0;
        if left && sweep < 0.0 {
            sweep += trig::TAU;
        }
        if !left && sweep > 0.0 {
            sweep -= trig::TAU;
        }
        let seg = segments(r, sweep);
        for q in arc(center, r, a0, sweep, seg) {
            c.points.push(q);
            c.sharp.push(false);
        }
    }
    Ok(Profile::from_contour(c))
}

/// The points strictly inside an arc edge from `a` to `b` on a
/// counter-clockwise outline, its middle `s` to the right of the chord
/// (out of the outline) or, negative, to the left.
fn arc_edge(c: &mut Contour, a: DVec2, b: DVec2, s: f64) -> Result<(), ProfileError> {
    let chord = b - a;
    let len = chord.length();
    if len <= 0.0 {
        return Err(ProfileError::NotPositive("the chord of an arc edge"));
    }
    // Radius from the chord and the sagitta; the centre lies r − s from
    // the chord's middle, into the outline (left of the travel).
    let half = len / 2.0;
    let r = (half * half + s * s) / (2.0 * s.abs());
    let left = DVec2::new(-chord.y, chord.x) / len;
    let center = (a + b) / 2.0 + left * (r - s.abs()) * s.signum();
    // Seen from the centre the arc turns counter-clockwise when it bows
    // out of a counter-clockwise outline: 4 atan(2s / chord), signed.
    let sweep = 4.0 * trig::atan2(s, half);
    let a0 = trig::atan2(a.y - center.y, a.x - center.x);
    let mut pts = arc(center, r, a0, sweep, segments(r, sweep));
    pts.pop();
    for q in pts.into_iter().skip(1) {
        c.points.push(q);
        c.sharp.push(false);
    }
    Ok(())
}

/// A superellipse: |x/rx|ⁿ + |y/ry|ⁿ = 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Superellipse {
    /// Semi-axis along x.
    pub rx: f64,
    /// Semi-axis along y.
    pub ry: f64,
    /// The exponent n: 2 is an ellipse, larger is squarer.
    pub exponent: f64,
    /// Segments; 0 for the chord rule at the larger radius.
    pub segments: u32,
}

/// A superellipse centred on the origin.
pub fn superellipse(d: &Superellipse) -> Result<Profile, ProfileError> {
    let (rx, ry) = (positive(d.rx, "rx")?, positive(d.ry, "ry")?);
    let e = positive(d.exponent, "exponent")?;
    let n = if d.segments >= 4 {
        d.segments
    } else {
        segments(rx.max(ry), trig::TAU)
    };
    let p = (0..n)
        .map(|i| {
            let (s, c) = trig::sin_cos(trig::TAU * (i as f64 / n as f64));
            let f = |v: f64| v.signum() * trig::pow(v.abs(), 2.0 / e);
            DVec2::new(rx * f(c), ry * f(s))
        })
        .collect();
    Ok(Profile::from_contour(Contour::smooth(p)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn areas_converge_on_the_analytic_ones() {
        let r = rect(&Rect {
            width: 20.0,
            height: 10.0,
            radius: 3.0,
        })
        .unwrap();
        let want = 200.0 - (4.0 - trig::PI) * 9.0;
        // 24 segments a turn lose about 1.1 % of a circle's area.
        assert!((r.area() - want).abs() / want < 2e-3, "{}", r.area());
        let s = slot(&Slot {
            length: 30.0,
            width: 10.0,
        })
        .unwrap();
        let want = 20.0 * 10.0 + trig::PI * 25.0;
        assert!((s.area() - want).abs() / want < 1e-2);
    }

    #[test]
    fn poly_rounds_only_the_corners_asked() {
        let p = |x, y, radius| PolyPoint {
            x,
            y,
            radius,
            ..Default::default()
        };
        let l = poly(&Poly {
            points: vec![
                p(0.0, 0.0, 0.0),
                p(10.0, 0.0, 2.0),
                p(10.0, 10.0, 0.0),
                p(0.0, 10.0, 0.0),
            ],
        })
        .unwrap();
        let c = &l.regions[0].outer;
        assert_eq!(c.sharp.iter().filter(|s| **s).count(), 3);
        let want = 100.0 - (4.0 - trig::PI);
        assert!((l.area() - want).abs() < 5e-2, "{}", l.area());
        let too_big = poly(&Poly {
            points: vec![
                p(0.0, 0.0, 0.0),
                p(10.0, 0.0, 6.0),
                p(10.0, 10.0, 6.0),
                p(0.0, 10.0, 0.0),
            ],
        });
        assert!(matches!(too_big, Err(ProfileError::RadiusTooLarge(_))));
    }

    #[test]
    fn a_bulge_makes_an_arc_edge_whichever_way_the_points_run() {
        let p = |x, y, bulge| PolyPoint {
            x,
            y,
            bulge,
            ..Default::default()
        };
        // A 10 × 10 square whose bottom edge bows out 5: a half circle.
        let ccw = vec![
            p(0.0, 0.0, 5.0),
            p(10.0, 0.0, 0.0),
            p(10.0, 10.0, 0.0),
            p(0.0, 10.0, 0.0),
        ];
        let a = poly(&Poly {
            points: ccw.clone(),
        })
        .unwrap();
        let want = 100.0 + trig::PI * 25.0 / 2.0;
        // 12 chords on the half circle lose about 1.1 % of its area.
        assert!((a.area() - want).abs() / want < 5e-3, "{}", a.area());
        // The same outline listed clockwise: the bulge stays on its edge.
        let mut cw: Vec<PolyPoint> = ccw.iter().rev().copied().collect();
        for q in &mut cw {
            q.bulge = 0.0;
        }
        cw[2].bulge = 5.0; // the edge (10, 0) → (0, 0)
        let b = poly(&Poly { points: cw }).unwrap();
        assert!((a.area() - b.area()).abs() < 1e-9);
        // Bowing in takes the half circle out; the ends are corners.
        let inward = poly(&Poly {
            points: vec![
                p(0.0, 0.0, -5.0),
                p(10.0, 0.0, 0.0),
                p(10.0, 10.0, 0.0),
                p(0.0, 10.0, 0.0),
            ],
        })
        .unwrap();
        let want = 100.0 - trig::PI * 25.0 / 2.0;
        assert!((inward.area() - want).abs() / want < 1e-2);
        let c = &inward.regions[0].outer;
        assert_eq!(c.sharp.iter().filter(|s| **s).count(), 4);
        // A rounded corner next to an arc is refused.
        let mut round = ccw;
        round[1].radius = 1.0;
        assert_eq!(
            poly(&Poly { points: round }),
            Err(ProfileError::ArcCorner(1))
        );
    }

    #[test]
    fn polygon_flats_set_the_size_across_the_flats() {
        let hex = polygon(&Polygon {
            sides: 6,
            flats: 10.0,
            ..Default::default()
        })
        .unwrap();
        let ys = hex.regions[0].outer.points.iter().map(|p| p.y);
        let (lo, hi) = ys.fold((0.0f64, 0.0f64), |(a, b), y| (a.min(y), b.max(y)));
        assert!((hi - lo - 10.0).abs() < 1e-12, "{}", hi - lo);
        assert_eq!(
            polygon(&Polygon {
                sides: 6,
                radius: 5.0,
                flats: 10.0
            }),
            Err(ProfileError::Conflict("radius and flats"))
        );
    }

    #[test]
    fn bad_sizes_are_error_values() {
        assert_eq!(
            circle(&Circle::default()),
            Err(ProfileError::NotPositive("radius"))
        );
        assert_eq!(
            rect(&Rect {
                width: f64::NAN,
                height: 1.0,
                radius: 0.0
            }),
            Err(ProfileError::NotFinite("width"))
        );
    }
}
