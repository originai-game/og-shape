//! A solid's triangles with one vertex per (position, surface): normals
//! smooth within a surface and split at real edges. A solid with UVs gets
//! one vertex per (position, surface, UV): its seams split vertices too,
//! and the copies share their normal. Vertex colours split the same way.

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
    /// Per split vertex, the triangle and corner it was first met at.
    pub corners: Vec<(u32, u8)>,
    /// Per split vertex, its texture coordinates, if the solid has UVs
    /// (kernel convention: v up).
    pub uvs: Vec<DVec2>,
    /// Per split vertex, its colour (linear RGBA), if the solid has
    /// colours.
    pub colors: Vec<[f32; 4]>,
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
    // Likewise the colour's f32 bits; none for a solid without.
    let color_key = |i: usize, k: usize| {
        s.colors
            .as_ref()
            .map_or([0; 4], |c| c[i][k].map(f32::to_bits))
    };
    type Key = (u32, u32, (u32, u32), [u32; 4]);
    let mut ids: BTreeMap<Key, u32> = BTreeMap::new();
    let mut smooth: BTreeMap<(u32, u32), DVec3> = BTreeMap::new();
    let mut out = Split {
        positions: Vec::new(),
        normals: Vec::new(),
        indices: Vec::with_capacity(s.triangles.len() * 3),
        sources: Vec::new(),
        corners: Vec::new(),
        uvs: Vec::new(),
        colors: Vec::new(),
    };
    let mut normal_of: Vec<(u32, u32)> = Vec::new();
    for (i, t) in s.triangles.iter().enumerate() {
        let [a, b, c] = t.map(|v| s.positions[v as usize]);
        // Twice the area, along the normal.
        let n = (b - a).cross(c - a);
        for (k, &v) in t.iter().enumerate() {
            let key = (v, s.surfaces[i], uv_key(i, k), color_key(i, k));
            let next = out.positions.len() as u32;
            let id = *ids.entry(key).or_insert(next);
            if id == next {
                out.positions.push(s.positions[v as usize]);
                out.sources.push(v);
                out.corners.push((i as u32, k as u8));
                normal_of.push((v, s.surfaces[i]));
                if let Some(u) = &s.uvs {
                    out.uvs.push(u[i][k]);
                }
                if let Some(c) = &s.colors {
                    out.colors.push(c[i][k]);
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

/// A corner's shading frame as a plain GLB writes it: the unit normal
/// (smooth within a surface), and the unit tangent along +u with its
/// handedness `w` (±1; the bitangent is `w · normal × tangent`, along +v).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct CornerFrame {
    pub normal: DVec3,
    pub tangent: DVec3,
    pub w: f64,
}

/// Every corner's [`CornerFrame`], per triangle. Normals: the
/// area-weighted sum of the surface's triangle normals round the vertex.
/// Tangents (a solid with UVs; else any unit vector square to the normal,
/// `w` 1): MikkTSpace ([`crate::mikk`]), from each corner's position,
/// this normal and its UV, so a tool that derives its own tangents the
/// same way agrees with the written `TANGENT`.
pub(crate) fn corner_frames(s: &Solid) -> Vec<[CornerFrame; 3]> {
    let mut normal: BTreeMap<(u32, u32), DVec3> = BTreeMap::new();
    for (i, t) in s.triangles.iter().enumerate() {
        let [a, b, c] = t.map(|v| s.positions[v as usize]);
        let n = (b - a).cross(c - a);
        for &v in t {
            *normal.entry((v, s.surfaces[i])).or_default() += n;
        }
    }
    let unit_normal = |v: u32, surf: u32| {
        let n = normal[&(v, surf)];
        let len = n.length();
        if len > 0.0 { n / len } else { DVec3::Z }
    };
    // MikkTSpace, from each corner's position, normal and UV.
    let mikk = s
        .uvs
        .as_ref()
        .filter(|uv| uv.len() == s.triangles.len())
        .map(|uv| {
            let positions: Vec<[DVec3; 3]> = s
                .triangles
                .iter()
                .map(|t| t.map(|v| s.positions[v as usize]))
                .collect();
            let normals: Vec<[DVec3; 3]> = s
                .triangles
                .iter()
                .enumerate()
                .map(|(i, t)| t.map(|v| unit_normal(v, s.surfaces[i])))
                .collect();
            crate::mikk::tangents(&positions, &normals, uv)
        });
    let mut out = Vec::with_capacity(s.triangles.len());
    for (i, t) in s.triangles.iter().enumerate() {
        out.push(core::array::from_fn(|k| {
            let n = unit_normal(t[k], s.surfaces[i]);
            let (mut tangent, mut w) = mikk.as_ref().map_or((DVec3::ZERO, 1.0), |m| m[i][k]);
            // Square to the normal still (it is, but for rounding).
            tangent -= n * n.dot(tangent);
            let len = tangent.length();
            if len > 0.0 {
                tangent /= len;
            } else {
                // No UV extent: any direction square to the normal.
                w = 1.0;
                let a = n.abs();
                let helper = if a.x <= a.y && a.x <= a.z {
                    DVec3::X
                } else if a.y <= a.z {
                    DVec3::Y
                } else {
                    DVec3::Z
                };
                let c = n.cross(helper);
                let l = c.length();
                tangent = if l > 0.0 { c / l } else { DVec3::X };
            }
            CornerFrame {
                normal: n,
                tangent,
                w,
            }
        }));
    }
    out
}
