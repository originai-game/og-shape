//! A solid's triangles with one vertex per (position, surface): normals
//! smooth within a surface and split at real edges. A solid with UVs gets
//! one vertex per (position, surface, UV): its seams split vertices too,
//! and the copies share their normal.

use std::collections::BTreeMap;

use shape_core::{DVec2, DVec3};
use shape_mesh::Solid;

/// Vertices split by surface, in kernel space (millimetres, Z up).
pub(crate) struct Split {
    pub positions: Vec<DVec3>,
    pub normals: Vec<DVec3>,
    pub indices: Vec<u32>,
    /// Per split vertex, the solid's vertex it copies.
    pub sources: Vec<u32>,
    /// Per split vertex, its texture coordinates, if the solid has UVs
    /// (kernel convention: v up).
    pub uvs: Vec<DVec2>,
}

/// Splits `s` so that each surface has its own vertices. A vertex's normal
/// is the area-weighted sum of its surface's triangle normals around it,
/// normalized. Vertices are numbered in order of first use by the
/// triangles, so the result depends only on the solid.
pub(crate) fn split(s: &Solid) -> Split {
    // The UV's f32 bits, as written: corners that write the same share a
    // vertex. (0, 0) for a solid with none.
    let uv_key = |i: usize, k: usize| {
        s.uvs.as_ref().map_or((0, 0), |u| {
            let q = u[i][k];
            ((q.x as f32).to_bits(), (q.y as f32).to_bits())
        })
    };
    let mut ids: BTreeMap<(u32, u32, (u32, u32)), u32> = BTreeMap::new();
    let mut smooth: BTreeMap<(u32, u32), DVec3> = BTreeMap::new();
    let mut out = Split {
        positions: Vec::new(),
        normals: Vec::new(),
        indices: Vec::with_capacity(s.triangles.len() * 3),
        sources: Vec::new(),
        uvs: Vec::new(),
    };
    let mut normal_of: Vec<(u32, u32)> = Vec::new();
    for (i, t) in s.triangles.iter().enumerate() {
        let [a, b, c] = t.map(|v| s.positions[v as usize]);
        // Twice the area, along the normal.
        let n = (b - a).cross(c - a);
        for (k, &v) in t.iter().enumerate() {
            let key = (v, s.surfaces[i], uv_key(i, k));
            let next = out.positions.len() as u32;
            let id = *ids.entry(key).or_insert(next);
            if id == next {
                out.positions.push(s.positions[v as usize]);
                out.sources.push(v);
                normal_of.push((v, s.surfaces[i]));
                if let Some(u) = &s.uvs {
                    out.uvs.push(u[i][k]);
                }
            }
            *smooth.entry((v, s.surfaces[i])).or_default() += n;
            out.indices.push(id);
        }
    }
    out.normals = normal_of
        .iter()
        .map(|key| {
            let n = smooth[key];
            let len = n.length();
            // Only degenerate triangles touch a zero sum: any fixed unit
            // vector will do.
            if len > 0.0 { n / len } else { DVec3::Z }
        })
        .collect();
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
