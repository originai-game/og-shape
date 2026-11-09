//! A solid's triangles with one vertex per (position, surface): normals
//! smooth within a surface and split at real edges.

use std::collections::BTreeMap;

use shape_core::DVec3;
use shape_mesh::Solid;

/// Vertices split by surface, in kernel space (millimetres, Z up).
pub(crate) struct Split {
    pub positions: Vec<DVec3>,
    pub normals: Vec<DVec3>,
    pub indices: Vec<u32>,
}

/// Splits `s` so that each surface has its own vertices. A vertex's normal
/// is the area-weighted sum of its surface's triangle normals around it,
/// normalized. Vertices are numbered in order of first use by the
/// triangles, so the result depends only on the solid.
pub(crate) fn split(s: &Solid) -> Split {
    let mut ids: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut out = Split {
        positions: Vec::new(),
        normals: Vec::new(),
        indices: Vec::with_capacity(s.triangles.len() * 3),
    };
    for (i, t) in s.triangles.iter().enumerate() {
        let [a, b, c] = t.map(|v| s.positions[v as usize]);
        // Twice the area, along the normal.
        let n = (b - a).cross(c - a);
        for &v in t {
            let key = (v, s.surfaces[i]);
            let next = out.positions.len() as u32;
            let id = *ids.entry(key).or_insert(next);
            if id == next {
                out.positions.push(s.positions[v as usize]);
                out.normals.push(DVec3::ZERO);
            }
            out.normals[id as usize] += n;
            out.indices.push(id);
        }
    }
    for n in &mut out.normals {
        let len = n.length();
        // Only degenerate triangles touch a zero sum: any fixed unit vector
        // will do.
        *n = if len > 0.0 { *n / len } else { DVec3::Z };
    }
    out
}

/// A triangle's unit normal, or zero for a degenerate one.
pub(crate) fn face_normal(a: DVec3, b: DVec3, c: DVec3) -> DVec3 {
    let n = (b - a).cross(c - a);
    let len = n.length();
    if len > 0.0 { n / len } else { DVec3::ZERO }
}

/// Kernel space (millimetres, Z up) to glTF and OBJ space (metres, Y up):
/// Z becomes Y, Y becomes −Z.
pub(crate) fn y_up(p: DVec3) -> DVec3 {
    DVec3::new(p.x, p.z, -p.y)
}

/// Millimetres to metres.
pub(crate) const METRES: f64 = 0.001;
