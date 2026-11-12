//! The inner solid of a convex solid's shell, from its planes: [`inset`].
//!
//! `shell` moves each vertex of a copy inward. Where a face is narrower
//! than the wall (a chamfer, a fillet's strips) that copy folds through
//! itself (`docs/research.md` §27). A convex solid needs no copy: its
//! inner solid is the space inside every plane moved by the thickness, and
//! a face that shrinks to nothing is a plane that bounds none of it.
//!
//! That space is found through its dual: about a point inside it, a plane
//! `n · x = h` is the point `n / h`, and the planes that bound the space
//! are the corners of those points' convex hull (`shape_gen::hull`, exact
//! on the points it is given). Each flat side of the hull is one corner of
//! the inner solid, where its planes meet.

use core::cmp::Ordering;
use std::collections::BTreeMap;

use shape_core::{DVec2, DVec3};
use shape_gen::hull::convex_hull;
use shape_mesh::Solid;

/// More triangles than this and the solid is left to the caller's other
/// ways: planes are gathered pairwise.
const MAX_TRIANGLES: usize = 8_000;

struct Plane {
    n: DVec3,
    d: f64,
    /// How far the plane moves along its normal (the furthest out of its
    /// triangles').
    target: f64,
    /// The largest triangle on it: its surface and UVs stand for the plane.
    rep: usize,
    area: f64,
}

/// What [`inset`] found.
pub(crate) enum Inset {
    /// Not a convex solid, or one this does not take on: nothing learned.
    Unknown,
    /// Convex, and no point inside is `t` from every kept face: walls
    /// that thick meet. The deepest point is this far short of it.
    NoRoom(f64),
    /// The inner solid.
    Inner(Box<Solid>),
}

/// The inner solid of `s`: the space inside every plane moved along its
/// normal by its triangles' `targets` (the wall in, for a kept face; out,
/// for an open one). A `free` triangle (a patch over a hole, `pieces`)
/// has no plane of its own and is left out; `caps` are planes that bound
/// the space in its stead. Faces are tagged `wall`, on
/// the surface of the outer face they follow, with its UVs carried along
/// the plane if `s` has any.
pub(crate) fn inset(
    s: &Solid,
    targets: &[f64],
    free: &[bool],
    caps: &[(DVec3, f64, usize)],
    diag: f64,
) -> Inset {
    if s.triangles.is_empty() || s.triangles.len() > MAX_TRIANGLES {
        return Inset::Unknown;
    }
    let tol = diag * 1e-9;
    let mut planes: Vec<Plane> = Vec::new();
    for (i, tri) in s.triangles.iter().enumerate() {
        if free.get(i) == Some(&true) {
            continue;
        }
        let [a, b, c] = tri.map(|v| s.positions[v as usize]);
        let an = (b - a).cross(c - a);
        let area = an.length();
        // A needle (a boolean leaves triangles 1e-15 wide) has no plane
        // to speak of; its corners lie on its neighbours'.
        let longest = (b - a)
            .length_squared()
            .max((c - b).length_squared())
            .max((a - c).length_squared());
        if area <= 1e-9 * longest {
            continue;
        }
        let n = an / area;
        let d = n.dot(a);
        // Convex, but for rounding: no vertex outside any face's plane.
        // (A bevel's corners are rounded, so an exact test would refuse
        // them; what is built from the planes is checked exactly by the
        // caller.)
        if s.positions.iter().any(|&p| n.dot(p) - d > tol) {
            return Inset::Unknown;
        }
        match planes
            .iter_mut()
            .find(|p| p.n.dot(n) > 1.0 - 1e-12 && (p.d - d).abs() <= tol)
        {
            Some(p) => {
                p.target = p.target.max(targets[i]);
                if area > p.area {
                    p.rep = i;
                    p.area = area;
                }
            }
            None => planes.push(Plane {
                n,
                d,
                target: targets[i],
                rep: i,
                area,
            }),
        }
    }
    // Caps: planes that bound the space where free triangles stood (a
    // pocket's mouth), as they are, each `n · x = d` with the triangle
    // whose surface and UVs its face takes.
    for &(n, d, rep) in caps {
        if s.positions.iter().any(|&p| n.dot(p) - d > tol) {
            return Inset::Unknown;
        }
        planes.push(Plane {
            n,
            d,
            target: 0.0,
            rep,
            area: 0.0,
        });
    }
    let off: Vec<f64> = planes.iter().map(|p| p.d + p.target).collect();

    // The point deepest inside every moved plane: from the middle of the
    // vertices, step away from the nearest plane, ever shorter, keeping
    // the best seen; then again from the best with steps a sixteenth as
    // long, down to steps of 10⁻⁸ of the solid's size.
    let nearest = |c: DVec3| {
        let mut worst = (0usize, f64::INFINITY);
        for (i, p) in planes.iter().enumerate() {
            let h = off[i] - p.n.dot(c);
            if h < worst.1 {
                worst = (i, h);
            }
        }
        worst
    };
    let middle = s.positions.iter().copied().sum::<DVec3>() / s.positions.len() as f64;
    let mut best = (middle, f64::NEG_INFINITY);
    let mut scale = diag;
    for _ in 0..5 {
        let mut c = best.0;
        for k in 0..256u32 {
            let (i, h) = nearest(c);
            if h > best.1 {
                best = (c, h);
            }
            c -= planes[i].n * (scale / f64::from(8 + k));
        }
        scale /= 16.0;
    }
    let (c, room) = best;
    // Room for less than a millionth of the solid's size is none.
    if room <= diag * 1e-6 {
        return Inset::NoRoom((-room).max(0.0));
    }
    match inner(s, &planes, &off, c, tol) {
        Some(i) => Inset::Inner(Box::new(i)),
        None => Inset::Unknown,
    }
}

/// The space inside the planes `n · x = h`, about a point `c` inside
/// them all, as a closed solid of one surface; `None` if it does not
/// come out one (the planes do not close a space).
pub(crate) fn space(planes: &[(DVec3, f64)], c: DVec3, tol: f64) -> Option<Solid> {
    // A triangle to stand for every plane's surface.
    let mut stand = Solid::default();
    for p in [DVec3::ZERO, DVec3::X, DVec3::Y] {
        stand.push_vertex(p);
    }
    stand.push_triangle([0, 1, 2], 0, &["wall"]);
    let list: Vec<Plane> = planes
        .iter()
        .map(|&(n, d)| Plane {
            n,
            d,
            target: 0.0,
            rep: 0,
            area: 0.0,
        })
        .collect();
    let off: Vec<f64> = planes.iter().map(|p| p.1).collect();
    inner(&stand, &list, &off, c, tol)
}

/// The space inside the planes moved to `off`, about a point `c` inside
/// them all. `None` if it does not come out a closed solid.
fn inner(s: &Solid, planes: &[Plane], off: &[f64], c: DVec3, tol: f64) -> Option<Solid> {
    // The planes' dual points, and which plane each is.
    let key = |q: DVec3| DVec3::new(q.x + 0.0, q.y + 0.0, q.z + 0.0);
    let cmp = |a: &DVec3, b: &DVec3| {
        a.x.total_cmp(&b.x)
            .then(a.y.total_cmp(&b.y))
            .then(a.z.total_cmp(&b.z))
    };
    let mut dual: Vec<(DVec3, usize)> = planes
        .iter()
        .enumerate()
        .map(|(i, p)| (key(p.n / (off[i] - p.n.dot(c))), i))
        .collect();
    dual.sort_by(|a, b| cmp(&a.0, &b.0).then(a.1.cmp(&b.1)));
    let points: Vec<DVec3> = dual.iter().map(|d| d.0).collect();
    let hull = convex_hull(&points).ok()?;
    let plane_of: Vec<usize> = hull
        .positions
        .iter()
        .map(|q| {
            let at = dual.partition_point(|d| cmp(&d.0, q) == Ordering::Less);
            dual.get(at)
                .filter(|d| cmp(&d.0, q) == Ordering::Equal)
                .map(|d| d.1)
        })
        .collect::<Option<_>>()?;

    // One corner per flat side of the hull: where its planes meet.
    let mut sides: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (k, tri) in hull.triangles.iter().enumerate() {
        let side = sides.entry(hull.surfaces[k]).or_default();
        for &v in tri {
            side.push(plane_of[v as usize]);
        }
    }
    let mut corner_of: BTreeMap<u32, usize> = BTreeMap::new();
    let mut corners: Vec<DVec3> = Vec::new();
    for (&id, on) in &mut sides {
        on.sort_unstable();
        on.dedup();
        let rows: Vec<(DVec3, f64)> = on
            .iter()
            .map(|&i| (planes[i].n, off[i] - planes[i].n.dot(c)))
            .collect();
        corner_of.insert(id, corners.len());
        corners.push(c + meet(&rows)?);
    }
    // Corners that rounding left a hair apart are one.
    let mut same: Vec<usize> = (0..corners.len()).collect();
    for i in 0..corners.len() {
        if let Some(j) = (0..i).find(|&j| same[j] == j && (corners[i] - corners[j]).length() <= tol)
        {
            same[i] = j;
        }
    }

    // One face per corner of the hull: the corners of the sides round it.
    let mut fans: BTreeMap<u32, BTreeMap<u32, (u32, u32)>> = BTreeMap::new();
    for (k, tri) in hull.triangles.iter().enumerate() {
        for r in 0..3 {
            let (v, a, b) = (tri[r], tri[(r + 1) % 3], tri[(r + 2) % 3]);
            fans.entry(v).or_default().insert(a, (b, hull.surfaces[k]));
        }
    }
    let uvs = s.uvs.as_ref();
    let mut out = Solid::default();
    let mut index: BTreeMap<usize, u32> = BTreeMap::new();
    for (&v, fan) in &fans {
        let plane = &planes[plane_of[v as usize]];
        let start = *fan.keys().next()?;
        let mut ring: Vec<usize> = Vec::new();
        let mut at = start;
        for _ in 0..fan.len() {
            let &(next, side) = fan.get(&at)?;
            let corner = same[corner_of[&side]];
            if ring.last() != Some(&corner) {
                ring.push(corner);
            }
            at = next;
        }
        if at != start {
            return None;
        }
        if ring.len() > 1 && ring.first() == ring.last() {
            ring.pop();
        }
        if ring.len() < 3 {
            continue;
        }
        // Counter-clockwise seen from outside the inner solid.
        let mut normal = DVec3::ZERO;
        for k in 0..ring.len() {
            let (p, q) = (corners[ring[k]], corners[ring[(k + 1) % ring.len()]]);
            normal += (p - c).cross(q - c);
        }
        if normal.dot(plane.n) < 0.0 {
            ring.reverse();
        }
        let ids: Vec<u32> = ring
            .iter()
            .map(|&k| {
                *index
                    .entry(k)
                    .or_insert_with(|| out.push_vertex(corners[k]))
            })
            .collect();
        let rep = plane.rep;
        let surface = s.surfaces[rep];
        for k in 1..ids.len() - 1 {
            let tri = [ids[0], ids[k], ids[k + 1]];
            match uvs {
                Some(uvs) => {
                    let at = s.triangles[rep].map(|v| s.positions[v as usize]);
                    let uv =
                        [ring[0], ring[k], ring[k + 1]].map(|k| along(at, uvs[rep], corners[k]));
                    out.push_triangle_uv(tri, surface, &["wall"], uv);
                }
                None => out.push_triangle(tri, surface, &["wall"]),
            }
        }
    }
    out.canonicalize().ok()?;
    let m = out.measure();
    (m.closed && m.volume > 0.0).then_some(out)
}

/// The point where the planes `n · x = h` meet: of three, exactly theirs;
/// of more (they meet in one point but for rounding), the nearest to all.
fn meet(rows: &[(DVec3, f64)]) -> Option<DVec3> {
    let det3 = |m: &[[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let (m, r, least) = if rows.len() == 3 {
        (
            [0, 1, 2].map(|i| rows[i].0.to_array()),
            [rows[0].1, rows[1].1, rows[2].1],
            1e-9,
        )
    } else {
        let mut m = [[0.0f64; 3]; 3];
        let mut r = [0.0f64; 3];
        for &(n, h) in rows {
            let n = n.to_array();
            for i in 0..3 {
                r[i] += h * n[i];
                for j in 0..3 {
                    m[i][j] += n[i] * n[j];
                }
            }
        }
        (m, r, 1e-18)
    };
    let det = det3(&m);
    if det.abs() < least {
        return None;
    }
    let col = |k: usize| {
        let mut mk = m;
        for i in 0..3 {
            mk[i][k] = r[i];
        }
        det3(&mk) / det
    };
    Some(DVec3::new(col(0), col(1), col(2)))
}

/// The texture coordinate at `p`, carried along the plane of the triangle
/// `at` with corner coordinates `uv`.
pub(crate) fn along(at: [DVec3; 3], uv: [DVec2; 3], p: DVec3) -> DVec2 {
    let (e1, e2, w) = (at[1] - at[0], at[2] - at[0], p - at[0]);
    let (d11, d12, d22) = (e1.dot(e1), e1.dot(e2), e2.dot(e2));
    let det = d11 * d22 - d12 * d12;
    if det == 0.0 {
        return uv[0];
    }
    let (w1, w2) = (w.dot(e1), w.dot(e2));
    let b = (w1 * d22 - w2 * d12) / det;
    let g = (w2 * d11 - w1 * d12) / det;
    uv[0] + (uv[1] - uv[0]) * b + (uv[2] - uv[0]) * g
}
