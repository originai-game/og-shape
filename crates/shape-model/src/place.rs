//! Placement on solids in the world: frames read off a box, a face or an
//! edge, and the distributors that give lists of frames (`docs/design.md`
//! §2, the Placement row).
//!
//! Entry: the functions below. Each reads world geometry and returns
//! frames (or a [`Path`]); nothing here changes a solid. Frames follow
//! [`Frame::facing`]'s axes. Every choice among equals is broken by a fixed
//! rule (largest, then lowest index or coordinate), never by visiting
//! order.

use std::collections::BTreeMap;

use shape_core::{DVec3, Frame, trig};
use shape_mesh::Solid;

use crate::error::ModelError;

/// A polyline in the world, open or closed (a closed path does not repeat
/// its first point).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    /// The points in order.
    pub points: Vec<DVec3>,
    /// Whether the last point joins the first.
    pub closed: bool,
}

/// cos 1°: how close a face's normal must be to a direction to face it.
const FACING_COS: f64 = 0.999_847_695_156_391_3;
/// Points of an edge path closer than this (mm) are one.
const EDGE_WELD: f64 = 0.02;

/// `"+X"`, `"+X-Y"`, `"-Y+X+Z"` as `[(axis, ±1)]`: one to three different
/// axes, each with a sign (a lone axis may go without).
pub fn sides(which: &str) -> Result<Vec<(usize, f64)>, ModelError> {
    let bad = || ModelError::BadSides(which.to_string());
    let b = which.as_bytes();
    let mut out: Vec<(usize, f64)> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let (sign, at) = match b[i] {
            b'+' => (1.0, i + 1),
            b'-' => (-1.0, i + 1),
            _ if b.len() == 1 => (1.0, i),
            _ => return Err(bad()),
        };
        let axis = match b.get(at).map(u8::to_ascii_lowercase) {
            Some(b'x') => 0,
            Some(b'y') => 1,
            Some(b'z') => 2,
            _ => return Err(bad()),
        };
        if out.iter().any(|&(a, _)| a == axis) {
            return Err(bad());
        }
        out.push((axis, sign));
        i = at + 1;
    }
    if out.is_empty() {
        return Err(bad());
    }
    Ok(out)
}

/// A direction word (`"+X"`, `"-z"`, `"Y"`) as a unit vector, or `None`.
pub fn direction(which: &str) -> Option<DVec3> {
    let s = sides(which).ok()?;
    if s.len() != 1 {
        return None;
    }
    let mut v = DVec3::ZERO;
    v[s[0].0] = s[0].1;
    Some(v)
}

/// The frame on the box bounding `points` named by `which`: `"+X"` the
/// centre of the side looking +X, `"+X-Y"` the middle of an edge,
/// `"+X-Y+Z"` a corner. Its normal looks the first way named.
pub fn bbox(points: &[DVec3], which: &str) -> Result<Frame, ModelError> {
    let s = sides(which)?;
    let Some(&first) = points.first() else {
        return Err(ModelError::Degenerate("an empty selection".into()));
    };
    let (mut lo, mut hi) = (first, first);
    for &p in points {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let mut origin = (lo + hi) * 0.5;
    for &(axis, sign) in &s {
        origin[axis] = if sign > 0.0 { hi[axis] } else { lo[axis] };
    }
    let mut normal = DVec3::ZERO;
    normal[s[0].0] = s[0].1;
    Frame::facing(origin, normal).ok_or_else(|| ModelError::BadSides(which.into()))
}

fn tri(s: &Solid, i: usize) -> [DVec3; 3] {
    s.triangles[i].map(|v| s.positions[v as usize])
}

/// Twice the area vector of triangle `i`.
fn area2(s: &Solid, i: usize) -> DVec3 {
    let [a, b, c] = tri(s, i);
    (b - a).cross(c - a)
}

fn centroid(s: &Solid, i: usize) -> DVec3 {
    let [a, b, c] = tri(s, i);
    (a + b + c) / 3.0
}

/// The frame of a face of `s` (world geometry) named `which`, or of the
/// face furthest along a direction (`"+X"`, `"-Z"`):
/// - by name: the largest flat group of triangles carrying it (normal and
///   plane equal to 1e-4); refused as not flat when it has more than six
///   such groups and the largest holds under half the named area;
/// - by direction: the triangles facing it within 1° that lie furthest
///   along it.
///
/// The origin is the area-weighted centroid of the chosen triangles, +Z
/// their area-weighted normal, and X and Y as [`Frame::facing`] gives.
pub fn face(s: &Solid, part: &str, which: &str) -> Result<Frame, ModelError> {
    let chosen: Vec<usize> = if let Some(dir) = direction(which) {
        let facing: Vec<usize> = (0..s.triangles.len())
            .filter(|&i| area2(s, i).normalize_or_zero().dot(dir) > FACING_COS)
            .collect();
        let far = facing
            .iter()
            .map(|&i| centroid(s, i).dot(dir))
            .fold(f64::NEG_INFINITY, f64::max);
        let tol = 1e-6 * far.abs().max(1.0);
        facing
            .into_iter()
            .filter(|&i| centroid(s, i).dot(dir) > far - tol)
            .collect()
    } else {
        let tagged = s.faces(&[which]).map_err(|e| ModelError::NoSuchFace {
            part: part.into(),
            name: which.into(),
            available: e.available,
        })?;
        // Group by plane: the unit normal and the plane's offset, rounded
        // to 1e-4 as integers (an exact key).
        let mut groups: BTreeMap<[i64; 4], Vec<usize>> = BTreeMap::new();
        let mut total = 0.0;
        for &i in &tagged {
            let i = i as usize;
            let a = area2(s, i);
            total += a.length();
            let n = a.normalize_or_zero();
            let d = n.dot(centroid(s, i));
            let k = |v: f64| (v * 1e4).round() as i64;
            groups
                .entry([k(n.x), k(n.y), k(n.z), k(d)])
                .or_default()
                .push(i);
        }
        let area = |g: &Vec<usize>| g.iter().map(|&i| area2(s, i).length()).sum::<f64>();
        // Largest; among equals, the one holding the lowest triangle.
        let best = groups
            .values()
            .max_by(|a, b| {
                area(a)
                    .total_cmp(&area(b))
                    .then_with(|| b.iter().min().cmp(&a.iter().min()))
            })
            .cloned()
            .unwrap_or_default();
        if groups.len() > 6 && area(&best) < 0.5 * total {
            return Err(ModelError::NotFlat(part.into(), which.into()));
        }
        best
    };
    let mut normal = DVec3::ZERO;
    let mut weighted = DVec3::ZERO;
    let mut total = 0.0;
    for &i in &chosen {
        let a = area2(s, i);
        normal += a;
        weighted += centroid(s, i) * a.length();
        total += a.length();
    }
    if chosen.is_empty() || total <= 0.0 {
        return Err(ModelError::Degenerate(format!("{part:?}'s face {which:?}")));
    }
    Frame::facing(weighted / total, normal)
        .ok_or_else(|| ModelError::Degenerate(format!("{part:?}'s face {which:?}")))
}

/// The edges of `s` named `which` as a path in the world: the longest run
/// of them (among runs as long, the one whose lowest point is lowest in x,
/// then y, then z), closed if it closes. A closed run turns
/// anticlockwise about the axis its area faces most (Z, then Y, then X),
/// from the point nearest that view's first axis; an open one runs toward
/// +X, then +Y, then +Z. Points closer than 0.02 mm are one. Edges that
/// fork are refused.
pub fn edge(s: &Solid, part: &str, which: &str) -> Result<Path, ModelError> {
    let keys = s.edges(&[which]).map_err(|e| ModelError::NoSuchEdge {
        part: part.into(),
        name: which.into(),
        available: e.available,
    })?;
    let mut links: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (a, b) in keys {
        links.entry(a).or_default().push(b);
        links.entry(b).or_default().push(a);
    }
    if links.values().any(|n| n.len() > 2) {
        return Err(ModelError::Forks(part.into(), which.into()));
    }
    // Open runs start at an end, then closed loops at their lowest vertex.
    let mut starts: Vec<u32> = links.keys().copied().collect();
    starts.sort_by_key(|v| (links[v].len() != 1, *v));
    let mut seen = std::collections::BTreeSet::new();
    let mut runs: Vec<(f64, bool, Vec<DVec3>)> = Vec::new();
    for start in starts {
        if !seen.insert(start) {
            continue;
        }
        let mut run = vec![start];
        let mut here = start;
        while let Some(&next) = links[&here].iter().filter(|v| !seen.contains(*v)).min() {
            seen.insert(next);
            run.push(next);
            here = next;
        }
        let closed = links[&start].len() == 2;
        let mut pts = vec![s.positions[run[0] as usize]];
        for &v in &run[1..] {
            let p = s.positions[v as usize];
            if (p - pts[pts.len() - 1]).length() > EDGE_WELD {
                pts.push(p);
            }
        }
        if closed && pts.len() > 1 && (pts[pts.len() - 1] - pts[0]).length() <= EDGE_WELD {
            pts.pop();
        }
        let mut length = 0.0;
        for k in 1..pts.len() {
            length += (pts[k] - pts[k - 1]).length();
        }
        if closed && pts.len() > 1 {
            length += (pts[0] - pts[pts.len() - 1]).length();
        }
        runs.push((length, closed, pts));
    }
    let lowest = |pts: &[DVec3]| {
        pts.iter()
            .copied()
            .min_by(|a, b| shape_core::order::cmp_dvec3(*a, *b))
            .unwrap_or_default()
    };
    let best = runs
        .into_iter()
        .max_by(|a, b| {
            let la = (a.0 * 1e6).round();
            let lb = (b.0 * 1e6).round();
            la.total_cmp(&lb)
                .then_with(|| shape_core::order::cmp_dvec3(lowest(&b.2), lowest(&a.2)))
        })
        .ok_or_else(|| ModelError::Degenerate(format!("{part:?}'s edges {which:?}")))?;
    let (_, closed, pts) = best;
    if pts.len() < if closed { 3 } else { 2 } {
        return Err(ModelError::Degenerate(format!(
            "{part:?}'s edges {which:?}"
        )));
    }
    let points = if closed {
        loop_order(pts)
    } else {
        run_order(pts)
    };
    Ok(Path { points, closed })
}

fn loop_order(mut pts: Vec<DVec3>) -> Vec<DVec3> {
    let n = pts.len();
    let mut area = DVec3::ZERO;
    for k in 0..n {
        area += pts[k].cross(pts[(k + 1) % n]);
    }
    // The axis the area faces most: Z, then Y, then X on ties.
    let axis = [2usize, 1, 0]
        .into_iter()
        .max_by(|&a, &b| area[a].abs().total_cmp(&area[b].abs()).then(b.cmp(&a)))
        .unwrap_or(2);
    if area[axis] < 0.0 {
        pts.reverse();
    }
    let (u, v) = match axis {
        2 => (0, 1),
        1 => (2, 0),
        _ => (1, 2),
    };
    let c = pts.iter().copied().sum::<DVec3>() / n as f64;
    let angle = |p: DVec3| {
        let a = trig::atan2(p[v] - c[v], p[u] - c[u]);
        let a = if a < 0.0 { a + trig::TAU } else { a };
        (a * 1e9).round()
    };
    let first = (0..n)
        .min_by(|&i, &j| angle(pts[i]).total_cmp(&angle(pts[j])).then(i.cmp(&j)))
        .unwrap_or(0);
    pts.rotate_left(first);
    pts
}

fn run_order(mut pts: Vec<DVec3>) -> Vec<DVec3> {
    let key = |p: DVec3| [p.x, p.y, p.z].map(|c| (c * 1e6).round());
    let (a, b) = (key(pts[0]), key(pts[pts.len() - 1]));
    let before = a
        .iter()
        .zip(&b)
        .map(|(x, y)| x.total_cmp(y))
        .find(|o| o.is_ne())
        .unwrap_or(core::cmp::Ordering::Equal);
    if before.is_gt() {
        pts.reverse();
    }
    pts
}

/// `nx` × `ny` frames `px` and `py` apart on `on`'s plane, centred on its
/// origin; `stagger` shifts every other row by half a pitch.
pub fn grid(nx: u32, ny: u32, px: f64, py: f64, stagger: bool, on: &Frame) -> Vec<Frame> {
    let ny = ny.max(1);
    let mut out = Vec::new();
    for j in 0..ny {
        let shift = if stagger && j % 2 == 1 { px / 2.0 } else { 0.0 };
        let centre = if stagger && ny > 1 { px / 4.0 } else { 0.0 };
        for i in 0..nx {
            let x = (f64::from(i) - f64::from(nx - 1) / 2.0) * px + shift - centre;
            let y = (f64::from(j) - f64::from(ny - 1) / 2.0) * py;
            out.push(on.translate(x, y, 0.0));
        }
    }
    out
}

/// `n` frames on a circle of radius `r` round `on`'s normal, from `start`
/// radians (0 on its X), spread over `angle` radians (0 or a full turn:
/// evenly round it; less: first to last inclusive). Each frame's X points
/// outward and its normal is `on`'s.
pub fn ring(n: u32, r: f64, start: f64, angle: f64, on: &Frame) -> Vec<Frame> {
    let full = angle == 0.0 || (angle - trig::TAU).abs() < 1e-12;
    let span = if angle == 0.0 { trig::TAU } else { angle };
    let step = if full {
        span / f64::from(n.max(1))
    } else if n > 1 {
        span / f64::from(n - 1)
    } else {
        0.0
    };
    (0..n)
        .map(|k| {
            let (s, c) = trig::sin_cos(start + step * f64::from(k));
            let radial = on.x * c + on.y * s;
            Frame {
                origin: on.origin + radial * r,
                x: radial,
                y: on.z.cross(radial),
                z: on.z,
            }
        })
        .collect()
}

/// Frames along `path`, each looking the way the path runs: `count` of
/// them evenly from end to end (round a closed path), or, with `count`
/// 0, one every `pitch` from its start.
pub fn along(path: &Path, pitch: f64, count: u32) -> Result<Vec<Frame>, ModelError> {
    let mut pts = path.points.clone();
    if pts.len() < 2 {
        return Err(ModelError::Degenerate("the path".into()));
    }
    if path.closed {
        pts.push(pts[0]);
    }
    let mut lengths = vec![0.0];
    for k in 1..pts.len() {
        lengths.push(lengths[k - 1] + (pts[k] - pts[k - 1]).length());
    }
    let total = lengths[lengths.len() - 1];
    let spots: Vec<f64> = if count > 0 {
        let div = if path.closed {
            f64::from(count)
        } else {
            f64::from(count.max(2) - 1)
        };
        (0..count).map(|k| total * f64::from(k) / div).collect()
    } else if pitch > 0.0 && pitch.is_finite() {
        let n = (total / pitch + 1e-9).floor() as u64;
        (0..=n).map(|k| pitch * k as f64).collect()
    } else {
        return Err(ModelError::BadValue("along: a pitch or a count".into()));
    };
    let mut out = Vec::with_capacity(spots.len());
    let mut j = 0;
    for s in spots {
        while j < pts.len() - 2 && lengths[j + 1] < s {
            j += 1;
        }
        let seg = lengths[j + 1] - lengths[j];
        let f = if seg > 0.0 {
            (s - lengths[j]) / seg
        } else {
            0.0
        };
        let tangent = pts[j + 1] - pts[j];
        let at = pts[j] + tangent * f;
        out.push(
            Frame::facing(at, tangent).ok_or_else(|| ModelError::Degenerate("the path".into()))?,
        );
    }
    Ok(out)
}

/// The point of triangle `abc` nearest `p` (Ericson, *Real-Time Collision
/// Detection* §5.1.5).
fn nearest_on_triangle(p: DVec3, a: DVec3, b: DVec3, c: DVec3) -> DVec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// The distance along `dir` (unit) from `o` to triangle `abc`, if the ray
/// meets it (Möller–Trumbore, both sides).
fn ray_triangle(o: DVec3, dir: DVec3, a: DVec3, b: DVec3, c: DVec3) -> Option<f64> {
    let (e1, e2) = (b - a, c - a);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det == 0.0 {
        return None;
    }
    let t0 = o - a;
    let u = t0.dot(p) / det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t0.cross(e1);
    let v = dir.dot(q) / det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) / det;
    (t >= 0.0).then_some(t)
}

/// Frames on the surface of `targets` (world solids), one per point: at
/// the nearest spot, or, with a `direction`, where a ray from the point
/// along it first meets the surface (points it misses are dropped). Each
/// frame's normal is its triangle's, turned to face the point (or against
/// the ray). Ties go to the lower solid, then the lower triangle.
pub fn snap(targets: &[&Solid], points: &[DVec3], direction: Option<DVec3>) -> Vec<Frame> {
    let dir = direction.and_then(DVec3::try_normalize);
    let mut out = Vec::new();
    for &p in points {
        // (distance, solid, triangle, location)
        let mut best: Option<(f64, usize, usize, DVec3)> = None;
        for (si, s) in targets.iter().enumerate() {
            for ti in 0..s.triangles.len() {
                let [a, b, c] = tri(s, ti);
                let hit = match dir {
                    Some(d) => ray_triangle(p, d, a, b, c).map(|t| (t, p + d * t)),
                    None => {
                        let q = nearest_on_triangle(p, a, b, c);
                        Some(((q - p).length(), q))
                    }
                };
                if let Some((t, q)) = hit
                    && best.is_none_or(|(bt, ..)| t < bt)
                {
                    best = Some((t, si, ti, q));
                }
            }
        }
        let Some((_, si, ti, q)) = best else { continue };
        let mut n = area2(targets[si], ti).normalize_or_zero();
        match dir {
            Some(d) if n.dot(d) > 0.0 => n = -n,
            None if n.dot(p - q) < 0.0 => n = -n,
            _ => {}
        }
        if let Some(f) = Frame::facing(q, n) {
            out.push(f);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use shape_gen::solids::{BoxDesc, cuboid};

    fn block() -> Solid {
        cuboid(&BoxDesc {
            width: 600.0,
            depth: 400.0,
            height: 850.0,
        })
        .unwrap()
    }

    #[test]
    fn sides_parse_and_refuse() {
        assert_eq!(sides("+X-y").unwrap(), vec![(0, 1.0), (1, -1.0)]);
        assert_eq!(sides("z").unwrap(), vec![(2, 1.0)]);
        for bad in ["", "+X+x", "X-Y", "+W", "top"] {
            assert!(sides(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn face_frames_sit_on_the_centroid() {
        let b = block();
        let f = face(&b, "cabinet", "+X").unwrap();
        assert_eq!(f.origin, DVec3::new(300.0, 0.0, 425.0));
        assert_eq!((f.x, f.y, f.z), (DVec3::Y, DVec3::Z, DVec3::X));
        let top = face(&b, "cabinet", "top").unwrap();
        assert_eq!(top.origin, DVec3::new(0.0, 0.0, 850.0));
        match face(&b, "cabinet", "nope") {
            Err(ModelError::NoSuchFace { available, .. }) => {
                assert!(available.contains(&"top".to_string()))
            }
            other => panic!("{other:?}"),
        }
        let corner = bbox(&b.positions, "+X-Y+Z").unwrap();
        assert_eq!(corner.origin, DVec3::new(300.0, -200.0, 850.0));
    }

    #[test]
    fn edge_paths_are_ordered_loops() {
        let b = block();
        let top = edge(&b, "cabinet", "top").unwrap();
        assert!(top.closed);
        assert_eq!(top.points.len(), 4);
        // Anticlockwise from +X seen from above: starts at angle 0 or the
        // first corner after it.
        assert_eq!(top.points[0], DVec3::new(300.0, 200.0, 850.0));
        assert_eq!(top.points[1], DVec3::new(-300.0, 200.0, 850.0));
        assert!(matches!(
            edge(&b, "cabinet", "side"),
            Ok(Path { closed: false, .. })
        ));
    }

    #[test]
    fn distributors() {
        let g = grid(2, 1, 50.0, 0.0, false, &Frame::IDENTITY);
        assert_eq!(g[0].origin.x, -25.0);
        let r = ring(4, 10.0, 0.0, 0.0, &Frame::IDENTITY);
        assert_eq!(r.len(), 4);
        assert!((r[1].origin - DVec3::new(0.0, 10.0, 0.0)).length() < 1e-12);
        assert!((r[1].x - DVec3::Y).length() < 1e-12);
        let path = Path {
            points: vec![DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0)],
            closed: false,
        };
        let a = along(&path, 0.0, 3).unwrap();
        assert_eq!(a[1].origin, DVec3::new(5.0, 0.0, 0.0));
        let b = block();
        let s = snap(&[&b], &[DVec3::new(0.0, 0.0, 2000.0)], Some(DVec3::NEG_Z));
        assert_eq!(s[0].origin, DVec3::new(0.0, 0.0, 850.0));
        assert_eq!(s[0].z, DVec3::Z);
        let n = snap(&[&b], &[DVec3::new(10.0, 0.0, 900.0)], None);
        assert_eq!(n[0].origin, DVec3::new(10.0, 0.0, 850.0));
    }
}
