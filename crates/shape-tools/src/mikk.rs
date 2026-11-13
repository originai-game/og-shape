//! MikkTSpace: the tangent frames that Blender, Unity, Unreal and most
//! bakers agree on, so that a normal map baked against one reads right
//! in another (`docs/research.md` §56).
//!
//! Entry: [`tangents`]. Ported from Morten S. Mikkelsen's
//! `mikktspace.c` (zlib licence, `third_party/mikktspace`), for
//! triangles only and in f64; an altered version, as that licence asks
//! to be said. The steps are the reference's:
//!
//! 1. Corners at one position with one normal and one UV are one vertex.
//! 2. Each triangle's ∂P/∂u and ∂P/∂v, made unit, with whether its UVs
//!    keep its winding; a triangle with no UV area joins any group.
//! 3. Triangles are neighbours across an edge they wind opposite ways.
//! 4. Round a vertex, the triangles reached through neighbours that
//!    wind their UVs the same way are one group.
//! 5. A corner's tangent is the sum over its group of each triangle's
//!    ∂P/∂u, laid in the plane of the vertex's normal and made unit,
//!    weighted by the triangle's angle at the vertex; its sign is the
//!    group's winding.

use std::collections::BTreeMap;

use shape_core::{DVec2, DVec3, trig};

/// A corner's tangent: the unit direction of +u in the plane of its
/// normal (zero where the mesh gives none), and the side +v falls on
/// (1: `normal × tangent`; −1: the other).
pub(crate) type Tangent = (DVec3, f64);

fn unit_or_same(v: DVec3) -> DVec3 {
    let l = v.length();
    if l > f64::MIN_POSITIVE { v / l } else { v }
}

/// `v` in the plane square to the unit `n`, made unit.
fn in_plane(v: DVec3, n: DVec3) -> DVec3 {
    unit_or_same(v - n * n.dot(v))
}

struct Tri {
    /// Its corners' vertices (step 1).
    v: [u32; 3],
    os: DVec3,
    ot: DVec3,
    /// Its UVs keep its winding.
    keeps: bool,
    /// No UV area: it joins any group.
    any: bool,
    /// Two corners at one position: it takes no part.
    flat: bool,
    neighbours: [Option<u32>; 3],
    /// Each corner's group.
    group: [Option<u32>; 3],
}

struct Group {
    vertex: u32,
    keeps: bool,
    faces: Vec<u32>,
}

/// Each triangle's corners' tangents, from its corners' positions, unit
/// normals and UVs.
pub(crate) fn tangents(
    positions: &[[DVec3; 3]],
    normals: &[[DVec3; 3]],
    uvs: &[[DVec2; 3]],
) -> Vec<[Tangent; 3]> {
    let count = positions.len();
    // 1. One vertex a position, normal and UV.
    let mut ids: BTreeMap<[u64; 8], u32> = BTreeMap::new();
    let bits = |v: f64| (if v == 0.0 { 0.0 } else { v }).to_bits();
    let mut tris: Vec<Tri> = Vec::with_capacity(count);
    for f in 0..count {
        let v = core::array::from_fn(|k| {
            let (p, n, t) = (positions[f][k], normals[f][k], uvs[f][k]);
            let key = [p.x, p.y, p.z, n.x, n.y, n.z, t.x, t.y].map(bits);
            let next = ids.len() as u32;
            *ids.entry(key).or_insert(next)
        });
        let [p0, p1, p2] = positions[f];
        let flat = p0 == p1 || p0 == p2 || p1 == p2;
        // 2. The first derivatives (the reference's equations 18, 19).
        let (t21, t31) = (uvs[f][1] - uvs[f][0], uvs[f][2] - uvs[f][0]);
        let (d1, d2) = (p1 - p0, p2 - p0);
        let area = t21.x * t31.y - t21.y * t31.x;
        let os = d1 * t31.y - d2 * t21.y;
        let ot = d2 * t21.x - d1 * t31.x;
        let keeps = area > 0.0;
        let mut tri = Tri {
            v,
            os: DVec3::ZERO,
            ot: DVec3::ZERO,
            keeps,
            any: true,
            flat,
            neighbours: [None; 3],
            group: [None; 3],
        };
        if area.abs() > f64::MIN_POSITIVE {
            let sign = if keeps { 1.0 } else { -1.0 };
            let (ls, lt) = (os.length(), ot.length());
            if ls > f64::MIN_POSITIVE {
                tri.os = os * (sign / ls);
            }
            if lt > f64::MIN_POSITIVE {
                tri.ot = ot * (sign / lt);
            }
            tri.any = !(ls / area.abs() > f64::MIN_POSITIVE && lt / area.abs() > f64::MIN_POSITIVE);
        }
        tris.push(tri);
    }
    // 3. Neighbours: across an edge, the next triangle that winds it
    // the other way and has no neighbour there yet.
    let mut edges: BTreeMap<(u32, u32), Vec<(u32, u8)>> = BTreeMap::new();
    for (f, t) in tris.iter().enumerate().filter(|(_, t)| !t.flat) {
        for k in 0..3 {
            let (a, b) = (t.v[k], t.v[(k + 1) % 3]);
            edges
                .entry((a.min(b), a.max(b)))
                .or_default()
                .push((f as u32, k as u8));
        }
    }
    for users in edges.values() {
        for i in 0..users.len() {
            let (f, k) = users[i];
            if tris[f as usize].neighbours[k as usize].is_some() {
                continue;
            }
            let from = tris[f as usize].v[k as usize];
            let found = users[i + 1..].iter().find(|&&(g, j)| {
                let other = &tris[g as usize];
                g != f
                    && other.neighbours[j as usize].is_none()
                    && other.v[(j as usize + 1) % 3] == from
            });
            if let Some(&(g, j)) = found {
                tris[f as usize].neighbours[k as usize] = Some(g);
                tris[g as usize].neighbours[j as usize] = Some(f);
            }
        }
    }
    // 4. Groups round each vertex.
    let mut groups: Vec<Group> = Vec::new();
    for f in 0..count {
        for i in 0..3 {
            if tris[f].any || tris[f].flat || tris[f].group[i].is_some() {
                continue;
            }
            let id = groups.len() as u32;
            let (vertex, keeps) = (tris[f].v[i], tris[f].keeps);
            let mut faces = vec![f as u32];
            tris[f].group[i] = Some(id);
            // The neighbour across the edge leaving the corner, then
            // the one across the edge arriving, each followed to its
            // end before the next (the reference's recursion).
            let mut stack = vec![tris[f].neighbours[(i + 2) % 3], tris[f].neighbours[i]];
            while let Some(next) = stack.pop() {
                let Some(g) = next else { continue };
                let t = &mut tris[g as usize];
                let Some(j) = t.v.iter().position(|&v| v == vertex) else {
                    continue;
                };
                if t.group[j].is_some() {
                    continue;
                }
                if t.any && t.group.iter().all(Option::is_none) {
                    // The first group to reach it gives it its winding.
                    t.keeps = keeps;
                }
                if t.keeps != keeps {
                    continue;
                }
                t.group[j] = Some(id);
                faces.push(g);
                stack.push(t.neighbours[(j + 2) % 3]);
                stack.push(t.neighbours[j]);
            }
            groups.push(Group {
                vertex,
                keeps,
                faces,
            });
        }
    }
    // 5. A tangent a corner.
    let mut out = vec![[(DVec3::ZERO, 1.0); 3]; count];
    let mut done = vec![[false; 3]; count];
    let corner_of = |t: &Tri, vertex: u32, id: u32| {
        (0..3).find(|&k| t.v[k] == vertex && t.group[k] == Some(id))
    };
    for (id, group) in groups.iter().enumerate() {
        // The sub-groups met so far, with their tangents. (The
        // reference splits a group where two triangles' derivatives
        // turn more than its angular threshold; at its default of 180°
        // that is only where they point exactly opposite ways.)
        let mut subs: Vec<(Vec<u32>, DVec3)> = Vec::new();
        for &f in &group.faces {
            let tf = &tris[f as usize];
            let Some(index) = corner_of(tf, group.vertex, id as u32) else {
                continue;
            };
            let n = normals[f as usize][index];
            let (os, ot) = (in_plane(tf.os, n), in_plane(tf.ot, n));
            let mut members: Vec<u32> = group
                .faces
                .iter()
                .copied()
                .filter(|&t| {
                    let tt = &tris[t as usize];
                    tf.any
                        || tt.any
                        || (os.dot(in_plane(tt.os, n)) > -1.0 && ot.dot(in_plane(tt.ot, n)) > -1.0)
                })
                .collect();
            members.sort_unstable();
            let tangent = match subs.iter().find(|(m, _)| *m == members) {
                Some((_, t)) => *t,
                None => {
                    let mut sum = DVec3::ZERO;
                    for &t in &members {
                        let tt = &tris[t as usize];
                        if tt.any {
                            continue;
                        }
                        let Some(i) = tt.v.iter().position(|&v| v == group.vertex) else {
                            continue;
                        };
                        let [before, here, after] =
                            [(i + 2) % 3, i, (i + 1) % 3].map(|k| positions[t as usize][k]);
                        let (v1, v2) = (in_plane(before - here, n), in_plane(after - here, n));
                        let angle = trig::acos(v1.dot(v2).clamp(-1.0, 1.0));
                        sum += in_plane(tt.os, n) * angle;
                    }
                    let t = unit_or_same(sum);
                    subs.push((members, t));
                    t
                }
            };
            out[f as usize][index] = (tangent, if group.keeps { 1.0 } else { -1.0 });
            done[f as usize][index] = true;
        }
    }
    // A triangle that took no part has, at each corner, the tangent of
    // a corner that did at the same vertex.
    let mut known: BTreeMap<u32, Tangent> = BTreeMap::new();
    for f in 0..count {
        for k in 0..3 {
            if done[f][k] {
                known.entry(tris[f].v[k]).or_insert(out[f][k]);
            }
        }
    }
    for f in 0..count {
        for k in 0..3 {
            if !done[f][k]
                && let Some(t) = known.get(&tris[f].v[k])
            {
                out[f][k] = *t;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two flat triangles round a vertex whose UVs bend at the edge
    /// between them: the shared corners' tangent is each triangle's
    /// ∂P/∂u weighted by its angle there (60° and 30° at the first
    /// vertex), not by area or equally. Where the second triangle's UVs
    /// fold back over the first's, it winds the other way: its corners
    /// take the other sign and a tangent of their own.
    #[test]
    fn a_corner_s_tangent_is_weighted_by_angle() {
        let n = DVec3::Z;
        let (s60, c60) = trig::sin_cos(trig::to_radians(60.0));
        let o = DVec3::ZERO;
        let a = DVec3::new(10.0, 0.0, 0.0);
        let b = DVec3::new(c60, s60, 0.0);
        let c = DVec3::new(0.0, 5.0, 0.0);
        let positions = [[o, a, b], [o, b, c]];
        let normals = [[n; 3]; 2];
        let (uo, ua, ub) = (DVec2::ZERO, DVec2::new(1.0, 0.0), DVec2::new(0.5, 1.0));
        // Each triangle's unit ∂P/∂u, from its own corners.
        let du = |p: [DVec3; 3], t: [DVec2; 3]| {
            let (d1, d2) = (p[1] - p[0], p[2] - p[0]);
            let (t1, t2) = (t[1] - t[0], t[2] - t[0]);
            let sign = if t1.x * t2.y - t1.y * t2.x > 0.0 {
                1.0
            } else {
                -1.0
            };
            (d1 * t2.y - d2 * t1.y).normalize() * sign
        };
        let uc = DVec2::new(-0.5, 0.4);
        let uvs = [[uo, ua, ub], [uo, ub, uc]];
        let t = tangents(&positions, &normals, &uvs);
        let (first, second) = (du(positions[0], uvs[0]), du(positions[1], uvs[1]));
        assert!(first.dot(second) < 0.9, "the UVs bend: {first} {second}");
        let at_o = (first * trig::to_radians(60.0) + second * trig::to_radians(30.0)).normalize();
        for (f, k) in [(0, 0), (1, 0)] {
            assert!((t[f][k].0 - at_o).length() < 1e-9, "{:?} {at_o}", t[f][k]);
            assert_eq!(t[f][k].1, 1.0);
        }
        // `a` is the first triangle's alone.
        assert!((t[0][1].0 - first).length() < 1e-12);
        // Fold the second triangle's UVs back over the first's.
        let folded = [uvs[0], [uo, ub, DVec2::new(0.9, 0.5)]];
        let m = tangents(&positions, &normals, &folded);
        assert_eq!((m[0][0].1, m[1][0].1), (1.0, -1.0));
        assert!((m[0][0].0 - first).length() < 1e-12);
        assert!((m[1][0].0 - du(positions[1], folded[1])).length() < 1e-12);
    }
}
