//! 3D paths given as pieces (lines, arcs, splines) and flattened by the one
//! curve rule (`shape_2d::curve`), for [`crate::paths::sweep`] and
//! [`crate::wire`].
//!
//! Entry: [`flatten`]. A [`Path3`] starts at a point; each [`Piece`]
//! continues from where the last ended. Arcs get
//! [`shape_2d::curve::segments`] of their radius and sweep; splines are
//! cubic Béziers (Catmull-Rom through their points), each span sized as
//! `shape_2d::curve::bezier_segments` sizes a 2D one. The counts depend
//! only on the arguments.

use shape_2d::curve::segments;
use shape_core::{DVec3, trig};

use crate::solids::GenError;

/// One piece of a path, from the point the path has reached.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    /// A straight line to the point.
    Line(DVec3),
    /// A circular arc through `through`, ending at `to`.
    Arc {
        /// A point on the arc between its ends.
        through: DVec3,
        /// Where it ends.
        to: DVec3,
    },
    /// A circular arc to the point that leaves tangent to the path so far
    /// (in the plane of that direction and the chord). A chord along the
    /// direction is a straight line.
    TangentArc(DVec3),
    /// A smooth curve through the points (Catmull-Rom, as cubic Béziers).
    Spline(Vec<DVec3>),
}

/// A path: a start and its pieces.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path3 {
    /// Where the path starts.
    pub start: DVec3,
    /// The pieces, in order.
    pub pieces: Vec<Piece>,
}

/// A flattened path: its points, and per point whether it is a corner
/// (the start, the end, and where two pieces meet without a tangent
/// piece leaving it). Points inside a curve are not corners.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Flat {
    /// The points.
    pub points: Vec<DVec3>,
    /// Per point: a corner.
    pub corner: Vec<bool>,
}

fn finite3(p: DVec3, what: &str) -> Result<DVec3, GenError> {
    if p.is_finite() {
        Ok(p)
    } else {
        Err(GenError::Invalid(format!("path: {what} is not finite")))
    }
}

/// Points on the arc about `o` of radius `r` from `o + r·e1` toward `e2`,
/// sweeping `sweep` radians, without its first point; the last is `end`.
fn arc_points(o: DVec3, r: f64, e1: DVec3, e2: DVec3, sweep: f64, end: DVec3) -> Vec<DVec3> {
    let n = segments(r, sweep);
    let mut out: Vec<DVec3> = (1..=n)
        .map(|i| {
            let (s, c) = trig::sin_cos(sweep * (i as f64 / n as f64));
            o + e1 * (r * c) + e2 * (r * s)
        })
        .collect();
    if let Some(l) = out.last_mut() {
        *l = end;
    }
    out
}

/// The angle from `e1` toward `e2` of `v`, in (0, 2π].
fn turn(v: DVec3, e1: DVec3, e2: DVec3) -> f64 {
    let mut a = trig::atan2(v.dot(e2), v.dot(e1));
    if a <= 0.0 {
        a += trig::TAU;
    }
    a
}

/// A cubic Bézier, flattened like a 2D one; the first point left out.
fn cubic(p0: DVec3, p1: DVec3, p2: DVec3, p3: DVec3) -> Vec<DVec3> {
    let length = (p1 - p0).length() + (p2 - p1).length() + (p3 - p2).length();
    let n = segments(length * 2.0 / trig::PI, trig::PI / 2.0).max(4);
    (1..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t)
        })
        .collect()
}

/// The path's points. Refused: a non-finite point, an arc whose three
/// points are on one line or repeat, a tangent arc with no direction
/// before it or whose end lies straight behind it, and a piece that does
/// not move.
pub fn flatten(d: &Path3) -> Result<Flat, GenError> {
    let mut f = Flat {
        points: vec![finite3(d.start, "the start")?],
        corner: vec![true],
    };
    for (k, piece) in d.pieces.iter().enumerate() {
        let a = *f.points.last().unwrap_or(&d.start);
        let heading = match f.points.len() {
            n if n >= 2 => (f.points[n - 1] - f.points[n - 2]).try_normalize(),
            _ => None,
        };
        let what = |w: &str| GenError::Invalid(format!("path piece {}: {w}", k + 1));
        let mut smooth_start = false;
        let pts: Vec<DVec3> = match piece {
            Piece::Line(p) => vec![finite3(*p, "a point")?],
            Piece::Arc { through, to } => {
                let (b, c) = (finite3(*through, "a point")?, finite3(*to, "a point")?);
                let (u, v) = (b - a, c - a);
                let w = u.cross(v);
                let ww = w.length_squared();
                if ww == 0.0 || !ww.is_finite() {
                    return Err(what(
                        "the arc's start, `through` and `to` are on one line (or repeat)",
                    ));
                }
                let o = a
                    + (w.cross(u) * v.length_squared() + v.cross(w) * u.length_squared())
                        / (2.0 * ww);
                let r = (a - o).length();
                let e1 = (a - o) / r;
                let e2 = (w / ww.sqrt()).cross(e1);
                arc_points(o, r, e1, e2, turn(c - o, e1, e2), c)
            }
            Piece::TangentArc(p) => {
                let c = finite3(*p, "a point")?;
                let t = heading.ok_or_else(|| what("a tangent arc needs a direction before it"))?;
                let dv = c - a;
                let n = t.cross(dv);
                smooth_start = true;
                if n.length_squared() == 0.0 {
                    if dv.dot(t) > 0.0 {
                        vec![c]
                    } else {
                        return Err(what("a tangent arc's end lies straight behind it"));
                    }
                } else {
                    let m = n.normalize().cross(t);
                    let k = dv.length_squared() / (2.0 * m.dot(dv));
                    let o = a + m * k;
                    let r = k.abs();
                    arc_points(o, r, -m, t, turn(c - o, -m, t), c)
                }
            }
            Piece::Spline(points) => {
                if points.is_empty() {
                    return Err(what("a spline needs at least one point"));
                }
                let mut all = vec![a];
                for p in points {
                    all.push(finite3(*p, "a point")?);
                }
                let m = all.len();
                let mut out = Vec::new();
                for i in 0..m - 1 {
                    let p0 = all[i.saturating_sub(1)];
                    let (p1, p2) = (all[i], all[i + 1]);
                    let p3 = all[(i + 2).min(m - 1)];
                    out.extend(cubic(p1, p1 + (p2 - p0) / 6.0, p2 - (p3 - p1) / 6.0, p2));
                }
                out
            }
        };
        if pts.iter().all(|p| *p == a) {
            return Err(what("it does not move"));
        }
        if smooth_start && let Some(s) = f.corner.last_mut() {
            *s = false;
        }
        let n = pts.len();
        for (i, p) in pts.into_iter().enumerate() {
            f.points.push(p);
            f.corner.push(i + 1 == n);
        }
    }
    Ok(f)
}

/// The helix of [`crate::paths::helix`] as a [`Flat`]: its ends are
/// corners, the rest smooth.
pub fn helix(d: &crate::paths::Helix) -> Result<Flat, GenError> {
    let points = crate::paths::helix(d)?;
    let n = points.len();
    Ok(Flat {
        points,
        corner: (0..n).map(|i| i == 0 || i + 1 == n).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_three_point_arc_is_on_its_circle_with_the_rule_count() {
        // A half circle of radius 5 in the xz plane, through the top.
        let d = Path3 {
            start: DVec3::new(-5.0, 0.0, 0.0),
            pieces: vec![Piece::Arc {
                through: DVec3::new(0.0, 0.0, 5.0),
                to: DVec3::new(5.0, 0.0, 0.0),
            }],
        };
        let f = flatten(&d).unwrap();
        assert_eq!(f.points.len() as u32 - 1, segments(5.0, trig::PI));
        for p in &f.points {
            assert!((p.length() - 5.0).abs() < 1e-12, "{p:?}");
            assert!(p.z >= -1e-12);
        }
        assert_eq!(f.corner.iter().filter(|c| **c).count(), 2);
    }

    #[test]
    fn a_tangent_arc_leaves_along_the_line_and_is_no_corner() {
        let d = Path3 {
            start: DVec3::ZERO,
            pieces: vec![
                Piece::Line(DVec3::new(10.0, 0.0, 0.0)),
                Piece::TangentArc(DVec3::new(10.0, 0.0, 10.0)),
            ],
        };
        let f = flatten(&d).unwrap();
        // A half circle of radius 5 about (10, 0, 5), ahead of the line.
        let o = DVec3::new(10.0, 0.0, 5.0);
        for p in &f.points[1..] {
            assert!(((*p - o).length() - 5.0).abs() < 1e-12, "{p:?}");
        }
        assert!(f.points[2].x > 10.0, "it bulges ahead: {:?}", f.points[2]);
        assert!(!f.corner[1]);
        assert!(
            flatten(&Path3 {
                start: DVec3::ZERO,
                pieces: vec![Piece::TangentArc(DVec3::X)],
            })
            .is_err()
        );
        assert!(
            flatten(&Path3 {
                start: DVec3::ZERO,
                pieces: vec![Piece::Arc {
                    through: DVec3::X,
                    to: DVec3::X * 2.0,
                }],
            })
            .is_err()
        );
    }
}
