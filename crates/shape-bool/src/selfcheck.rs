//! Exact self-intersection tests on a triangle mesh: which pairs of its
//! triangles meet where the surface does not join them.
//!
//! Entry: [`self_crossings`]. Every side is decided by Shewchuk's
//! predicates (`orient3d`, `orient2d`); no tolerance is used. Pairs to test
//! come from a sweep along x over the triangles' boxes, in a fixed order.
//!
//! Two triangles of a closed surface cross improperly when:
//! - they share no vertex and still meet (touching counts);
//! - they share one vertex and meet anywhere else;
//! - they share an edge, lie in one plane and overlap (a fold flat onto
//!   itself);
//! - they share all three corners.
//!
//! Degenerate triangles (three corners in a line) are left out; callers
//! count them on their own. `shape-tools`' measure and repair use the same
//! tests ([`touching_pairs`], [`degenerate`]).

use core::cmp::Ordering;

use shape_core::predicates::{orient2d, orient3d};
use shape_core::{DVec2, DVec3};

/// The triangle's normal, each component its signed area in one axis plane
/// by `orient2d`'s exact sign: the axis with a non-zero area is never
/// missed.
fn normal_axis(t: [DVec3; 3]) -> usize {
    let area = |f: fn(DVec3) -> DVec2| {
        let [a, b, c] = t.map(f);
        robust::orient2d(
            robust::Coord { x: a.x, y: a.y },
            robust::Coord { x: b.x, y: b.y },
            robust::Coord { x: c.x, y: c.y },
        )
        .abs()
    };
    let n = [area(yz), area(zx), area(xy)];
    if n[0] >= n[1] && n[0] >= n[2] {
        0
    } else if n[1] >= n[2] {
        1
    } else {
        2
    }
}

fn yz(p: DVec3) -> DVec2 {
    DVec2::new(p.y, p.z)
}

fn zx(p: DVec3) -> DVec2 {
    DVec2::new(p.z, p.x)
}

fn xy(p: DVec3) -> DVec2 {
    DVec2::new(p.x, p.y)
}

/// The projection that drops the axis `t`'s normal leans along most.
fn projection(t: [DVec3; 3]) -> fn(DVec3) -> DVec2 {
    match normal_axis(t) {
        0 => yz,
        1 => zx,
        _ => xy,
    }
}

/// Closed segments `a b` and `c d` share a point.
fn segments_meet(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let (o1, o2) = (orient2d(a, b, c), orient2d(a, b, d));
    let (o3, o4) = (orient2d(c, d, a), orient2d(c, d, b));
    let within = |p: DVec2, q: DVec2, r: DVec2| {
        r.x >= p.x.min(q.x) && r.x <= p.x.max(q.x) && r.y >= p.y.min(q.y) && r.y <= p.y.max(q.y)
    };
    let eq = Ordering::Equal;
    if o1 != eq && o2 != eq && o3 != eq && o4 != eq {
        return o1 != o2 && o3 != o4;
    }
    // An endpoint on the other's line meets it only inside its span.
    (o1 == eq && within(a, b, c))
        || (o2 == eq && within(a, b, d))
        || (o3 == eq && within(c, d, a))
        || (o4 == eq && within(c, d, b))
}

/// Point `p` lies inside or on the triangle `t` (either winding).
fn point_in_tri(p: DVec2, t: [DVec2; 3]) -> bool {
    let o = [
        orient2d(t[0], t[1], p),
        orient2d(t[1], t[2], p),
        orient2d(t[2], t[0], p),
    ];
    !(o.contains(&Ordering::Greater) && o.contains(&Ordering::Less))
}

/// Two coplanar triangles, projected, share a point.
fn coplanar_meet(a: [DVec2; 3], b: [DVec2; 3]) -> bool {
    for i in 0..3 {
        for j in 0..3 {
            if segments_meet(a[i], a[(i + 1) % 3], b[j], b[(j + 1) % 3]) {
                return true;
            }
        }
    }
    point_in_tri(a[0], b) || point_in_tri(b[0], a)
}

/// The closed segment `p q` meets the closed triangle `t`.
fn segment_meets_tri(p: DVec3, q: DVec3, t: [DVec3; 3]) -> bool {
    let sp = orient3d(t[0], t[1], t[2], p);
    let sq = orient3d(t[0], t[1], t[2], q);
    if sp == sq && sp != Ordering::Equal {
        return false;
    }
    if sp == Ordering::Equal && sq == Ordering::Equal {
        let f = projection(t);
        let tt = t.map(f);
        let (fp, fq) = (f(p), f(q));
        return point_in_tri(fp, tt)
            || point_in_tri(fq, tt)
            || (0..3).any(|k| segments_meet(fp, fq, tt[k], tt[(k + 1) % 3]));
    }
    let o = [
        orient3d(p, q, t[0], t[1]),
        orient3d(p, q, t[1], t[2]),
        orient3d(p, q, t[2], t[0]),
    ];
    !(o.contains(&Ordering::Greater) && o.contains(&Ordering::Less))
}

/// The triangle has three corners in a line (zero area), exactly.
pub fn degenerate(t: [DVec3; 3]) -> bool {
    orient2d(xy(t[0]), xy(t[1]), xy(t[2])) == Ordering::Equal
        && orient2d(yz(t[0]), yz(t[1]), yz(t[2])) == Ordering::Equal
        && orient2d(zx(t[0]), zx(t[1]), zx(t[2])) == Ordering::Equal
}

/// Two non-degenerate triangles share a point (touching counts).
pub fn tris_meet(a: [DVec3; 3], b: [DVec3; 3]) -> bool {
    let sb = b.map(|p| orient3d(a[0], a[1], a[2], p));
    if sb[0] == sb[1] && sb[1] == sb[2] {
        if sb[0] != Ordering::Equal {
            return false;
        }
        let f = projection(a);
        return coplanar_meet(a.map(f), b.map(f));
    }
    let sa = a.map(|p| orient3d(b[0], b[1], b[2], p));
    if sa[0] == sa[1] && sa[1] == sa[2] && sa[0] != Ordering::Equal {
        return false;
    }
    (0..3).any(|k| segment_meets_tri(a[k], a[(k + 1) % 3], b))
        || (0..3).any(|k| segment_meets_tri(b[k], b[(k + 1) % 3], a))
}

/// `p`, in the plane of the triangle `s a1 a2`, lies in the closed corner
/// of that triangle at `s` (on the ray towards `a1` or `a2` counts, the
/// rays back from `s` do not).
fn in_corner(s: DVec3, a1: DVec3, a2: DVec3, p: DVec3) -> bool {
    let f = projection([s, a1, a2]);
    let (s, a1, a2, p) = (f(s), f(a1), f(a2), f(p));
    let turn = orient2d(s, a1, a2);
    let o1 = orient2d(s, a1, p);
    let o2 = orient2d(s, a2, p);
    (o1 == turn || o1 == Ordering::Equal) && (o2 == turn.reverse() || o2 == Ordering::Equal)
}

/// Triangles `s a1 a2` and `s b1 b2`, sharing only the corner `s`, meet
/// somewhere else too.
fn shared_corner_cross(s: DVec3, a: [DVec3; 2], b: [DVec3; 2]) -> bool {
    let ob = b.map(|p| orient3d(s, a[0], a[1], p));
    let eq = Ordering::Equal;
    if ob == [eq, eq] {
        // One plane: the two corners at `s` overlap.
        return in_corner(s, a[0], a[1], b[0])
            || in_corner(s, a[0], a[1], b[1])
            || in_corner(s, b[0], b[1], a[0])
            || in_corner(s, b[0], b[1], a[1]);
    }
    if ob[0] == ob[1] {
        // b leans off a's plane on one side, touching it at `s` only.
        return false;
    }
    let oa = a.map(|p| orient3d(s, b[0], b[1], p));
    if oa[0] == oa[1] && oa[0] != eq {
        return false;
    }
    // The two meet along a segment of the planes' common line from `s`.
    // Its far end lies on an edge of one: the edge facing `s`, or an edge
    // from `s` lying in the other's plane.
    let ta = [s, a[0], a[1]];
    let tb = [s, b[0], b[1]];
    segment_meets_tri(b[0], b[1], ta)
        || segment_meets_tri(a[0], a[1], tb)
        || (0..2).any(|k| ob[k] == eq && in_corner(s, a[0], a[1], b[k]))
        || (0..2).any(|k| oa[k] == eq && in_corner(s, b[0], b[1], a[k]))
}

/// Two non-degenerate triangles, with their vertex ids (welded by exact
/// position), cross where the surface does not join them (see the
/// module's list).
pub fn crossing(ia: [u32; 3], a: [DVec3; 3], ib: [u32; 3], b: [DVec3; 3]) -> bool {
    let mut shared = [(0, 0); 3];
    let mut n = 0;
    for (i, x) in ia.iter().enumerate() {
        for (j, y) in ib.iter().enumerate() {
            if x == y && n < 3 {
                shared[n] = (i, j);
                n += 1;
            }
        }
    }
    match &shared[..n] {
        [] => tris_meet(a, b),
        &[(i, j)] => shared_corner_cross(
            a[i],
            [a[(i + 1) % 3], a[(i + 2) % 3]],
            [b[(j + 1) % 3], b[(j + 2) % 3]],
        ),
        &[(i0, j0), (i1, j1)] => {
            // The corners off the shared edge `u v`.
            let (u, v) = (a[i0], a[i1]);
            let w = a[3 - i0 - i1];
            let x = b[3 - j0 - j1];
            if orient3d(u, v, w, x) != Ordering::Equal {
                return false;
            }
            let f = projection(a);
            orient2d(f(u), f(v), f(w)) == orient2d(f(u), f(v), f(x))
        }
        _ => true,
    }
}

/// Pairs `(i, j)`, `i < j`, of non-degenerate triangles whose boxes
/// overlap, sorted. A uniform grid of cells about a triangle's size: each
/// triangle is listed in the cells its box covers, and a pair is taken in
/// one cell only, the one holding the low corner of the two boxes'
/// overlap. A triangle covering very many cells is tested against every
/// other instead.
pub fn box_pairs(positions: &[DVec3], triangles: &[[u32; 3]]) -> Vec<(u32, u32)> {
    let boxes: Vec<(DVec3, DVec3)> = triangles
        .iter()
        .map(|t| {
            let p = t.map(|v| positions[v as usize]);
            (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]))
        })
        .collect();
    let live: Vec<u32> = (0..triangles.len() as u32)
        .filter(|&i| !degenerate(triangles[i as usize].map(|v| positions[v as usize])))
        .collect();
    if live.is_empty() {
        return Vec::new();
    }
    let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
    let mut extent = 0.0;
    for &i in &live {
        let (a, b) = boxes[i as usize];
        lo = lo.min(a);
        hi = hi.max(b);
        extent += (b - a).max_element();
    }
    // Cells a mean triangle across, at most 2^20 a side.
    const SIDE: f64 = (1u64 << 20) as f64;
    let span = (hi - lo).max_element();
    let mut h = extent / live.len() as f64 * 2.0;
    if h <= 0.0 || !h.is_finite() || span / h > SIDE - 2.0 {
        h = (span / (SIDE - 2.0)).max(f64::MIN_POSITIVE);
    }
    // Truncation is the floor here: every coordinate is at least `lo`.
    let cell = |p: DVec3| {
        let c = (p - lo) / h;
        [c.x as u64, c.y as u64, c.z as u64].map(|x| x.min(SIDE as u64 - 1))
    };
    let key = |c: [u64; 3]| (c[0] << 40) | (c[1] << 20) | c[2];
    const MAX_CELLS: u64 = 1 << 12;
    let mut entries: Vec<(u64, u32)> = Vec::new();
    let mut big: Vec<u32> = Vec::new();
    // Each triangle's lowest cell: the cell of a pair's overlap corner is
    // the larger of theirs on each axis (`cell` is monotone).
    let mut low = vec![[0u64; 3]; triangles.len()];
    for &i in &live {
        let (a, b) = boxes[i as usize];
        let (ca, cb) = (cell(a), cell(b));
        low[i as usize] = ca;
        let count = (0..3).map(|k| cb[k] - ca[k] + 1).product::<u64>();
        if count > MAX_CELLS {
            big.push(i);
            continue;
        }
        for x in ca[0]..=cb[0] {
            for y in ca[1]..=cb[1] {
                for z in ca[2]..=cb[2] {
                    entries.push((key([x, y, z]), i));
                }
            }
        }
    }
    entries.sort_unstable();
    let overlap = |i: u32, j: u32| {
        let ((a0, a1), (b0, b1)) = (boxes[i as usize], boxes[j as usize]);
        a0.cmple(b1).all() && b0.cmple(a1).all()
    };
    let mut out = Vec::new();
    let mut start = 0;
    while start < entries.len() {
        let k = entries[start].0;
        let mut end = start;
        while end < entries.len() && entries[end].0 == k {
            end += 1;
        }
        for x in start..end {
            for y in x + 1..end {
                let (i, j) = (entries[x].1, entries[y].1);
                let (li, lj) = (low[i as usize], low[j as usize]);
                let corner = [li[0].max(lj[0]), li[1].max(lj[1]), li[2].max(lj[2])];
                if key(corner) == k && overlap(i, j) {
                    out.push(pack(i, j));
                }
            }
        }
        start = end;
    }
    // Large triangles against every other (each pair once).
    let is_big = |i: u32| big.binary_search(&i).is_ok();
    for &i in &big {
        for &j in &live {
            if j != i && (!is_big(j) || j > i) && overlap(i, j) {
                out.push(pack(i, j));
            }
        }
    }
    out.sort_unstable();
    out.into_iter()
        .map(|p| ((p >> 32) as u32, p as u32))
        .collect()
}

/// A pair `(min, max)` as one sortable number.
fn pack(i: u32, j: u32) -> u64 {
    (u64::from(i.min(j)) << 32) | u64::from(i.max(j))
}

/// Pairs `(i, j)`, `i < j`, of triangles that share no vertex index and
/// still meet: a surface passing through (or touching) itself.
/// Degenerate triangles are left out. This is what `shape-tools`' measure
/// and repair report.
pub fn touching_pairs(positions: &[DVec3], triangles: &[[u32; 3]]) -> Vec<(u32, u32)> {
    box_pairs(positions, triangles)
        .into_iter()
        .filter(|&(i, j)| {
            let (ti, tj) = (triangles[i as usize], triangles[j as usize]);
            !ti.iter().any(|v| tj.contains(v))
                && tris_meet(
                    ti.map(|v| positions[v as usize]),
                    tj.map(|v| positions[v as usize]),
                )
        })
        .collect()
}

/// Pairs `(i, j)`, `i < j`, of triangles that cross where the surface does
/// not join them ([`crossing`]): [`touching_pairs`] plus pairs sharing a
/// vertex or an edge that meet improperly. Vertex ids are taken as given;
/// weld by exact position first. Degenerate triangles are left out.
pub fn self_crossings(positions: &[DVec3], triangles: &[[u32; 3]]) -> Vec<(u32, u32)> {
    box_pairs(positions, triangles)
        .into_iter()
        .filter(|&(i, j)| {
            let (ti, tj) = (triangles[i as usize], triangles[j as usize]);
            crossing(
                ti,
                ti.map(|v| positions[v as usize]),
                tj,
                tj.map(|v| positions[v as usize]),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64, z: f64) -> DVec3 {
        DVec3::new(x, y, z)
    }

    #[test]
    fn exact_tests_separate_touching_from_crossing() {
        let a = [v(0.0, 0.0, 0.0), v(2.0, 0.0, 0.0), v(0.0, 2.0, 0.0)];
        // Pierces a.
        let b = [v(0.5, 0.5, -1.0), v(0.5, 0.5, 1.0), v(3.0, 3.0, 0.0)];
        assert!(tris_meet(a, b));
        // Above a, apart.
        let c = [v(0.5, 0.5, 1.0), v(1.0, 0.5, 1.0), v(0.5, 1.0, 2.0)];
        assert!(!tris_meet(a, c));
        // Coplanar, overlapping.
        let d = [v(0.5, 0.5, 0.0), v(3.0, 0.5, 0.0), v(0.5, 3.0, 0.0)];
        assert!(tris_meet(a, d));
        assert!(degenerate([
            v(0.0, 0.0, 0.0),
            v(1.0, 1.0, 1.0),
            v(3.0, 3.0, 3.0)
        ]));
    }

    #[test]
    fn shared_corners_cross_only_where_they_overlap() {
        let s = v(0.0, 0.0, 0.0);
        let a = [s, v(2.0, 0.0, 0.0), v(0.0, 2.0, 0.0)];
        let ia = [0, 1, 2];
        // A fan neighbour across the shared corner: apart.
        let b = [s, v(-1.0, -0.5, 0.0), v(-0.5, -1.0, 0.0)];
        assert!(!crossing(ia, a, [0, 3, 4], b));
        // Coplanar, its corner inside a's: overlapping.
        let b = [s, v(1.0, 0.5, 0.0), v(-1.0, -1.0, 0.0)];
        assert!(crossing(ia, a, [0, 3, 4], b));
        // Off the plane, leaning away: touching at s only.
        let b = [s, v(1.0, 1.0, 1.0), v(-1.0, 0.5, 2.0)];
        assert!(!crossing(ia, a, [0, 3, 4], b));
        // Through the plane, inside a's corner: crossing along a segment.
        let b = [s, v(1.0, 0.5, 1.0), v(0.5, 1.0, -1.0)];
        assert!(crossing(ia, a, [0, 3, 4], b));
        // Through the plane, outside a's corner: the common line misses a.
        let b = [s, v(-1.0, -0.5, 1.0), v(-0.5, -1.0, -1.0)];
        assert!(!crossing(ia, a, [0, 3, 4], b));
        // An edge lying in a's plane, inside its corner.
        let b = [s, v(0.5, 0.5, 0.0), v(0.0, 0.0, 1.0)];
        assert!(crossing(ia, a, [0, 3, 4], b));
        // Sharing an edge: a hinge is fine, a flat fold is not.
        let hinge = [v(2.0, 0.0, 0.0), v(0.0, 2.0, 0.0), v(2.0, 2.0, 1.0)];
        assert!(!crossing(ia, a, [1, 2, 5], hinge));
        let fold = [v(2.0, 0.0, 0.0), v(0.0, 2.0, 0.0), v(0.5, 0.5, 0.0)];
        assert!(crossing(ia, a, [1, 2, 5], fold));
        let flat = [v(2.0, 0.0, 0.0), v(0.0, 2.0, 0.0), v(2.0, 2.0, 0.0)];
        assert!(!crossing(ia, a, [1, 2, 5], flat));
    }
}
