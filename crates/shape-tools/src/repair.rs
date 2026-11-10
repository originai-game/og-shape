//! Repair of imported meshes: apply transforms, join, weld, remove
//! degenerate and duplicate triangles, remove floaters, orient faces
//! consistently, close small holes, and fuse interpenetrating shells.
//!
//! Entry: [`repair`] runs the steps in that order from a [`Repair`] desc
//! and reports what each changed in a [`RepairReport`]. Every step is also
//! its own function.
//!
//! Names: every surviving triangle keeps its face names and surface;
//! welding keeps the names of edges whose two ends survive. Triangles that
//! close a hole are named `fill` plus the face names around the hole and
//! take the surface of the first triangle on its rim. [`join`] keeps each
//! part's names (imported faces carry their part's name).
//!
//! Skins (`Solid::skin`) are carried through every step: weights follow
//! their vertices, vertices merged into one take the mean of their weights
//! (`Skin::weld`), and a vertex a hole fill adds takes the blend of its
//! rim's. [`apply_transform`] bakes a rigid transform into the vertices,
//! the bones' rest frames and their inverse bind matrices; [`join`] merges
//! skeletons by bone name. What cannot be kept is an error value
//! ([`RepairError::Skinned`]): a scaled or mirroring transform on a
//! skinned part, bones of one name that disagree, skinned and unskinned
//! parts joined. The self-union of [`Repair::fuse`] refuses skins (the
//! report says so).
//!
//! UVs (`Solid::uvs`, per triangle corner) are carried through every
//! step: a kept triangle keeps its corners' UVs (welding moves positions,
//! never UVs), a turned triangle turns its UVs with it, and the fuse step
//! interpolates them on the faces it cuts (`shape_bool`). A hole fill
//! takes, at each rim vertex, the UV its rim triangle gives it, and its
//! centre vertex (if any) their mean; a rim that crosses a UV seam gives
//! a stretched fill, which [`RepairReport::uv_seam_fills`] counts.
//! [`join`] gives the triangles of parts without UVs (0, 0) at every
//! corner when another part has them: zero-area UV triangles, which
//! `measure` counts.
//!
//! A single closed surface that folds through itself is untangled by the
//! fuse step too (`shape_bool::self_union`: the space it winds round at
//! least once). An open one cannot be: [`repair`] reports its intersecting
//! triangle pairs and leaves it, and booleans refuse it
//! (`docs/research.md` §5.4, §9).

use std::collections::BTreeMap;
use std::fmt;

use shape_core::order::sorted_order;
use shape_core::{DVec2, DVec3, HashError};
use shape_mesh::skin::Mat4;
use shape_mesh::{Bone, Skin, Solid, Tag, Weights, edge_key};

use crate::import::{Affine, ImportedPart};
use crate::intersect::{degenerate, self_intersections};

/// Which small disconnected pieces [`remove_floaters`] drops. The zero
/// value drops none.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Floaters {
    /// Drops a piece whose bounding-box diagonal is below this, in mm.
    pub size: f64,
    /// Drops a piece whose diagonal is below this fraction of the largest
    /// piece's.
    pub fraction: f64,
}

/// What [`repair`] does. The zero value welds exact duplicates only,
/// removes degenerate and duplicate triangles, orients faces and leaves
/// pieces and holes alone.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Repair {
    /// Welds vertices on open edges closer than this, in mm (0: exact
    /// duplicates only); see [`Weld`].
    pub weld: f64,
    /// Welds every vertex within `weld`, not only those on open edges.
    pub weld_all: bool,
    /// Small pieces to drop.
    pub floaters: Floaters,
    /// Closes holes whose rim has at most this many edges (0: none).
    pub max_hole_edges: usize,
    /// Joins closed, interpenetrating shells into one by self-union
    /// (`shape_bool::fuse`) once the rest is done, and untangles a shell
    /// that passes through itself. A union that is not closed (shells
    /// touching along an edge) is not taken: the shells are kept, and the
    /// report says why.
    pub fuse: bool,
    /// Collapses edges shorter than this, in mm, where that keeps the mesh
    /// manifold (0: none): the slivers a self-union leaves, whose vertices
    /// f32 export would merge.
    pub min_edge: f64,
    /// Counts self-intersecting triangle pairs at the end (exact; costs a
    /// sweep over the triangles).
    pub check_intersections: bool,
}

/// What each step of [`repair`] changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RepairReport {
    /// Vertices that went in the weld (merged, exact duplicates
    /// included, or used by no triangle).
    pub welded: usize,
    /// Triangles dropped for having no area.
    pub degenerate: usize,
    /// Triangles dropped for repeating another (same corners, same
    /// winding).
    pub duplicates: usize,
    /// Pieces dropped as floaters.
    pub floaters: usize,
    /// Triangles those pieces had.
    pub floater_triangles: usize,
    /// Triangles turned over to agree with their neighbours or to face
    /// out.
    pub flipped: usize,
    /// Holes closed, and triangles added to close them.
    pub holes_filled: usize,
    /// Triangles added by closing holes.
    pub fill_triangles: usize,
    /// Holes left open: too large, or with a rim that does not close (it
    /// runs into a non-manifold edge).
    pub holes_left: usize,
    /// The outcome of fusing, if asked: `Some(Ok(pieces before))`, or why
    /// it was not done.
    pub fused: Option<Result<usize, String>>,
    /// Vertices removed by collapsing short edges.
    pub short_edges: usize,
    /// Self-intersecting triangle pairs at the end, if asked.
    pub self_intersections: Option<usize>,
    /// Filled holes whose rim crosses a UV seam (a rim vertex with
    /// different UVs on its two rim triangles): the fill's UVs stretch
    /// across the seam. 0 on a solid without UVs.
    pub uv_seam_fills: usize,
}

/// Why repair could not run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepairError {
    /// A position is NaN.
    NaN,
    /// A weld distance or floater size is negative or not finite.
    BadDistance,
    /// A skin that cannot be kept: what and why (a skinned part placed by
    /// a scaled or mirroring transform, bones of one name that disagree,
    /// skinned and unskinned parts joined).
    Skinned(String),
}

impl fmt::Display for RepairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepairError::NaN => f.write_str("a position is NaN"),
            RepairError::BadDistance => {
                f.write_str("a weld distance or size is negative or not finite")
            }
            RepairError::Skinned(w) => {
                write!(f, "{w}: the skin cannot be kept, so this is refused")
            }
        }
    }
}

impl std::error::Error for RepairError {}

impl From<HashError> for RepairError {
    fn from(_: HashError) -> Self {
        RepairError::NaN
    }
}

/// The part's solid placed by its transform. A mirroring transform
/// (negative determinant) reverses every triangle so faces still point
/// out. Names are kept. On a skinned part the transform must be rigid: it
/// moves the vertices, the bones' rest frames, and their inverse bind
/// matrices together (`IBM · T⁻¹`); a scaled or mirroring one is
/// [`RepairError::Skinned`].
pub fn apply_transform(part: &ImportedPart) -> Result<Solid, RepairError> {
    let m = &part.transform;
    let Some(skin) = &part.solid.skin else {
        return Ok(transformed(&part.solid, m));
    };
    if *m == Affine::IDENTITY {
        return Ok(part.solid.clone());
    }
    let rigid = [m.x, m.y, m.z]
        .iter()
        .all(|a| (a.length() - 1.0).abs() < 1e-9)
        && m.x.dot(m.y).abs() < 1e-9
        && m.y.dot(m.z).abs() < 1e-9
        && m.x.dot(m.z).abs() < 1e-9
        && m.determinant() > 0.0;
    if !rigid {
        return Err(RepairError::Skinned(format!(
            "part {:?} is skinned and placed by a scaled or mirroring transform",
            part.name
        )));
    }
    let mut out = transformed(&part.solid, m);
    let inverse = crate::import::rigid_inverse(m);
    let inv = affine_matrix(&inverse);
    out.skin = Some(Skin {
        bones: skin
            .bones
            .iter()
            .map(|b| Bone {
                name: b.name.clone(),
                parent: b.parent,
                rest: shape_core::Frame {
                    origin: m.apply(b.rest.origin),
                    x: m.rotate(b.rest.x),
                    y: m.rotate(b.rest.y),
                    z: m.rotate(b.rest.z),
                },
                inverse_bind: mat_mul(&b.inverse_bind, &inv),
            })
            .collect(),
        weights: skin.weights.clone(),
    });
    Ok(out)
}

/// A column-major 4×4 matrix of an affine map.
fn affine_matrix(a: &Affine) -> Mat4 {
    [
        a.x.x, a.x.y, a.x.z, 0.0, a.y.x, a.y.y, a.y.z, 0.0, a.z.x, a.z.y, a.z.z, 0.0, a.t.x, a.t.y,
        a.t.z, 1.0,
    ]
}

/// `a · b`, column-major.
fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[4 * c + r] = (0..4).map(|k| a[4 * k + r] * b[4 * c + k]).sum();
        }
    }
    out
}

/// A merged skeleton, and each skinned part's bone indices in it.
type Merged = (Vec<Bone>, Vec<Vec<u32>>);

/// The skeletons of `parts` merged by bone name, and each skinned part's
/// bone indices in it. Bones of one name must agree: the same parent's
/// name, rest frame and inverse bind matrix to rounding (1e-6 mm, 1e-9 in
/// the axes).
fn merge_skeletons(parts: &[(&str, &Solid)]) -> Result<Option<Merged>, RepairError> {
    let skinned: Vec<&str> = parts
        .iter()
        .filter(|p| p.1.skin.is_some())
        .map(|p| p.0)
        .collect();
    if skinned.is_empty() {
        return Ok(None);
    }
    let bare: Vec<&str> = parts
        .iter()
        .filter(|p| p.1.skin.is_none())
        .map(|p| p.0)
        .collect();
    if !bare.is_empty() {
        return Err(RepairError::Skinned(format!(
            "parts {skinned:?} are skinned and {bare:?} are not; join them apart"
        )));
    }
    let mut bones: Vec<Bone> = Vec::new();
    let mut maps = Vec::new();
    let mut conflicts: Vec<String> = Vec::new();
    let near = |a: &Bone, b: &Bone| {
        let f = (&a.rest, &b.rest);
        (f.0.origin - f.1.origin).length() <= 1e-6 * f.0.origin.length().max(1.0)
            && (f.0.x - f.1.x).length() <= 1e-9
            && (f.0.y - f.1.y).length() <= 1e-9
            && (f.0.z - f.1.z).length() <= 1e-9
            && a.inverse_bind
                .iter()
                .zip(&b.inverse_bind)
                .all(|(x, y)| (x - y).abs() <= 1e-6 * x.abs().max(1.0))
    };
    for (name, s) in parts {
        let k = s.skin.as_ref().unwrap_or_else(|| unreachable!("{name}"));
        let mut map = Vec::with_capacity(k.bones.len());
        for b in &k.bones {
            let parent_name = b.parent.map(|p| k.bones[p as usize].name.as_str());
            let parent = b.parent.map(|p| map[p as usize]);
            match bones.iter().position(|x| x.name == b.name) {
                Some(i) => {
                    let had = bones[i].parent.map(|p| bones[p as usize].name.as_str());
                    if had != parent_name || !near(&bones[i], b) {
                        conflicts.push(b.name.clone());
                    }
                    map.push(i as u32);
                }
                None => {
                    bones.push(Bone {
                        parent,
                        ..b.clone()
                    });
                    map.push(bones.len() as u32 - 1);
                }
            }
        }
        maps.push(map);
    }
    if !conflicts.is_empty() {
        conflicts.sort();
        conflicts.dedup();
        return Err(RepairError::Skinned(format!(
            "bones {conflicts:?} differ between the parts (parent, rest frame or inverse bind)"
        )));
    }
    Ok(Some((bones, maps)))
}

fn transformed(s: &Solid, m: &Affine) -> Solid {
    let mut out = s.clone();
    out.map_positions(|p| m.apply(p), m.determinant() < 0.0);
    out
}

/// Every part placed by its transform and joined into one solid, in
/// canonical order (exactly coincident vertices are welded). Each part's
/// surfaces stay their own; faces keep their names, so each part stays
/// addressable by its name. Skinned parts' skeletons are merged by bone
/// name: bones of one name must agree (parent, rest frame, inverse bind
/// matrix), and skinned and unskinned parts are not joined; either is
/// [`RepairError::Skinned`], naming them.
pub fn join(parts: &[ImportedPart]) -> Result<Solid, RepairError> {
    let placed: Vec<Solid> = parts
        .iter()
        .map(apply_transform)
        .collect::<Result<_, _>>()?;
    let named: Vec<(&str, &Solid)> = parts.iter().map(|p| p.name.as_str()).zip(&placed).collect();
    let merged = merge_skeletons(&named)?;
    let mut out = Solid::default();
    for (i, s) in placed.into_iter().enumerate() {
        let mut s = s;
        if let (Some((bones, maps)), Some(k)) = (&merged, &s.skin) {
            let map = &maps[i];
            let weights = k
                .weights
                .iter()
                .map(|w| Weights::from_raw(w.pairs().map(|(b, x)| (map[b as usize], x))))
                .collect();
            s.skin = Some(Skin {
                bones: bones.clone(),
                weights,
            });
        }
        out.append(&s);
    }
    out.canonicalize()?;
    Ok(out)
}

/// The solid without the vertices no triangle uses (canonical order kept
/// once canonicalized again).
pub(crate) fn drop_unused(s: &Solid) -> Solid {
    let mut remap = vec![u32::MAX; s.positions.len()];
    let mut positions = Vec::new();
    for t in &s.triangles {
        for &v in t {
            if remap[v as usize] == u32::MAX {
                remap[v as usize] = positions.len() as u32;
                positions.push(s.positions[v as usize]);
            }
        }
    }
    let all: Vec<usize> = (0..s.triangles.len()).collect();
    rebuild(s, &all, &remap, positions)
}

fn rebuild(s: &Solid, triangles: &[usize], remap: &[u32], positions: Vec<DVec3>) -> Solid {
    let mut out = Solid {
        positions,
        uvs: s.uvs.as_ref().map(|_| Vec::new()),
        ..Default::default()
    };
    for &i in triangles {
        let tags: Vec<&str> = s.face_tags[i].iter().map(|&x| s.names.name(x)).collect();
        let t = s.triangles[i].map(|v| remap[v as usize]);
        match &s.uvs {
            Some(u) => out.push_triangle_uv(t, s.surfaces[i], &tags, u[i]),
            None => out.push_triangle(t, s.surfaces[i], &tags),
        }
    }
    for (&(a, b), tags) in &s.edge_tags {
        let (a, b) = (remap[a as usize], remap[b as usize]);
        if a != b && a != u32::MAX && b != u32::MAX {
            let tags: Vec<&str> = tags.iter().map(|&x| s.names.name(x)).collect();
            out.tag_edge(a, b, &tags);
        }
    }
    out.skin = s
        .skin
        .as_ref()
        .map(|k| remap_skin(k, remap, out.positions.len()));
    out
}

/// A skin's weights moved to new vertex indices (`remap[old]`, or
/// `u32::MAX` for gone); vertices merged into one take the mean of their
/// weights (`Skin::weld`).
pub(crate) fn remap_skin(k: &Skin, remap: &[u32], vertices: usize) -> Skin {
    let mut copies: Vec<Vec<Weights>> = vec![Vec::new(); vertices];
    for (old, &new) in remap.iter().enumerate() {
        if let (Some(c), Some(w)) = (copies.get_mut(new as usize), k.weights.get(old)) {
            c.push(*w);
        }
    }
    Skin {
        bones: k.bones.clone(),
        weights: copies.into_iter().map(|mut c| Skin::weld(&mut c)).collect(),
    }
}

/// How [`weld`] merges vertices. The zero value welds exact duplicates
/// only.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Weld {
    /// Merges vertices closer than this, in mm.
    pub distance: f64,
    /// Merges any vertices, not only those on open edges (Blender's merge
    /// by distance). Off, a weld only closes cracks: a closed surface's
    /// vertices never move, so two close but separate sheets are not
    /// pinched together.
    pub all: bool,
}

/// Welds exactly coincident vertices, then each vertex on an open edge
/// (or each vertex, with `all`) to an earlier one within `distance` mm,
/// and canonicalizes (which drops triangles that lost a corner and
/// vertices no triangle uses). Vertices are visited in position order;
/// each joins the nearest kept vertex within reach, or is kept itself.
/// Kept vertices do not move, so the result depends only on the
/// positions, not on their order. Returns the solid and how many vertices
/// went.
pub fn weld(s: &Solid, d: &Weld) -> Result<(Solid, usize), RepairError> {
    let distance = d.distance;
    if !(distance >= 0.0 && distance.is_finite()) {
        return Err(RepairError::BadDistance);
    }
    if s.positions.iter().any(|p| p.is_nan()) {
        return Err(RepairError::NaN);
    }
    let before = s.positions.len();
    let mut exact = s.clone();
    exact.canonicalize()?;
    let s = &exact;
    let mut out = s.clone();
    if distance > 0.0 {
        let mut uses: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for t in &s.triangles {
            for k in 0..3 {
                *uses.entry(edge_key(t[k], t[(k + 1) % 3])).or_default() += 1;
            }
        }
        let mut open = vec![d.all; s.positions.len()];
        for (&(a, b), &n) in &uses {
            if n == 1 {
                open[a as usize] = true;
                open[b as usize] = true;
            }
        }
        let order = sorted_order(&s.positions);
        let cell = |p: DVec3| {
            let c = (p / distance).floor();
            (c.x as i64, c.y as i64, c.z as i64)
        };
        let mut grid: BTreeMap<(i64, i64, i64), Vec<u32>> = BTreeMap::new();
        let mut remap = vec![0u32; s.positions.len()];
        let mut kept: Vec<DVec3> = Vec::new();
        let d2 = distance * distance;
        for &v in &order {
            let p = s.positions[v as usize];
            if !open[v as usize] {
                kept.push(p);
                remap[v as usize] = kept.len() as u32 - 1;
                continue;
            }
            let (cx, cy, cz) = cell(p);
            let mut best: Option<(f64, u32)> = None;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        let Some(ids) = grid.get(&(cx + dx, cy + dy, cz + dz)) else {
                            continue;
                        };
                        for &k in ids {
                            let e = (kept[k as usize] - p).length_squared();
                            if e <= d2 && best.is_none_or(|(be, bk)| e < be || (e == be && k < bk))
                            {
                                best = Some((e, k));
                            }
                        }
                    }
                }
            }
            remap[v as usize] = match best {
                Some((_, k)) => k,
                None => {
                    kept.push(p);
                    let k = kept.len() as u32 - 1;
                    grid.entry((cx, cy, cz)).or_default().push(k);
                    k
                }
            };
        }
        let all: Vec<usize> = (0..s.triangles.len()).collect();
        out = rebuild(s, &all, &remap, kept);
    }
    let mut out = drop_unused(&out);
    out.canonicalize()?;
    let removed = before - out.positions.len();
    Ok((out, removed))
}

/// Drops triangles with no area (three corners in a line, decided
/// exactly) and triangles that repeat another's corners in the same
/// winding (the first is kept). Two triangles on the same corners wound
/// opposite ways (a two-sided sheet) are both kept. Returns the solid and
/// the counts `(degenerate, duplicates)`.
pub fn remove_degenerate(s: &Solid) -> Result<(Solid, usize, usize), RepairError> {
    let mut seen: BTreeMap<[u32; 3], ()> = BTreeMap::new();
    let (mut deg, mut dup) = (0, 0);
    let mut keep = Vec::with_capacity(s.triangles.len());
    for (i, t) in s.triangles.iter().enumerate() {
        if degenerate(t.map(|v| s.positions[v as usize])) {
            deg += 1;
            continue;
        }
        let r = (0..3).min_by_key(|&k| t[k]).unwrap_or(0);
        let key = [t[r], t[(r + 1) % 3], t[(r + 2) % 3]];
        if seen.insert(key, ()).is_some() {
            dup += 1;
            continue;
        }
        keep.push(i);
    }
    let remap: Vec<u32> = (0..s.positions.len() as u32).collect();
    let mut out = drop_unused(&rebuild(s, &keep, &remap, s.positions.clone()));
    out.canonicalize()?;
    Ok((out, deg, dup))
}

fn find(p: &mut [u32], mut i: u32) -> u32 {
    while p[i as usize] != i {
        p[i as usize] = p[p[i as usize] as usize];
        i = p[i as usize];
    }
    i
}

/// Per triangle, its piece (sharing a vertex), numbered by first
/// triangle; and the number of pieces.
pub(crate) fn pieces(s: &Solid) -> (Vec<u32>, usize) {
    let mut parent: Vec<u32> = (0..s.positions.len() as u32).collect();
    for t in &s.triangles {
        for k in 1..3 {
            let (a, b) = (find(&mut parent, t[0]), find(&mut parent, t[k]));
            if a != b {
                parent[a.max(b) as usize] = a.min(b);
            }
        }
    }
    let mut ids: BTreeMap<u32, u32> = BTreeMap::new();
    let of: Vec<u32> = s
        .triangles
        .iter()
        .map(|t| {
            let r = find(&mut parent, t[0]);
            let n = ids.len() as u32;
            *ids.entry(r).or_insert(n)
        })
        .collect();
    (of, ids.len())
}

/// Drops pieces (connected by shared vertices) smaller than `d` says.
/// The largest piece (by bounding-box diagonal, then triangle count, then
/// order) is always kept. Returns the solid and the counts
/// `(pieces dropped, triangles dropped)`.
pub fn remove_floaters(s: &Solid, d: &Floaters) -> Result<(Solid, usize, usize), RepairError> {
    for v in [d.size, d.fraction] {
        if !(v >= 0.0 && v.is_finite()) {
            return Err(RepairError::BadDistance);
        }
    }
    let (of, n) = pieces(s);
    if n <= 1 {
        let mut out = s.clone();
        out.canonicalize()?;
        return Ok((out, 0, 0));
    }
    let mut lo = vec![DVec3::splat(f64::INFINITY); n];
    let mut hi = vec![DVec3::splat(f64::NEG_INFINITY); n];
    let mut count = vec![0usize; n];
    for (i, t) in s.triangles.iter().enumerate() {
        let c = of[i] as usize;
        count[c] += 1;
        for &v in t {
            lo[c] = lo[c].min(s.positions[v as usize]);
            hi[c] = hi[c].max(s.positions[v as usize]);
        }
    }
    let diag: Vec<f64> = (0..n).map(|c| (hi[c] - lo[c]).length()).collect();
    let largest = (0..n)
        .max_by(|&a, &b| {
            diag[a]
                .total_cmp(&diag[b])
                .then(count[a].cmp(&count[b]))
                .then(b.cmp(&a))
        })
        .unwrap_or(0);
    let limit = d.size.max(d.fraction * diag[largest]);
    let drop: Vec<bool> = (0..n).map(|c| c != largest && diag[c] < limit).collect();
    let keep: Vec<usize> = (0..s.triangles.len())
        .filter(|&i| !drop[of[i] as usize])
        .collect();
    let dropped = drop.iter().filter(|&&x| x).count();
    let tris = s.triangles.len() - keep.len();
    let remap: Vec<u32> = (0..s.positions.len() as u32).collect();
    let mut out = drop_unused(&rebuild(s, &keep, &remap, s.positions.clone()));
    out.canonicalize()?;
    Ok((out, dropped, tris))
}

/// Turns triangles over so that neighbours across every manifold edge
/// (two triangles) agree, piece by piece from each piece's first
/// triangle, then turns each piece over whole if its signed volume (about
/// its vertices' mean) is negative, so closed pieces face out. Edges with
/// more than two triangles do not propagate. Returns the solid and how
/// many triangles were turned.
pub fn orient(s: &Solid) -> Result<(Solid, usize), RepairError> {
    let mut s = s.clone();
    s.canonicalize()?;
    let n = s.triangles.len();
    let mut by_edge: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for (i, t) in s.triangles.iter().enumerate() {
        for k in 0..3 {
            by_edge
                .entry(edge_key(t[k], t[(k + 1) % 3]))
                .or_default()
                .push(i as u32);
        }
    }
    // Whether triangle i runs a → b.
    let runs = |t: [u32; 3], a: u32, b: u32| (0..3).any(|k| t[k] == a && t[(k + 1) % 3] == b);
    let mut flip = vec![false; n];
    let mut seen = vec![false; n];
    let mut piece_of = vec![u32::MAX; n];
    let mut pieces: Vec<Vec<u32>> = Vec::new();
    for start in 0..n {
        if seen[start] {
            continue;
        }
        let id = pieces.len() as u32;
        let mut members = vec![start as u32];
        seen[start] = true;
        piece_of[start] = id;
        let mut head = 0;
        while head < members.len() {
            let i = members[head] as usize;
            head += 1;
            let t = s.triangles[i];
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                let users = &by_edge[&edge_key(a, b)];
                if users.len() != 2 {
                    continue;
                }
                let j = if users[0] as usize == i {
                    users[1]
                } else {
                    users[0]
                } as usize;
                if seen[j] {
                    continue;
                }
                // Neighbours agree when they run the edge opposite ways.
                let same_way = runs(s.triangles[j], a, b);
                flip[j] = flip[i] ^ same_way;
                seen[j] = true;
                piece_of[j] = id;
                members.push(j as u32);
            }
        }
        pieces.push(members);
    }
    for members in &pieces {
        let mut verts: Vec<u32> = members
            .iter()
            .flat_map(|&i| s.triangles[i as usize])
            .collect();
        verts.sort_unstable();
        verts.dedup();
        let centre = verts
            .iter()
            .map(|&v| s.positions[v as usize])
            .sum::<DVec3>()
            / verts.len() as f64;
        let mut volume = 0.0;
        for &i in members {
            let t = s.triangles[i as usize];
            let [a, b, c] = t.map(|v| s.positions[v as usize] - centre);
            let v = a.dot(b.cross(c));
            volume += if flip[i as usize] { -v } else { v };
        }
        if volume < 0.0 {
            for &i in members {
                flip[i as usize] = !flip[i as usize];
            }
        }
    }
    let mut turned = 0;
    for (i, &f) in flip.iter().enumerate() {
        if f {
            s.reverse_triangle(i);
            turned += 1;
        }
    }
    s.canonicalize()?;
    Ok((s, turned))
}

/// Closes every hole whose rim has at most `max_edges` edges, after
/// [`orient`]. A rim is traced along boundary edges; a rim that passes a
/// vertex twice (pinched) is cut there into simple loops, each closed on
/// its own. The fill is the triangulation of
/// the rim with the least total area (dynamic programming over the rim,
/// O(n³)), wound to match the faces around it; a triangle whose new edge
/// already exists in the mesh is not allowed, so the result stays
/// manifold; a rim with no such fill is fanned from a new vertex at its
/// centre instead. Returns the solid
/// and the counts `(holes filled, triangles added, holes left, fills
/// across a UV seam)`.
///
/// UVs: each rim vertex takes the UV its corner has in the rim triangle
/// of the rim edge leaving it; a centre vertex takes their mean. A rim
/// vertex whose two rim triangles give it different UVs sits on a seam:
/// the fill stretches across it, and the fourth count says how many
/// fills do.
pub fn close_holes(
    s: &Solid,
    max_edges: usize,
) -> Result<(Solid, usize, usize, usize, usize), RepairError> {
    let mut s = s.clone();
    s.canonicalize()?;
    let mut directed: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut undirected: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for (i, t) in s.triangles.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            directed.insert((a, b), i as u32);
            *undirected.entry(edge_key(a, b)).or_default() += 1;
        }
    }
    // Rim edges run against their boundary triangle: b → a for a → b.
    let mut out_edges: BTreeMap<u32, Vec<(u32, u32)>> = BTreeMap::new();
    for (&(a, b), &tri) in &directed {
        if undirected[&edge_key(a, b)] == 1 {
            out_edges.entry(b).or_default().push((a, tri));
        }
    }
    let mut used: BTreeMap<(u32, u32), ()> = BTreeMap::new();
    let (mut filled, mut added, mut left) = (0, 0, 0);
    let starts: Vec<(u32, u32)> = out_edges
        .iter()
        .flat_map(|(&v, es)| es.iter().map(move |&(w, _)| (v, w)))
        .collect();
    // Trace every rim; a rim that passes a vertex twice (pinched) is cut
    // there into simple loops, each closed on its own.
    let mut loops: Vec<(Vec<u32>, Vec<u32>)> = Vec::new();
    for (v0, w0) in starts {
        if used.contains_key(&(v0, w0)) {
            continue;
        }
        let mut path: Vec<u32> = vec![v0];
        let mut path_tris: Vec<u32> = Vec::new();
        let (mut v, mut w) = (v0, w0);
        loop {
            used.insert((v, w), ());
            let tri = out_edges[&v].iter().find(|e| e.0 == w).map_or(0, |e| e.1);
            path_tris.push(tri);
            if let Some(k) = path.iter().position(|&x| x == w) {
                // A loop closes at w: cut it off.
                loops.push((path.split_off(k), path_tris.split_off(k)));
                path.push(w);
            } else {
                path.push(w);
            }
            let Some(next) = out_edges.get(&w).and_then(|e| {
                e.iter()
                    .find(|(x, _)| !used.contains_key(&(w, *x)))
                    .map(|&(x, _)| x)
            }) else {
                break;
            };
            v = w;
            w = next;
        }
        // Whatever did not close (a rim through a non-manifold edge).
        if path.len() > 1 {
            left += 1;
        }
    }
    type Fill = ([u32; 3], u32, Vec<Tag>, [DVec2; 3]);
    let mut new_tris: Vec<Fill> = Vec::new();
    let mut seams = 0;
    for (rim, rim_tris) in loops {
        if rim.len() < 3 || rim.len() > max_edges {
            left += 1;
            continue;
        }
        // Each rim vertex's UV in its rim triangles: leaving (j) and
        // arriving (j − 1).
        let uv_in = |t: u32, v: u32| -> DVec2 {
            let tri = s.triangles[t as usize];
            let k = tri.iter().position(|&x| x == v).unwrap_or(0);
            s.uvs.as_ref().map_or(DVec2::ZERO, |u| u[t as usize][k])
        };
        let n = rim.len();
        let rim_uv: Vec<DVec2> = (0..n).map(|j| uv_in(rim_tris[j], rim[j])).collect();
        if s.uvs.is_some() && (0..n).any(|j| uv_in(rim_tris[(j + n - 1) % n], rim[j]) != rim_uv[j])
        {
            seams += 1;
        }
        let centre_uv = rim_uv.iter().copied().sum::<DVec2>() / n as f64;
        let tris = fill(&s, &rim, &undirected).unwrap_or_else(|| {
            // Every chord already exists: fan from a new vertex at the
            // rim's centre, whose edges are all new.
            let c = rim.iter().map(|&v| s.positions[v as usize]).sum::<DVec3>() / rim.len() as f64;
            let ci = s.push_vertex(c);
            if let Some(k) = &mut s.skin {
                // The centre takes the blend of its rim's weights.
                let items: Vec<(Weights, f64)> = rim
                    .iter()
                    .map(|&v| (k.weights[v as usize], 1.0 / rim.len() as f64))
                    .collect();
                k.weights[ci as usize] = Weights::blend(&items);
            }
            (0..rim.len())
                .map(|k| [rim[k], rim[(k + 1) % rim.len()], ci])
                .collect()
        });
        let mut tags: Vec<Tag> = vec![s.names.intern("fill")];
        for &t in &rim_tris {
            tags.extend(&s.face_tags[t as usize]);
        }
        tags.sort_unstable();
        tags.dedup();
        let surface = s.surfaces[rim_tris[0] as usize];
        let uv_of = |v: u32| {
            rim.iter()
                .position(|&x| x == v)
                .map_or(centre_uv, |j| rim_uv[j])
        };
        for t in &tris {
            for k in 0..3 {
                *undirected
                    .entry(edge_key(t[k], t[(k + 1) % 3]))
                    .or_default() += 1;
            }
        }
        added += tris.len();
        filled += 1;
        new_tris.extend(
            tris.into_iter()
                .map(|t| (t, surface, tags.clone(), t.map(uv_of))),
        );
    }
    for (t, surface, tags, uv) in new_tris {
        s.triangles.push(t);
        s.surfaces.push(surface);
        s.face_tags.push(tags);
        if let Some(u) = &mut s.uvs {
            u.push(uv);
        }
    }
    s.canonicalize()?;
    Ok((s, filled, added, left, seams))
}

/// The least-area triangulation of a rim, avoiding edges the mesh already
/// has (other than the rim's own).
fn fill(s: &Solid, rim: &[u32], edges: &BTreeMap<(u32, u32), u32>) -> Option<Vec<[u32; 3]>> {
    let n = rim.len();
    let p = |i: usize| s.positions[rim[i] as usize];
    let chord_ok = |i: usize, j: usize| {
        j == i + 1 || (i == 0 && j == n - 1) || {
            let (a, b) = (rim[i], rim[j]);
            a != b && !edges.contains_key(&edge_key(a, b))
        }
    };
    // cost[i][j]: least area of the polygon rim[i..=j] closed by the
    // chord j → i; choice[i][j]: its apex.
    let mut cost = vec![f64::INFINITY; n * n];
    let mut choice = vec![usize::MAX; n * n];
    for i in 0..n - 1 {
        cost[i * n + i + 1] = 0.0;
    }
    for len in 2..n {
        for i in 0..n - len {
            let j = i + len;
            if !chord_ok(i, j) {
                continue;
            }
            for m in i + 1..j {
                let (a, b) = (cost[i * n + m], cost[m * n + j]);
                if !(a.is_finite() && b.is_finite()) {
                    continue;
                }
                let area = (p(m) - p(i)).cross(p(j) - p(i)).length();
                let c = a + b + area;
                if c < cost[i * n + j] {
                    cost[i * n + j] = c;
                    choice[i * n + j] = m;
                }
            }
        }
    }
    if !cost[n - 1].is_finite() {
        return None;
    }
    let mut out = Vec::with_capacity(n - 2);
    let mut stack = vec![(0usize, n - 1)];
    while let Some((i, j)) = stack.pop() {
        if j <= i + 1 {
            continue;
        }
        let m = choice[i * n + j];
        out.push([rim[i], rim[m], rim[j]]);
        stack.push((i, m));
        stack.push((m, j));
    }
    Some(out)
}

/// Runs the repair steps in order (weld, degenerate and duplicate
/// triangles, floaters, orientation, holes, fuse, short edges) and reports
/// what each changed. The result is in canonical order.
pub fn repair(s: &Solid, d: &Repair) -> Result<(Solid, RepairReport), RepairError> {
    let mut r = RepairReport::default();
    let (s, welded) = weld(
        s,
        &Weld {
            distance: d.weld,
            all: d.weld_all,
        },
    )?;
    r.welded = welded;
    let (s, deg, dup) = remove_degenerate(&s)?;
    r.degenerate = deg;
    r.duplicates = dup;
    let (s, f, ft) = remove_floaters(&s, &d.floaters)?;
    r.floaters = f;
    r.floater_triangles = ft;
    let (s, flipped) = orient(&s)?;
    r.flipped = flipped;
    let s = if d.max_hole_edges > 0 {
        let (s, filled, added, left, seams) = close_holes(&s, d.max_hole_edges)?;
        r.uv_seam_fills = seams;
        r.holes_filled = filled;
        r.fill_triangles = added;
        r.holes_left = left;
        // Filling can join pieces that were apart; orient once more.
        let (s, more) = orient(&s)?;
        r.flipped += more;
        s
    } else {
        s
    };
    let s = if d.fuse {
        let n = pieces(&s).1;
        // One shell is left as it is unless it is closed and passes
        // through itself: then fuse untangles it (`shape_bool::self_union`).
        let tangled = n == 1 && s.measure().closed && shape_bool::crossing_pairs(&s) > 0;
        if n <= 1 && !tangled {
            r.fused = Some(Ok(n));
            s
        } else {
            match shape_bool::fuse(&s) {
                Ok(f) if f.measure().closed => {
                    r.fused = Some(Ok(n));
                    f
                }
                Ok(f) => {
                    let m = f.measure();
                    r.fused = Some(Err(format!(
                        "the union is not closed ({} open, {} non-manifold edges); the shells are kept apart",
                        m.boundary_edges, m.non_manifold_edges
                    )));
                    s
                }
                Err(e) => {
                    r.fused = Some(Err(e.to_string()));
                    s
                }
            }
        }
    } else {
        s
    };
    let s = if d.min_edge > 0.0 {
        let (s, gone) = crate::decimate::collapse_short_edges(&s, d.min_edge)
            .map_err(|_| RepairError::BadDistance)?;
        r.short_edges = gone;
        s
    } else {
        s
    };
    if d.check_intersections {
        r.self_intersections = Some(self_intersections(&s.positions, &s.triangles).len());
    }
    Ok((s, r))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(offset: DVec3, size: f64) -> Solid {
        let mut s = Solid::default();
        for i in 0..8 {
            let p = DVec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64);
            s.push_vertex(offset + p * size);
        }
        let quads = [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ];
        for (k, q) in quads.iter().enumerate() {
            s.push_triangle([q[0], q[1], q[2]], k as u32, &["cube"]);
            s.push_triangle([q[0], q[2], q[3]], k as u32, &["cube"]);
        }
        s
    }

    #[test]
    fn repairs_a_broken_cube() {
        let mut s = cube(DVec3::ZERO, 10.0);
        assert!(s.measure().closed);
        // Split one corner into two copies 1e-4 apart, turn two faces
        // over, drop one triangle, add a floater and a sliver.
        let extra = s.push_vertex(DVec3::new(10.0, 0.0, 10.0 + 1e-4));
        s.triangles[11][2] = extra;
        s.triangles[0].swap(1, 2);
        s.triangles[5].swap(1, 2);
        s.triangles.remove(7);
        s.surfaces.remove(7);
        s.face_tags.remove(7);
        s.append(&cube(DVec3::splat(30.0), 0.5));
        let a = s.push_vertex(DVec3::ZERO);
        let b = s.push_vertex(DVec3::new(5.0, 0.0, 0.0));
        let c = s.push_vertex(DVec3::new(10.0, 0.0, 0.0));
        s.push_triangle([a, b, c], 0, &["sliver"]);
        assert!(!s.measure().closed);
        let d = Repair {
            weld: 0.01,
            floaters: Floaters {
                size: 2.0,
                ..Default::default()
            },
            max_hole_edges: 8,
            check_intersections: true,
            ..Default::default()
        };
        let (out, r) = repair(&s, &d).unwrap();
        let m = out.measure();
        assert!(m.closed, "{r:?} {m:?}");
        assert!((m.volume - 1000.0).abs() < 1e-9);
        assert_eq!((r.floaters, r.floater_triangles), (1, 12));
        assert_eq!((r.holes_filled, r.fill_triangles), (1, 1));
        assert_eq!(r.degenerate, 1);
        assert_eq!(r.self_intersections, Some(0));
        assert_eq!(out.faces(&["fill"]).unwrap().len(), 1);
    }

    #[test]
    fn joins_parts_and_mirrors_keep_faces_out() {
        let part = |name: &str, t: Affine| ImportedPart {
            name: name.into(),
            solid: cube(DVec3::ZERO, 1.0),
            transform: t,
            ..Default::default()
        };
        let mirror = Affine {
            x: -DVec3::X,
            t: DVec3::new(-2.0, 0.0, 0.0),
            ..Affine::IDENTITY
        };
        let s = join(&[part("a", Affine::IDENTITY), part("b", mirror)]).unwrap();
        let m = s.measure();
        assert!(m.closed && m.components == 2);
        assert!((m.volume - 2.0).abs() < 1e-12);
    }

    #[test]
    fn fuse_untangles_a_shell_folded_through_itself() {
        // A sphere with one vertex pulled through the far side
        // (`docs/research.md` §9.6): closed, one piece, crossing itself.
        let mut ball = shape_gen::solids::sphere(10.0).unwrap();
        let dir = DVec3::new(-0.51, -0.38, 0.77);
        let v = (0..ball.positions.len())
            .max_by(|&a, &b| {
                ball.positions[a]
                    .dot(dir)
                    .total_cmp(&ball.positions[b].dot(dir))
            })
            .unwrap();
        ball.positions[v] = -ball.positions[v] * 1.5;
        let d = Repair {
            fuse: true,
            check_intersections: true,
            ..Default::default()
        };
        let (out, r) = repair(&ball, &d).unwrap();
        assert_eq!(r.fused, Some(Ok(1)));
        assert_eq!(r.self_intersections, Some(0));
        let m = out.measure();
        assert!(m.closed && m.components == 1, "{m:?}");
        // Without fuse it is left, and reported.
        let (_, r) = repair(
            &ball,
            &Repair {
                check_intersections: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(r.self_intersections.unwrap() > 0);
    }
}
