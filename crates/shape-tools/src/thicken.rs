//! [`thicken`]: an open sheet given a thickness, so it encloses a volume.
//!
//! A generated or scanned surface with open edges (a leaf, a cape, half a
//! shell) has no inside: it cannot be cut, measured for volume or printed.
//! Each piece with an open edge gets a second sheet under it and a rim
//! between the two along its open edges; closed pieces are left as they
//! are.
//!
//! | faces | |
//! |---|---|
//! | the sheet's own names | the sheet, unchanged |
//! | its names and `inner` | the second sheet, facing the other way |
//! | its names and `rim` | the strip along each open edge |
//!
//! The second sheet's vertices lie against each vertex's normal (the sum
//! of its triangles' area normals, so larger faces count for more), moved
//! further where the faces round a vertex turn away from that normal, so
//! flat regions are `thickness` apart and a crease keeps its faces that
//! far apart too (at most four times the distance, at a knife edge). The
//! sheet must be oriented first ([`crate::orient`]): the second sheet goes
//! under it, wherever under is.
//!
//! Where the sheet curves tighter than the thickness the second sheet
//! folds through itself; `measure` with `intersections` counts that.

use std::collections::BTreeMap;

use shape_core::DVec3;
use shape_mesh::{Solid, edge_key};

use crate::repair::RepairError;

/// What [`thicken`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Thickened {
    /// Pieces with an open edge, given a second sheet and a rim.
    pub pieces: usize,
    /// Open pieces left as they were: an edge of theirs has more than
    /// two faces, so they have no one underside
    /// ([`crate::split_non_manifold`] first).
    pub skipped: usize,
    /// Triangles added: the second sheets and the rims.
    pub triangles: usize,
}

/// Gives every piece of `s` with an open edge a second sheet `thickness`
/// mm under it and a rim along its open edges. UVs and colours are
/// carried: the second sheet takes the sheet's, a rim its edge's. A skin
/// is carried too (each new vertex weighs as the one it lies under). The
/// result is in canonical order.
pub fn thicken(s: &Solid, thickness: f64) -> Result<(Solid, Thickened), RepairError> {
    if !(thickness > 0.0 && thickness.is_finite()) {
        return Err(RepairError::BadDistance);
    }
    if s.positions.iter().any(|p| p.is_nan()) {
        return Err(RepairError::NaN);
    }
    let nv = s.positions.len();
    // Pieces: vertices joined by triangles.
    let mut parent: Vec<u32> = (0..nv as u32).collect();
    fn find(parent: &mut [u32], mut v: u32) -> u32 {
        while parent[v as usize] != v {
            parent[v as usize] = parent[parent[v as usize] as usize];
            v = parent[v as usize];
        }
        v
    }
    for t in &s.triangles {
        let a = find(&mut parent, t[0]);
        for &v in &t[1..] {
            let b = find(&mut parent, v);
            let (lo, hi) = (a.min(b), a.max(b));
            parent[hi as usize] = lo;
        }
    }
    let piece: Vec<u32> = (0..nv as u32).map(|v| find(&mut parent, v)).collect();
    let mut uses: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for t in &s.triangles {
        for k in 0..3 {
            *uses.entry(edge_key(t[k], t[(k + 1) % 3])).or_default() += 1;
        }
    }
    // Per piece (by its least vertex): open, and whether it can be done.
    let mut open: BTreeMap<u32, bool> = BTreeMap::new();
    for (&(a, _), &n) in &uses {
        let p = piece[a as usize];
        if n == 1 {
            open.entry(p).or_insert(true);
        }
    }
    for (&(a, _), &n) in &uses {
        if n > 2
            && let Some(ok) = open.get_mut(&piece[a as usize])
        {
            *ok = false;
        }
    }
    let mut report = Thickened {
        pieces: open.values().filter(|&&ok| ok).count(),
        skipped: open.values().filter(|&&ok| !ok).count(),
        triangles: 0,
    };
    let doing = |v: u32| open.get(&piece[v as usize]) == Some(&true);

    // Each vertex's normal and how squarely its faces meet it.
    let mut normal = vec![DVec3::ZERO; nv];
    for t in &s.triangles {
        let [a, b, c] = t.map(|v| s.positions[v as usize]);
        let n = (b - a).cross(c - a);
        for &v in t {
            normal[v as usize] += n;
        }
    }
    let unit: Vec<DVec3> = normal.iter().map(|n| n.normalize_or_zero()).collect();
    let mut facing = vec![(0.0f64, 0u32); nv];
    for t in &s.triangles {
        let [a, b, c] = t.map(|v| s.positions[v as usize]);
        let n = (b - a).cross(c - a).normalize_or_zero();
        for &v in t {
            let f = &mut facing[v as usize];
            f.0 += n.dot(unit[v as usize]);
            f.1 += 1;
        }
    }

    let mut out = s.clone();
    let mut under = vec![u32::MAX; nv];
    for v in 0..nv as u32 {
        if !doing(v) {
            continue;
        }
        let (sum, count) = facing[v as usize];
        let square = if count == 0 {
            1.0
        } else {
            (sum / f64::from(count)).max(0.25)
        };
        let p = s.positions[v as usize] - unit[v as usize] * (thickness / square);
        let k = out.push_vertex(p);
        under[v as usize] = k;
        if let Some(skin) = &mut out.skin {
            skin.weights.resize(k as usize + 1, Default::default());
            skin.weights[k as usize] = skin.weights[v as usize];
        }
        if let Some(m) = &mut out.morphs {
            m.copy(v as usize, k as usize);
        }
    }
    let surfaces = s.surfaces.iter().copied().max().map_or(0, |m| m + 1);
    let names = |i: usize, extra: &'static str| -> Vec<String> {
        let mut tags: Vec<String> = s.face_tags[i]
            .iter()
            .map(|&x| s.names.name(x).to_string())
            .collect();
        tags.push(extra.to_string());
        tags
    };
    let mut add = |out: &mut Solid,
                   i: usize,
                   t: [u32; 3],
                   corners: [usize; 3],
                   extra: &'static str,
                   surface: u32| {
        let tags = names(i, extra);
        let tags: Vec<&str> = tags.iter().map(String::as_str).collect();
        match &s.uvs {
            Some(u) => out.push_triangle_uv(t, surface, &tags, corners.map(|k| u[i][k])),
            None => out.push_triangle(t, surface, &tags),
        }
        if let Some(c) = &s.colors {
            let at = out.triangles.len() - 1;
            out.set_colors(at, corners.map(|k| c[i][k]));
        }
        report.triangles += 1;
    };
    for (i, t) in s.triangles.iter().enumerate() {
        if !doing(t[0]) {
            continue;
        }
        let u = t.map(|v| under[v as usize]);
        // The second sheet, facing the other way.
        add(
            &mut out,
            i,
            [u[0], u[2], u[1]],
            [0, 2, 1],
            "inner",
            surfaces + s.surfaces[i],
        );
        // The rim along each open edge, facing out of the gap.
        for k in 0..3 {
            let k1 = (k + 1) % 3;
            if uses[&edge_key(t[k], t[k1])] != 1 {
                continue;
            }
            add(
                &mut out,
                i,
                [t[k1], t[k], u[k]],
                [k1, k, k],
                "rim",
                2 * surfaces,
            );
            add(
                &mut out,
                i,
                [t[k1], u[k], u[k1]],
                [k1, k, k1],
                "rim",
                2 * surfaces,
            );
        }
    }
    out.canonicalize()?;
    Ok((out, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shape_core::DVec2;

    /// A flat sheet of `n` × `n` squares of side 1 in z = 0, facing +z,
    /// with UVs and colours.
    fn sheet(n: u32) -> Solid {
        let mut s = Solid::default();
        for y in 0..=n {
            for x in 0..=n {
                s.push_vertex(DVec3::new(f64::from(x), f64::from(y), 0.0));
            }
        }
        let at = |x: u32, y: u32| y * (n + 1) + x;
        let uv = |x: u32, y: u32| DVec2::new(f64::from(x), f64::from(y)) / f64::from(n);
        for y in 0..n {
            for x in 0..n {
                let q = [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)];
                for c in [[0, 1, 2], [0, 2, 3]] {
                    let t = c.map(|k| at(q[k].0, q[k].1));
                    s.push_triangle_uv(t, 0, &["leaf"], c.map(|k| uv(q[k].0, q[k].1)));
                    let i = s.triangles.len() - 1;
                    s.set_colors(i, [[0.5, 0.25, 0.125, 1.0]; 3]);
                }
            }
        }
        s
    }

    #[test]
    fn a_flat_sheet_becomes_a_slab_of_its_thickness() {
        let s = sheet(3);
        let (o, r) = thicken(&s, 0.5).unwrap();
        let m = o.measure();
        assert!(m.closed && m.non_manifold_edges == 0, "{m:?}");
        assert!((m.volume - 9.0 * 0.5).abs() < 1e-12, "{}", m.volume);
        // 18 under, and two a rim edge (12 of them).
        assert_eq!(
            r,
            Thickened {
                pieces: 1,
                skipped: 0,
                triangles: 18 + 24
            }
        );
        assert_eq!(o.faces(&["leaf"]).unwrap().len(), 18 + 18 + 24);
        assert_eq!(o.faces(&["inner"]).unwrap().len(), 18);
        assert_eq!(o.faces(&["rim"]).unwrap().len(), 24);
        // The slab lies under the sheet, and UVs and colours came along.
        assert_eq!((m.bounds_min.z, m.bounds_max.z), (-0.5, 0.0));
        assert_eq!(o.uvs.as_ref().unwrap().len(), o.triangles.len());
        assert!(
            o.colors
                .as_ref()
                .unwrap()
                .iter()
                .flatten()
                .all(|c| c[0] == 0.5)
        );
    }

    #[test]
    fn a_crease_keeps_its_faces_apart_and_closed_pieces_are_left() {
        // Two squares meeting at a right angle along x: an L in section.
        let mut s = Solid::default();
        for p in [
            [0.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [4.0, 4.0, 0.0],
            [0.0, 4.0, 0.0],
            [0.0, 0.0, 4.0],
            [4.0, 0.0, 4.0],
        ] {
            s.push_vertex(DVec3::from_array(p));
        }
        // Floor facing +z, wall (in y = 0) facing +y.
        for t in [[0, 1, 2], [0, 2, 3], [0, 5, 1], [0, 4, 5]] {
            s.push_triangle(t, 0, &["l"]);
        }
        // A closed tetrahedron apart from it.
        let base = s.positions.len() as u32;
        for p in [
            [10.0, 0.0, 0.0],
            [11.0, 0.0, 0.0],
            [10.0, 1.0, 0.0],
            [10.0, 0.0, 1.0],
        ] {
            s.push_vertex(DVec3::from_array(p));
        }
        for t in [[0, 2, 1], [0, 1, 3], [1, 2, 3], [0, 3, 2]] {
            s.push_triangle(t.map(|v| v + base), 1, &["tet"]);
        }
        let (o, r) = thicken(&s, 1.0).unwrap();
        assert_eq!((r.pieces, r.skipped), (1, 0));
        let m = o.measure();
        assert!(m.closed, "{m:?}");
        // The tetrahedron is as it was.
        assert_eq!(o.faces(&["tet"]).unwrap().len(), 4);
        // Along the crease the under-vertex is 1 from both faces: at
        // (·, −1, −1), not 1/√2 from each.
        let under = o
            .positions
            .iter()
            .filter(|p| p.z < -0.5 && p.y < -0.5)
            .count();
        assert_eq!(under, 2);
        assert!(
            o.positions
                .iter()
                .any(|p| (p.y + 1.0).abs() < 1e-12 && (p.z + 1.0).abs() < 1e-12)
        );
        assert!(matches!(thicken(&s, 0.0), Err(RepairError::BadDistance)));
    }
}
