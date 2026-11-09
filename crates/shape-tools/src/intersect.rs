//! Exact intersection tests between triangles, in 3D (self-intersections
//! of a surface) and in 2D (overlapping UV triangles). Every side is
//! decided by Shewchuk's predicates; no tolerance is used.

use core::cmp::Ordering;

use shape_core::predicates::{orient2d, orient3d};
use shape_core::{DVec2, DVec3};

fn drop_axis(n: DVec3) -> impl Fn(DVec3) -> DVec2 {
    let a = n.abs();
    let axis = if a.x >= a.y && a.x >= a.z {
        0
    } else if a.y >= a.z {
        1
    } else {
        2
    };
    move |p: DVec3| match axis {
        0 => DVec2::new(p.y, p.z),
        1 => DVec2::new(p.z, p.x),
        _ => DVec2::new(p.x, p.y),
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
        let f = drop_axis((t[1] - t[0]).cross(t[2] - t[0]));
        let tt = t.map(&f);
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
pub(crate) fn degenerate(t: [DVec3; 3]) -> bool {
    let xy = |p: DVec3| DVec2::new(p.x, p.y);
    let yz = |p: DVec3| DVec2::new(p.y, p.z);
    let zx = |p: DVec3| DVec2::new(p.z, p.x);
    orient2d(xy(t[0]), xy(t[1]), xy(t[2])) == Ordering::Equal
        && orient2d(yz(t[0]), yz(t[1]), yz(t[2])) == Ordering::Equal
        && orient2d(zx(t[0]), zx(t[1]), zx(t[2])) == Ordering::Equal
}

/// Two non-degenerate triangles share a point (touching counts).
pub(crate) fn tris_meet(a: [DVec3; 3], b: [DVec3; 3]) -> bool {
    let sb = b.map(|p| orient3d(a[0], a[1], a[2], p));
    if sb[0] == sb[1] && sb[1] == sb[2] {
        if sb[0] != Ordering::Equal {
            return false;
        }
        let f = drop_axis((a[1] - a[0]).cross(a[2] - a[0]));
        return coplanar_meet(a.map(&f), b.map(&f));
    }
    let sa = a.map(|p| orient3d(b[0], b[1], b[2], p));
    if sa[0] == sa[1] && sa[1] == sa[2] && sa[0] != Ordering::Equal {
        return false;
    }
    (0..3).any(|k| segment_meets_tri(a[k], a[(k + 1) % 3], b))
        || (0..3).any(|k| segment_meets_tri(b[k], b[(k + 1) % 3], a))
}

/// Pairs of triangles `(i, j)`, `i < j`, that share no vertex index and
/// still meet: a surface passing through (or touching) itself. Degenerate
/// triangles are left out (they are counted on their own). A sweep along
/// x over the triangles' boxes picks the pairs to test, in a fixed order.
pub(crate) fn self_intersections(positions: &[DVec3], triangles: &[[u32; 3]]) -> Vec<(u32, u32)> {
    let boxes: Vec<(DVec3, DVec3)> = triangles
        .iter()
        .map(|t| {
            let p = t.map(|v| positions[v as usize]);
            (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]))
        })
        .collect();
    let live: Vec<bool> = triangles
        .iter()
        .map(|t| !degenerate(t.map(|v| positions[v as usize])))
        .collect();
    let mut order: Vec<u32> = (0..triangles.len() as u32)
        .filter(|&i| live[i as usize])
        .collect();
    order.sort_by(|&i, &j| {
        shape_core::order::cmp_f64(boxes[i as usize].0.x, boxes[j as usize].0.x).then(i.cmp(&j))
    });
    let mut out = Vec::new();
    let mut active: Vec<u32> = Vec::new();
    for &i in &order {
        let (lo, hi) = boxes[i as usize];
        active.retain(|&j| boxes[j as usize].1.x >= lo.x);
        let ti = triangles[i as usize];
        for &j in &active {
            let (lo2, hi2) = boxes[j as usize];
            if lo2.y > hi.y || hi2.y < lo.y || lo2.z > hi.z || hi2.z < lo.z {
                continue;
            }
            let tj = triangles[j as usize];
            if ti.iter().any(|v| tj.contains(v)) {
                continue;
            }
            let a = ti.map(|v| positions[v as usize]);
            let b = tj.map(|v| positions[v as usize]);
            if tris_meet(a, b) {
                out.push((i.min(j), i.max(j)));
            }
        }
        active.push(i);
    }
    out.sort_unstable();
    out
}

/// Two 2D triangles overlap in a region of positive area (touching along
/// an edge or at a point does not count). Zero-area triangles overlap
/// nothing.
pub(crate) fn tris_overlap_2d(a: [DVec2; 3], b: [DVec2; 3]) -> bool {
    let ccw = |t: [DVec2; 3]| match orient2d(t[0], t[1], t[2]) {
        Ordering::Greater => Some(t),
        Ordering::Less => Some([t[0], t[2], t[1]]),
        Ordering::Equal => None,
    };
    let (Some(a), Some(b)) = (ccw(a), ccw(b)) else {
        return false;
    };
    // Separating axis: an edge of either with the other wholly on its
    // outer side (or on it).
    let separates = |t: [DVec2; 3], o: [DVec2; 3]| {
        (0..3).any(|k| {
            o.iter()
                .all(|&p| orient2d(t[k], t[(k + 1) % 3], p) != Ordering::Greater)
        })
    };
    !(separates(a, b) || separates(b, a))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_tests_separate_touching_from_crossing() {
        let v = DVec3::new;
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
        let w = DVec2::new;
        let s = [w(0.0, 0.0), w(1.0, 0.0), w(0.0, 1.0)];
        // Shares an edge: no overlap.
        assert!(!tris_overlap_2d(s, [w(1.0, 0.0), w(1.0, 1.0), w(0.0, 1.0)]));
        assert!(tris_overlap_2d(s, [w(0.2, 0.2), w(1.0, 1.0), w(0.0, 1.0)]));
        assert!(degenerate([
            v(0.0, 0.0, 0.0),
            v(1.0, 1.0, 1.0),
            v(3.0, 3.0, 3.0)
        ]));
    }
}
