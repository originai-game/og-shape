//! The kernel's solid: a triangle mesh in f64 with names on its faces and
//! edges (`docs/design.md` §4), canonical ordering (§6) and measures (§7).
//!
//! Entry: [`Solid`]. A generator builds one, an operation consumes one and
//! returns another, and every operation ends with [`Solid::canonicalize`].

mod measure;
pub mod morph;
mod names;
pub mod skin;

pub use measure::Measures;
pub use morph::{
    Morph, MorphError, Morphs, SkinsDiffer, bare, carried, carried_skin, carry, carry_redone,
    one_skin, rides,
};
pub use names::{LookupError, Names, Tag};
pub use skin::{Bone, Skin, SkinClip, SkinError, Weights};

use std::collections::BTreeMap;

use shape_core::order::{cmp_dvec2, cmp_dvec3, cmp_f64, sorted_order};
use shape_core::{DVec2, DVec3, HashError, OutputHash};

/// One triangle's texture coordinates, a corner each, in the order of
/// its vertices.
pub type CornerUvs = [DVec2; 3];

/// One triangle's vertex colours, a corner each, in the order of its
/// vertices: linear RGBA, each component in 0..=1 (glTF `COLOR_0`).
pub type CornerColors = [[f64; 4]; 3];

/// The colour a corner gets where a solid has colours and a tool adds a
/// triangle without saying: opaque white, which multiplies a material by
/// 1 (glTF's meaning of no `COLOR_0`).
pub const WHITE: [f64; 4] = [1.0; 4];

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
/// - `uvs`: optional texture coordinates, per triangle corner
///   (`docs/design.md` §4), so a seam is two corners of one vertex with
///   different coordinates. f64 here, f32 on output; (0, 0) at the
///   bottom left of the image (glTF's v is flipped on export). `None`
///   leaves canonical order and the hash as they were before UVs. When
///   present it has one entry per triangle; [`Solid::push_triangle`]
///   gives a new triangle (0, 0) at every corner, so a tool that adds
///   triangles sets their coordinates with [`Solid::push_triangle_uv`].
/// - `colors`: optional vertex colours, per triangle corner, linear RGBA
///   in 0..=1, exactly parallel to `uvs`: a hard colour border is two
///   corners of one vertex with different colours. `None` leaves
///   canonical order and the hash as they were before colours. A new
///   triangle gets [`WHITE`] at every corner; set them with
///   [`Solid::set_colors`].
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
    /// Per triangle, its corners' texture coordinates, if it has any.
    pub uvs: Option<Vec<CornerUvs>>,
    /// Per triangle, its corners' colours (linear RGBA), if it has any.
    pub colors: Option<Vec<[[f64; 4]; 3]>>,
    /// Morph targets, if the solid has any ([`morph`]). `None` leaves
    /// every other part, and the hash, as it was before morphs.
    pub morphs: Option<Morphs>,
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
        if let Some(uvs) = &mut self.uvs {
            uvs.resize(self.triangles.len(), [DVec2::ZERO; 3]);
        }
        if let Some(colors) = &mut self.colors {
            colors.resize(self.triangles.len(), [WHITE; 3]);
        }
    }

    /// Sets triangle `i`'s corner colours. On a solid with no colours
    /// yet, every other triangle gets [`WHITE`] at its corners.
    pub fn set_colors(&mut self, i: usize, c: CornerColors) {
        let n = self.triangles.len();
        let colors = self.colors.get_or_insert_with(Vec::new);
        colors.resize(n, [WHITE; 3]);
        if let Some(x) = colors.get_mut(i) {
            *x = c;
        }
    }

    /// The colour triangle `i` gives point `p`, interpolated as
    /// [`Solid::uv_at`] interpolates UVs. `None` when the solid has no
    /// colours or the triangle has no area.
    pub fn color_at(&self, i: usize, p: DVec3) -> Option<[f64; 4]> {
        let c = self.colors.as_ref()?.get(i)?;
        let t = self.triangles[i];
        let [a, b, q] = t.map(|v| self.positions[v as usize]);
        if let Some(k) = [a, b, q].iter().position(|&x| x == p) {
            return Some(c[k]);
        }
        let (e1, e2, d) = (b - a, q - a, p - a);
        let n = e1.cross(e2);
        let nn = n.dot(n);
        if nn == 0.0 {
            return None;
        }
        let w1 = d.cross(e2).dot(n) / nn;
        let w2 = e1.cross(d).dot(n) / nn;
        Some(core::array::from_fn(|k| {
            c[0][k] + (c[1][k] - c[0][k]) * w1 + (c[2][k] - c[0][k]) * w2
        }))
    }

    /// Appends a triangle with its corners' texture coordinates. On a
    /// solid with no UVs yet, every earlier triangle gets (0, 0) at its
    /// corners.
    pub fn push_triangle_uv(&mut self, t: [u32; 3], surface: u32, tags: &[&str], uv: CornerUvs) {
        let uvs = self.uvs.get_or_insert_with(Vec::new);
        uvs.resize(self.triangles.len(), [DVec2::ZERO; 3]);
        self.push_triangle(t, surface, tags);
        if let Some(uvs) = &mut self.uvs {
            uvs[self.triangles.len() - 1] = uv;
        }
    }

    /// The texture coordinate triangle `i` gives point `p`: `p` projected
    /// onto the triangle's plane and interpolated barycentrically there
    /// (extended affinely past its edges). A point at one of its corners
    /// takes that corner's coordinate exactly. `None` when the solid has
    /// no UVs or the triangle has no area.
    pub fn uv_at(&self, i: usize, p: DVec3) -> Option<DVec2> {
        let uv = self.uvs.as_ref()?.get(i)?;
        let t = self.triangles[i];
        let [a, b, c] = t.map(|v| self.positions[v as usize]);
        if let Some(k) = [a, b, c].iter().position(|&q| q == p) {
            return Some(uv[k]);
        }
        let (e1, e2, d) = (b - a, c - a, p - a);
        let n = e1.cross(e2);
        let nn = n.dot(n);
        if nn == 0.0 {
            return None;
        }
        let w1 = d.cross(e2).dot(n) / nn;
        let w2 = e1.cross(d).dot(n) / nn;
        Some(uv[0] + (uv[1] - uv[0]) * w1 + (uv[2] - uv[0]) * w2)
    }

    /// Turns triangle `i` over (its second and third corners swapped),
    /// with its texture coordinates.
    pub fn reverse_triangle(&mut self, i: usize) {
        self.triangles[i].swap(1, 2);
        if let Some(uv) = self.uvs.as_mut().and_then(|u| u.get_mut(i)) {
            uv.swap(1, 2);
        }
        if let Some(c) = self.colors.as_mut().and_then(|c| c.get_mut(i)) {
            c.swap(1, 2);
        }
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
        if let Some(m) = &mut self.morphs {
            m.resize(self.positions.len());
        }
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
    /// Texture coordinates stay with their triangle's corners (rotated
    /// with it). Two triangles on the same corners, surface and names
    /// are ordered by their coordinates, so UVs never reorder a solid
    /// that has none. Colours likewise, after the UVs.
    ///
    /// Fails on a NaN position, texture coordinate or colour.
    pub fn canonicalize(&mut self) -> Result<(), HashError> {
        if self.positions.iter().any(|p| p.is_nan()) {
            return Err(HashError::NaN);
        }
        if let Some(uvs) = &mut self.uvs {
            if uvs.iter().flatten().any(|q| q.is_nan()) {
                return Err(HashError::NaN);
            }
            uvs.resize(self.triangles.len(), [DVec2::ZERO; 3]);
        }
        if let Some(colors) = &mut self.colors {
            if colors.iter().flatten().flatten().any(|c| c.is_nan()) {
                return Err(HashError::NaN);
            }
            colors.resize(self.triangles.len(), [WHITE; 3]);
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
        if let Some(m) = &mut self.morphs {
            m.resize(self.positions.len());
            *m = m.remapped(&remap, positions.len());
        }
        // 2. Triangles: renumbered, degenerate ones dropped, each rotated
        // to start at its smallest index (winding kept).
        type Tri = ([u32; 3], u32, Vec<Tag>, CornerUvs, CornerColors);
        let mut tris: Vec<Tri> = Vec::with_capacity(self.triangles.len());
        for (i, t) in self.triangles.iter().enumerate() {
            let t = t.map(|v| remap[v as usize]);
            if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
                continue;
            }
            let r = (0..3).min_by_key(|&k| t[k]).unwrap_or(0);
            let t = [t[r], t[(r + 1) % 3], t[(r + 2) % 3]];
            let uv = self.uvs.as_ref().map_or([DVec2::ZERO; 3], |u| {
                let u = u[i];
                [u[r], u[(r + 1) % 3], u[(r + 2) % 3]]
            });
            let color = self.colors.as_ref().map_or([WHITE; 3], |c| {
                let c = c[i];
                [c[r], c[(r + 1) % 3], c[(r + 2) % 3]]
            });
            tris.push((t, self.surfaces[i], self.face_tags[i].clone(), uv, color));
        }
        let cmp_uv = |a: &CornerUvs, b: &CornerUvs| {
            (0..3)
                .map(|k| cmp_dvec2(a[k], b[k]))
                .find(|o| o.is_ne())
                .unwrap_or(core::cmp::Ordering::Equal)
        };
        let cmp_color = |a: &CornerColors, b: &CornerColors| {
            a.iter()
                .flatten()
                .zip(b.iter().flatten())
                .map(|(x, y)| cmp_f64(*x, *y))
                .find(|o| o.is_ne())
                .unwrap_or(core::cmp::Ordering::Equal)
        };
        tris.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
                .then_with(|| cmp_uv(&a.3, &b.3))
                .then_with(|| cmp_color(&a.4, &b.4))
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
        for (t, ..) in &tris {
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
        if self.uvs.is_some() {
            self.uvs = Some(tris.iter().map(|t| t.3).collect());
        }
        if self.colors.is_some() {
            self.colors = Some(tris.iter().map(|t| t.4).collect());
        }
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
        // Likewise morph targets.
        if let Some(m) = &self.morphs {
            h.str("morphs");
            h.u64(m.targets.len() as u64);
            for t in &m.targets {
                h.str(&t.name);
                h.f64s(&[t.weight])?;
                h.dvec3s(&t.deltas)?;
            }
        }
        // Likewise UVs: only a solid that has them adds them.
        if let Some(uvs) = &self.uvs {
            h.str("uvs");
            h.u64(uvs.len() as u64);
            for q in uvs.iter().flatten() {
                h.f64s(&[q.x, q.y])?;
            }
        }
        // And colours.
        if let Some(colors) = &self.colors {
            h.str("colors");
            h.u64(colors.len() as u64);
            for c in colors.iter().flatten() {
                h.f64s(c)?;
            }
        }
        Ok(())
    }

    /// Appends another solid; its surfaces are renumbered after this one's.
    ///
    /// Morph targets: the result has both solids' targets, matched by
    /// name; a target one side lacks moves nothing there.
    ///
    /// Skins: when both solids have the same skeleton, the weights are
    /// appended with the vertices. Otherwise the result keeps this
    /// solid's skeleton (or the other's, if this one has none) and the
    /// vertices from the side without it have no weights, which
    /// [`Skin::check`] reports: a skin is never dropped silently.
    ///
    /// UVs: when either solid has them, the result does; triangles from
    /// the side without get (0, 0) at every corner (a zero-area UV
    /// triangle, which `measure` counts). Colours likewise, [`WHITE`] on
    /// the side without.
    pub fn append(&mut self, other: &Solid) {
        let base = self.positions.len() as u32;
        let first = self.triangles.len();
        if self.uvs.is_some() || other.uvs.is_some() {
            self.uvs
                .get_or_insert_with(Vec::new)
                .resize(first, [DVec2::ZERO; 3]);
        }
        if self.colors.is_some() || other.colors.is_some() {
            self.colors
                .get_or_insert_with(Vec::new)
                .resize(first, [WHITE; 3]);
        }
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
        if self.morphs.is_some() || other.morphs.is_some() {
            let total = base as usize + other.positions.len();
            let mine = self.morphs.get_or_insert_with(Morphs::default);
            mine.resize(base as usize);
            for t in other.morphs.iter().flat_map(|m| &m.targets) {
                let slot = match mine.targets.iter().position(|m| m.name == t.name) {
                    Some(i) => i,
                    None => {
                        mine.targets.push(Morph {
                            name: t.name.clone(),
                            weight: t.weight,
                            deltas: vec![DVec3::ZERO; base as usize],
                        });
                        mine.targets.len() - 1
                    }
                };
                mine.targets[slot].deltas.extend_from_slice(&t.deltas);
            }
            mine.resize(total);
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
        if let (Some(mine), Some(theirs)) = (&mut self.uvs, &other.uvs) {
            for (k, uv) in theirs.iter().enumerate() {
                mine[first + k] = *uv;
            }
        }
        if let (Some(mine), Some(theirs)) = (&mut self.colors, &other.colors) {
            for (k, c) in theirs.iter().enumerate() {
                mine[first + k] = *c;
            }
        }
    }

    /// Every vertex mapped by `f`. A map that mirrors (`mirrors == true`)
    /// reverses each triangle so it still faces out. A skin's weights stay
    /// with their vertices; its bones are not moved (`f` need not be
    /// rigid), so place a solid before skinning it.
    pub fn map_positions(&mut self, f: impl Fn(DVec3) -> DVec3, mirrors: bool) {
        // A morph offset becomes the difference of where its two ends go.
        if let Some(m) = &mut self.morphs {
            for t in &mut m.targets {
                for (d, p) in t.deltas.iter_mut().zip(&self.positions) {
                    *d = f(*p + *d) - f(*p);
                }
            }
        }
        for p in &mut self.positions {
            *p = f(*p);
        }
        if mirrors {
            for i in 0..self.triangles.len() {
                self.reverse_triangle(i);
            }
        }
    }

    /// The solid with its morph targets applied at `weights` (one a
    /// target; a missing one is the target's rest weight): a snapshot,
    /// which carries no targets. A solid without targets comes back as
    /// it is.
    pub fn morphed(&self, weights: &[f64]) -> Solid {
        let mut out = self.clone();
        if let Some(m) = out.morphs.take() {
            out.positions = m.applied(&self.positions, weights);
        }
        out
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
    fn uvs_follow_their_corners_through_canonical_order() {
        // Each corner's UV is a function of its position (and, on one
        // face, of the face too: a seam), so after welding, sorting and
        // rotating, every corner must still carry its own.
        let uv_of =
            |p: DVec3, seam: bool| DVec2::new(p.x + 2.0 * p.z + f64::from(u8::from(seam)), p.y);
        let build = |reversed: bool| {
            let mut s = tetra(DVec3::ZERO, reversed);
            s.uvs = Some(
                s.triangles
                    .iter()
                    .enumerate()
                    .map(|(i, t)| t.map(|v| uv_of(s.positions[v as usize], s.surfaces[i] == 1)))
                    .collect(),
            );
            s.canonicalize().unwrap();
            s
        };
        let (a, b) = (build(false), build(true));
        assert_eq!(a, b);
        for (i, t) in a.triangles.iter().enumerate() {
            for k in 0..3 {
                let seam = a.surfaces[i] == a.surfaces[a.faces(&["side"]).unwrap()[0] as usize];
                assert_eq!(
                    a.uvs.as_ref().unwrap()[i][k],
                    uv_of(a.positions[t[k] as usize], seam)
                );
            }
        }
        // Without UVs, the hash is what it was; with them, it differs.
        let mut bare = a.clone();
        bare.uvs = None;
        let (mut hb, mut hu) = (OutputHash::new(), OutputHash::new());
        bare.hash_into(&mut hb).unwrap();
        a.hash_into(&mut hu).unwrap();
        assert_ne!(hb.finish_hex(), hu.finish_hex());
    }

    #[test]
    fn colors_follow_their_corners_through_canonical_order() {
        // As the UV test: each corner's colour a function of its position
        // and face, built in either order, must come back on its corner.
        let color_of = |p: DVec3, side: bool| [p.x, p.y, p.z, if side { 0.5 } else { 1.0 }];
        let build = |reversed: bool| {
            let mut s = tetra(DVec3::ZERO, reversed);
            for i in 0..s.triangles.len() {
                let side = s.surfaces[i] == 1;
                let c = s.triangles[i].map(|v| color_of(s.positions[v as usize], side));
                s.set_colors(i, c);
            }
            s.canonicalize().unwrap();
            s
        };
        let (a, b) = (build(false), build(true));
        assert_eq!(a, b);
        let side = a.surfaces[a.faces(&["side"]).unwrap()[0] as usize];
        for (i, t) in a.triangles.iter().enumerate() {
            for k in 0..3 {
                let want = color_of(a.positions[t[k] as usize], a.surfaces[i] == side);
                assert_eq!(a.colors.as_ref().unwrap()[i][k], want);
            }
        }
        // Absent: the hash is the bare one; present, it differs.
        let mut bare = a.clone();
        bare.colors = None;
        let (mut hb, mut hc) = (OutputHash::new(), OutputHash::new());
        bare.hash_into(&mut hb).unwrap();
        a.hash_into(&mut hc).unwrap();
        assert_ne!(hb.finish_hex(), hc.finish_hex());
        // A triangle added without colours is white.
        let mut c = a.clone();
        c.push_triangle([0, 1, 2], 0, &["x"]);
        assert_eq!(c.colors.as_ref().unwrap().last(), Some(&[WHITE; 3]));
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
    fn a_needle_of_non_zero_exact_area_is_not_degenerate() {
        // 3 · (1/3) rounds to 1, so the f64 cross product is zero, but the
        // corners are not on one line; exactly collinear ones are.
        let mut s = Solid::default();
        let third = 1.0 / 3.0;
        let p = [
            DVec3::ZERO,
            DVec3::new(3.0, 1.0, 0.0),
            DVec3::new(1.0, third, 0.0),
            DVec3::new(1.5, 0.5, 0.0),
        ]
        .map(|p| s.push_vertex(p));
        s.push_triangle([p[0], p[1], p[2]], 0, &["needle"]);
        let a = s.positions[2] - s.positions[0];
        assert_eq!((s.positions[1] - s.positions[0]).cross(a), DVec3::ZERO);
        assert_eq!(s.measure().degenerate_triangles, 0);
        s.push_triangle([p[0], p[1], p[3]], 0, &["line"]);
        assert_eq!(s.measure().degenerate_triangles, 1);
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
