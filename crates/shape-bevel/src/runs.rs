//! Straight runs (`docs/research.md` §7.4): a vertex inside a straight run
//! of bevelled edges, with only diagonals of its two planar faces besides,
//! is no corner of the solid. Booleans leave such vertices close
//! together, and a bevel wider than the gap between two of them makes the
//! faces overlap. When a bevel does not fit, it is tried again with those
//! vertices dissolved: each face around one is re-triangulated without
//! it, and its two edges become one, carrying both edges' names.

use std::collections::BTreeSet;

use shape_mesh::{EdgeKey, Solid, edge_key};

use super::{PLANAR_DOT, clip, fan, topology, tri_normal};

/// The solid and edges with every run vertex dissolved, or `None` when
/// there is none.
pub(super) fn dissolve(s: &Solid, edges: &[EdgeKey]) -> Option<(Solid, Vec<EdgeKey>)> {
    let mut cur = s.clone();
    let mut es: BTreeSet<EdgeKey> = edges.iter().copied().collect();
    let mut any = false;
    loop {
        let verts: BTreeSet<u32> = es.iter().flat_map(|&(a, b)| [a, b]).collect();
        let next = verts.iter().find_map(|&v| one(&cur, &es, v));
        let Some((solid, set)) = next else {
            break;
        };
        cur = solid;
        es = set;
        any = true;
    }
    any.then(|| (cur, es.into_iter().collect()))
}

fn one(s: &Solid, es: &BTreeSet<EdgeKey>, v: u32) -> Option<(Solid, BTreeSet<EdgeKey>)> {
    let topo = topology(s);
    let f = fan(s, &topo, v).ok()?;
    let k = f.spokes.len();
    let p = s.positions[v as usize];
    let bev: Vec<usize> = (0..k)
        .filter(|&i| es.contains(&edge_key(v, f.spokes[i])))
        .collect();
    if bev.len() != 2 {
        return None;
    }
    let real = |i: usize| s.surfaces[f.tris[(i + k - 1) % k]] != s.surfaces[f.tris[i]];
    if (0..k).any(|i| real(i) && !bev.contains(&i)) {
        return None;
    }
    let (i0, i1) = (bev[0], bev[1]);
    let (a, b) = (f.spokes[i0], f.spokes[i1]);
    let (da, db) = (
        (s.positions[a as usize] - p).normalize_or_zero(),
        (s.positions[b as usize] - p).normalize_or_zero(),
    );
    if da.dot(db) > -1.0 + 1e-12 || da.cross(db).length() > 1e-9 {
        return None;
    }
    if topo.dir.contains_key(&(a, b)) || topo.dir.contains_key(&(b, a)) {
        return None;
    }
    let mut out = s.clone();
    let gone: BTreeSet<usize> = f.tris.iter().copied().collect();
    let mut keep = Vec::new();
    for (t, tri) in s.triangles.iter().enumerate() {
        if !gone.contains(&t) {
            keep.push((*tri, s.surfaces[t], s.face_tags[t].clone()));
        }
    }
    for (lo, hi) in [(i0, i1), (i1, i0 + k)] {
        let tris: Vec<usize> = (lo..hi).map(|i| f.tris[i % k]).collect();
        let n = tri_normal(s, tris[0]);
        if tris.iter().any(|&t| tri_normal(s, t).dot(n) <= PLANAR_DOT) {
            return None;
        }
        let ring: Vec<u32> = (lo..=hi).map(|i| f.spokes[i % k]).collect();
        for t in clip(&ring, &s.positions, n)? {
            keep.push((t, s.surfaces[tris[0]], s.face_tags[tris[0]].clone()));
        }
    }
    out.triangles = keep.iter().map(|x| x.0).collect();
    out.surfaces = keep.iter().map(|x| x.1).collect();
    out.face_tags = keep.into_iter().map(|x| x.2).collect();
    let (ka, kb) = (edge_key(a, v), edge_key(v, b));
    let mut tags: Vec<_> = out.edge_tags.remove(&ka).unwrap_or_default();
    tags.extend(out.edge_tags.remove(&kb).unwrap_or_default());
    tags.sort_unstable();
    tags.dedup();
    if !tags.is_empty() {
        out.edge_tags.insert(edge_key(a, b), tags);
    }
    let mut set = es.clone();
    set.remove(&ka);
    set.remove(&kb);
    set.insert(edge_key(a, b));
    Some((out, set))
}
