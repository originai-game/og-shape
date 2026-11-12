//! A solid grown by its faces, with the exact booleans: [`grown`]
//! (`docs/research.md` §48).
//!
//! Moving every vertex out along its faces' normals folds a solid that
//! is not convex wherever a hollow of it is narrower than the move: two
//! holes that cross, where their walls meet; a pocket's mouth rounded
//! narrower than the wall. What growing means does not fold: every point
//! within the move of the solid. Of a convex part of its surface those
//! points are a convex solid, bounded by the part's planes moved out;
//! so the grown solid is the solid itself joined with one convex solid
//! for each convex part of its surface. They are handed back apart: what
//! is cut by the grown solid is cut by each in turn, and none of the
//! booleans has to take in the whole.
//!
//! The parts: triangles gathered across their edges while each lies
//! behind the planes of all the others (a hole's whole wall with its
//! floor; one band of a rounded mouth), and pieces of one surface that
//! another cut in two. A part's solid: inside each of its own planes
//! moved out by the move, so a flat face moves exactly as far as it is
//! told and faceted round walls keep their facets; and inside 32 planes
//! round a sphere of the move's radius (an icosahedron's corners and
//! faces), each as far beyond the part as the move, which round the
//! part's open ends: their corners reach 1.084 of the move. Each part's
//! sphere is a
//! hair larger than the last's (a thousandth of the move a part), so no
//! two parts' solids share a plane, which the booleans do not take.

use std::collections::BTreeMap;

use shape_core::DVec3;
use shape_mesh::Solid;

/// The 32 directions of the ball: an icosahedron's corners and its
/// faces' middles, unit long.
fn directions() -> Vec<DVec3> {
    let t = (1.0 + 5f64.sqrt()) / 2.0;
    let corners = [
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ]
    .map(DVec3::from_array);
    #[rustfmt::skip]
    let faces = [
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
        [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
        [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
        [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];
    let unit = |v: DVec3| v / v.length();
    let mut out: Vec<DVec3> = corners.iter().map(|c| unit(*c)).collect();
    for f in faces {
        out.push(unit(corners[f[0]] + corners[f[1]] + corners[f[2]]));
    }
    out
}

/// The triangles of `c` that move, in parts: each a set whose every
/// corner lies behind every other's plane. A part grows from the
/// largest triangle left, across edges; then parts that hold pieces of
/// one surface (a hole's wall another hole cut in two) and still lie
/// behind each other's planes are one.
fn parts(c: &Solid, moving: &[bool], tol: f64) -> Vec<Vec<usize>> {
    let nt = c.triangles.len();
    let planes: Vec<(DVec3, f64)> = (0..nt)
        .map(|i| {
            let [p, q, r] = c.triangles[i].map(|v| c.positions[v as usize]);
            let n = (q - p).cross(r - p).normalize_or_zero();
            (n, n.dot(p))
        })
        .collect();
    let behind = |i: usize, j: usize| {
        // Triangle `j`'s corners behind triangle `i`'s plane.
        let (n, d) = planes[i];
        c.triangles[j]
            .iter()
            .all(|&v| n.dot(c.positions[v as usize]) - d <= tol)
    };
    let fits = |part: &[usize], j: usize| part.iter().all(|&m| behind(m, j) && behind(j, m));
    let mut by_edge: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (i, t) in c.triangles.iter().enumerate() {
        if moving[i] {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                by_edge.entry((a.min(b), a.max(b))).or_default().push(i);
            }
        }
    }
    let area = |i: usize| {
        let [p, q, r] = c.triangles[i].map(|v| c.positions[v as usize]);
        (q - p).cross(r - p).length()
    };
    let mut order: Vec<usize> = (0..nt).filter(|&i| moving[i]).collect();
    order.sort_by(|&a, &b| area(b).total_cmp(&area(a)).then(a.cmp(&b)));
    let mut taken = vec![false; nt];
    let mut out: Vec<Vec<usize>> = Vec::new();
    for &seed in &order {
        if taken[seed] {
            continue;
        }
        taken[seed] = true;
        let mut part = vec![seed];
        let mut at = 0;
        while at < part.len() {
            let t = c.triangles[part[at]];
            at += 1;
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                for &j in by_edge.get(&(a.min(b), a.max(b))).into_iter().flatten() {
                    if !taken[j] && fits(&part, j) {
                        taken[j] = true;
                        part.push(j);
                    }
                }
            }
        }
        out.push(part);
    }
    // Pieces of one surface, where they still fit each other.
    loop {
        let mut merged = None;
        'pairs: for a in 0..out.len() {
            for b in a + 1..out.len() {
                let shares = out[a]
                    .iter()
                    .any(|&i| out[b].iter().any(|&j| c.surfaces[i] == c.surfaces[j]));
                if shares && out[b].iter().all(|&j| fits(&out[a], j)) {
                    merged = Some((a, b));
                    break 'pairs;
                }
            }
        }
        let Some((a, b)) = merged else { break };
        let moved = out.remove(b);
        out[a].extend(moved);
    }
    out
}

/// `c` grown, as the solids that make it up when joined: every
/// triangle's plane moved out by `t` along its normal, the `free` ones
/// left where they are, as the module says. Each convex part grown comes
/// first, `c` itself last. `None` when a part's planes close no space.
pub(crate) fn grown(c: &Solid, t: f64, free: &[bool], diag: f64) -> Option<Vec<Solid>> {
    let dirs = directions();
    let moving: Vec<bool> = c
        .triangles
        .iter()
        .enumerate()
        .map(|(i, tri)| {
            let p = tri.map(|v| c.positions[v as usize]);
            let area = (p[1] - p[0]).cross(p[2] - p[0]).length();
            let longest = (p[1] - p[0])
                .length_squared()
                .max((p[2] - p[1]).length_squared())
                .max((p[0] - p[2]).length_squared());
            !free[i] && area > 1e-9 * longest
        })
        .collect();
    let mut solids = Vec::new();
    for (k, part) in parts(c, &moving, 1e-9 * diag).iter().enumerate() {
        let mut corners: Vec<u32> = part.iter().flat_map(|&i| c.triangles[i]).collect();
        corners.sort_unstable();
        corners.dedup();
        let at: Vec<DVec3> = corners.iter().map(|&v| c.positions[v as usize]).collect();
        // How far the part reaches along `n`.
        let reach_of = |n: DVec3| {
            at.iter()
                .map(|p| n.dot(*p))
                .fold(f64::NEG_INFINITY, f64::max)
        };
        // Its own faces' planes, moved out by `t`; then the ball's, a
        // hair further for each part, which round what lies between.
        let mut planes: Vec<(DVec3, f64)> = Vec::new();
        for &i in part {
            let [p, q, r] = c.triangles[i].map(|v| c.positions[v as usize]);
            let n = (q - p).cross(r - p).normalize_or_zero();
            if planes.iter().all(|(m, _)| (*m - n).length() > 1e-9) {
                planes.push((n, reach_of(n) + t));
            }
        }
        let size = t * (1.0 + 1e-3 * (k + 1) as f64);
        for d in &dirs {
            if planes.iter().all(|(m, _)| (*m - *d).length() > 1e-6) {
                planes.push((*d, reach_of(*d) + size));
            }
        }
        let middle = at.iter().copied().sum::<DVec3>() / at.len() as f64;
        solids.push(crate::inset::space(&planes, middle, 1e-9 * diag)?);
    }
    solids.push(c.clone());
    Some(solids)
}
