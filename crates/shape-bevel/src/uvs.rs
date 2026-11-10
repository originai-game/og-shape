//! UVs through a bevel (`docs/research.md` §21.2): a pass over the
//! bevel's result and its correspondence with the input, so the layout
//! never sees UVs.
//!
//! - **Old faces** (every output triangle on a face of the input): the
//!   input triangle it lies in is found by its face names and the
//!   distance from its centroid (a bevel only moves a corner within its
//!   triangle's plane, so a trimmed triangle lies inside its source). Its
//!   corners take UVs by barycentric interpolation there: untouched faces
//!   keep theirs exactly, trimmed ones are re-fitted to the same map.
//! - **New faces** (`fillet`, `chamfer`), per connected patch: each
//!   triangle takes the chart of the old face around the patch it faces
//!   most (largest dot of normals, then the nearer, then the lower
//!   surface), and its corners the map of that chart's triangle nearest
//!   it: exact at the corners they share, extended past its edges. On a
//!   chart whose UVs are an affine function of position (a cap, any flat
//!   face of a generator) that is the chart's own map projected onto the
//!   new face, which is what mean-value interpolation from that chart's
//!   boundary gives (it reproduces linear data exactly), continuous with
//!   the chart and across the patch. A fillet between two faces is split
//!   where its triangles turn from facing one to facing the other: a seam
//!   along its middle, each half continuous with its own face. On a
//!   curved chart (a bore, a cylinder's wall) each triangle takes the
//!   tangent plane of the facet nearest it, so the patch has small cracks
//!   in UV where that facet changes, and a chart's own seam stays a seam.

use std::collections::BTreeMap;

use shape_core::{DVec2, DVec3};
use shape_mesh::{CornerUvs, Solid, edge_key};

use crate::BevelError;

/// The point of triangle `abc` nearest `p` (Ericson, Real-Time Collision
/// Detection, 5.1.5), and its distance.
fn nearest_on(p: DVec3, a: DVec3, b: DVec3, c: DVec3) -> f64 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return (p - a).length();
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return (p - b).length();
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return (p - (a + ab * v)).length();
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return (p - c).length();
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return (p - (a + ac * w)).length();
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (p - (b + (c - b) * w)).length();
    }
    let denom = va + vb + vc;
    if denom == 0.0 {
        return (p - a).length().min((p - b).length()).min((p - c).length());
    }
    let (v, w) = (vb / denom, vc / denom);
    (p - (a + ab * v + ac * w)).length()
}

fn corners(s: &Solid, i: usize) -> [DVec3; 3] {
    s.triangles[i].map(|v| s.positions[v as usize])
}

fn distance(s: &Solid, i: usize, p: DVec3) -> f64 {
    let [a, b, c] = corners(s, i);
    nearest_on(p, a, b, c)
}

fn normal(s: &Solid, i: usize) -> DVec3 {
    let [a, b, c] = corners(s, i);
    (b - a).cross(c - a).normalize_or_zero()
}

fn area(s: &Solid, i: usize) -> f64 {
    let [a, b, c] = corners(s, i);
    (b - a).cross(c - a).length()
}

/// The triangle of `tris` nearest `p`, and its distance. Of those within
/// `eps` of the nearest, the largest: a sliver's map, extended, would
/// magnify its rounding.
fn nearest(s: &Solid, tris: &[usize], p: DVec3, eps: f64) -> Option<(f64, usize)> {
    let d: Vec<f64> = tris.iter().map(|&r| distance(s, r, p)).collect();
    let least = d.iter().copied().fold(f64::INFINITY, f64::min);
    let mut best: Option<(f64, f64, usize)> = None;
    for (k, &r) in tris.iter().enumerate() {
        if d[k] <= least + eps {
            let a = area(s, r);
            if best.is_none_or(|(ba, _, _)| a > ba) {
                best = Some((a, d[k], r));
            }
        }
    }
    best.map(|(_, d, r)| (d, r))
}

fn centroid(s: &Solid, i: usize) -> DVec3 {
    let [a, b, c] = corners(s, i);
    (a + b + c) / 3.0
}

fn names(s: &Solid, i: usize) -> Vec<String> {
    let mut n: Vec<String> = s.face_tags[i]
        .iter()
        .map(|&t| s.names.name(t).to_string())
        .collect();
    n.sort();
    n
}

/// `out` (the bevel of `input` with its UVs taken off) given UVs from
/// `input`'s, by the module's rule. New faces carry only `kind`.
pub(crate) fn carry(input: &Solid, mut out: Solid, kind: &str) -> Result<Solid, BevelError> {
    if input.uvs.is_none() {
        return Ok(out);
    }
    let n = out.triangles.len();
    let span = {
        let m = input.measure();
        (m.bounds_max - m.bounds_min).length().max(1.0)
    };
    let eps = 1e-7 * span;
    let mut by_names: BTreeMap<Vec<String>, Vec<usize>> = BTreeMap::new();
    for i in 0..input.triangles.len() {
        by_names.entry(names(input, i)).or_default().push(i);
    }
    // Old faces.
    let mut uvs: Vec<CornerUvs> = vec![[DVec2::ZERO; 3]; n];
    let mut old = vec![false; n];
    let only_kind = vec![kind.to_string()];
    for (i, uv) in uvs.iter_mut().enumerate() {
        let key = names(&out, i);
        let c = centroid(&out, i);
        let group = by_names.get(&key).map_or(&[][..], Vec::as_slice);
        let Some((d, j)) = nearest(input, group, c, eps) else {
            continue;
        };
        if key == only_kind && d > eps {
            continue;
        }
        old[i] = true;
        let fallback = input.uvs.as_ref().map_or(DVec2::ZERO, |u| u[j][0]);
        *uv = corners(&out, i).map(|p| input.uv_at(j, p).unwrap_or(fallback));
    }
    out.uvs = Some(uvs);
    // New faces, by connected patch.
    let mut by_edge: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (i, t) in out.triangles.iter().enumerate() {
        for k in 0..3 {
            by_edge
                .entry(edge_key(t[k], t[(k + 1) % 3]))
                .or_default()
                .push(i);
        }
    }
    let mut patch = vec![usize::MAX; n];
    let mut patches: Vec<Vec<usize>> = Vec::new();
    for start in 0..n {
        if old[start] || patch[start] != usize::MAX {
            continue;
        }
        let id = patches.len();
        let mut members = vec![start];
        patch[start] = id;
        let mut k = 0;
        while k < members.len() {
            let t = out.triangles[members[k]];
            for e in 0..3 {
                for &o in &by_edge[&edge_key(t[e], t[(e + 1) % 3])] {
                    if !old[o] && patch[o] == usize::MAX {
                        patch[o] = id;
                        members.push(o);
                    }
                }
            }
            k += 1;
        }
        members.sort_unstable();
        patches.push(members);
    }
    let mut new_uvs: Vec<(usize, CornerUvs)> = Vec::new();
    for members in &patches {
        // The old triangles around the patch (sharing a corner with it),
        // by chart (output surface).
        let mut verts: Vec<u32> = members.iter().flat_map(|&i| out.triangles[i]).collect();
        verts.sort_unstable();
        verts.dedup();
        let mut charts: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for i in 0..n {
            if old[i]
                && out.triangles[i]
                    .iter()
                    .any(|v| verts.binary_search(v).is_ok())
            {
                charts.entry(out.surfaces[i]).or_default().push(i);
            }
        }
        if charts.is_empty() {
            let at = out.positions[out.triangles[members[0]][0] as usize];
            return Err(BevelError::Unsupported(
                at,
                "UVs for new faces with no old face around them",
            ));
        }
        for &i in members {
            let nt = normal(&out, i);
            let c = centroid(&out, i);
            // (dot, distance, surface, nearest chart triangle)
            let mut pick: Option<(f64, f64, u32, usize)> = None;
            for (&surface, tris) in &charts {
                let Some(near) = nearest(&out, tris, c, eps) else {
                    continue;
                };
                let dot = nt.dot(normal(&out, near.1));
                let better = match pick {
                    None => true,
                    Some((bd, bdist, _, _)) => {
                        if (dot - bd).abs() > 1e-9 {
                            dot > bd
                        } else {
                            near.0 < bdist
                        }
                    }
                };
                if better {
                    pick = Some((dot, near.0, surface, near.1));
                }
            }
            let Some((_, _, _, r)) = pick else {
                continue;
            };
            // The chart's map from its triangle nearest this one: exact at
            // corners that triangle shares, extended past its edges.
            let own = corners(&out, i).map(|p| out.uv_at(r, p).unwrap_or(DVec2::ZERO));
            new_uvs.push((i, own));
        }
    }
    if let Some(u) = out.uvs.as_mut() {
        for (i, uv) in new_uvs {
            u[i] = uv;
        }
    }
    out.canonicalize()
        .map_err(|_| BevelError::NotFinite("UVs"))?;
    Ok(out)
}
