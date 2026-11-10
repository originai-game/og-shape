//! [`wire`]: a round wire along a path (sand3d's `wire`), a pipe, a
//! spring, a rod bent to shape.
//!
//! | faces | edges |
//! |---|---|
//! | `side`; `start`, `end` on an open wire | `start`, `end` (cap rims), `seam` (at mitred corners) |
//!
//! A corner of the path is either bent round an arc of the radius given
//! (a tube bender's `fillet`, sand3d's `_filleted`) or mitred: the ring
//! there lies in the plane that halves the turn, stretched so both
//! straight runs meet it whole. Each ring is carried to the next by the
//! smallest rotation that turns one run's direction into the next
//! (rotation-minimizing, two reflections), so the wire never twists; a
//! closed wire spreads the turn it gains going round over its length.
//! Straight runs meeting at a mitre are separate surfaces, so the mitre
//! is a real edge, `seam`; bends are smooth.

use shape_2d::{Contour, Profile};
use shape_core::{DVec2, DVec3, trig};
use shape_mesh::Solid;

use crate::solids::{GenError, count, finish, positive, segments_field};

/// A round wire along a path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wire {
    /// The centre line, at least two points. A point written twice is one.
    pub path: Vec<DVec3>,
    /// The wire's diameter.
    pub diameter: f64,
    /// The path closes on itself: a ring with no ends.
    pub closed: bool,
    /// Bend radius of every corner, on the centre line; 0 mitres them. At
    /// least `diameter / 2`.
    pub fillet: f64,
    /// A bend radius per path point, 0 mitring that corner; overrides
    /// `fillet` when not empty.
    pub fillets: Vec<f64>,
    /// Segments round the wire, and a bend's share of as many a turn; 0
    /// for the chord rule.
    pub segments: u32,
}

/// Points nearer than this are one (millimetres).
const EPS: f64 = 1e-6;

/// `v` turned by `angle` about the unit `axis` (Rodrigues).
fn rotate(v: DVec3, axis: DVec3, angle: f64) -> DVec3 {
    let (s, c) = trig::sin_cos(angle);
    v * c + axis.cross(v) * s + axis * (axis.dot(v) * (1.0 - c))
}

/// The angle between two directions, 0 to π.
fn turn(u: DVec3, v: DVec3) -> f64 {
    trig::atan2(u.cross(v).length(), u.dot(v))
}

fn said(p: DVec3) -> String {
    format!("({}, {}, {})", p.x, p.y, p.z)
}

/// A bent corner: setback, radius, direction in, direction out, turn.
type Bend = (f64, f64, DVec3, DVec3, f64);

/// The centre line with its corners bent: points, and per point whether
/// it is a mitred corner.
fn bent(d: &Wire, least: f64) -> Result<(Vec<DVec3>, Vec<bool>), GenError> {
    if d.path.iter().any(|p| !p.is_finite()) {
        return Err(GenError::NotFinite("path"));
    }
    if !d.fillets.is_empty() && d.fillets.len() != d.path.len() {
        return Err(GenError::Invalid(format!(
            "wire: {} fillets for {} points: one per point, or `fillet` for all",
            d.fillets.len(),
            d.path.len()
        )));
    }
    let radius_of = |i: usize| {
        if d.fillets.is_empty() {
            d.fillet
        } else {
            d.fillets[i]
        }
    };
    // A point written twice is one: it keeps the larger radius.
    let (mut p, mut rs): (Vec<DVec3>, Vec<f64>) = (Vec::new(), Vec::new());
    for (i, &q) in d.path.iter().enumerate() {
        let r = radius_of(i);
        if !r.is_finite() || r < 0.0 {
            return Err(GenError::NotPositive("fillet"));
        }
        match p.last() {
            Some(&last) if (q - last).length() <= EPS => {
                let k = rs.len() - 1;
                rs[k] = rs[k].max(r);
            }
            _ => {
                p.push(q);
                rs.push(r);
            }
        }
    }
    if d.closed && p.len() > 1 && (p[p.len() - 1] - p[0]).length() <= EPS {
        let r = rs.pop().unwrap_or(0.0);
        rs[0] = rs[0].max(r);
        p.pop();
    }
    let m = p.len();
    if m < 2 || (d.closed && m < 3) {
        return Err(GenError::Invalid(format!(
            "wire: a path needs {} distinct points, got {m}",
            if d.closed { 3 } else { 2 }
        )));
    }
    let mut corners: Vec<Option<Bend>> = vec![None; m];
    let mut mitred = vec![false; m];
    let inner = if d.closed { 0..m } else { 1..m - 1 };
    for i in inner {
        let (a, b, c) = (p[(i + m - 1) % m], p[i], p[(i + 1) % m]);
        let (u, v) = ((b - a).normalize(), (c - b).normalize());
        let t = turn(u, v);
        if t < 1e-6 {
            continue;
        }
        if t >= trig::PI - 1e-4 {
            return Err(GenError::Invalid(format!(
                "wire: the path turns back on itself at {}: a bend turns less than 180°",
                said(b)
            )));
        }
        let r = rs[i];
        if r == 0.0 {
            mitred[i] = true;
            continue;
        }
        if r < least - 1e-9 {
            return Err(GenError::Invalid(format!(
                "wire: fillet {r} at {} is tighter than the wire's radius {least}: \
                 a tube bent tighter folds on itself",
                said(b)
            )));
        }
        corners[i] = Some((r * trig::tan(t / 2.0), r, u, v, t));
    }
    let sides = if d.closed { m } else { m - 1 };
    for i in 0..sides {
        let j = (i + 1) % m;
        let used = corners[i].map_or(0.0, |c| c.0) + corners[j].map_or(0.0, |c| c.0);
        let length = (p[j] - p[i]).length();
        if used > length + 1e-9 {
            return Err(GenError::Invalid(format!(
                "wire: fillets too large for the side from {} to {} ({used} > {length})",
                said(p[i]),
                said(p[j])
            )));
        }
    }
    let (mut out, mut sharp) = (Vec::new(), Vec::new());
    for i in 0..m {
        let Some((setback, r, u, v, t)) = corners[i] else {
            out.push(p[i]);
            sharp.push(mitred[i]);
            continue;
        };
        let start = p[i] - u * setback;
        let centre = start + (v - u * u.dot(v)).normalize() * r;
        let axis = u.cross(v).normalize();
        let k = count(d.segments, r, t);
        for j in 0..=k {
            out.push(centre + rotate(start - centre, axis, t * (j as f64 / k as f64)));
            sharp.push(false);
        }
    }
    // Arcs that meet exactly share their end point.
    let (mut kept, mut flags) = (vec![out[0]], vec![sharp[0]]);
    for (q, f) in out.into_iter().zip(sharp).skip(1) {
        if (q - kept[kept.len() - 1]).length() > EPS {
            kept.push(q);
            flags.push(f);
        }
    }
    if d.closed && kept.len() > 1 && (kept[kept.len() - 1] - kept[0]).length() <= EPS {
        kept.pop();
        flags.pop();
    }
    Ok((kept, flags))
}

/// Smallest rotation taking unit `u` to unit `v`, applied to `r`: the
/// reflection across the plane normal to `u + v` (u → −v), then the one
/// normal to `v` (−v → v).
fn carry(r: DVec3, u: DVec3, v: DVec3) -> DVec3 {
    let a = u + v;
    let r1 = r - a * (2.0 * a.dot(r) / a.dot(a));
    r1 - v * (2.0 * v.dot(r1))
}

/// A round wire of `diameter` along `path`; see the module.
pub fn wire(d: &Wire) -> Result<Solid, GenError> {
    let radius = positive(d.diameter, "diameter")? / 2.0;
    segments_field(d.segments, "segments")?;
    let (p, sharp) = bent(d, radius)?;
    let m = p.len();
    let sides = if d.closed { m } else { m - 1 };
    let dirs: Vec<DVec3> = (0..sides)
        .map(|i| (p[(i + 1) % m] - p[i]).normalize())
        .collect();
    // A frame normal per run, carried from run to run.
    let up = if dirs[0].z.abs() < 0.9 {
        DVec3::Z
    } else {
        DVec3::X
    };
    let mut normals = vec![(up - dirs[0] * up.dot(dirs[0])).normalize()];
    for i in 1..sides {
        let r = carry(normals[i - 1], dirs[i - 1], dirs[i]);
        normals.push((r - dirs[i] * r.dot(dirs[i])).normalize());
    }
    // A closed wire comes back turned by `gap` about its first run; each
    // point takes its share by length.
    let mut along = vec![0.0; m + 1];
    for i in 0..sides {
        along[i + 1] = along[i] + (p[(i + 1) % m] - p[i]).length();
    }
    let total = along[sides];
    let gap = if d.closed {
        let back = carry(normals[sides - 1], dirs[sides - 1], dirs[0]);
        let r0 = normals[0];
        trig::atan2(dirs[0].dot(back.cross(r0)), back.dot(r0))
    } else {
        0.0
    };
    // The profile: a polygon in the run's (normal, binormal) plane.
    let n = count(d.segments, radius, trig::TAU);
    let circle: Vec<DVec2> = (0..n)
        .map(|k| {
            let (s, c) = trig::sin_cos(trig::TAU * (k as f64 / n as f64));
            DVec2::new(radius * c, radius * s)
        })
        .collect();
    // Point q of the ring at p[i]: in the frame of run `run` (direction w,
    // normal r turned by `twist`), moved along w onto the plane normal to
    // `t` through the centre.
    let place = |c: DVec3, w: DVec3, r: DVec3, twist: f64, t: DVec3, q: DVec2| {
        let r = rotate(r, w, twist);
        let b = w.cross(r);
        let off = r * q.x + b * q.y;
        let lambda = -off.dot(t) / w.dot(t);
        c + off + w * lambda
    };
    let frame = |i: usize| -> (DVec3, DVec3, f64, DVec3) {
        // The run arriving at p[i] (the first run at an open start, and
        // at a closed start the run leaving it, untwisted).
        let twist = gap * (along[i] / total);
        if i == 0 {
            let t = if d.closed {
                (dirs[sides - 1] + dirs[0]).normalize()
            } else {
                dirs[0]
            };
            return (dirs[0], normals[0], 0.0, t);
        }
        let (w, r) = (dirs[i - 1], normals[i - 1]);
        let t = if i < sides {
            (w + dirs[i]).normalize()
        } else {
            w
        };
        (w, r, twist, t)
    };
    let mut s = Solid::default();
    let rings: Vec<Vec<u32>> = (0..m)
        .map(|i| {
            let (w, r, twist, t) = frame(i);
            circle
                .iter()
                .map(|q| s.push_vertex(place(p[i], w, r, twist, t, *q)))
                .collect()
        })
        .collect();
    // Surfaces: a new one after each mitred corner.
    let total_sharp = sharp.iter().skip(1).filter(|&&f| f).count();
    let mut sid = 0;
    let n = n as usize;
    for i in 0..sides {
        if i > 0 && sharp[i] {
            sid += 1;
        }
        let surface = if d.closed && !sharp[0] && sid == total_sharp {
            2
        } else {
            2 + sid as u32
        };
        let (a, b) = (&rings[i], &rings[(i + 1) % m]);
        for k in 0..n {
            let k1 = (k + 1) % n;
            s.push_triangle([a[k], a[k1], b[k1]], surface, &["side"]);
            s.push_triangle([a[k], b[k1], b[k]], surface, &["side"]);
        }
    }
    for (i, ring) in rings.iter().enumerate() {
        if sharp[i] {
            for k in 0..n {
                s.tag_edge(ring[k], ring[(k + 1) % n], &["seam"]);
            }
        }
    }
    if !d.closed {
        let fill = Profile::from_contour(Contour::smooth(circle.clone())).triangulate()?;
        for (end, name, surface) in [(0, "start", 0), (m - 1, "end", 1)] {
            let (w, r, twist, t) = frame(end);
            let ids: Vec<u32> = fill
                .points
                .iter()
                .map(|q| s.push_vertex(place(p[end], w, r, twist, t, *q)))
                .collect();
            for tri in &fill.triangles {
                let tri = if end == 0 {
                    [tri[0], tri[2], tri[1]]
                } else {
                    *tri
                };
                s.push_triangle(tri.map(|i| ids[i as usize]), surface, &[name]);
            }
            let ring = &rings[end];
            for k in 0..n {
                s.tag_edge(ring[k], ring[(k + 1) % n], &[name]);
            }
        }
    }
    finish(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shape_2d::curve::segments;

    fn closed(s: &Solid) -> f64 {
        let m = s.measure();
        assert!(m.closed, "open: {m:?}");
        assert_eq!(m.degenerate_triangles, 0);
        assert_eq!(m.components, 1);
        m.volume
    }

    /// The area of the wire's 24-to-64-gon.
    fn section(d: f64) -> f64 {
        let r = d / 2.0;
        let n = segments(r, trig::TAU) as f64;
        0.5 * n * r * r * trig::sin(trig::TAU / n)
    }

    #[test]
    fn a_mitred_wire_has_its_centre_lines_volume() {
        // A stool's leg without the bend: a mitre keeps area × length.
        let path = vec![
            DVec3::ZERO,
            DVec3::new(0.0, 0.0, 65.0),
            DVec3::new(-25.0, 0.0, 65.0),
            DVec3::new(-25.0, 30.0, 80.0),
        ];
        let len = 65.0 + 25.0 + (900.0f64 + 225.0).sqrt();
        let s = wire(&Wire {
            path,
            diameter: 6.0,
            ..Default::default()
        })
        .unwrap();
        let want = section(6.0) * len;
        assert!(
            (closed(&s) - want).abs() < 1e-9 * want,
            "{} {want}",
            s.measure().volume
        );
        // Two mitres, each a named ring; three straight runs.
        let n = segments(3.0, trig::TAU) as usize;
        assert_eq!(s.edges(&["seam"]).unwrap().len(), 2 * n);
        assert_eq!(s.edges(&["start"]).unwrap().len(), n);
        assert_eq!(s.real_edges().len(), 4 * n);
    }

    #[test]
    fn a_bent_wire_is_smooth_and_follows_its_arcs() {
        // sand3d's stool leg: 33 thick, bent at 60.
        let s = wire(&Wire {
            path: vec![
                DVec3::ZERO,
                DVec3::new(0.0, 0.0, 650.0),
                DVec3::new(-250.0, 0.0, 650.0),
            ],
            diameter: 33.0,
            fillet: 60.0,
            ..Default::default()
        })
        .unwrap();
        // The centre line: two straights shortened by the setback 60, and a
        // quarter arc of 60 (Pappus, up to the polygon's chords).
        let len = 590.0 + 190.0 + trig::PI / 2.0 * 60.0;
        let want = section(33.0) * len;
        let v = closed(&s);
        assert!((v - want).abs() / want < 1e-3, "{v} {want}");
        assert!(s.edges(&["seam"]).is_err());
        let m = s.measure();
        assert!((m.bounds_max.z - (650.0 + 16.5)).abs() < 1e-9);
    }

    #[test]
    fn a_closed_wire_is_a_ring_with_no_ends() {
        // A square loop, bent at every corner.
        let sq = vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(40.0, 0.0, 0.0),
            DVec3::new(40.0, 40.0, 10.0),
            DVec3::new(0.0, 40.0, 10.0),
        ];
        let s = wire(&Wire {
            path: sq.clone(),
            diameter: 4.0,
            closed: true,
            fillet: 8.0,
            ..Default::default()
        })
        .unwrap();
        closed(&s);
        assert!(s.faces(&["start"]).is_err());
        assert!(s.real_edges().is_empty());
        // Mitred instead: four seams, and the volume of area × length.
        let s = wire(&Wire {
            path: sq,
            diameter: 4.0,
            closed: true,
            ..Default::default()
        })
        .unwrap();
        let len = 2.0 * 40.0 + 2.0 * (1600.0f64 + 100.0).sqrt();
        let want = section(4.0) * len;
        assert!((closed(&s) - want).abs() < 1e-9 * want);
        assert_eq!(
            s.edges(&["seam"]).unwrap().len(),
            4 * segments(2.0, trig::TAU) as usize
        );
    }

    #[test]
    fn a_wire_refuses_what_it_cannot_bend() {
        let path = vec![
            DVec3::ZERO,
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(10.0, 10.0, 0.0),
        ];
        let w = |fillet: f64| {
            wire(&Wire {
                path: path.clone(),
                diameter: 4.0,
                fillet,
                ..Default::default()
            })
        };
        assert!(matches!(w(1.0), Err(GenError::Invalid(_))));
        assert!(matches!(w(20.0), Err(GenError::Invalid(_))));
        assert!(w(10.0).is_ok());
        let back = wire(&Wire {
            path: vec![DVec3::ZERO, DVec3::X * 10.0, DVec3::ZERO],
            diameter: 1.0,
            ..Default::default()
        });
        assert!(matches!(back, Err(GenError::Invalid(_))));
    }
}
