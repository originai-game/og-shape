//! The kernel's solid: a triangle mesh in f64 with names on its faces and
//! edges (`docs/design.md` §4), canonical ordering (§6) and measures (§7).
//!
//! Entry: [`Solid`]. A generator builds one, an operation consumes one and
//! returns another, and every operation ends with [`Solid::canonicalize`].

mod measure;
mod names;
pub mod skin;

pub use measure::Measures;
pub use names::{LookupError, Names, Tag};
pub use skin::{Bone, Skin, SkinError, Weights};

use std::collections::BTreeMap;

use shape_core::order::{cmp_dvec3, sorted_order};
use shape_core::{DVec3, HashError, OutputHash};

/// An undirected edge, by its two vertex indices, smaller first.
pub type EdgeKey = (u32, u32);

/// The edge key of `a`–`b`.
pub fn edge_key(a: u32, b: u32) -> EdgeKey {
    (a.min(b), a.max(b))
}

/// A triangle mesh with names.
///
/// - `positions`: vertices, welded by exact position after
///   [`Solid::canonicalize`].
/// - `triangles`: counter-clockwise seen from outside.
/// - `surfaces`: per triangle, the surface it belongs to. Triangles of one
///   surface are one smooth face; an edge between two surfaces is a real
///   edge.
/// - `face_tags`: per triangle, the names of its face (sorted tag ids).
/// - `edge_tags`: the names of real edges.
/// - `names`: the strings behind the tag ids.
/// - `skin`: optional bones and per-vertex weights ([`skin`]). `None`
///   leaves every other part, and the hash, as it was before skins.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Solid {
    /// Vertex positions.
    pub positions: Vec<DVec3>,
    /// Triangles, counter-clockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
    /// Per triangle, its surface id.
    pub surfaces: Vec<u32>,
    /// Per triangle, its face names (sorted tags).
    pub face_tags: Vec<Vec<Tag>>,
    /// Names of real edges (sorted tags).
    pub edge_tags: BTreeMap<EdgeKey, Vec<Tag>>,
    /// The strings behind the tags.
    pub names: Names,
    /// Bones and per-vertex weights, if the solid is skinned.
    pub skin: Option<Skin>,
}

impl Solid {
    /// Appends a triangle on `surface` with the given face names.
    pub fn push_triangle(&mut self, t: [u32; 3], surface: u32, tags: &[&str]) {
        let mut ids: Vec<Tag> = tags.iter().map(|n| self.names.intern(n)).collect();
        ids.sort_unstable();
        ids.dedup();
        self.triangles.push(t);
        self.surfaces.push(surface);
        self.face_tags.push(ids);
    }

    /// Appends a vertex and returns its index. On a skinned solid the
    /// vertex has no weights until it is given some ([`Skin::check`]
    /// reports it).
    pub fn push_vertex(&mut self, p: DVec3) -> u32 {
        if let Some(skin) = &mut self.skin {
            skin.weights
                .resize(self.positions.len(), Weights::default());
            skin.weights.push(Weights::default());
        }
        self.positions.push(p);
        self.positions.len() as u32 - 1
    }

    /// Adds names to an edge.
    pub fn tag_edge(&mut self, a: u32, b: u32, tags: &[&str]) {
        let ids: Vec<Tag> = tags.iter().map(|n| self.names.intern(n)).collect();
        let e = self.edge_tags.entry(edge_key(a, b)).or_default();
        e.extend(ids);
        e.sort_unstable();
        e.dedup();
    }

    /// Adds names to every face of the solid, e.g. its part name.
    pub fn tag_all_faces(&mut self, tag: &str) {
        let id = self.names.intern(tag);
        for t in &mut self.face_tags {
            if let Err(i) = t.binary_search(&id) {
                t.insert(i, id);
            }
        }
    }

    /// Triangles whose face carries any of `names`. A name that no face
    /// carries is an error that lists the names faces do carry.
    pub fn faces(&self, names: &[&str]) -> Result<Vec<u32>, LookupError> {
        let ids = self.lookup(names, self.face_tags.iter().flatten().copied())?;
        Ok((0..self.triangles.len() as u32)
            .filter(|&i| self.face_tags[i as usize].iter().any(|t| ids.contains(t)))
            .collect())
    }

    /// Edges carrying any of `names`, in key order. A name no edge carries
    /// is an error that lists the edge names that exist.
    pub fn edges(&self, names: &[&str]) -> Result<Vec<EdgeKey>, LookupError> {
        let ids = self.lookup(names, self.edge_tags.values().flatten().copied())?;
        Ok(self
            .edge_tags
            .iter()
            .filter(|(_, t)| t.iter().any(|x| ids.contains(x)))
            .map(|(&e, _)| e)
            .collect())
    }

    fn lookup(
        &self,
        names: &[&str],
        used: impl Iterator<Item = Tag>,
    ) -> Result<Vec<Tag>, LookupError> {
        let mut present: Vec<Tag> = used.collect();
        present.sort_unstable();
        present.dedup();
        let mut ids = Vec::new();
        let mut missing = Vec::new();
        for n in names {
            match self.names.get(n) {
                Some(id) if present.binary_search(&id).is_ok() => ids.push(id),
                _ => missing.push(n.to_string()),
            }
        }
        if missing.is_empty() {
            Ok(ids)
        } else {
            let mut available: Vec<String> = present
                .iter()
                .map(|&t| self.names.name(t).to_string())
                .collect();
            available.sort();
            Err(LookupError { missing, available })
        }
    }

    /// The real edges: every edge between triangles of different surfaces,
    /// plus every boundary edge, in key order.
    pub fn real_edges(&self) -> Vec<EdgeKey> {
        let mut by_edge: BTreeMap<EdgeKey, Vec<u32>> = BTreeMap::new();
        for (i, t) in self.triangles.iter().enumerate() {
            for k in 0..3 {
                by_edge
                    .entry(edge_key(t[k], t[(k + 1) % 3]))
                    .or_default()
                    .push(i as u32);
            }
        }
        by_edge
            .into_iter()
            .filter(|(_, ts)| {
                ts.len() != 2 || self.surfaces[ts[0] as usize] != self.surfaces[ts[1] as usize]
            })
            .map(|(e, _)| e)
            .collect()
    }

    /// Welds by exact position, drops triangles with two corners at one
    /// point, sorts vertices by position and triangles by their vertices,
    /// and renumbers surfaces in order of first use. The result depends
    /// only on the geometry and names, never on the order things were
    /// built in (`docs/design.md` §6).
    ///
    /// A skin's weights follow their vertices. Vertices welded into one
    /// take the mean of their weights ([`Skin::weld`]: the copies sorted,
    /// summed per bone, the four largest kept, normalized); a copy with no
    /// weights does not count.
    ///
    /// Fails on a NaN position.
    pub fn canonicalize(&mut self) -> Result<(), HashError> {
        if self.positions.iter().any(|p| p.is_nan()) {
            return Err(HashError::NaN);
        }
        // Tags in name order first, so ties between triangles break the
        // same way whatever order the names were interned in.
        self.names.compact(&mut self.face_tags, &mut self.edge_tags);
        // 1. Vertices: sorted, equal positions merged (-0.0 equals 0.0).
        let order = sorted_order(&self.positions);
        let mut remap = vec![0u32; self.positions.len()];
        let mut positions: Vec<DVec3> = Vec::with_capacity(self.positions.len());
        for &old in &order {
            let p = self.positions[old as usize] + DVec3::ZERO;
            if positions.last().is_none_or(|q| cmp_dvec3(*q, p).is_ne()) {
                positions.push(p);
            }
            remap[old as usize] = positions.len() as u32 - 1;
        }
        if let Some(skin) = &mut self.skin {
            skin.weights
                .resize(self.positions.len(), Weights::default());
            let mut welded = Vec::with_capacity(positions.len());
            let mut copies: Vec<Weights> = Vec::new();
            for (k, &old) in order.iter().enumerate() {
                copies.push(skin.weights[old as usize]);
                let last = order
                    .get(k + 1)
                    .is_none_or(|&next| remap[next as usize] != remap[old as usize]);
                if last {
                    welded.push(Skin::weld(&mut copies));
                    copies.clear();
                }
            }
            skin.weights = welded;
        }
        // 2. Triangles: renumbered, degenerate ones dropped, each rotated
        // to start at its smallest index (winding kept).
        let mut tris: Vec<([u32; 3], u32, Vec<Tag>)> = Vec::with_capacity(self.triangles.len());
        for (i, t) in self.triangles.iter().enumerate() {
            let t = t.map(|v| remap[v as usize]);
            if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
                continue;
            }
            let r = (0..3).min_by_key(|&k| t[k]).unwrap_or(0);
            let t = [t[r], t[(r + 1) % 3], t[(r + 2) % 3]];
            tris.push((t, self.surfaces[i], self.face_tags[i].clone()));
        }
        tris.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        // 3. Surfaces renumbered by first use.
        let mut surface_ids: BTreeMap<u32, u32> = BTreeMap::new();
        for t in &mut tris {
            let n = surface_ids.len() as u32;
            t.1 = *surface_ids.entry(t.1).or_insert(n);
        }
        // 4. Edge names follow their vertices; names on edges that no
        // longer exist are dropped.
        let mut present: BTreeMap<EdgeKey, ()> = BTreeMap::new();
        for (t, _, _) in &tris {
            for k in 0..3 {
                present.insert(edge_key(t[k], t[(k + 1) % 3]), ());
            }
        }
        let mut edge_tags: BTreeMap<EdgeKey, Vec<Tag>> = BTreeMap::new();
        for (&(a, b), tags) in &self.edge_tags {
            let e = edge_key(remap[a as usize], remap[b as usize]);
            if e.0 != e.1 && present.contains_key(&e) {
                let entry = edge_tags.entry(e).or_default();
                entry.extend(tags);
                entry.sort_unstable();
                entry.dedup();
            }
        }
        self.positions = positions;
        self.triangles = tris.iter().map(|t| t.0).collect();
        self.surfaces = tris.iter().map(|t| t.1).collect();
        self.face_tags = tris.into_iter().map(|t| t.2).collect();
        self.edge_tags = edge_tags;
        // Again: names on dropped triangles and edges are gone now.
        self.names.compact(&mut self.face_tags, &mut self.edge_tags);
        Ok(())
    }

    /// Writes the whole solid, names included, into an output hash.
    pub fn hash_into(&self, h: &mut OutputHash) -> Result<(), HashError> {
        h.str("positions");
        h.dvec3s(&self.positions)?;
        h.str("triangles");
        h.u64(self.triangles.len() as u64);
        for (i, t) in self.triangles.iter().enumerate() {
            t.iter().for_each(|&v| h.u64(u64::from(v)));
            h.u64(u64::from(self.surfaces[i]));
            h.u64(self.face_tags[i].len() as u64);
            self.face_tags[i]
                .iter()
                .for_each(|&x| h.str(self.names.name(x)));
        }
        h.str("edges");
        h.u64(self.edge_tags.len() as u64);
        for (&(a, b), tags) in &self.edge_tags {
            h.u64(u64::from(a));
            h.u64(u64::from(b));
            h.u64(tags.len() as u64);
            tags.iter().for_each(|&x| h.str(self.names.name(x)));
        }
        // Only a skinned solid adds to the stream, so unskinned hashes stay
        // as they were.
        if let Some(skin) = &self.skin {
            h.str("skin");
            h.u64(skin.bones.len() as u64);
            for b in &skin.bones {
                h.str(&b.name);
                h.u64(b.parent.map_or(0, |p| u64::from(p) + 1));
                let f = &b.rest;
                h.dvec3s(&[f.origin, f.x, f.y, f.z])?;
                h.f64s(&b.inverse_bind)?;
            }
            h.u64(skin.weights.len() as u64);
            for w in &skin.weights {
                w.bones.iter().for_each(|&b| h.u64(u64::from(b)));
                h.f64s(&w.weights)?;
            }
        }
        Ok(())
    }

    /// Appends another solid; its surfaces are renumbered after this one's.
    ///
    /// Skins: when both solids have the same skeleton, the weights are
    /// appended with the vertices. Otherwise the result keeps this
    /// solid's skeleton (or the other's, if this one has none) and the
    /// vertices from the side without it have no weights, which
    /// [`Skin::check`] reports: a skin is never dropped silently.
    pub fn append(&mut self, other: &Solid) {
        let base = self.positions.len() as u32;
        if self.skin.is_some() || other.skin.is_some() {
            let mine = self.skin.get_or_insert_with(|| Skin {
                bones: other
                    .skin
                    .as_ref()
                    .map(|s| s.bones.clone())
                    .unwrap_or_default(),
                weights: Vec::new(),
            });
            mine.weights.resize(base as usize, Weights::default());
            match &other.skin {
                Some(o) if o.bones == mine.bones => {
                    let mut w = o.weights.clone();
                    w.resize(other.positions.len(), Weights::default());
                    mine.weights.extend(w);
                }
                _ => mine
                    .weights
                    .resize(base as usize + other.positions.len(), Weights::default()),
            }
        }
        let surface_base = self.surfaces.iter().max().map_or(0, |s| s + 1);
        self.positions.extend_from_slice(&other.positions);
        for (i, t) in other.triangles.iter().enumerate() {
            let tags: Vec<&str> = other.face_tags[i]
                .iter()
                .map(|&x| other.names.name(x))
                .collect();
            self.push_triangle(t.map(|v| v + base), other.surfaces[i] + surface_base, &tags);
        }
        for (&(a, b), tags) in &other.edge_tags {
            let tags: Vec<&str> = tags.iter().map(|&x| other.names.name(x)).collect();
            self.tag_edge(a + base, b + base, &tags);
        }
    }

    /// Every vertex mapped by `f`. A map that mirrors (`mirrors == true`)
    /// reverses each triangle so it still faces out. A skin's weights stay
    /// with their vertices; its bones are not moved (`f` need not be
    /// rigid), so place a solid before skinning it.
    pub fn map_positions(&mut self, f: impl Fn(DVec3) -> DVec3, mirrors: bool) {
        for p in &mut self.positions {
            *p = f(*p);
        }
        if mirrors {
            for t in &mut self.triangles {
                t.swap(1, 2);
            }
        }
    }

    /// Measures for the solid (`docs/design.md` §7).
    pub fn measure(&self) -> Measures {
        measure::measure(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tetra(offset: DVec3, reversed_build: bool) -> Solid {
        let mut s = Solid::default();
        let p = [DVec3::ZERO, DVec3::X, DVec3::Y, DVec3::Z].map(|p| p + offset);
        let order: Vec<usize> = if reversed_build {
            vec![3, 2, 1, 0]
        } else {
            vec![0, 1, 2, 3]
        };
        let mut id = [0u32; 4];
        for &i in &order {
            id[i] = s.push_vertex(p[i]);
        }
        let faces = [
            ([0, 2, 1], "bottom"),
            ([0, 1, 3], "side"),
            ([0, 3, 2], "side"),
            ([1, 2, 3], "top"),
        ];
        let faces: Vec<_> = if reversed_build {
            faces.iter().rev().collect()
        } else {
            faces.iter().collect()
        };
        for (f, name) in faces {
            s.push_triangle(
                f.map(|k| id[k]),
                if *name == "side" { 1 } else { 0 },
                &[name],
            );
        }
        s.tag_edge(id[1], id[2], &["top"]);
        s
    }

    #[test]
    fn canonical_order_is_independent_of_build_order() {
        let mut a = tetra(DVec3::ZERO, false);
        let mut b = tetra(DVec3::ZERO, true);
        a.canonicalize().unwrap();
        b.canonicalize().unwrap();
        let (mut ha, mut hb) = (OutputHash::new(), OutputHash::new());
        a.hash_into(&mut ha).unwrap();
        b.hash_into(&mut hb).unwrap();
        assert_eq!(ha.finish_hex(), hb.finish_hex());
    }

    #[test]
    fn lookup_misses_list_the_names_that_exist() {
        let mut s = tetra(DVec3::ZERO, false);
        s.canonicalize().unwrap();
        assert_eq!(s.faces(&["top"]).unwrap().len(), 1);
        assert_eq!(s.faces(&["top", "side"]).unwrap().len(), 3);
        let err = s.faces(&["lid"]).unwrap_err();
        assert_eq!(err.missing, vec!["lid"]);
        assert_eq!(err.available, vec!["bottom", "side", "top"]);
        assert_eq!(s.edges(&["top"]).unwrap().len(), 1);
    }

    #[test]
    fn welding_drops_degenerate_triangles() {
        let mut s = tetra(DVec3::ZERO, false);
        let a = s.push_vertex(DVec3::X);
        s.push_triangle([a, 1, 2], 0, &["sliver"]);
        s.canonicalize().unwrap();
        assert_eq!(s.triangles.len(), 4);
        assert!(s.faces(&["sliver"]).is_err());
    }

    #[test]
    fn welding_averages_the_weights_of_merged_vertices() {
        // Two copies of the tetrahedron's apex with different weights,
        // built in either order, weld to one vertex with their mean.
        let build = |swap: bool| {
            let mut s = tetra(DVec3::ZERO, false);
            s.skin = Some(Skin {
                bones: vec![
                    Bone::new("a", None, shape_core::Frame::IDENTITY),
                    Bone::new("b", Some(0), shape_core::Frame::IDENTITY),
                ],
                weights: vec![Weights::single(0); 4],
            });
            let apex = s.push_vertex(DVec3::Z);
            let (wa, wb) = (Weights::single(0), Weights::from_raw([(0, 1.0), (1, 1.0)]));
            let skin = s.skin.as_mut().unwrap();
            skin.weights[3] = if swap { wb } else { wa };
            skin.weights[apex as usize] = if swap { wa } else { wb };
            // One side face uses the copy.
            s.triangles[1] = [0, 1, apex];
            s.canonicalize().unwrap();
            s
        };
        let (a, b) = (build(false), build(true));
        assert_eq!(a, b);
        let skin = a.skin.as_ref().unwrap();
        assert_eq!(skin.weights.len(), a.positions.len());
        skin.check(a.positions.len()).unwrap();
        let apex = a.positions.iter().position(|p| *p == DVec3::Z).unwrap();
        assert_eq!(skin.weights[apex].bones[..2], [0, 1]);
        assert!((skin.weights[apex].weights[0] - 0.75).abs() < 1e-15);
        assert!((skin.weights[apex].weights[1] - 0.25).abs() < 1e-15);
        // Weights follow their vertices through sorting.
        let base = a.positions.iter().position(|p| *p == DVec3::X).unwrap();
        assert_eq!(skin.weights[base], Weights::single(0));
    }

    #[test]
    fn appending_a_different_skeleton_leaves_vertices_unweighted() {
        let mut a = tetra(DVec3::ZERO, false);
        a.skin = Some(Skin {
            bones: vec![Bone::new("a", None, shape_core::Frame::IDENTITY)],
            weights: vec![Weights::single(0); 4],
        });
        let mut same = a.clone();
        same.map_positions(|p| p + DVec3::X * 3.0, false);
        let mut joined = a.clone();
        joined.append(&same);
        joined.skin.as_ref().unwrap().check(8).unwrap();
        joined.append(&tetra(DVec3::Y * 5.0, false));
        assert_eq!(
            joined.skin.as_ref().unwrap().check(12),
            Err(SkinError::Unweighted(8))
        );
    }
}
