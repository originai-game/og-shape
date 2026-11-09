//! Generators along paths: [`sweep`], [`loft`], [`helix`], [`thread`].
//!
//! | generator | faces | edges |
//! |---|---|---|
//! | [`sweep`] | `start`, `end`, `side` | `start`, `end`, `side` (along corners) |
//! | [`loft`] | `bottom`, `top`, `side` | `bottom`, `top` |
//! | [`thread`] | as [`sweep`], also `thread` | as [`sweep`] |

use shape_2d::curve::segments;
use shape_2d::{Contour, Profile, ProfileError};
use shape_core::{DVec2, DVec3, trig};
use shape_mesh::Solid;

use crate::solids::GenError;

/// Rotation-minimizing frames along a polyline by double reflection
/// (Wang, Jüttler, Zheng, Liu, TOG 2008): `(tangent, normal)` per point.
/// The first normal is `up` made perpendicular to the first tangent.
pub fn rmf(path: &[DVec3], up: DVec3) -> Result<Vec<(DVec3, DVec3)>, GenError> {
    let n = path.len();
    if n < 2 {
        return Err(GenError::Profile(ProfileError::TooFewPoints(n)));
    }
    let tangent = |i: usize| {
        let d = if i == 0 {
            path[1] - path[0]
        } else if i == n - 1 {
            path[n - 1] - path[n - 2]
        } else {
            path[i + 1] - path[i - 1]
        };
        d.try_normalize().ok_or(GenError::NotPositive("path step"))
    };
    let t0 = tangent(0)?;
    let mut r = (up - t0 * up.dot(t0)).try_normalize();
    if r.is_none() {
        let alt = if t0.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
        r = (alt - t0 * alt.dot(t0)).try_normalize();
    }
    let mut r = r.ok_or(GenError::NotPositive("path step"))?;
    let mut t = t0;
    let mut frames = vec![(t, r)];
    for i in 0..n - 1 {
        let v1 = path[i + 1] - path[i];
        let c1 = v1.dot(v1);
        if c1 == 0.0 {
            return Err(GenError::NotPositive("path step"));
        }
        let r_l = r - v1 * (2.0 / c1 * v1.dot(r));
        let t_l = t - v1 * (2.0 / c1 * v1.dot(t));
        let t_next = tangent(i + 1)?;
        let v2 = t_next - t_l;
        let c2 = v2.dot(v2);
        r = if c2 == 0.0 {
            r_l
        } else {
            r_l - v2 * (2.0 / c2 * v2.dot(r_l))
        };
        t = t_next;
        frames.push((t, r));
    }
    Ok(frames)
}

/// A sweep of a profile along a path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sweep {
    /// The path, at least two points.
    pub path: Vec<DVec3>,
    /// The direction the profile's +y should face at the start; zero for
    /// any direction perpendicular to the path.
    pub up: DVec3,
    /// Turn of the profile over the whole path, radians, spread evenly.
    pub twist: f64,
    /// Scale of the profile at the end (0 keeps it).
    pub scale_end: f64,
}

/// Sweeps `profile` (in its xy plane, y towards `up`) along `d.path` with
/// rotation-minimizing frames. The ends are capped `start` and `end`.
pub fn sweep(profile: &Profile, d: &Sweep) -> Result<Solid, GenError> {
    if profile.regions.is_empty() {
        return Err(ProfileError::Empty.into());
    }
    if d.path.iter().any(|p| !p.is_finite()) || !d.twist.is_finite() || !d.scale_end.is_finite() {
        return Err(GenError::NotFinite("sweep"));
    }
    let frames = rmf(&d.path, d.up)?;
    let n = d.path.len();
    let end_scale = if d.scale_end == 0.0 { 1.0 } else { d.scale_end };
    // Arc length, so twist and scale spread by distance, not by point.
    let mut along = vec![0.0; n];
    for i in 1..n {
        along[i] = along[i - 1] + (d.path[i] - d.path[i - 1]).length();
    }
    let total = along[n - 1].max(f64::MIN_POSITIVE);
    let place = |p: DVec2, i: usize| {
        let f = along[i] / total;
        let (s, c) = trig::sin_cos(d.twist * f);
        let k = 1.0 + (end_scale - 1.0) * f;
        let q = DVec2::new(c * p.x - s * p.y, s * p.x + c * p.y) * k;
        let (t, r) = frames[i];
        let b = t.cross(r);
        // Profile x along the binormal, y along the normal.
        d.path[i] + b * q.x + r * q.y
    };
    let mut s = Solid::default();
    let mut surface = 2;
    for c in profile.contours() {
        let m = c.points.len();
        let rings: Vec<Vec<u32>> = (0..n)
            .map(|i| {
                c.points
                    .iter()
                    .map(|p| s.push_vertex(place(*p, i)))
                    .collect()
            })
            .collect();
        for (a, b) in contour_runs(c) {
            for j in a..b {
                let (j0, j1) = (j % m, (j + 1) % m);
                for i in 0..n - 1 {
                    let (p, q) = (&rings[i], &rings[i + 1]);
                    s.push_triangle([p[j0], p[j1], q[j1]], surface, &["side"]);
                    s.push_triangle([p[j0], q[j1], q[j0]], surface, &["side"]);
                }
            }
            surface += 1;
        }
        for j in 0..m {
            s.tag_edge(rings[0][j], rings[0][(j + 1) % m], &["start"]);
            s.tag_edge(rings[n - 1][j], rings[n - 1][(j + 1) % m], &["end"]);
            if c.sharp[j] {
                for i in 0..n - 1 {
                    s.tag_edge(rings[i][j], rings[i + 1][j], &["side"]);
                }
            }
        }
    }
    let fill = profile.triangulate()?;
    let first: Vec<u32> = fill
        .points
        .iter()
        .map(|p| s.push_vertex(place(*p, 0)))
        .collect();
    let last: Vec<u32> = fill
        .points
        .iter()
        .map(|p| s.push_vertex(place(*p, n - 1)))
        .collect();
    for t in &fill.triangles {
        s.push_triangle(
            [
                first[t[0] as usize],
                first[t[2] as usize],
                first[t[1] as usize],
            ],
            0,
            &["start"],
        );
        s.push_triangle(t.map(|i| last[i as usize]), 1, &["end"]);
    }
    finish(s)
}

fn contour_runs(c: &Contour) -> Vec<(usize, usize)> {
    let n = c.points.len();
    let corners: Vec<usize> = (0..n).filter(|&i| c.sharp[i]).collect();
    if corners.is_empty() {
        return vec![(0, n)];
    }
    (0..corners.len())
        .map(|k| {
            let (a, b) = (corners[k], corners[(k + 1) % corners.len()]);
            (a, if b > a { b } else { b + n })
        })
        .collect()
}

fn finish(mut s: Solid) -> Result<Solid, GenError> {
    if s.measure().volume < 0.0 {
        for t in &mut s.triangles {
            t.swap(1, 2);
        }
    }
    s.canonicalize()
        .map_err(|_| GenError::NotFinite("positions"))?;
    Ok(s)
}

/// A helix about +z, starting at (radius, 0, 0), counter-clockwise seen
/// from +z (right-handed for positive `pitch`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Helix {
    /// Distance from the axis.
    pub radius: f64,
    /// Rise per turn.
    pub pitch: f64,
    /// Number of turns (may be fractional).
    pub turns: f64,
}

/// The helix's points, segments by the chord rule.
pub fn helix(d: &Helix) -> Result<Vec<DVec3>, GenError> {
    if !(d.radius.is_finite() && d.pitch.is_finite() && d.turns.is_finite()) {
        return Err(GenError::NotFinite("helix"));
    }
    if d.radius <= 0.0 || d.turns <= 0.0 {
        return Err(GenError::NotPositive("radius, turns"));
    }
    let angle = trig::TAU * d.turns;
    let n = segments(d.radius, angle);
    Ok((0..=n)
        .map(|i| {
            let f = i as f64 / n as f64;
            let (s, c) = trig::sin_cos(angle * f);
            DVec3::new(d.radius * c, d.radius * s, d.pitch * d.turns * f)
        })
        .collect())
}

/// An external screw thread's ridge.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Thread {
    /// Major radius (the crest).
    pub radius: f64,
    /// Rise per turn.
    pub pitch: f64,
    /// Length along +z.
    pub length: f64,
    /// Ridge depth, crest to root; 0 for the ISO 60° depth (0.6134 × pitch).
    pub depth: f64,
}

/// The ridge of a 60° thread as a swept triangle along a helix, its root
/// sinking 0.1 × depth below the root radius so a boolean join with a
/// core of `radius - depth` fuses cleanly. Faces carry `thread` too.
pub fn thread(d: &Thread) -> Result<Solid, GenError> {
    if !(d.radius > 0.0 && d.pitch > 0.0 && d.length > 0.0) {
        return Err(GenError::NotPositive("radius, pitch, length"));
    }
    let depth = if d.depth == 0.0 {
        0.6134 * d.pitch
    } else {
        d.depth
    };
    if depth <= 0.0 || depth >= d.radius {
        return Err(GenError::NotPositive("radius - depth"));
    }
    let root = d.radius - depth;
    let path = helix(&Helix {
        radius: root - 0.1 * depth,
        pitch: d.pitch,
        turns: d.length / d.pitch,
    })?;
    // In the profile plane: x along the binormal (≈ along the axis), y
    // along the normal. The normal starts radial; make the ridge point out.
    let half = 0.45 * d.pitch;
    let tri = Profile::from_contour(Contour::corners(vec![
        DVec2::new(-half, 0.0),
        DVec2::new(half, 0.0),
        DVec2::new(0.0, 1.1 * depth),
    ]));
    let start_radial = DVec3::X;
    let mut s = sweep(
        &tri,
        &Sweep {
            path,
            up: start_radial,
            ..Default::default()
        },
    )?;
    s.tag_all_faces("thread");
    s.canonicalize()
        .map_err(|_| GenError::NotFinite("positions"))?;
    Ok(s)
}

/// A loft through sections stacked along +z.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Loft {
    /// The sections: a single-contour profile and its height, bottom
    /// first, heights increasing.
    pub sections: Vec<(Profile, f64)>,
    /// Points each section is resampled to; 0 for the most any has.
    pub samples: u32,
    /// Rings between consecutive sections (0 for 1). With more than one,
    /// the surface follows a Catmull-Rom curve through the sections.
    pub steps: u32,
}

/// Resamples a closed contour to `n` points evenly by length, starting at
/// the point with the largest x (then the smallest y).
fn resample(c: &Contour, n: usize) -> Vec<DVec2> {
    let m = c.points.len();
    let start = (0..m)
        .max_by(|&a, &b| {
            let (p, q) = (c.points[a], c.points[b]);
            p.x.total_cmp(&q.x).then(q.y.total_cmp(&p.y))
        })
        .unwrap_or(0);
    let pts: Vec<DVec2> = (0..=m).map(|i| c.points[(start + i) % m]).collect();
    let mut acc = vec![0.0];
    for w in pts.windows(2) {
        acc.push(acc.last().copied().unwrap_or(0.0) + (w[1] - w[0]).length());
    }
    let total = *acc.last().unwrap_or(&0.0);
    let mut out = Vec::with_capacity(n);
    let mut seg = 0;
    for k in 0..n {
        let target = total * (k as f64 / n as f64);
        while seg + 1 < acc.len() - 1 && acc[seg + 1] < target {
            seg += 1;
        }
        let len = acc[seg + 1] - acc[seg];
        let f = if len > 0.0 {
            (target - acc[seg]) / len
        } else {
            0.0
        };
        out.push(pts[seg] + (pts[seg + 1] - pts[seg]) * f);
    }
    out
}

/// A surface through the sections, capped `bottom` and `top`.
pub fn loft(d: &Loft) -> Result<Solid, GenError> {
    let k = d.sections.len();
    if k < 2 {
        return Err(GenError::Profile(ProfileError::TooFewPoints(k)));
    }
    let mut outlines = Vec::with_capacity(k);
    for (p, z) in &d.sections {
        if p.regions.len() != 1 || !p.regions[0].holes.is_empty() {
            return Err(GenError::Profile(ProfileError::Empty));
        }
        if !z.is_finite() {
            return Err(GenError::NotFinite("height"));
        }
        outlines.push(&p.regions[0].outer);
    }
    if d.sections.windows(2).any(|w| w[1].1 <= w[0].1) {
        return Err(GenError::NotPositive("height step"));
    }
    let n = if d.samples >= 3 {
        d.samples as usize
    } else {
        outlines.iter().map(|c| c.points.len()).max().unwrap_or(3)
    };
    let rings2: Vec<Vec<DVec2>> = outlines.iter().map(|c| resample(c, n)).collect();
    let steps = d.steps.max(1) as usize;
    // Ring j of the result at parameter u in [0, k-1].
    let at = |sec: usize, f: f64, j: usize| -> DVec3 {
        let get = |i: isize| {
            let i = i.clamp(0, k as isize - 1) as usize;
            (rings2[i][j], d.sections[i].1)
        };
        let i = sec as isize;
        let (p0, z0) = get(i - 1);
        let (p1, z1) = get(i);
        let (p2, z2) = get(i + 1);
        let (p3, z3) = get(i + 2);
        if steps == 1 {
            let p = p1 + (p2 - p1) * f;
            return DVec3::new(p.x, p.y, z1 + (z2 - z1) * f);
        }
        let cr = |a: f64, b: f64, c: f64, e: f64| {
            let (f2, f3) = (f * f, f * f * f);
            0.5 * (2.0 * b
                + (c - a) * f
                + (2.0 * a - 5.0 * b + 4.0 * c - e) * f2
                + (3.0 * b - a - 3.0 * c + e) * f3)
        };
        DVec3::new(
            cr(p0.x, p1.x, p2.x, p3.x),
            cr(p0.y, p1.y, p2.y, p3.y),
            cr(z0, z1, z2, z3),
        )
    };
    let mut s = Solid::default();
    let mut rings: Vec<Vec<u32>> = Vec::new();
    for sec in 0..k - 1 {
        for st in 0..steps {
            let f = st as f64 / steps as f64;
            rings.push((0..n).map(|j| s.push_vertex(at(sec, f, j))).collect());
        }
    }
    rings.push(
        (0..n)
            .map(|j| {
                s.push_vertex(DVec3::new(
                    rings2[k - 1][j].x,
                    rings2[k - 1][j].y,
                    d.sections[k - 1].1,
                ))
            })
            .collect(),
    );
    for w in rings.windows(2) {
        for j in 0..n {
            let j1 = (j + 1) % n;
            s.push_triangle([w[0][j], w[0][j1], w[1][j1]], 2, &["side"]);
            s.push_triangle([w[0][j], w[1][j1], w[1][j]], 2, &["side"]);
        }
    }
    for (ring, (name, surface, z)) in [
        (&rings[0], ("bottom", 0, d.sections[0].1)),
        (&rings[rings.len() - 1], ("top", 1, d.sections[k - 1].1)),
    ] {
        let pts: Vec<DVec2> = ring
            .iter()
            .map(|&v| s.positions[v as usize].truncate())
            .collect();
        let cap = Profile::from_contour(Contour::smooth(pts.clone()));
        let fill = cap.triangulate()?;
        let ids: Vec<u32> = fill
            .points
            .iter()
            .map(|p| s.push_vertex(p.extend(z)))
            .collect();
        for t in &fill.triangles {
            let t = if name == "bottom" {
                [t[0], t[2], t[1]]
            } else {
                *t
            };
            s.push_triangle(t.map(|i| ids[i as usize]), surface, &[name]);
        }
        for j in 0..n {
            s.tag_edge(ring[j], ring[(j + 1) % n], &[name]);
        }
    }
    finish(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shape_2d::{Circle, Rect, circle, rect};

    #[test]
    fn rmf_frames_stay_orthonormal_on_a_helix() {
        let path = helix(&Helix {
            radius: 5.0,
            pitch: 2.0,
            turns: 3.0,
        })
        .unwrap();
        for (t, r) in rmf(&path, DVec3::Z).unwrap() {
            assert!((t.length() - 1.0).abs() < 1e-12 && (r.length() - 1.0).abs() < 1e-12);
            assert!(t.dot(r).abs() < 1e-9);
        }
    }

    #[test]
    fn a_straight_sweep_is_an_extrusion() {
        let p = rect(&Rect {
            width: 2.0,
            height: 3.0,
            radius: 0.0,
        })
        .unwrap();
        let s = sweep(
            &p,
            &Sweep {
                path: vec![DVec3::ZERO, DVec3::new(0.0, 0.0, 4.0)],
                ..Default::default()
            },
        )
        .unwrap();
        let m = s.measure();
        assert!(m.closed);
        assert!((m.volume - 24.0).abs() < 1e-12);
        assert_eq!(s.edges(&["side"]).unwrap().len(), 4);
    }

    #[test]
    fn a_bent_pipe_and_a_thread_are_closed() {
        let path: Vec<DVec3> = (0..=16)
            .map(|i| {
                let (s, c) = trig::sin_cos(trig::PI / 2.0 * (i as f64 / 16.0));
                DVec3::new(20.0 * c, 20.0 * s, 0.0)
            })
            .collect();
        let pipe = sweep(
            &circle(&Circle {
                radius: 3.0,
                segments: 0,
            })
            .unwrap(),
            &Sweep {
                path,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(pipe.measure().closed);
        let th = thread(&Thread {
            radius: 5.0,
            pitch: 1.5,
            length: 6.0,
            depth: 0.0,
        })
        .unwrap();
        let m = th.measure();
        assert!(m.closed, "{m:?}");
        assert!(m.volume > 0.0);
    }

    #[test]
    fn a_loft_from_square_to_circle_is_closed() {
        let sq = rect(&Rect {
            width: 10.0,
            height: 10.0,
            radius: 0.0,
        })
        .unwrap();
        let ci = circle(&Circle {
            radius: 4.0,
            segments: 0,
        })
        .unwrap();
        for steps in [1, 4] {
            let s = loft(&Loft {
                sections: vec![(sq.clone(), 0.0), (ci.clone(), 10.0), (sq.clone(), 20.0)],
                samples: 32,
                steps,
            })
            .unwrap();
            let m = s.measure();
            assert!(m.closed, "{m:?}");
            assert!(!s.faces(&["top"]).unwrap().is_empty());
        }
    }
}
