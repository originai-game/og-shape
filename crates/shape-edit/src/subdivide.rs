//! Subdivision: [`subdivide`] rounds a solid by Catmull and Clark's
//! rule, the way a modeller turns a blocky cage into a smooth form.
//!
//! The solid's flat faces are the cage. Triangles of one surface that lie
//! in one plane across an edge are one face (a box has six, an extruded
//! polygon its two caps and a wall face per side), decided exactly; a
//! face with a hole in it, and anything not flat, stays as its
//! triangles. Each level then replaces every face of n sides by n
//! quadrilaterals round a point at its middle, and moves every corner
//! toward the average of its neighbours, so the cage shrinks toward a
//! smooth surface inside it.
//!
//! Edges named in [`Subdivide::sharp`] stay sharp: their points stay on
//! them, a corner where two meet moves along them only, and a corner
//! where three or more meet stays where it is. So a box with its bottom
//! rim sharp becomes a cushion standing on a flat base.
//!
//! Names: each new face keeps its face's names. A sharp edge's pieces
//! keep its names; an edge that was smoothed away loses them, since no
//! edge is left there. Faces that meet along a smoothed edge become one
//! surface, so the result's real edges are the sharp ones.
//!
//! Ask for all the levels at once. A subdivided solid's quadrilaterals
//! are not flat, so subdividing it a second time starts from its
//! triangles and gives a different, lumpier surface.
//!
//! The arithmetic is `+ - * /` only, in a fixed order, so the result is
//! the same on every host.

use core::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use shape_core::DVec3;
use shape_core::predicates::orient3d;
use shape_mesh::{EdgeKey, Solid, Tag, edge_key};

use crate::EditError;

/// How to subdivide. The zero value is one level with no sharp edge.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Subdivide {
    /// Levels; 0 is one. Each multiplies the faces by about four; at most
    /// [`MAX_LEVELS`].
    pub levels: u32,
    /// Names of edges that stay sharp; none by default.
    pub sharp: Vec<String>,
}

/// The most levels [`subdivide`] takes: six make 4 096 quadrilaterals of
/// each face.
pub const MAX_LEVELS: u32 = 6;

/// A face of the cage: its corners in order, the names it carries and
/// the surface it came from.
#[derive(Clone)]
struct Face {
    corners: Vec<u32>,
    tags: shape_mesh::Tags,
    surface: u32,
}

/// The cage: points, faces, and the sharp edges with their names.
struct Cage {
    points: Vec<DVec3>,
    faces: Vec<Face>,
    sharp: BTreeMap<EdgeKey, Vec<Tag>>,
}

/// The solid subdivided, as [`Subdivide`] says. The solid must be closed
/// and manifold (every edge between exactly two faces), with no skin, UVs
/// or vertex colours: those are refused, not dropped.
pub fn subdivide(s: &Solid, d: &Subdivide) -> Result<Solid, EditError> {
    if shape_mesh::rides(s) {
        // Subdividing is linear in the positions for a given mesh, so
        // each target is exactly what subdividing the shape at that
        // target gives, less the rest shape subdivided; the skin is
        // carried from the surface.
        let mut out = subdivide(&shape_mesh::bare(s), d)?;
        let _ = shape_mesh::carry_redone(s, &mut out, &|moved| subdivide(moved, d).ok());
        return Ok(out);
    }
    if s.uvs.is_some() || s.colors.is_some() {
        return Err(EditError::Unsupported(
            "subdivide does not carry UVs or vertex colours yet: subdivide first, then ask for them",
        ));
    }
    if d.levels > MAX_LEVELS {
        return Err(EditError::Unsupported(
            "subdivide takes at most 6 levels (each makes about four times the faces)",
        ));
    }
    let names: Vec<&str> = d.sharp.iter().map(String::as_str).collect();
    let sharp: BTreeMap<EdgeKey, Vec<Tag>> = if names.is_empty() {
        BTreeMap::new()
    } else {
        s.edges(&names)
            .map_err(|e| EditError::NoSuchEdge(e.missing, e.available))?
            .into_iter()
            .map(|e| (e, s.edge_tags.get(&e).cloned().unwrap_or_default()))
            .collect()
    };
    let mut cage = cage_of(s, sharp)?;
    for _ in 0..d.levels.max(1) {
        cage = refine(&cage)?;
    }
    Ok(solid_of(s, &cage))
}

/// Directed edge → the triangle it belongs to; `NotClosed` unless every
/// edge is used once each way.
fn twins(s: &Solid) -> Result<BTreeMap<(u32, u32), usize>, EditError> {
    let mut of: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for (t, tri) in s.triangles.iter().enumerate() {
        for k in 0..3 {
            if of.insert((tri[k], tri[(k + 1) % 3]), t).is_some() {
                return Err(EditError::NotClosed);
            }
        }
    }
    if of.keys().any(|&(a, b)| !of.contains_key(&(b, a))) {
        return Err(EditError::NotClosed);
    }
    Ok(of)
}

/// The solid's cage: its triangles joined into flat faces.
fn cage_of(s: &Solid, sharp: BTreeMap<EdgeKey, Vec<Tag>>) -> Result<Cage, EditError> {
    let of = twins(s)?;
    // Triangles of one surface, in one plane across an edge that carries
    // no name, are one face.
    let mut group: Vec<usize> = (0..s.triangles.len()).collect();
    fn root(g: &mut [usize], mut i: usize) -> usize {
        while g[i] != i {
            g[i] = g[g[i]];
            i = g[i];
        }
        i
    }
    for (t, tri) in s.triangles.iter().enumerate() {
        for k in 0..3 {
            let (u, v, w) = (tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]);
            let o = of[&(v, u)];
            if o <= t || s.surfaces[o] != s.surfaces[t] {
                continue;
            }
            if s.edge_tags.contains_key(&edge_key(u, v)) || s.face_tags[o] != s.face_tags[t] {
                continue;
            }
            let Some(far) = s.triangles[o].iter().copied().find(|&x| x != u && x != v) else {
                continue;
            };
            let p = |i: u32| s.positions[i as usize];
            if orient3d(p(u), p(v), p(w), p(far)) == Ordering::Equal {
                let (a, b) = (root(&mut group, t), root(&mut group, o));
                group[a.max(b)] = a.min(b);
            }
        }
    }
    let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for t in 0..s.triangles.len() {
        let r = root(&mut group, t);
        members.entry(r).or_default().push(t);
    }
    let mut faces = Vec::new();
    for tris in members.values() {
        let face = |corners: Vec<u32>, t: usize| Face {
            corners,
            tags: s.face_tags[t].clone(),
            surface: s.surfaces[t],
        };
        match outline(s, tris, &group, &of) {
            Some(corners) => faces.push(face(corners, tris[0])),
            // A hole, or an outline that touches itself: its triangles.
            None => faces.extend(tris.iter().map(|&t| face(s.triangles[t].to_vec(), t))),
        }
    }
    Ok(Cage {
        points: s.positions.clone(),
        faces,
        sharp,
    })
}

/// The one simple loop round a group of triangles, if its border is one:
/// its corners in order, starting at the least.
fn outline(
    s: &Solid,
    tris: &[usize],
    group: &[usize],
    of: &BTreeMap<(u32, u32), usize>,
) -> Option<Vec<u32>> {
    let mut next: BTreeMap<u32, u32> = BTreeMap::new();
    for &t in tris {
        let tri = s.triangles[t];
        for k in 0..3 {
            let (u, v) = (tri[k], tri[(k + 1) % 3]);
            // `group` holds roots here: every member was pointed at its
            // root when the members were listed.
            if group[of[&(v, u)]] != group[t] && next.insert(u, v).is_some() {
                return None;
            }
        }
    }
    let start = *next.keys().next()?;
    let mut loop_ = vec![start];
    let mut at = next[&start];
    while at != start {
        loop_.push(at);
        at = *next.get(&at)?;
        if loop_.len() > next.len() {
            return None;
        }
    }
    (loop_.len() == next.len()).then_some(loop_)
}

/// One level of Catmull and Clark's rule.
fn refine(c: &Cage) -> Result<Cage, EditError> {
    let n = c.points.len();
    // Each edge's two faces, and each point's edges and faces.
    let mut edge_faces: BTreeMap<EdgeKey, Vec<usize>> = BTreeMap::new();
    let mut point_faces: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (f, face) in c.faces.iter().enumerate() {
        let m = face.corners.len();
        for k in 0..m {
            let (u, v) = (face.corners[k], face.corners[(k + 1) % m]);
            edge_faces.entry(edge_key(u, v)).or_default().push(f);
            point_faces[u as usize].push(f);
        }
    }
    if edge_faces.values().any(|f| f.len() != 2) {
        return Err(EditError::NotClosed);
    }
    let mut point_edges: Vec<Vec<EdgeKey>> = vec![Vec::new(); n];
    for &e in edge_faces.keys() {
        point_edges[e.0 as usize].push(e);
        point_edges[e.1 as usize].push(e);
    }
    let mean = |ps: &mut dyn Iterator<Item = DVec3>| {
        let mut sum = DVec3::ZERO;
        let mut count = 0.0;
        for p in ps {
            sum += p;
            count += 1.0;
        }
        sum / count
    };
    let at = |i: u32| c.points[i as usize];

    // The new points: the old ones moved, then one a face, then one an edge.
    let face_point: Vec<DVec3> = c
        .faces
        .iter()
        .map(|f| mean(&mut f.corners.iter().map(|&i| at(i))))
        .collect();
    let mut points: Vec<DVec3> = Vec::with_capacity(n + c.faces.len() + edge_faces.len());
    for i in 0..n {
        let p = c.points[i];
        let edges = &point_edges[i];
        let other = |e: &EdgeKey| if e.0 as usize == i { e.1 } else { e.0 };
        let sharp: Vec<u32> = edges
            .iter()
            .filter(|e| c.sharp.contains_key(e))
            .map(other)
            .collect();
        points.push(match sharp.len() {
            0 | 1 => {
                let valence = edges.len() as f64;
                let q = mean(&mut point_faces[i].iter().map(|&f| face_point[f]));
                let r = mean(&mut edges.iter().map(|e| (p + at(other(e))) * 0.5));
                (q + r * 2.0 + p * (valence - 3.0)) / valence
            }
            2 => (p * 6.0 + at(sharp[0]) + at(sharp[1])) / 8.0,
            _ => p,
        });
    }
    let face_at = points.len() as u32;
    points.extend(face_point.iter().copied());
    let mut edge_at: BTreeMap<EdgeKey, u32> = BTreeMap::new();
    for (&e, faces) in &edge_faces {
        let mid = (at(e.0) + at(e.1)) * 0.5;
        let p = if c.sharp.contains_key(&e) {
            mid
        } else {
            (at(e.0) + at(e.1) + face_point[faces[0]] + face_point[faces[1]]) / 4.0
        };
        edge_at.insert(e, points.len() as u32);
        points.push(p);
    }

    let mut faces = Vec::with_capacity(c.faces.len() * 4);
    for (f, face) in c.faces.iter().enumerate() {
        let m = face.corners.len();
        for k in 0..m {
            let (prev, v, next) = (
                face.corners[(k + m - 1) % m],
                face.corners[k],
                face.corners[(k + 1) % m],
            );
            faces.push(Face {
                corners: vec![
                    v,
                    edge_at[&edge_key(v, next)],
                    face_at + f as u32,
                    edge_at[&edge_key(prev, v)],
                ],
                tags: face.tags.clone(),
                surface: face.surface,
            });
        }
    }
    let mut sharp = BTreeMap::new();
    for (e, tags) in &c.sharp {
        if let Some(&m) = edge_at.get(e) {
            sharp.insert(edge_key(e.0, m), tags.clone());
            sharp.insert(edge_key(m, e.1), tags.clone());
        }
    }
    Ok(Cage {
        points,
        faces,
        sharp,
    })
}

/// The cage as a solid: each face fanned from its first corner, faces
/// that meet along a smoothed edge on one surface, sharp edges named.
fn solid_of(source: &Solid, c: &Cage) -> Solid {
    // Surfaces: faces joined across every edge that is not sharp.
    let mut edge_faces: BTreeMap<EdgeKey, Vec<usize>> = BTreeMap::new();
    for (f, face) in c.faces.iter().enumerate() {
        let m = face.corners.len();
        for k in 0..m {
            edge_faces
                .entry(edge_key(face.corners[k], face.corners[(k + 1) % m]))
                .or_default()
                .push(f);
        }
    }
    let mut surface: Vec<usize> = (0..c.faces.len()).collect();
    fn root(g: &mut [usize], mut i: usize) -> usize {
        while g[i] != i {
            g[i] = g[g[i]];
            i = g[i];
        }
        i
    }
    for (e, faces) in &edge_faces {
        if !c.sharp.contains_key(e) && faces.len() == 2 {
            let (a, b) = (root(&mut surface, faces[0]), root(&mut surface, faces[1]));
            surface[a.max(b)] = a.min(b);
        }
    }
    let mut ids: BTreeMap<usize, u32> = BTreeMap::new();
    let mut out = Solid {
        positions: c.points.clone(),
        names: source.names.clone(),
        ..Default::default()
    };
    for (f, face) in c.faces.iter().enumerate() {
        let r = root(&mut surface, f);
        let next = ids.len() as u32;
        let id = *ids.entry(r).or_insert(next);
        for k in 1..face.corners.len() - 1 {
            out.triangles
                .push([face.corners[0], face.corners[k], face.corners[k + 1]]);
            out.surfaces.push(id);
            out.face_tags.push(face.tags.clone());
        }
    }
    let used: BTreeSet<EdgeKey> = edge_faces.keys().copied().collect();
    for (e, tags) in &c.sharp {
        if used.contains(e) && !tags.is_empty() {
            out.edge_tags.insert(*e, tags.clone());
        }
    }
    // Finite positions come out finite; canonical order cannot fail.
    let _ = out.canonicalize();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use shape_gen::solids::{BoxDesc, cuboid};

    fn cube() -> Solid {
        cuboid(&BoxDesc {
            width: 10.0,
            depth: 10.0,
            height: 10.0,
        })
        .unwrap()
    }

    fn levels(n: u32) -> Subdivide {
        Subdivide {
            levels: n,
            ..Default::default()
        }
    }

    #[test]
    fn a_cube_s_six_faces_round_toward_a_ball() {
        let one = subdivide(&cube(), &levels(1)).unwrap();
        let m = one.measure();
        // Six faces of four quadrilaterals, two triangles each; 8 corners,
        // 6 face points and 12 edge points.
        assert_eq!((m.triangles, m.vertices), (48, 26));
        assert!(m.closed && m.degenerate_triangles == 0, "{m:?}");
        // Catmull and Clark's corner: (Q + 2R) / 3 at valence 3.
        let corner = DVec3::new(25.0 / 9.0, 25.0 / 9.0, 70.0 / 9.0);
        assert!(
            one.positions.iter().any(|p| (*p - corner).length() < 1e-12),
            "{:?}",
            one.positions
        );
        // No edge is left sharp: one surface, no real edge, no edge name.
        assert!(one.real_edges().is_empty() && one.edge_tags.is_empty());
        assert_eq!(one.faces(&["top"]).unwrap().len(), 8);

        let three = subdivide(&cube(), &levels(3)).unwrap();
        let m3 = three.measure();
        assert_eq!(m3.triangles, 6 * 64 * 2);
        assert!(
            m3.closed && m3.volume < m.volume && m3.volume > 300.0,
            "{m3:?}"
        );
        // Ask for the levels at once: a subdivided solid's quadrilaterals
        // are not flat, so subdividing it again starts from its triangles.
        let again = subdivide(&one, &levels(1)).unwrap();
        assert_eq!(again.measure().triangles, 48 * 3 * 2);
    }

    #[test]
    fn a_sharp_rim_keeps_the_base_flat_and_its_names() {
        let d = Subdivide {
            levels: 2,
            sharp: vec!["bottom".into()],
        };
        let s = subdivide(&cube(), &d).unwrap();
        let m = s.measure();
        assert!(m.closed, "{m:?}");
        // The base stays in its plane, and the rest rounds above it.
        let lowest = s
            .positions
            .iter()
            .map(|p| p.z)
            .fold(f64::INFINITY, f64::min);
        assert_eq!(lowest, 0.0);
        let base = s.faces(&["bottom"]).unwrap();
        assert!(base.iter().all(|&t| {
            s.triangles[t as usize]
                .iter()
                .all(|&v| s.positions[v as usize].z == 0.0)
        }));
        // Its rim is the only real edge, in pieces that keep the name.
        let rim = s.edges(&["bottom"]).unwrap();
        assert_eq!(rim.len(), 4 * 4);
        assert_eq!(s.real_edges(), rim);
        // More of the cube is left than with nothing sharp.
        assert!(m.volume > subdivide(&cube(), &levels(2)).unwrap().measure().volume);
    }

    #[test]
    fn what_it_cannot_take_is_an_error() {
        let mut open = cube();
        open.triangles.pop();
        open.surfaces.pop();
        open.face_tags.pop();
        assert_eq!(subdivide(&open, &levels(1)), Err(EditError::NotClosed));
        let d = Subdivide {
            sharp: vec!["rim".into()],
            ..Default::default()
        };
        assert!(matches!(
            subdivide(&cube(), &d),
            Err(EditError::NoSuchEdge(missing, _)) if missing == ["rim"]
        ));
        assert!(matches!(
            subdivide(&cube(), &levels(7)),
            Err(EditError::Unsupported(_))
        ));
    }
}
