//! Two cones at one vertex (`docs/research.md` §7.5): a vertex whose
//! triangles form more than one fan, as where two boxes touch at a corner
//! after a join. The solid is closed, every edge has its two faces, but
//! the vertex is shared by surfaces that only touch there.
//!
//! Before bevelling, each such vertex at the end of a bevelled edge is
//! split: one copy per fan (cone), at the same position, each taking the
//! triangles, edges and edge names of its cone. Every cone is then
//! bevelled as its own corner. The copies left unbevelled weld back into
//! one vertex when the result is canonicalized, since they share a
//! position.

use std::collections::{BTreeMap, BTreeSet};

use shape_mesh::{EdgeKey, Solid, edge_key};

use super::{Topology, rotate_to, topology};

/// The fans around `v`, each as its triangles, in the order of their
/// smallest triangle; `None` when an edge has no twin.
fn cones(s: &Solid, topo: &Topology, v: u32) -> Option<Vec<Vec<usize>>> {
    let mut left: BTreeSet<usize> = topo.at[v as usize].iter().copied().collect();
    let mut out = Vec::new();
    while let Some(&start) = left.iter().next() {
        let mut cone = Vec::new();
        let mut cur = start;
        loop {
            if !left.remove(&cur) {
                return None;
            }
            cone.push(cur);
            let t = rotate_to(s.triangles[cur], v);
            cur = *topo.dir.get(&(v, t[2]))?;
            if cur == start {
                break;
            }
        }
        out.push(cone);
    }
    Some(out)
}

/// The solid and edges with every vertex of two or more cones, at the end
/// of a bevelled edge, split into one vertex per cone; `None` when there
/// is none.
pub(super) fn split(s: &Solid, edges: &[EdgeKey]) -> Option<(Solid, Vec<EdgeKey>)> {
    let topo = topology(s);
    let verts: BTreeSet<u32> = edges.iter().flat_map(|&(a, b)| [a, b]).collect();
    let mut out = s.clone();
    let mut any = false;
    for &v in &verts {
        let cs = cones(s, &topo, v)?;
        for cone in cs.iter().skip(1) {
            any = true;
            let copy = out.push_vertex(s.positions[v as usize]);
            for &t in cone {
                for c in out.triangles[t].iter_mut() {
                    if *c == v {
                        *c = copy;
                    }
                }
            }
        }
    }
    if !any {
        return None;
    }
    // Each old edge → its new ends, read off the triangles holding it.
    let mut moved: BTreeMap<EdgeKey, EdgeKey> = BTreeMap::new();
    for (old, new) in s.triangles.iter().zip(&out.triangles) {
        for k in 0..3 {
            let (a, b) = (
                edge_key(old[k], old[(k + 1) % 3]),
                edge_key(new[k], new[(k + 1) % 3]),
            );
            if a != b {
                moved.insert(a, b);
            }
        }
    }
    let remap = |k: EdgeKey| moved.get(&k).copied().unwrap_or(k);
    let mut tags = BTreeMap::new();
    for (k, t) in std::mem::take(&mut out.edge_tags) {
        tags.insert(remap(k), t);
    }
    out.edge_tags = tags;
    Some((out, edges.iter().map(|&k| remap(k)).collect()))
}
