//! Measures of a solid, with no thresholds (`docs/design.md` §7).

use std::collections::BTreeMap;

use shape_core::DVec3;

use crate::{EdgeKey, Solid, edge_key};

/// What [`Solid::measure`] reports.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Measures {
    /// Triangle count.
    pub triangles: usize,
    /// Vertex count.
    pub vertices: usize,
    /// The bounding box's low corner.
    pub bounds_min: DVec3,
    /// The bounding box's high corner.
    pub bounds_max: DVec3,
    /// Surface area.
    pub area: f64,
    /// Signed: positive for a closed solid facing out.
    pub volume: f64,
    /// Every edge used once each way.
    pub closed: bool,
    /// Edges used by one triangle.
    pub boundary_edges: usize,
    /// Edges used by more than two triangles, or twice the same way.
    pub non_manifold_edges: usize,
    /// Triangles of zero area.
    pub degenerate_triangles: usize,
    /// Connected pieces (by shared vertices).
    pub components: usize,
}

pub(crate) fn measure(s: &Solid) -> Measures {
    let mut m = Measures {
        triangles: s.triangles.len(),
        vertices: s.positions.len(),
        ..Default::default()
    };
    if let Some(&first) = s.positions.first() {
        let (mut lo, mut hi) = (first, first);
        for &p in &s.positions {
            lo = lo.min(p);
            hi = hi.max(p);
        }
        m.bounds_min = lo;
        m.bounds_max = hi;
    }
    let mut directed: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut undirected: BTreeMap<EdgeKey, u32> = BTreeMap::new();
    let mut parent: Vec<u32> = (0..s.positions.len() as u32).collect();
    fn find(p: &mut [u32], mut i: u32) -> u32 {
        while p[i as usize] != i {
            p[i as usize] = p[p[i as usize] as usize];
            i = p[i as usize];
        }
        i
    }
    for t in &s.triangles {
        let [a, b, c] = t.map(|i| s.positions[i as usize]);
        let n = (b - a).cross(c - a);
        let area = n.length() / 2.0;
        if area == 0.0 {
            m.degenerate_triangles += 1;
        }
        m.area += area;
        m.volume += a.dot(b.cross(c)) / 6.0;
        for k in 0..3 {
            let (x, y) = (t[k], t[(k + 1) % 3]);
            *directed.entry((x, y)).or_default() += 1;
            *undirected.entry(edge_key(x, y)).or_default() += 1;
            let (rx, ry) = (find(&mut parent, x), find(&mut parent, y));
            if rx != ry {
                parent[rx.max(ry) as usize] = rx.min(ry);
            }
        }
    }
    for (&(a, b), &n) in &undirected {
        if n == 1 {
            m.boundary_edges += 1;
        } else if n > 2 || directed.get(&(a, b)) != Some(&1) || directed.get(&(b, a)) != Some(&1) {
            m.non_manifold_edges += 1;
        }
    }
    m.closed = !s.triangles.is_empty() && m.boundary_edges == 0 && m.non_manifold_edges == 0;
    let mut roots: Vec<u32> = s
        .triangles
        .iter()
        .map(|t| find(&mut parent, t[0]))
        .collect();
    roots.sort_unstable();
    roots.dedup();
    m.components = roots.len();
    m
}
