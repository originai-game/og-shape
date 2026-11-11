//! Repair of imported meshes: apply transforms, join, weld, remove
//! degenerate and duplicate triangles, remove floaters, orient faces
//! consistently, close small holes, and fuse interpenetrating shells.
//!
//! Non-manifold edges (more than two faces) are split into manifold
//! sheets when asked ([`split_non_manifold`]), after orienting.
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
//! interpolates them on the faces it cuts (`shape_bool`); a corner it
//! leaves on an input vertex gets that vertex's own UV back (the nearest
//! of its UVs, so a seam keeps its sides), not an interpolation, so
//! neighbours agree and no seam appears where the file had none. A hole fill
//! takes, at each rim vertex, the UV its rim triangle gives it, and its
//! centre vertex (if any) their mean; a fill whose rim crosses a UV seam
//! would fan across the texture's atlas, so it takes one UV at every
//! corner instead ([`RepairReport::uv_seam_fills`] counts those).
//!
//! Vertex colours (`Solid::colors`) are carried the same way as UVs: a
//! kept triangle keeps its corners' colours, the fuse step interpolates
//! them on cut faces and gives a corner on an input vertex that vertex's
//! own colour back, and a hole fill takes its rim's colours (a fan's
//! centre their mean), also across a UV seam.
//! [`join`] gives the triangles of parts without UVs (0, 0) at every
//! corner when another part has them: zero-area UV triangles, which
//! `measure` counts.
//!
//! A single closed surface that folds through itself is untangled by the
//! fuse step too (`shape_bool::self_union`: the space it winds round at
//! least once); the pockets an untangling leaves as shells of their own
//! are counted, and dropped when they are floaters. An open one cannot
//! be: [`repair`] reports its intersecting triangle pairs and leaves it,
//! and booleans refuse it (`docs/research.md` §5.4, §9, §32).

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

use shape_core::order::sorted_order;
use shape_core::predicates::{orient2d, orient3d};
use shape_core::{DVec2, DVec3, HashError};
use shape_mesh::skin::Mat4;
use shape_mesh::{Bone, CornerColors, Skin, Solid, Tag, WHITE, Weights, edge_key};

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
    /// Splits edges with more than two faces into manifold sheets
    /// ([`split_non_manifold`]) after orienting, then drops the pieces
    /// that come apart and are floaters (by `floaters`), before holes are
    /// closed.
    pub non_manifold: bool,
    /// Closes holes whose rim has at most this many edges (0: none).
    pub max_hole_edges: usize,
    /// Gives every piece still open after the holes are closed a second
    /// sheet this far under it, in mm, and a rim along its open edges
    /// ([`crate::thicken`]; 0: none), so a sheet encloses a volume.
    pub thicken: f64,
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
    /// What splitting non-manifold edges did, if asked.
    pub non_manifold: Option<NonManifold>,
    /// Holes closed, and triangles added to close them.
    pub holes_filled: usize,
    /// Of `holes_filled`, holes with more than `max_hole_edges` edges,
    /// closed because their fill is flat and crosses nothing.
    pub large_holes_filled: usize,
    /// Triangles added by closing holes.
    pub fill_triangles: usize,
    /// Holes left open: too large, or with a rim that does not close (it
    /// runs into a non-manifold edge).
    pub holes_left: usize,
    /// The outcome of fusing, if asked: `Some(Ok(pieces before))`, or why
    /// it was not done.
    pub fused: Option<Result<usize, String>>,
    /// Shells the fuse step's untangling left besides each untangled
    /// shell's largest: pockets the surface folded round.
    pub pockets: usize,
    /// Of those, dropped as floaters (by `floaters`), and their
    /// triangles.
    pub pockets_dropped: usize,
    /// See `pockets_dropped`.
    pub pocket_triangles_dropped: usize,
    /// Vertices removed by collapsing short edges.
    pub short_edges: usize,
    /// Self-intersecting triangle pairs at the end, if asked.
    pub self_intersections: Option<usize>,
    /// Filled holes whose rim crosses a UV seam (a rim vertex with
    /// different UVs on its two rim triangles): the fill takes one UV at
    /// every corner (one colour, covering no area of the texture), as a
    /// fan across the seam would span the atlas. 0 on a solid without
    /// UVs.
    pub uv_seam_fills: usize,
    /// What thickening the open pieces did, if it was asked for.
    pub thickened: Option<crate::thicken::Thickened>,
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
/// name (or none, in a skin that does not hold the parent), rest frame
/// and inverse bind matrix to rounding (1e-6 mm, 1e-9 in the axes).
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
    // The part with the most bones first: a skin may hold only some of
    // the skeleton (a visor skinned to the neck and head alone), and its
    // first bones then have no parent in it. Such a bone takes the parent
    // the fuller skin gives it; a bone with a parent on both sides must
    // name the same one.
    let mut order: Vec<usize> = (0..parts.len()).collect();
    order.sort_by_key(|&i| {
        let n = parts[i].1.skin.as_ref().map_or(0, |k| k.bones.len());
        (core::cmp::Reverse(n), i)
    });
    maps.resize(parts.len(), Vec::new());
    for at in order {
        let (name, s) = &parts[at];
        let k = s.skin.as_ref().unwrap_or_else(|| unreachable!("{name}"));
        let mut map = Vec::with_capacity(k.bones.len());
        for b in &k.bones {
            let parent_name = b.parent.map(|p| k.bones[p as usize].name.as_str());
            let parent = b.parent.map(|p| map[p as usize]);
            match bones.iter().position(|x| x.name == b.name) {
                Some(i) => {
                    let had = bones[i].parent.map(|p| bones[p as usize].name.as_str());
                    let same_parent = match (had, parent_name) {
                        (_, None) => true,
                        (None, Some(_)) => {
                            // Known as a root so far: it may take this
                            // parent if that comes before it.
                            match parent {
                                Some(p) if (p as usize) < i => {
                                    bones[i].parent = Some(p);
                                    true
                                }
                                _ => false,
                            }
                        }
                        (Some(a), Some(b)) => a == b,
                    };
                    if !same_parent || !near(&bones[i], b) {
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
        maps[at] = map;
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

/// The solid with only the triangles `keep` (indices, in order) and the
/// vertices they use: one copy, not a rebuilt one and then a compacted one.
fn kept(s: &Solid, keep: &[usize]) -> Solid {
    let mut remap = vec![u32::MAX; s.positions.len()];
    let mut positions = Vec::new();
    for &i in keep {
        for &v in &s.triangles[i] {
            if remap[v as usize] == u32::MAX {
                remap[v as usize] = positions.len() as u32;
                positions.push(s.positions[v as usize]);
            }
        }
    }
    rebuild(s, keep, &remap, positions)
}

fn rebuild(s: &Solid, triangles: &[usize], remap: &[u32], positions: Vec<DVec3>) -> Solid {
    let mut out = Solid {
        positions,
        uvs: s.uvs.as_ref().map(|_| Vec::new()),
        colors: s.colors.as_ref().map(|_| Vec::new()),
        ..Default::default()
    };
    for &i in triangles {
        let tags: Vec<&str> = s.face_tags[i].iter().map(|&x| s.names.name(x)).collect();
        let t = s.triangles[i].map(|v| remap[v as usize]);
        match &s.uvs {
            Some(u) => out.push_triangle_uv(t, s.surfaces[i], &tags, u[i]),
            None => out.push_triangle(t, s.surfaces[i], &tags),
        }
        if let Some(c) = &s.colors {
            out.set_colors(out.triangles.len() - 1, c[i]);
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
    weld_owned(s.clone(), d)
}

/// [`weld`], on a solid it may take apart: no second copy is made.
fn weld_owned(s: Solid, d: &Weld) -> Result<(Solid, usize), RepairError> {
    let distance = d.distance;
    if !(distance >= 0.0 && distance.is_finite()) {
        return Err(RepairError::BadDistance);
    }
    if s.positions.iter().any(|p| p.is_nan()) {
        return Err(RepairError::NaN);
    }
    let before = s.positions.len();
    let mut exact = s;
    exact.canonicalize()?;
    let welded = if distance > 0.0 {
        let s = &exact;
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
        Some(rebuild(s, &all, &remap, kept))
    } else {
        None
    };
    // One copy at a time: each is dropped before the next step's stands.
    let out = match welded {
        Some(w) => {
            drop(exact);
            w
        }
        None => exact,
    };
    let used = drop_unused(&out);
    drop(out);
    let mut out = used;
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
    let mut out = kept(s, &keep);
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
    let mut out = kept(s, &keep);
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
    orient_owned(s.clone())
}

/// [`orient`], on a solid it may change in place.
fn orient_owned(mut s: Solid) -> Result<(Solid, usize), RepairError> {
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

/// What [`split_non_manifold`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NonManifold {
    /// Edges with more than two faces, found.
    pub edges: usize,
    /// Of those, edges split into manifold sheets.
    pub split: usize,
    /// Faces on a split edge that pair with none (an odd fan, or a fin
    /// sheet): the edge is open on their side now.
    pub unpaired: usize,
    /// Edges left with more than two faces: their faces' directions do
    /// not alternate round the edge (not one oriented surface), or the
    /// sheets are joined round both ends, so no vertex could be split.
    pub left: usize,
    /// Vertices copied, one copy per extra sheet through the vertex.
    pub vertices: usize,
    /// The largest distance a copy was moved into its own fan's cone, mm
    /// (1 % of its shortest edge), so that the sheets stay apart once
    /// written to a file and read back.
    pub moved: f64,
    /// Crossing pairs the moves made (a fin's lone face moved through the
    /// sheets round it), counted as `measure` counts them.
    pub crossings_added: usize,
    /// Pieces dropped as floaters once the sheets came apart (with the
    /// repair's `floaters` setting).
    pub sheets_dropped: usize,
    /// Triangles those pieces had.
    pub sheet_triangles_dropped: usize,
}

/// A triangle by its corners' positions, sorted: the same triangle
/// whatever its vertex numbers.
type TriKey = [[u64; 3]; 3];

fn tri_key(s: &Solid, t: u32) -> TriKey {
    let mut k = s.triangles[t as usize].map(|v| {
        let p = s.positions[v as usize];
        [p.x + 0.0, p.y + 0.0, p.z + 0.0].map(f64::to_bits)
    });
    k.sort_unstable();
    k
}

/// The touching pairs of `s` (as `measure` counts them, on vertices
/// welded where positions are equal), each by its triangles' keys,
/// sorted.
fn crossing_keys(s: &Solid) -> Vec<(TriKey, TriKey)> {
    let (positions, triangles) = crate::measure::weld_exact(s);
    let mut out: Vec<(TriKey, TriKey)> = self_intersections(&positions, &triangles)
        .into_iter()
        .map(|(i, j)| {
            let (a, b) = (tri_key(s, i), tri_key(s, j));
            (a.min(b), a.max(b))
        })
        .collect();
    out.sort_unstable();
    out
}

/// Where the face with third corner `c` lies round the edge `a → b`,
/// against the reference face's third corner `c0`: 0 on its half-plane, 1
/// in the half turn after it, 2 on the half-plane opposite, 3 in the
/// half turn back. Exact.
fn half_turn(a: DVec3, b: DVec3, c0: DVec3, c: DVec3) -> u8 {
    match orient3d(a, b, c0, c) {
        Ordering::Less => 1,
        Ordering::Greater => 3,
        Ordering::Equal => {
            // In the reference plane: the same side of the line a b as c0,
            // in the projection that drops the plane's largest normal axis
            // (exact: a coordinate dropped, nothing computed).
            let n = (b - a).cross(c0 - a).abs();
            let f = |p: DVec3| {
                if n.x >= n.y && n.x >= n.z {
                    DVec2::new(p.y, p.z)
                } else if n.y >= n.z {
                    DVec2::new(p.z, p.x)
                } else {
                    DVec2::new(p.x, p.y)
                }
            };
            if orient2d(f(a), f(b), f(c0)) == orient2d(f(a), f(b), f(c)) {
                0
            } else {
                2
            }
        }
    }
}

/// Splits each edge with more than two faces into manifold sheets. The
/// faces are put in their order round the edge (exactly: `orient3d`), and
/// each face running `b → a` pairs with the next face round, running
/// `a → b`: the two bound one wedge of solid, so they are one sheet. At
/// each end of the edge, the faces round the vertex joined through
/// manifold edges and these pairs fall into fans; every fan but the
/// first gets its own copy of the vertex, moved towards the mean of the
/// fan's other corners (inside the cone its faces span) by 1 % of its
/// shortest edge there: two sheets
/// that touched along an edge are then apart, and stay apart in a file
/// (where equal positions would be welded back). Faces that pair with
/// none are left open along the edge. Run after [`orient`]. Names, UVs
/// and skins are kept (a copy takes its vertex's weights).
pub fn split_non_manifold(s: &Solid) -> Result<(Solid, NonManifold), RepairError> {
    let mut s = s.clone();
    s.canonicalize()?;
    let mut r = NonManifold::default();
    let mut by_edge: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for (i, t) in s.triangles.iter().enumerate() {
        for k in 0..3 {
            by_edge
                .entry(edge_key(t[k], t[(k + 1) % 3]))
                .or_default()
                .push(i as u32);
        }
    }
    let wide: Vec<(u32, u32)> = by_edge
        .iter()
        .filter(|e| e.1.len() > 2)
        .map(|e| *e.0)
        .collect();
    r.edges = wide.len();
    if wide.is_empty() {
        return Ok((s, r));
    }
    let mut touched: Vec<u32> = wide.iter().flat_map(|&(a, b)| [a, b]).collect();
    touched.sort_unstable();
    touched.dedup();
    let mut around: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (i, t) in s.triangles.iter().enumerate() {
        for &v in t {
            if touched.binary_search(&v).is_ok() {
                around.entry(v).or_default().push(i as u32);
            }
        }
    }
    // Fans round each vertex: its faces joined across manifold edges from
    // it, and across the pairs given (one sheet through a split edge).
    let fans_of = |v: u32, pairs: &BTreeMap<(u32, u32), Vec<(u32, u32)>>| -> Vec<u32> {
        let tris = &around[&v];
        let mut parent: Vec<u32> = (0..tris.len() as u32).collect();
        let at = |t: u32| tris.binary_search(&t).map_or(0, |i| i as u32);
        let join = |x: u32, y: u32, parent: &mut Vec<u32>| {
            let (x, y) = (find(parent, x), find(parent, y));
            if x != y {
                parent[x.max(y) as usize] = x.min(y);
            }
        };
        let mut others: Vec<u32> = tris
            .iter()
            .flat_map(|&t| s.triangles[t as usize])
            .filter(|&x| x != v)
            .collect();
        others.sort_unstable();
        others.dedup();
        for x in others {
            let e = edge_key(v, x);
            let faces = &by_edge[&e];
            if faces.len() == 2 {
                join(at(faces[0]), at(faces[1]), &mut parent);
            } else if let Some(ps) = pairs.get(&e) {
                for &(f, g) in ps {
                    join(at(f), at(g), &mut parent);
                }
            }
        }
        (0..tris.len() as u32)
            .map(|i| find(&mut parent, i))
            .collect()
    };
    let none = BTreeMap::new();
    let manifold: BTreeMap<u32, Vec<u32>> =
        touched.iter().map(|&v| (v, fans_of(v, &none))).collect();
    let runs = |t: [u32; 3], a: u32, b: u32| (0..3).any(|k| t[k] == a && t[(k + 1) % 3] == b);
    // Per non-manifold edge, the pairs of faces that are one sheet.
    let mut pairs: BTreeMap<(u32, u32), Vec<(u32, u32)>> = BTreeMap::new();
    for &(a, b) in &wide {
        let faces = &by_edge[&(a, b)];
        let k = faces.len();
        // Faces the surface already joins round either end, through
        // manifold edges: those pairs are given.
        let fan = |v: u32, t: u32| {
            let tris = &around[&v];
            manifold[&v][tris.binary_search(&t).unwrap_or(0)]
        };
        let mut sheet: Vec<(u32, u32)> = Vec::new();
        let mut paired = vec![false; k];
        let mut clash = false;
        for i in 0..k {
            for j in i + 1..k {
                let (f, g) = (faces[i], faces[j]);
                if fan(a, f) == fan(a, g) || fan(b, f) == fan(b, g) {
                    if paired[i] || paired[j] {
                        clash = true;
                    }
                    paired[i] = true;
                    paired[j] = true;
                    sheet.push((f, g));
                }
            }
        }
        if clash {
            r.left += 1;
            continue;
        }
        // The rest by their order round the edge (exactly: `orient3d`):
        // a face running b → a pairs with the next one round, running
        // a → b; the two bound one wedge of solid.
        let rest: Vec<u32> = (0..k).filter(|&i| !paired[i]).map(|i| faces[i]).collect();
        if rest.len() >= 2 {
            let (pa, pb) = (s.positions[a as usize], s.positions[b as usize]);
            let third = |t: u32| {
                let tri = s.triangles[t as usize];
                s.positions[tri[(0..3).find(|&q| tri[q] != a && tri[q] != b).unwrap_or(0)] as usize]
            };
            let c0 = third(rest[0]);
            let mut round: Vec<(u8, u32)> = rest
                .iter()
                .map(|&t| (half_turn(pa, pb, c0, third(t)), t))
                .collect();
            round.sort_by(|x, y| {
                x.0.cmp(&y.0).then_with(|| {
                    if x.0 % 2 == 0 {
                        return x.1.cmp(&y.1);
                    }
                    // y after x within the half turn: on x's normal side.
                    match orient3d(pa, pb, third(x.1), third(y.1)) {
                        Ordering::Equal => x.1.cmp(&y.1),
                        o => o,
                    }
                })
            });
            let n = round.len();
            let forward: Vec<bool> = round
                .iter()
                .map(|&(_, t)| runs(s.triangles[t as usize], a, b))
                .collect();
            let mut taken = vec![false; n];
            for i in 0..n {
                let j = (i + 1) % n;
                if i != j && !forward[i] && forward[j] && !taken[i] && !taken[j] {
                    taken[i] = true;
                    taken[j] = true;
                    sheet.push((round[i].1, round[j].1));
                }
            }
            r.unpaired += taken.iter().filter(|&&x| !x).count();
        } else {
            r.unpaired += rest.len();
        }
        pairs.insert((a, b), sheet);
    }
    let split: Vec<(u32, Vec<Vec<u32>>)> = touched
        .iter()
        .map(|&v| {
            let roots = fans_of(v, &pairs);
            let mut fans: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
            for (i, &t) in around[&v].iter().enumerate() {
                fans.entry(roots[i]).or_default().push(t);
            }
            (v, fans.into_values().skip(1).collect())
        })
        .collect();
    let mut renames: Vec<(u32, u32, u32)> = Vec::new();
    // Each copy and the vertex it copies.
    let mut copies: Vec<(u32, u32)> = Vec::new();
    for (v, fans) in split {
        for fan in &fans {
            // Towards the mean of the fan's other corners: inside the cone
            // its own faces span, away from the other sheets through v.
            let p = s.positions[v as usize];
            let mut corners: Vec<u32> = fan
                .iter()
                .flat_map(|&t| s.triangles[t as usize])
                .filter(|&w| w != v)
                .collect();
            corners.sort_unstable();
            corners.dedup();
            let mut mean = DVec3::ZERO;
            let mut shortest = f64::INFINITY;
            for &w in &corners {
                let q = s.positions[w as usize];
                mean += q;
                shortest = shortest.min((q - p).length());
            }
            let dir = mean / corners.len().max(1) as f64 - p;
            let len = dir.length();
            if !(len > 0.0 && shortest.is_finite() && shortest > 0.0) {
                continue;
            }
            let step = 0.01 * shortest;
            let copy = s.push_vertex(p + dir / len * step);
            if let Some(k) = &mut s.skin {
                k.weights[copy as usize] = k.weights[v as usize];
            }
            copies.push((copy, v));
            for &t in fan {
                renames.push((t, v, copy));
            }
        }
    }
    let before = crossing_keys(&s);
    for (t, v, copy) in renames {
        for w in &mut s.triangles[t as usize] {
            if *w == v {
                *w = copy;
            }
        }
    }
    // A fin's lone face, its corner moved, can pass through the sheets
    // round it: those pairs are counted (the fuse step untangles a closed
    // shell).
    let after = crossing_keys(&s);
    r.crossings_added = after
        .iter()
        .filter(|k| before.binary_search(k).is_err())
        .count();
    for (copy, v) in copies {
        r.vertices += 1;
        r.moved = r
            .moved
            .max((s.positions[copy as usize] - s.positions[v as usize]).length());
    }
    s.canonicalize()?;
    // What is still non-manifold.
    let mut count: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for t in &s.triangles {
        for k in 0..3 {
            *count.entry(edge_key(t[k], t[(k + 1) % 3])).or_default() += 1;
        }
    }
    r.left = count.values().filter(|&&n| n > 2).count();
    r.split = r.edges.saturating_sub(r.left);
    Ok((s, r))
}

/// A fill triangle: corners, surface, names, UVs.
type Fill = ([u32; 3], u32, Vec<Tag>, [DVec2; 3], CornerColors);

/// What [`close_holes`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Holes {
    /// Holes closed.
    pub filled: usize,
    /// Triangles added to close them.
    pub triangles: usize,
    /// Of the holes closed, those with more than `max_edges` edges (up to
    /// [`LARGE_HOLE_EDGES`]): their least-area fill turns no triangle
    /// against the fill's mean normal and crosses nothing.
    pub large: usize,
    /// Holes left open: larger than `max_edges` and not flat or clear
    /// enough to fill, larger than [`LARGE_HOLE_EDGES`], or with a rim
    /// that does not close (it runs into a non-manifold edge).
    pub left: usize,
    /// Filled holes whose rim crosses a UV seam (a rim vertex with
    /// different UVs on its two rim triangles). Their fill takes one UV
    /// at every corner (its first rim vertex's), so it covers no area of
    /// the texture: one colour, the rim's there, and nothing overlaps.
    pub uv_seam_fills: usize,
}

/// The largest hole, in rim edges, filled when it has more edges than
/// asked (and is flat and clear enough): the fill costs O(n³).
pub const LARGE_HOLE_EDGES: usize = 512;

/// Closes every hole whose rim has at most `max_edges` edges, after
/// [`orient`]. A rim is traced along boundary edges; a rim that passes a
/// vertex twice (pinched) is cut there into simple loops, each closed on
/// its own. The fill is the triangulation of
/// the rim with the least total area (dynamic programming over the rim,
/// O(n³)), wound to match the faces around it; a triangle whose new edge
/// already exists in the mesh is not allowed, so the result stays
/// manifold; a rim with no such fill is fanned from a new vertex at its
/// centre instead. A hole with more edges than `max_edges` (up to
/// [`LARGE_HOLE_EDGES`]) is closed too when its least-area fill is flat
/// enough and clear: no fill triangle faces against the fill's
/// area-weighted mean normal, and none crosses the mesh or another fill
/// (`shape_bool::selfcheck::crossing`, exact). Returns the solid and
/// what was done ([`Holes`]).
///
/// UVs: each rim vertex takes the UV its corner has in the rim triangle
/// of the rim edge leaving it; a centre vertex takes their mean. A rim
/// vertex whose two rim triangles give it different UVs sits on a seam,
/// and a fan through it would stretch across the texture's atlas: such
/// a fill takes its first rim vertex's UV at every corner instead.
pub fn close_holes(s: &Solid, max_edges: usize) -> Result<(Solid, Holes), RepairError> {
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
    let mut h = Holes::default();
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
            h.left += 1;
        }
    }
    let mut new_tris: Vec<Fill> = Vec::new();
    for (rim, rim_tris) in loops {
        if rim.len() < 3 || rim.len() > max_edges.max(LARGE_HOLE_EDGES) {
            h.left += 1;
            continue;
        }
        let large = rim.len() > max_edges;
        let large_fill = if large {
            match fill(&s, &rim, &undirected) {
                Some(tris) if flat(&s, &tris) && !crosses(&s, &tris, &new_tris) => Some(tris),
                _ => None,
            }
        } else {
            None
        };
        if large && large_fill.is_none() {
            h.left += 1;
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
        let seam = s.uvs.is_some()
            && (0..n).any(|j| uv_in(rim_tris[(j + n - 1) % n], rim[j]) != rim_uv[j]);
        let centre_uv = rim_uv.iter().copied().sum::<DVec2>() / n as f64;
        // Colours likewise: each rim vertex its colour in the rim
        // triangle leaving it, a fan's centre their mean.
        let rim_color: Vec<[f64; 4]> = (0..n)
            .map(|j| {
                let t = rim_tris[j] as usize;
                let k = s.triangles[t]
                    .iter()
                    .position(|&x| x == rim[j])
                    .unwrap_or(0);
                s.colors.as_ref().map_or(WHITE, |c| c[t][k])
            })
            .collect();
        let centre_color: [f64; 4] =
            core::array::from_fn(|ch| rim_color.iter().map(|c| c[ch]).sum::<f64>() / n as f64);
        let tris = large_fill
            .or_else(|| fill(&s, &rim, &undirected))
            .unwrap_or_else(|| {
                // Every chord already exists: fan from a new vertex at the
                // rim's centre, whose edges are all new.
                let c =
                    rim.iter().map(|&v| s.positions[v as usize]).sum::<DVec3>() / rim.len() as f64;
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
            if seam {
                return rim_uv[0];
            }
            rim.iter()
                .position(|&x| x == v)
                .map_or(centre_uv, |j| rim_uv[j])
        };
        let color_of = |v: u32| {
            rim.iter()
                .position(|&x| x == v)
                .map_or(centre_color, |j| rim_color[j])
        };
        for t in &tris {
            for k in 0..3 {
                *undirected
                    .entry(edge_key(t[k], t[(k + 1) % 3]))
                    .or_default() += 1;
            }
        }
        h.triangles += tris.len();
        h.filled += 1;
        h.large += usize::from(large);
        h.uv_seam_fills += usize::from(seam);
        new_tris.extend(
            tris.into_iter()
                .map(|t| (t, surface, tags.clone(), t.map(uv_of), t.map(color_of))),
        );
    }
    for (t, surface, tags, uv, color) in new_tris {
        s.triangles.push(t);
        s.surfaces.push(surface);
        s.face_tags.push(tags);
        if let Some(u) = &mut s.uvs {
            u.push(uv);
        }
        if let Some(c) = &mut s.colors {
            c.push(color);
        }
    }
    s.canonicalize()?;
    Ok((s, h))
}

/// No triangle of the fill faces against its area-weighted mean normal.
fn flat(s: &Solid, tris: &[[u32; 3]]) -> bool {
    let normal = |t: &[u32; 3]| {
        let [a, b, c] = t.map(|v| s.positions[v as usize]);
        (b - a).cross(c - a)
    };
    let mean: DVec3 = tris.iter().map(normal).sum();
    tris.iter().all(|t| normal(t).dot(mean) > 0.0)
}

/// A triangle of the fill crosses the mesh, another fill triangle, or a
/// fill already made (exactly; corners shared by index are joins).
fn crosses(s: &Solid, tris: &[[u32; 3]], made: &[Fill]) -> bool {
    let at = |t: [u32; 3]| t.map(|v| s.positions[v as usize]);
    let others: Vec<[u32; 3]> = s
        .triangles
        .iter()
        .copied()
        .chain(made.iter().map(|m| m.0))
        .chain(tris.iter().copied())
        .collect();
    let boxes: Vec<(DVec3, DVec3)> = others
        .iter()
        .map(|&t| {
            let p = at(t);
            (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]))
        })
        .collect();
    let first = others.len() - tris.len();
    for (k, &t) in tris.iter().enumerate() {
        let p = at(t);
        if degenerate(p) {
            return true;
        }
        let (lo, hi) = boxes[first + k];
        for (j, &o) in others.iter().enumerate() {
            if j == first + k {
                continue;
            }
            let (olo, ohi) = boxes[j];
            if !(lo.cmple(ohi).all() && olo.cmple(hi).all()) {
                continue;
            }
            let q = at(o);
            if !degenerate(q) && shape_bool::selfcheck::crossing(t, p, o, q) {
                return true;
            }
        }
    }
    false
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

/// After the fuse step: each corner of `after` on a vertex of `before`
/// (the same position, exactly) takes the UV `before` gives that vertex,
/// the nearest when it has several (one per side of a seam), in place of
/// the boolean's interpolation. Interpolating rounds differently in each
/// triangle around a vertex, which made every edge a UV seam: on an
/// image-to-3D mesh of 150 000 triangles, 31 611 seam edges after fusing
/// for 6 237 before, and decimation then stopped at 31 304 triangles for
/// a target of 2 000 (`docs/research.md` §28).
fn keep_input_uvs(before: &Solid, mut after: Solid) -> Result<Solid, RepairError> {
    let (Some(old), Some(new)) = (&before.uvs, &mut after.uvs) else {
        return Ok(after);
    };
    // `+ 0.0` makes −0 and 0 one key; positions are finite.
    let key = |p: DVec3| [p.x + 0.0, p.y + 0.0, p.z + 0.0].map(f64::to_bits);
    let mut at: BTreeMap<[u64; 3], Vec<DVec2>> = BTreeMap::new();
    for (t, tri) in before.triangles.iter().enumerate() {
        for (k, &v) in tri.iter().enumerate() {
            at.entry(key(before.positions[v as usize]))
                .or_default()
                .push(old[t][k]);
        }
    }
    for uvs in at.values_mut() {
        uvs.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        uvs.dedup();
    }
    for (t, tri) in after.triangles.iter().enumerate() {
        for (k, &v) in tri.iter().enumerate() {
            let Some(uvs) = at.get(&key(after.positions[v as usize])) else {
                continue;
            };
            let here = new[t][k];
            // The nearest; the first of equals in sorted order.
            let mut best = uvs[0];
            for &u in &uvs[1..] {
                if u.distance_squared(here) < best.distance_squared(here) {
                    best = u;
                }
            }
            new[t][k] = best;
        }
    }
    after.canonicalize()?;
    Ok(after)
}

/// After the fuse step, as [`keep_input_uvs`] for colours: a corner on
/// a vertex of `before` takes the colour `before` gives it there (the
/// nearest of its colours), so no colour border appears where the file
/// had none. Corners the union made (on cut lines) keep its
/// interpolation.
fn keep_input_colors(before: &Solid, mut after: Solid) -> Result<Solid, RepairError> {
    let Some(old) = &before.colors else {
        return Ok(after);
    };
    let key = |p: DVec3| [p.x + 0.0, p.y + 0.0, p.z + 0.0].map(f64::to_bits);
    let mut at: BTreeMap<[u64; 3], Vec<[f64; 4]>> = BTreeMap::new();
    for (t, tri) in before.triangles.iter().enumerate() {
        for (k, &v) in tri.iter().enumerate() {
            at.entry(key(before.positions[v as usize]))
                .or_default()
                .push(old[t][k]);
        }
    }
    let cmp = |a: &[f64; 4], b: &[f64; 4]| {
        (0..4)
            .map(|k| a[k].total_cmp(&b[k]))
            .find(|o| o.is_ne())
            .unwrap_or(core::cmp::Ordering::Equal)
    };
    for c in at.values_mut() {
        c.sort_by(cmp);
        c.dedup();
    }
    let n = after.triangles.len();
    let new = after.colors.get_or_insert_with(|| vec![[WHITE; 3]; n]);
    let dist =
        |a: &[f64; 4], b: &[f64; 4]| (0..4).map(|k| (a[k] - b[k]) * (a[k] - b[k])).sum::<f64>();
    // Corners the union made, per vertex.
    let mut made: BTreeMap<u32, Vec<(usize, usize)>> = BTreeMap::new();
    for (t, tri) in after.triangles.iter().enumerate() {
        for (k, &v) in tri.iter().enumerate() {
            let Some(cs) = at.get(&key(after.positions[v as usize])) else {
                made.entry(v).or_default().push((t, k));
                continue;
            };
            let here = new[t][k];
            let mut best = cs[0];
            for c in &cs[1..] {
                if dist(c, &here) < dist(&best, &here) {
                    best = *c;
                }
            }
            new[t][k] = best;
        }
    }
    // A vertex on a cut line is interpolated once per triangle around it,
    // each rounding its own way: corners that differ by less than half an
    // 8-bit step take their mean (in sorted order), so rounding makes no
    // colour border; a larger difference (two sheets of different colour
    // meeting) stays one.
    for corners in made.values() {
        let mut cs: Vec<[f64; 4]> = corners.iter().map(|&(t, k)| new[t][k]).collect();
        let spread = (0..4)
            .map(|ch| {
                let (lo, hi) = cs
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), c| {
                        (l.min(c[ch]), h.max(c[ch]))
                    });
                hi - lo
            })
            .fold(0.0, f64::max);
        if spread == 0.0 || spread >= 1.0 / 512.0 {
            continue;
        }
        cs.sort_by(cmp);
        let n = cs.len() as f64;
        let mean: [f64; 4] = core::array::from_fn(|ch| cs.iter().map(|c| c[ch]).sum::<f64>() / n);
        for &(t, k) in corners {
            new[t][k] = mean;
        }
    }
    after.canonicalize()?;
    Ok(after)
}

/// The fuse step: `shape_bool::fuse`, made to scale on a large shell that
/// passes through itself. Each shell is untangled on its own
/// (`shape_bool::self_union`); the shells an untangling leaves besides its
/// largest are pockets the surface folded round ([`RepairReport::pockets`]),
/// and those that are floaters by `floaters` are dropped. Then only the
/// shells that meet another (exact, `selfcheck::tris_meet`) or may lie
/// inside one (its box within the other's) are joined by booleans, group
/// by group; the rest are kept as they are, since their union is
/// themselves. Before, every pocket was joined to the whole by its own
/// boolean: 133 s on a 619 000-triangle chair (`docs/research.md` §32).
fn fuse_shells(s: &Solid, floaters: &Floaters, r: &mut RepairReport) -> Result<Solid, String> {
    let comps = shape_bool::components(s);
    let tangled: Vec<bool> = comps
        .iter()
        .map(|c| shape_bool::crossing_pairs(c) > 0)
        .collect();
    if !tangled.iter().any(|&t| t) {
        // Nothing to untangle: the booleans' own fuse, as before.
        return shape_bool::fuse(s).map_err(|e| e.to_string());
    }
    let mut all = Solid::default();
    let mut whole: Option<Solid> = None;
    let alone = comps.len() == 1;
    // Each shell is given up as it is taken, and each copy dropped once
    // the next stands (`docs/research.md` §36).
    for (c, t) in comps.into_iter().zip(tangled) {
        let shells = if t {
            match shape_bool::self_union(&c) {
                Ok(u) => {
                    drop(c);
                    let parts = shape_bool::components(&u);
                    if alone && parts.len() == 1 {
                        whole = Some(u);
                    }
                    parts
                }
                Err(_) => vec![c],
            }
        } else {
            vec![c]
        };
        for x in &shells {
            all.append(x);
        }
        r.pockets += shells.len().saturating_sub(1);
    }
    all.canonicalize().map_err(|e| e.to_string())?;
    if let Some(u) = whole {
        // One shell, untangled whole: the self-union is the result.
        return Ok(u);
    }
    let (kept, dropped, gone) = remove_floaters(&all, floaters).map_err(|e| e.to_string())?;
    drop(all);
    let all = kept;
    r.pockets_dropped = dropped;
    r.pocket_triangles_dropped = gone;
    // Which shells must be joined: they meet, or one's box holds the
    // other's.
    let shells = shape_bool::components(&all);
    if shells.len() <= 1 {
        return Ok(all);
    }
    let mut points: Vec<DVec3> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut owner: Vec<u32> = Vec::new();
    let mut boxes: Vec<(DVec3, DVec3)> = Vec::new();
    for (k, x) in shells.iter().enumerate() {
        let base = points.len() as u32;
        points.extend(&x.positions);
        let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
        for p in &x.positions {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        boxes.push((lo, hi));
        for t in &x.triangles {
            tris.push(t.map(|v| v + base));
            owner.push(k as u32);
        }
    }
    let mut parent: Vec<u32> = (0..shells.len() as u32).collect();
    let join = |parent: &mut Vec<u32>, a: u32, b: u32| {
        let (a, b) = (find(parent, a), find(parent, b));
        if a != b {
            parent[a.max(b) as usize] = a.min(b);
        }
    };
    for (i, j) in shape_bool::selfcheck::box_pairs(&points, &tris) {
        let (a, b) = (owner[i as usize], owner[j as usize]);
        if a != b
            && find(&mut parent, a) != find(&mut parent, b)
            && shape_bool::selfcheck::tris_meet(
                tris[i as usize].map(|v| points[v as usize]),
                tris[j as usize].map(|v| points[v as usize]),
            )
        {
            join(&mut parent, a, b);
        }
    }
    for a in 0..shells.len() {
        for b in 0..shells.len() {
            let ((alo, ahi), (blo, bhi)) = (boxes[a], boxes[b]);
            if a != b && blo.cmple(alo).all() && ahi.cmple(bhi).all() {
                join(&mut parent, a as u32, b as u32);
            }
        }
    }
    let mut groups: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for k in 0..shells.len() {
        groups
            .entry(find(&mut parent, k as u32))
            .or_default()
            .push(k);
    }
    let mut out = Solid::default();
    for members in groups.values() {
        if members.len() == 1 {
            out.append(&shells[members[0]]);
            continue;
        }
        // The largest shell against the others joined first among
        // themselves: one boolean on the large one, not one per shell.
        let big = *members
            .iter()
            .max_by_key(|&&k| (shells[k].triangles.len(), std::cmp::Reverse(k)))
            .unwrap_or(&members[0]);
        let mut rest = Solid::default();
        for &k in members {
            if k != big {
                rest.append(&shells[k]);
            }
        }
        rest.canonicalize().map_err(|e| e.to_string())?;
        let rest = shape_bool::fuse(&rest).map_err(|e| e.to_string())?;
        let joined = shape_bool::boolean(&shells[big], &rest, &shape_bool::Boolean::default())
            .map_err(|e| e.to_string())?;
        out.append(&joined);
    }
    out.canonicalize().map_err(|e| e.to_string())?;
    Ok(out)
}

/// Runs the repair steps in order (weld, degenerate and duplicate
/// triangles, floaters, orientation, holes, thickening, fuse, short edges)
/// and reports
/// what each changed. The result is in canonical order.
pub fn repair(s: &Solid, d: &Repair) -> Result<(Solid, RepairReport), RepairError> {
    repair_owned(s.clone(), d)
}

/// [`repair`], taking the solid: a caller done with its input gives it
/// up, and the steps work on one copy less (300 MiB on 600 000 triangles
/// with colours, `docs/research.md` §36). The result is the same.
pub fn repair_owned(s: Solid, d: &Repair) -> Result<(Solid, RepairReport), RepairError> {
    let mut r = RepairReport::default();
    // Each step's input is dropped as soon as its output stands: a mesh
    // of 600 000 triangles with colours is some 300 MiB, and nine of them
    // held to the end were most of the peak (`docs/research.md` §36).
    let (mut s, welded) = weld_owned(
        s,
        &Weld {
            distance: d.weld,
            all: d.weld_all,
        },
    )?;
    r.welded = welded;
    let (next, deg, dup) = remove_degenerate(&s)?;
    s = next;
    r.degenerate = deg;
    r.duplicates = dup;
    let (next, f, ft) = remove_floaters(&s, &d.floaters)?;
    s = next;
    r.floaters = f;
    r.floater_triangles = ft;
    let (next, flipped) = orient_owned(s)?;
    s = next;
    r.flipped = flipped;
    if d.non_manifold {
        let (next, mut nm) = split_non_manifold(&s)?;
        s = next;
        // Sheets apart now may be floaters, and orientation can now run
        // across the edges that were split.
        let (next, f, ft) = remove_floaters(&s, &d.floaters)?;
        s = next;
        nm.sheets_dropped = f;
        nm.sheet_triangles_dropped = ft;
        let (next, more) = orient_owned(s)?;
        s = next;
        r.flipped += more;
        r.non_manifold = Some(nm);
    }
    if d.max_hole_edges > 0 {
        let (next, h) = close_holes(&s, d.max_hole_edges)?;
        s = next;
        r.uv_seam_fills = h.uv_seam_fills;
        r.holes_filled = h.filled;
        r.large_holes_filled = h.large;
        r.fill_triangles = h.triangles;
        r.holes_left = h.left;
        // Filling can join pieces that were apart; orient once more.
        let (next, more) = orient_owned(s)?;
        s = next;
        r.flipped += more;
    }
    if d.thicken > 0.0 {
        let (next, t) = crate::thicken::thicken(&s, d.thicken)?;
        s = next;
        r.thickened = Some(t);
    }
    let s = if d.fuse {
        let n = pieces(&s).1;
        // One shell is left as it is unless it is closed and passes
        // through itself: then fuse untangles it (`shape_bool::self_union`).
        let tangled = n == 1 && s.measure().closed && shape_bool::crossing_pairs(&s) > 0;
        if n <= 1 && !tangled {
            r.fused = Some(Ok(n));
            s
        } else {
            match fuse_shells(&s, &d.floaters, &mut r) {
                Ok(f) if f.measure().closed => {
                    r.fused = Some(Ok(n));
                    keep_input_colors(&s, keep_input_uvs(&s, f)?)?
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
        // On vertices welded where positions are equal, as `measure` counts.
        let (positions, triangles) = crate::measure::weld_exact(&s);
        r.self_intersections = Some(self_intersections(&positions, &triangles).len());
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

    /// Edges whose two triangles give either end different UVs.
    fn uv_seams(s: &Solid) -> usize {
        let uvs = s.uvs.as_ref().unwrap();
        let mut sides: BTreeMap<(u32, u32), Vec<[[u64; 2]; 2]>> = BTreeMap::new();
        for (t, tri) in s.triangles.iter().enumerate() {
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                let (ua, ub) = (uvs[t][k], uvs[t][(k + 1) % 3]);
                let bits = |u: DVec2| [u.x.to_bits(), u.y.to_bits()];
                let side = if a < b {
                    [bits(ua), bits(ub)]
                } else {
                    [bits(ub), bits(ua)]
                };
                sides.entry(edge_key(a, b)).or_default().push(side);
            }
        }
        sides
            .values()
            .filter(|v| v.len() == 2 && v[0] != v[1])
            .count()
    }

    #[test]
    fn fuse_keeps_the_files_uvs_on_the_vertices_it_keeps() {
        // Two textured balls apart: the union cuts nothing, so every
        // corner is an input corner and the seams are the generator's.
        let ball = shape_gen::solids::textured(&shape_gen::solids::Generator::Sphere(
            shape_gen::solids::SphereDesc {
                radius: 7.3,
                ..Default::default()
            },
        ))
        .unwrap();
        let mut two = ball.clone();
        let mut other = ball.clone();
        for p in &mut other.positions {
            *p += DVec3::new(31.7, 0.3, 0.0);
        }
        two.append(&other);
        two.canonicalize().unwrap();
        let d = Repair {
            fuse: true,
            ..Default::default()
        };
        let (out, r) = repair(&two, &d).unwrap();
        assert_eq!(r.fused, Some(Ok(2)));
        assert_eq!(uv_seams(&out), uv_seams(&two));
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

    #[test]
    fn fuse_drops_the_pockets_an_untangling_leaves() {
        // A ball whose vertices are moved by up to 1.5 per axis
        // (splitmix64): it folds through itself, and its self-union is a
        // main shell and three small pockets the surface folded round.
        let mut ball = shape_gen::solids::sphere(10.0).unwrap();
        let mut state = 5u64;
        let mut next = || {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            (z >> 11) as f64 / (1u64 << 52) as f64 - 1.0
        };
        for p in &mut ball.positions {
            *p += DVec3::new(next(), next(), next()) * 1.5;
        }
        let d = Repair {
            fuse: true,
            floaters: Floaters {
                fraction: 0.05,
                ..Default::default()
            },
            check_intersections: true,
            ..Default::default()
        };
        let (out, r) = repair(&ball, &d).unwrap();
        assert_eq!(r.fused, Some(Ok(1)));
        assert_eq!((r.pockets, r.pockets_dropped), (3, 3), "{r:?}");
        assert_eq!(r.self_intersections, Some(0));
        assert_eq!(pieces(&out).1, 1);
        assert!(out.measure().closed);
    }

    #[test]
    fn two_cubes_on_one_edge_come_apart_closed() {
        // Two cubes touching along one edge: four faces on it.
        let mut s = cube(DVec3::ZERO, 10.0);
        s.append(&cube(DVec3::new(10.0, 10.0, 0.0), 10.0));
        s.canonicalize().unwrap();
        assert_eq!(s.measure().non_manifold_edges, 1);
        let (out, r) = repair(
            &s,
            &Repair {
                non_manifold: true,
                check_intersections: true,
                ..Default::default()
            },
        )
        .unwrap();
        let nm = r.non_manifold.unwrap();
        assert_eq!(
            (nm.edges, nm.split, nm.left, nm.unpaired),
            (1, 1, 0, 0),
            "{nm:?}"
        );
        assert_eq!(nm.vertices, 2);
        let m = out.measure();
        assert!(m.closed, "{m:?}");
        assert_eq!(pieces(&out).1, 2);
        assert_eq!(r.self_intersections, Some(0));
        // Each copy moved into its own cube, 1 % of its shortest edge.
        assert!((nm.moved - 0.1).abs() < 1e-12, "{nm:?}");
    }

    #[test]
    fn a_fill_across_a_uv_seam_covers_no_texture() {
        let ball = shape_gen::solids::textured(&shape_gen::solids::Generator::Sphere(
            shape_gen::solids::SphereDesc {
                radius: 7.3,
                ..Default::default()
            },
        ))
        .unwrap();
        // A vertex on the seam (two UVs), away from the poles: its fan
        // removed leaves a hole whose rim crosses the seam.
        let uvs = ball.uvs.as_ref().unwrap();
        let mut at: BTreeMap<u32, Vec<[u64; 2]>> = BTreeMap::new();
        for (t, tri) in ball.triangles.iter().enumerate() {
            for k in 0..3 {
                let u = uvs[t][k];
                at.entry(tri[k])
                    .or_default()
                    .push([u.x.to_bits(), u.y.to_bits()]);
            }
        }
        let v = *at
            .iter()
            .find(|(v, u)| {
                let mut u = (*u).clone();
                u.sort_unstable();
                u.dedup();
                u.len() == 2 && ball.positions[**v as usize].z.abs() < 3.0
            })
            .unwrap()
            .0;
        let mut open = ball.clone();
        let keep: Vec<usize> = (0..open.triangles.len())
            .filter(|&t| !open.triangles[t].contains(&v))
            .collect();
        let remap: Vec<u32> = (0..open.positions.len() as u32).collect();
        open = drop_unused(&rebuild(&open, &keep, &remap, open.positions.clone()));
        let (out, h) = close_holes(&open, 64).unwrap();
        assert_eq!((h.filled, h.uv_seam_fills), (1, 1), "{h:?}");
        let fill = out.faces(&["fill"]).unwrap();
        assert!(!fill.is_empty());
        let uvs = out.uvs.as_ref().unwrap();
        for t in fill {
            let u = uvs[t as usize];
            assert!(u[0] == u[1] && u[1] == u[2], "{u:?}");
        }
        assert!(out.measure().closed);
    }

    #[test]
    fn a_hole_over_the_limit_is_filled_when_flat_and_clear() {
        // A cube without one face: a square hole of four edges.
        let mut s = cube(DVec3::ZERO, 10.0);
        for _ in 0..2 {
            s.triangles.pop();
            s.surfaces.pop();
            s.face_tags.pop();
        }
        let (out, h) = close_holes(&s, 3).unwrap();
        assert_eq!((h.filled, h.large, h.left), (1, 1, 0), "{h:?}");
        assert!(out.measure().closed);
        // Within the limit it is an ordinary fill.
        let (_, h) = close_holes(&s, 4).unwrap();
        assert_eq!((h.filled, h.large), (1, 0));
    }

    /// A skin may hold only part of the skeleton: Mixamo's soldier has a
    /// body skinned to 49 bones and a visor skinned to the neck and head
    /// alone, whose neck has no parent in its own skin. The two are one
    /// skeleton; bones whose parents are named differently are not.
    #[test]
    fn a_skin_of_part_of_the_skeleton_joins_the_whole_one() {
        use shape_core::Frame;
        use shape_mesh::{Bone, Weights};
        let at = |z: f64| Frame {
            origin: DVec3::new(0.0, 0.0, z),
            ..Frame::IDENTITY
        };
        let skinned = |bones: Vec<Bone>, offset: f64| {
            let mut s = cube(DVec3::new(offset, 0.0, 0.0), 1.0);
            let last = bones.len() as u32 - 1;
            s.skin = Some(Skin {
                bones,
                weights: vec![Weights::single(last); s.positions.len()],
            });
            s
        };
        let body = skinned(
            vec![
                Bone::new("spine", None, at(0.0)),
                Bone::new("neck", Some(0), at(5.0)),
            ],
            0.0,
        );
        let visor = skinned(vec![Bone::new("neck", None, at(5.0))], 3.0);
        for parts in [
            [("body", &body), ("visor", &visor)],
            [("visor", &visor), ("body", &body)],
        ] {
            let (bones, maps) = merge_skeletons(&parts).unwrap().unwrap();
            let names: Vec<&str> = bones.iter().map(|b| b.name.as_str()).collect();
            assert_eq!(names, ["spine", "neck"]);
            assert_eq!(bones[1].parent, Some(0));
            // Each part's bones, by its own order, in the merged list.
            let visor_at = usize::from(parts[0].0 == "body");
            assert_eq!(maps[visor_at], [1]);
            assert_eq!(maps[1 - visor_at], [0, 1]);
        }
        // A neck hung on another bone is another skeleton.
        let other = skinned(
            vec![
                Bone::new("hips", None, at(0.0)),
                Bone::new("neck", Some(0), at(5.0)),
            ],
            6.0,
        );
        let e = merge_skeletons(&[("body", &body), ("other", &other)]).unwrap_err();
        assert!(e.to_string().contains("neck"), "{e}");
    }
}
