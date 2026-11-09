//! Surface distance on a mesh, and edge splitting near seams.

use core::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use shape_core::DVec3;
use shape_mesh::{EdgeKey, Solid, edge_key};

/// Each vertex's neighbours with the edge lengths, in index order.
pub(crate) fn adjacency(s: &Solid) -> Vec<Vec<(u32, f64)>> {
    let mut adj: Vec<BTreeMap<u32, f64>> = vec![BTreeMap::new(); s.positions.len()];
    for t in &s.triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let len = (s.positions[a as usize] - s.positions[b as usize]).length();
            adj[a as usize].insert(b, len);
            adj[b as usize].insert(a, len);
        }
    }
    adj.into_iter().map(|m| m.into_iter().collect()).collect()
}

/// A heap entry: smallest distance first, then lowest vertex.
#[derive(PartialEq)]
struct Entry(f64, u32);

impl Eq for Entry {}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.total_cmp(&self.0).then(other.1.cmp(&self.1))
    }
}

/// Surface distance along the edges from the `sources` (distance 0) to
/// every vertex, up to `limit`; vertices farther away get infinity.
/// Dijkstra with a fixed tie-break, so the same mesh gives the same bits.
pub(crate) fn distances(adj: &[Vec<(u32, f64)>], sources: &[u32], limit: f64) -> Vec<f64> {
    let mut dist = vec![f64::INFINITY; adj.len()];
    let mut heap = BinaryHeap::new();
    for &v in sources {
        dist[v as usize] = 0.0;
        heap.push(Entry(0.0, v));
    }
    while let Some(Entry(d, v)) = heap.pop() {
        if d > dist[v as usize] || d > limit {
            continue;
        }
        for &(w, len) in &adj[v as usize] {
            let nd = d + len;
            if nd < dist[w as usize] && nd <= limit {
                dist[w as usize] = nd;
                heap.push(Entry(nd, w));
            }
        }
    }
    dist
}

/// The solid with each edge of `edges` split at its midpoint. Pieces keep
/// their triangle's surface and face names; a named edge's halves keep
/// its names. Not canonicalized.
pub(crate) fn split_edges(s: &Solid, edges: &BTreeSet<EdgeKey>) -> Solid {
    let mut out = Solid {
        positions: s.positions.clone(),
        ..Default::default()
    };
    let mut mid: BTreeMap<EdgeKey, u32> = BTreeMap::new();
    for &(a, b) in edges {
        let m = out.push_vertex((s.positions[a as usize] + s.positions[b as usize]) / 2.0);
        mid.insert((a, b), m);
    }
    for (i, t) in s.triangles.iter().enumerate() {
        let tags: Vec<&str> = s.face_tags[i].iter().map(|&x| s.names.name(x)).collect();
        let m = [0, 1, 2].map(|k| mid.get(&edge_key(t[k], t[(k + 1) % 3])).copied());
        for tri in split_triangle(&out.positions, *t, m) {
            out.push_triangle(tri, s.surfaces[i], &tags);
        }
    }
    for (&(a, b), tags) in &s.edge_tags {
        let names: Vec<&str> = tags.iter().map(|&x| s.names.name(x)).collect();
        match mid.get(&(a, b)) {
            Some(&m) => {
                out.tag_edge(a, m, &names);
                out.tag_edge(m, b, &names);
            }
            None => out.tag_edge(a, b, &names),
        }
    }
    out
}

/// A triangle `t` with midpoints `m[k]` on its edge `k → k+1`. With two
/// edges split, the quad left over is cut along its shorter diagonal
/// (ties to the first), so splitting adds no needless slivers.
fn split_triangle(p: &[DVec3], t: [u32; 3], m: [Option<u32>; 3]) -> Vec<[u32; 3]> {
    if let [Some(ab), Some(bc), Some(ca)] = m {
        let [a, b, c] = t;
        return vec![[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]];
    }
    // Rotate so a split edge comes first and the edge before it is whole.
    let Some(k) = (0..3).find(|&k| m[k].is_some() && m[(k + 2) % 3].is_none()) else {
        return vec![t];
    };
    let v = [t[k], t[(k + 1) % 3], t[(k + 2) % 3]];
    match [m[k], m[(k + 1) % 3]] {
        [Some(x), None] => vec![[v[0], x, v[2]], [x, v[1], v[2]]],
        [Some(x), Some(y)] => {
            let len = |a: u32, b: u32| (p[a as usize] - p[b as usize]).length();
            if len(x, v[2]) <= len(y, v[0]) {
                vec![[v[0], x, v[2]], [x, v[1], y], [x, y, v[2]]]
            } else {
                vec![[v[0], x, y], [x, v[1], y], [v[0], y, v[2]]]
            }
        }
        _ => vec![t],
    }
}
