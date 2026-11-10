//! Generators along paths: [`sweep`], [`loft`] (in [`crate::loft`]), [`helix`], [`thread`].
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

use crate::solids::{GenError, RingUvs, finish, push};

pub use crate::loft::{Loft, LoftCurve, loft};

/// Rotation-minimizing frames along a polyline by double reflection
/// (Wang, Jüttler, Zheng, Liu, TOG 2008): `(tangent, normal)` per point.
/// The first normal is `up` made perpendicular to the first tangent. The
/// tangent at an end is the first or last chord's direction; inside, the
/// direction from the point before to the point after.
pub fn rmf(path: &[DVec3], up: DVec3) -> Result<Vec<(DVec3, DVec3)>, GenError> {
    rmf_ends(path, up, [DVec3::ZERO; 2])
}

/// [`rmf`] with the tangents at the two ends given: `ends[0]` at the
/// start and `ends[1]` at the end, each zero for the chord's direction.
/// Refused: a given tangent that is not finite, or at 90° or more from
/// its chord.
pub fn rmf_ends(
    path: &[DVec3],
    up: DVec3,
    ends: [DVec3; 2],
) -> Result<Vec<(DVec3, DVec3)>, GenError> {
    let n = path.len();
    if n < 2 {
        return Err(GenError::Profile(ProfileError::TooFewPoints(n)));
    }
    let ends = end_tangents(path, ends)?;
    let tangent = |i: usize| {
        if i == 0
            && let Some(t) = ends[0]
        {
            return Ok(t);
        }
        if i == n - 1
            && let Some(t) = ends[1]
        {
            return Ok(t);
        }
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
    /// The path's direction at its start and at its end, which the end
    /// sections are square to; zero for the first or last chord's
    /// direction (a path of points has no other). A path of curve pieces
    /// gives them ([`crate::path3::Flat::tangents`]).
    pub tangents: [DVec3; 2],
}

/// Sweeps `profile` (in its xy plane, y towards `up`) along `d.path` with
/// rotation-minimizing frames. The ends are capped `start` and `end`,
/// each square to the path's direction there (`d.tangents`, else the
/// chord); a section inside is square to the direction from the point
/// before it to the point after.
pub fn sweep(profile: &Profile, d: &Sweep) -> Result<Solid, GenError> {
    sweep_with(profile, d, false)
}

/// [`sweep`], with UVs when `uvs`: walls by [`RingUvs`] (u round each
/// section, v along the path; the seam along the contour's first point),
/// caps planar in the section's own placed coordinates (twisted and
/// scaled as the section is there).
pub(crate) fn sweep_with(profile: &Profile, d: &Sweep, uvs: bool) -> Result<Solid, GenError> {
    if profile.regions.is_empty() {
        return Err(ProfileError::Empty.into());
    }
    if d.path.iter().any(|p| !p.is_finite()) || !d.twist.is_finite() || !d.scale_end.is_finite() {
        return Err(GenError::NotFinite("sweep"));
    }
    let frames = rmf_ends(&d.path, d.up, d.tangents)?;
    let n = d.path.len();
    let end_scale = if d.scale_end == 0.0 { 1.0 } else { d.scale_end };
    // Arc length, so twist and scale spread by distance, not by point.
    let mut along = vec![0.0; n];
    for i in 1..n {
        along[i] = along[i - 1] + (d.path[i] - d.path[i - 1]).length();
    }
    let total = along[n - 1].max(f64::MIN_POSITIVE);
    let local = |p: DVec2, i: usize| {
        let f = along[i] / total;
        let (s, c) = trig::sin_cos(d.twist * f);
        let k = 1.0 + (end_scale - 1.0) * f;
        DVec2::new(c * p.x - s * p.y, s * p.x + c * p.y) * k
    };
    let place = |p: DVec2, i: usize| {
        let q = local(p, i);
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
        let grid = RingUvs::new(&s.positions, &rings, false);
        for (a, b) in contour_runs(c) {
            for j in a..b {
                let (j0, j1) = (j % m, (j + 1) % m);
                for i in 0..n - 1 {
                    let (p, q) = (&rings[i], &rings[i + 1]);
                    let uv = |i: usize, j: usize| grid.at(i, j);
                    push(
                        &mut s,
                        uvs,
                        [p[j0], p[j1], q[j1]],
                        surface,
                        &["side"],
                        [uv(i, j0), uv(i, j0 + 1), uv(i + 1, j0 + 1)],
                    );
                    push(
                        &mut s,
                        uvs,
                        [p[j0], q[j1], q[j0]],
                        surface,
                        &["side"],
                        [uv(i, j0), uv(i + 1, j0 + 1), uv(i + 1, j0)],
                    );
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
        let b = [t[0], t[2], t[1]];
        push(
            &mut s,
            uvs,
            b.map(|i| first[i as usize]),
            0,
            &["start"],
            b.map(|i| local(fill.points[i as usize], 0)),
        );
        push(
            &mut s,
            uvs,
            t.map(|i| last[i as usize]),
            1,
            &["end"],
            t.map(|i| local(fill.points[i as usize], n - 1)),
        );
    }
    finish(s)
}

/// The given end tangents made unit (`None` where zero: the chord), each
/// checked against its chord.
pub(crate) fn end_tangents(
    path: &[DVec3],
    ends: [DVec3; 2],
) -> Result<[Option<DVec3>; 2], GenError> {
    let n = path.len();
    let chords = [path[1] - path[0], path[n - 1] - path[n - 2]];
    let mut out = [None; 2];
    for k in 0..2 {
        let t = ends[k];
        if !t.is_finite() {
            return Err(GenError::NotFinite("tangents"));
        }
        if t == DVec3::ZERO {
            continue;
        }
        let t = t.normalize();
        if t.dot(chords[k]) <= 0.0 {
            return Err(GenError::Invalid(format!(
                "the path's {} tangent is at 90° or more from its {} chord",
                ["start", "end"][k],
                ["first", "last"][k]
            )));
        }
        out[k] = Some(t);
    }
    Ok(out)
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

/// The helix's unit tangent at its start and its end (its derivative, not
/// a chord).
pub fn helix_tangents(d: &Helix) -> [DVec3; 2] {
    let angle = trig::TAU * d.turns;
    let rise = d.pitch * d.turns;
    let (s, c) = trig::sin_cos(angle);
    let unit = |v: DVec3| v.try_normalize().unwrap_or(DVec3::ZERO);
    [
        unit(DVec3::new(0.0, d.radius * angle, rise)),
        unit(DVec3::new(
            -d.radius * angle * s,
            d.radius * angle * c,
            rise,
        )),
    ]
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
    thread_with(d, false)
}

/// [`thread`], with UVs when `uvs` (as [`sweep_with`] gives them).
pub(crate) fn thread_with(d: &Thread, uvs: bool) -> Result<Solid, GenError> {
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
    let spiral = Helix {
        radius: root - 0.1 * depth,
        pitch: d.pitch,
        turns: d.length / d.pitch,
    };
    let path = helix(&spiral)?;
    // In the profile plane: x along the binormal (≈ along the axis), y
    // along the normal. The normal starts radial; make the ridge point out.
    let half = 0.45 * d.pitch;
    let tri = Profile::from_contour(Contour::corners(vec![
        DVec2::new(-half, 0.0),
        DVec2::new(half, 0.0),
        DVec2::new(0.0, 1.1 * depth),
    ]));
    let start_radial = DVec3::X;
    let mut s = sweep_with(
        &tri,
        &Sweep {
            path,
            up: start_radial,
            tangents: helix_tangents(&spiral),
            ..Default::default()
        },
        uvs,
    )?;
    s.tag_all_faces("thread");
    s.canonicalize()
        .map_err(|_| GenError::NotFinite("positions"))?;
    Ok(s)
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
}
