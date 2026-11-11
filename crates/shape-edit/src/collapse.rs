//! The inner solid of a shell whose copy folds, with the faces that turn
//! over taken out: [`collapsed`].
//!
//! `shell`'s inner solid is a copy with each vertex moved to where its
//! faces' planes, moved by the wall, meet. A face narrower than the wall
//! (a chamfer, a fillet's strip, a rounded corner's facet) shrinks past
//! nothing and comes out facing the other way: the copy passes through
//! itself there (`docs/research.md` §27). Such a face has no place in the
//! inner solid: its neighbours run on until they meet.
//!
//! So a face that turned over is taken out of its vertices' planes, and
//! each of those vertices is held inside the moved planes of the faces
//! round the patch of turned faces it belongs to: the plane it is
//! furthest outside of joins its planes, until it is outside none or
//! three planes fix it. The vertices of a turned patch then land on the
//! line or point where the patch's neighbours meet, the turned faces have
//! no area and are dropped. Faces that turn over only then are taken out
//! in the next round.
//!
//! This needs no convexity. It does not mend a hollow that pinches off
//! (two walls meeting across a neck): no face turns over there, they
//! cross. The caller checks what comes back exactly.

use std::collections::BTreeMap;

use shape_core::DVec3;
use shape_mesh::Solid;

use crate::{SAME_PLANE_COS, fit};

/// Rounds of taking turned faces out before giving up.
const ROUNDS: usize = 16;

/// The copy of `s` with every vertex moved by the wall `t` (in from kept
/// faces, out through `open` ones) and the faces that turn over
/// collapsed. Triangles keep `s`'s surfaces and UVs and are tagged
/// `wall`. `None` if no face turns over (there is nothing to collapse),
/// or faces still turn after [`ROUNDS`].
pub(crate) fn collapsed(s: &Solid, open: &[bool], t: f64, diag: f64) -> Option<Solid> {
    let nt = s.triangles.len();
    let nv = s.positions.len();
    let tol = diag * 1e-9;
    // Each triangle's plane and where it moves to.
    let mut normal = Vec::with_capacity(nt);
    let mut moved = Vec::with_capacity(nt);
    for (i, tri) in s.triangles.iter().enumerate() {
        let [a, b, c] = tri.map(|v| s.positions[v as usize]);
        let n = (b - a).cross(c - a).normalize_or_zero();
        normal.push(n);
        moved.push(n.dot(a) + if open[i] { t } else { -t });
    }
    let mut around: Vec<Vec<u32>> = vec![Vec::new(); nv];
    for (i, tri) in s.triangles.iter().enumerate() {
        for &v in tri {
            around[v as usize].push(i as u32);
        }
    }
    // What a vertex must reach on plane `i`: how far along its normal.
    let need = |v: usize, i: u32| {
        let i = i as usize;
        (normal[i], moved[i] - normal[i].dot(s.positions[v]))
    };
    // Distinct planes of a list of constraints (the first of each).
    let distinct = |all: &[(DVec3, f64)]| {
        let mut cs: Vec<(DVec3, f64)> = Vec::new();
        for &(n, h) in all {
            if n != DVec3::ZERO && !cs.iter().any(|c| c.0.dot(n) > SAME_PLANE_COS) {
                cs.push((n, h));
            }
        }
        cs
    };

    let mut gone = vec![false; nt];
    let mut at: Vec<DVec3> = (0..nv)
        .map(|v| {
            let own: Vec<(DVec3, f64)> = around[v].iter().map(|&i| need(v, i)).collect();
            s.positions[v] + fit(&distinct(&own))
        })
        .collect();
    let turned = |at: &[DVec3], gone: &[bool]| -> Vec<usize> {
        (0..nt)
            .filter(|&i| {
                let [a, b, c] = s.triangles[i].map(|v| at[v as usize]);
                !gone[i] && (b - a).cross(c - a).dot(normal[i]) < -tol * tol
            })
            .collect()
    };
    let mut any = false;
    for _ in 0..ROUNDS {
        let new = turned(&at, &gone);
        if new.is_empty() {
            if !any {
                return None;
            }
            return finish(s, &at, tol);
        }
        any = true;
        for i in new {
            gone[i] = true;
        }
        // Patches of turned faces: joined through shared vertices.
        let mut patch: Vec<u32> = (0..nv as u32).collect();
        fn find(p: &mut [u32], mut v: u32) -> u32 {
            while p[v as usize] != v {
                p[v as usize] = p[p[v as usize] as usize];
                v = p[v as usize];
            }
            v
        }
        let mut in_patch = vec![false; nv];
        for (i, tri) in s.triangles.iter().enumerate() {
            if !gone[i] {
                continue;
            }
            let a = find(&mut patch, tri[0]);
            for &v in tri {
                in_patch[v as usize] = true;
                let b = find(&mut patch, v);
                let (lo, hi) = (a.min(b), a.max(b));
                patch[hi as usize] = lo;
                let a2 = find(&mut patch, tri[0]);
                debug_assert!(a2 <= a);
            }
        }
        // The faces round each patch: those left that touch its vertices.
        let mut round: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for v in 0..nv {
            if !in_patch[v] {
                continue;
            }
            let id = find(&mut patch, v as u32);
            let list = round.entry(id).or_default();
            list.extend(around[v].iter().copied().filter(|&i| !gone[i as usize]));
        }
        for list in round.values_mut() {
            list.sort_unstable();
            list.dedup();
        }
        for v in 0..nv {
            if !in_patch[v] {
                continue;
            }
            let p = s.positions[v];
            let own: Vec<(DVec3, f64)> = around[v]
                .iter()
                .filter(|&&i| !gone[i as usize])
                .map(|&i| need(v, i))
                .collect();
            let mut active = distinct(&own);
            let id = find(&mut patch, v as u32);
            let others = &round[&id];
            loop {
                let x = p + fit(&active);
                if active.len() >= 3 {
                    at[v] = x;
                    break;
                }
                // The plane it is furthest outside of, not one it has.
                let mut worst: Option<(f64, u32)> = None;
                for &i in others {
                    let n = normal[i as usize];
                    if n == DVec3::ZERO || active.iter().any(|c| c.0.dot(n) > SAME_PLANE_COS) {
                        continue;
                    }
                    let out = n.dot(x) - moved[i as usize];
                    if out > tol && worst.is_none_or(|(w, _)| out > w) {
                        worst = Some((out, i));
                    }
                }
                match worst {
                    Some((_, i)) => active.push(need(v, i)),
                    None => {
                        at[v] = x;
                        break;
                    }
                }
            }
        }
    }
    None
}

/// The copy at `at`, with vertices closer than `tol` made one (the first
/// in order of position), so faces collapsed to a line or a point drop.
fn finish(s: &Solid, at: &[DVec3], tol: f64) -> Option<Solid> {
    let mut order: Vec<u32> = (0..at.len() as u32).collect();
    order.sort_by(|&a, &b| {
        let (p, q) = (at[a as usize], at[b as usize]);
        p.x.total_cmp(&q.x)
            .then(p.y.total_cmp(&q.y))
            .then(p.z.total_cmp(&q.z))
            .then(a.cmp(&b))
    });
    let cell = |p: DVec3| {
        let c = (p / tol).floor();
        (c.x as i64, c.y as i64, c.z as i64)
    };
    let mut grid: BTreeMap<(i64, i64, i64), Vec<u32>> = BTreeMap::new();
    let mut snapped = at.to_vec();
    for &v in &order {
        let p = at[v as usize];
        let (cx, cy, cz) = cell(p);
        let mut found = None;
        'near: for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(ids) = grid.get(&(cx + dx, cy + dy, cz + dz)) {
                        for &k in ids {
                            if (at[k as usize] - p).length() <= tol {
                                found = Some(k);
                                break 'near;
                            }
                        }
                    }
                }
            }
        }
        match found {
            Some(k) => snapped[v as usize] = at[k as usize],
            None => grid.entry((cx, cy, cz)).or_default().push(v),
        }
    }
    let mut inner = Solid::default();
    for p in &snapped {
        inner.push_vertex(*p);
    }
    for (i, tri) in s.triangles.iter().enumerate() {
        inner.push_triangle(*tri, s.surfaces[i], &["wall"]);
    }
    inner.uvs.clone_from(&s.uvs);
    inner.canonicalize().ok()?;
    Some(inner)
}
