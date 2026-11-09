//! Exact self-intersection test for a triangle mesh.
//!
//! Entry: [`self_intersections`]. Two triangles that share no vertex must
//! not meet: a closed, oriented surface that folds through itself
//! passes the edge check but is not a solid, and a boolean on it gives
//! wrong output (`docs/design.md` §6). Every side is decided with exact
//! predicates (`orient3d`, `orient2d`) or exact coordinate comparisons, so
//! the answer is the same on every host.

use core::cmp::Ordering::{self, Equal, Greater, Less};

use shape_core::predicates::{orient2d, orient3d};
use shape_core::{DVec2, DVec3};

/// Every pair `(i, j)`, `i < j`, of triangles that share no vertex and
/// meet (touching counts), sorted. Two vertices closer than `weld` count
/// as shared: a boolean's output can hold points a few ulps apart that
/// were never welded, and the ulp-deep fold between their triangles is
/// not a fault the caller can act on (pass 0 for index sharing only).
/// Candidates come from a sweep along x over the triangles' boxes; each is
/// decided exactly.
pub fn self_intersections(
    positions: &[DVec3],
    triangles: &[[u32; 3]],
    weld: f64,
) -> Vec<(u32, u32)> {
    let near = |a: u32, b: u32| {
        a == b || (positions[a as usize] - positions[b as usize]).length_squared() <= weld * weld
    };
    let tri = |t: [u32; 3]| t.map(|i| positions[i as usize]);
    let mut boxes: Vec<(u32, DVec3, DVec3)> = triangles
        .iter()
        .enumerate()
        .map(|(i, &t)| {
            let p = tri(t);
            (i as u32, p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]))
        })
        .collect();
    boxes.sort_by(|a, b| a.1.x.total_cmp(&b.1.x).then(a.0.cmp(&b.0)));
    let mut active: Vec<(u32, DVec3, DVec3)> = Vec::new();
    let mut found = Vec::new();
    for &cur in &boxes {
        active.retain(|o| o.2.x >= cur.1.x);
        let tc = triangles[cur.0 as usize];
        for o in &active {
            if !(cur.1.cmple(o.2).all() && o.1.cmple(cur.2).all()) {
                continue;
            }
            let to = triangles[o.0 as usize];
            if tc.iter().any(|&v| to.iter().any(|&w| near(v, w))) {
                continue;
            }
            if tris_meet(tri(tc), tri(to)) {
                found.push((cur.0.min(o.0), cur.0.max(o.0)));
            }
        }
        active.push(cur);
    }
    found.sort_unstable();
    found
}

/// Whether the closed triangles `a` and `b` share a point. Two convex sets
/// meet exactly when an edge of one meets the other, so that is the test.
pub fn tris_meet(a: [DVec3; 3], b: [DVec3; 3]) -> bool {
    // Quick reject: one triangle strictly on one side of the other's plane.
    // (A degenerate triangle gives Equal everywhere and never rejects.)
    let apart = |s: [DVec3; 3], t: [DVec3; 3]| {
        let o = t.map(|p| orient3d(s[0], s[1], s[2], p));
        o.iter().all(|&x| x == Greater) || o.iter().all(|&x| x == Less)
    };
    if apart(a, b) || apart(b, a) {
        return false;
    }
    (0..3).any(|k| seg_tri(a[k], a[(k + 1) % 3], b) || seg_tri(b[k], b[(k + 1) % 3], a))
}

fn drop_axis(p: DVec3, axis: usize) -> DVec2 {
    match axis {
        0 => DVec2::new(p.y, p.z),
        1 => DVec2::new(p.z, p.x),
        _ => DVec2::new(p.x, p.y),
    }
}

fn collinear(a: DVec3, b: DVec3, c: DVec3) -> bool {
    (0..3).all(|k| orient2d(drop_axis(a, k), drop_axis(b, k), drop_axis(c, k)) == Equal)
}

/// For coplanar points: an axis whose projection is one-to-one on their
/// affine hull, or `None` when they are all one point.
fn flat_axis(p: &[DVec3]) -> Option<usize> {
    let n = p.len();
    for k in 0..3 {
        for i in 0..n {
            for j in i + 1..n {
                for l in j + 1..n {
                    let q = [p[i], p[j], p[l]].map(|x| drop_axis(x, k));
                    if orient2d(q[0], q[1], q[2]) != Equal {
                        return Some(k);
                    }
                }
            }
        }
    }
    // Collinear: any axis the line is not parallel to.
    (0..3).find(|&k| p.iter().any(|&x| drop_axis(x, k) != drop_axis(p[0], k)))
}

/// Whether the closed segment `p`–`q` meets the closed triangle `t`.
fn seg_tri(p: DVec3, q: DVec3, t: [DVec3; 3]) -> bool {
    let [a, b, c] = t;
    if collinear(a, b, c) {
        return seg_seg(p, q, a, b) || seg_seg(p, q, b, c) || seg_seg(p, q, c, a);
    }
    let (sp, sq) = (orient3d(a, b, c, p), orient3d(a, b, c, q));
    if sp != Equal && sp == sq {
        return false;
    }
    if sp != Equal || sq != Equal {
        // The segment reaches the plane at one point; the line through it
        // passes inside or on the triangle when the three signs agree.
        let o = [
            orient3d(p, q, a, b),
            orient3d(p, q, b, c),
            orient3d(p, q, c, a),
        ];
        return !(o.contains(&Greater) && o.contains(&Less));
    }
    let Some(k) = flat_axis(&[a, b, c]) else {
        return false;
    };
    let t2 = t.map(|x| drop_axis(x, k));
    seg_tri_2d(drop_axis(p, k), drop_axis(q, k), t2)
}

/// Whether the closed segments `p`–`q` and `r`–`s` meet.
fn seg_seg(p: DVec3, q: DVec3, r: DVec3, s: DVec3) -> bool {
    if orient3d(p, q, r, s) != Equal {
        return false;
    }
    match flat_axis(&[p, q, r, s]) {
        Some(k) => {
            let d = |x| drop_axis(x, k);
            seg_seg_2d(d(p), d(q), d(r), d(s))
        }
        None => true,
    }
}

fn within(a: DVec2, b: DVec2, x: DVec2) -> bool {
    a.min(b).cmple(x).all() && x.cmple(a.max(b)).all()
}

/// Whether the closed segments `p`–`q` and `r`–`s` meet, in the plane.
fn seg_seg_2d(p: DVec2, q: DVec2, r: DVec2, s: DVec2) -> bool {
    let (o1, o2) = (orient2d(p, q, r), orient2d(p, q, s));
    let (o3, o4) = (orient2d(r, s, p), orient2d(r, s, q));
    let opposite = |x: Ordering, y: Ordering| x != Equal && y != Equal && x != y;
    if opposite(o1, o2) && opposite(o3, o4) {
        return true;
    }
    (o1 == Equal && within(p, q, r))
        || (o2 == Equal && within(p, q, s))
        || (o3 == Equal && within(r, s, p))
        || (o4 == Equal && within(r, s, q))
}

/// Whether the closed segment `p`–`q` meets the closed triangle `t` (not
/// degenerate), in the plane.
fn seg_tri_2d(p: DVec2, q: DVec2, t: [DVec2; 3]) -> bool {
    let inside = |x: DVec2| {
        let o = [
            orient2d(t[0], t[1], x),
            orient2d(t[1], t[2], x),
            orient2d(t[2], t[0], x),
        ];
        !(o.contains(&Greater) && o.contains(&Less))
    };
    inside(p) || inside(q) || (0..3).any(|k| seg_seg_2d(p, q, t[k], t[(k + 1) % 3]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64, z: f64) -> DVec3 {
        DVec3::new(x, y, z)
    }

    const T: [DVec3; 3] = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(4.0, 0.0, 0.0),
        DVec3::new(0.0, 4.0, 0.0),
    ];

    #[test]
    fn piercing_touching_and_missing() {
        // Pierces the interior.
        assert!(tris_meet(
            T,
            [v(1.0, 1.0, -1.0), v(1.0, 1.0, 1.0), v(1.0, 2.0, 1.0)]
        ));
        // Touches the plane at one vertex inside: meets.
        assert!(tris_meet(
            T,
            [v(1.0, 1.0, 0.0), v(1.0, 1.0, 1.0), v(1.0, 2.0, 1.0)]
        ));
        // Above the plane: misses.
        assert!(!tris_meet(
            T,
            [v(1.0, 1.0, 1e-300), v(1.0, 1.0, 1.0), v(1.0, 2.0, 1.0)]
        ));
        // Crosses the plane outside the triangle (past the hypotenuse).
        assert!(!tris_meet(
            T,
            [v(2.5, 2.5, -1.0), v(2.5, 2.5, 1.0), v(3.0, 3.0, 1.0)]
        ));
        // Coplanar overlap and coplanar miss.
        assert!(tris_meet(
            T,
            [v(1.0, 1.0, 0.0), v(5.0, 1.0, 0.0), v(1.0, 5.0, 0.0)]
        ));
        assert!(!tris_meet(
            T,
            [v(3.0, 3.0, 0.0), v(5.0, 3.0, 0.0), v(3.0, 5.0, 0.0)]
        ));
    }

    #[test]
    fn a_closed_tetrahedron_has_none() {
        let p = [
            v(0.0, 0.0, 0.0),
            v(1.0, 0.0, 0.0),
            v(0.0, 1.0, 0.0),
            v(0.0, 0.0, 1.0),
        ];
        let t = [[0, 2, 1], [0, 1, 3], [1, 2, 3], [0, 3, 2]];
        assert!(self_intersections(&p, &t, 0.0).is_empty());
    }
}
