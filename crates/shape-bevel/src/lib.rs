//! Fillet, chamfer and soften on named edges of a [`Solid`]
//! (`docs/design.md` §2, `docs/plan.md` phase 3).
//!
//! Entry: [`fillet`], [`chamfer`] and [`soften`], each taking a solid and
//! a zero-value desc and returning a new canonical solid or a
//! [`BevelError`]. [`quad_pairs`] measures how much of a patch is quads.
//!
//! Written clean-room from Blender's published Bevel design (Trickey's
//! notes and the Bevel V2 design issue; sources in `docs/research.md` §7):
//! offset lines on each face, four offsets per edge, a superellipse
//! profile, edge strips, and corner patches by the number of bevelled
//! edges at a vertex. Gaps with several unbevelled edges, tips, pipe
//! joints, arc and patch miters and loop slide follow the same notes
//! (`docs/research.md` §7.4). A lone piece of a longer crease tapers to
//! a tip at both ends, and a corner on a face whose triangles are not one
//! plane is cut on its folds (§7.8). A band deeper than the rows of a
//! smooth wall (a loft's top rim) is cut across them (§7.9).
//! A run of edges the offset line swallows on a face (a round corner no
//! wider than the offset) is shrunk to a point, where its strips end as
//! a fan (§37).
//!
//! Names (`docs/design.md` §4):
//! - New faces carry only `fillet` or `chamfer`.
//! - An edge where a new face meets an old one at more than 10° is named
//!   `fillet` or `chamfer` plus the cap names (`top`, `bottom`, `start`,
//!   `end`, also qualified) of the old face. Between two new faces of
//!   different surfaces the limit is 10° for fillets and 40° for
//!   chamfers, and the name is the kind alone.
//! - Unbevelled edges keep their names; bevelled edges are gone.
//!
//! UVs (`Solid::uvs`) are carried by a pass over the result (`uvs.rs`,
//! `docs/research.md` §21.2): faces the bevel does not touch keep theirs,
//! trimmed faces are re-fitted to the same map, and new faces take the
//! map of the old face they face most, projected (a seam along the
//! middle of a fillet between two charts).
//!
//! An offset larger than the faces allow is an error value
//! ([`BevelError::TooLarge`]); nothing is clipped silently. Every result
//! is checked (`docs/research.md` §7.7): no two of its triangles cross, no
//! old face is tilted off its plane, no bevelled edge is left without
//! width. The input must be closed, with two faces on every edge
//! ([`BevelError::NotManifold`]), and must not cross itself
//! ([`BevelError::SelfCrossing`]; a boolean's points 1e-15 apart are
//! welded first).

mod bevel;
mod patch;
mod profile;
mod uvs;

use std::collections::BTreeMap;

use shape_core::{DVec3, trig};
use shape_mesh::{EdgeKey, Solid};

use bevel::{Kind, Miters, Offsets, Spec};

/// Why a bevel could not be honoured.
#[derive(Clone, Debug, PartialEq)]
pub enum BevelError {
    /// A size that must be positive was not (the field named).
    NotPositive(&'static str),
    /// A value was NaN or infinite (the field named).
    NotFinite(&'static str),
    /// No edge is named so: the names asked for, then the names that exist.
    NoSuchEdge(Vec<String>, Vec<String>),
    /// The solid is not closed and oriented.
    NotClosed,
    /// A named edge lies inside one smooth surface (its ends given).
    NotAnEdge(DVec3, DVec3),
    /// The offsets do not fit the faces at this vertex.
    TooLarge(DVec3),
    /// A configuration this version does not bevel, at this vertex.
    Unsupported(DVec3, &'static str),
    /// The solid is skinned: bevels do not carry skins yet, and never drop
    /// one silently.
    Skinned,
    /// A field holds a value it does not take (the field and why).
    Invalid(&'static str),
    /// An edge joins more than two faces (its ends given), as where two
    /// solids touch along an edge after a join: the solid is not closed.
    NotManifold(DVec3, DVec3),
    /// The solid passes through itself near this point (two of its
    /// triangles cross): repair it before bevelling.
    SelfCrossing(DVec3),
    /// A lone piece of a longer crease (its ends given, then its names)
    /// tapers to a tip at both ends, and that taper does not fit: select
    /// the whole crease by one of its names (`docs/research.md` §7.8).
    LonePiece(DVec3, DVec3, Vec<String>),
}

impl std::fmt::Display for BevelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BevelError::NotPositive(n) => write!(f, "{n} must be positive"),
            BevelError::NotFinite(n) => write!(f, "{n} must be finite"),
            BevelError::NoSuchEdge(m, a) => write!(
                f,
                "no edge is named {}; names: {}",
                m.join(", "),
                a.join(", ")
            ),
            BevelError::NotClosed => f.write_str("the solid is not closed"),
            BevelError::NotAnEdge(a, b) => {
                write!(f, "the edge {a}–{b} lies inside one smooth face")
            }
            BevelError::TooLarge(p) => {
                write!(f, "the bevel is larger than the faces allow at {p}")
            }
            BevelError::Unsupported(p, what) => write!(f, "at {p}: {what} is not supported"),
            BevelError::Invalid(why) => f.write_str(why),
            BevelError::NotManifold(a, b) => write!(
                f,
                "the edge {a}–{b} joins more than two faces (solids touching along an edge): bevel the parts before joining them, or let them overlap"
            ),
            BevelError::SelfCrossing(p) => write!(
                f,
                "the solid passes through itself near {p}: repair it before bevelling"
            ),
            BevelError::LonePiece(a, b, names) => write!(
                f,
                "the edge {a}–{b} is a piece of a longer crease whose neighbours are not bevelled, and its bevel, tapering to a tip at both ends, does not fit: select the whole crease (named {})",
                names.join(", ")
            ),
            BevelError::Skinned => f.write_str(
                "the solid is skinned; bevels do not carry a skin yet: bevel first, then skin it",
            ),
        }
    }
}

impl std::error::Error for BevelError {}

/// How two bevels meet at a gap between them with no unbevelled edge
/// (`docs/research.md` §7.4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Miter {
    /// The bevels meet along one line.
    #[default]
    Sharp,
    /// The meeting point splits in two, slid by the spread along each
    /// bevelled edge, joined by an arc of the bevel's profile.
    Arc,
    /// As `Arc`, with a third point slid by the spread straight away from
    /// the vertex and two arcs.
    Patch,
}

/// Rounds edges.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fillet {
    /// The distance the fillet reaches along each face (the radius where
    /// the faces meet square).
    pub radius: f64,
    /// Edge names; empty for every edge whose faces turn more than 10°.
    pub edges: Vec<String>,
    /// Segments across the fillet; 0 for the chord rule at `radius`
    /// (`shape_2d::curve::segments`), made even so corners are all quads.
    pub segments: u32,
    /// Profile shape: 0.5 (or 0) a circle, 0.25 straight, toward 1 square.
    pub profile: f64,
    /// The miter where a face corner between two bevelled edges is
    /// reflex (more than 180°).
    pub outer_miter: Miter,
    /// The miter where it is less than 180°: `Sharp` or `Arc`.
    pub inner_miter: Miter,
    /// How far an arc or patch miter slides its points; 0 for the offset.
    pub spread: f64,
    /// Where a bevel's end slides along an unbevelled edge, even out the
    /// bevel's width along its whole edge (Blender's Loop Slide).
    pub loop_slide: bool,
}

/// Bevels edges flat.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chamfer {
    /// The distance along the cap face (the face carrying the edge's
    /// name), and along the other unless `distance2` or `angle` is set.
    pub distance: f64,
    /// The distance along the other face; 0 for `distance`.
    pub distance2: f64,
    /// Instead of `distance2`: the angle between the chamfer and the cap
    /// face, radians; 0 for none.
    pub angle: f64,
    /// Edge names; empty for every edge whose faces turn more than 10°.
    pub edges: Vec<String>,
    /// Segments; 0 for one.
    pub segments: u32,
    /// As for [`Fillet::outer_miter`].
    pub outer_miter: Miter,
    /// As for [`Fillet::inner_miter`].
    pub inner_miter: Miter,
    /// As for [`Fillet::spread`].
    pub spread: f64,
    /// As for [`Fillet::loop_slide`].
    pub loop_slide: bool,
}

/// Rounds every sharp edge a little.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Soften {
    /// Fillet radius.
    pub radius: f64,
    /// Edges whose faces turn more than this (radians) are rounded; 0 for
    /// 30°. Ignored when `edges` is given.
    pub angle: f64,
    /// Edge names; empty for every edge sharper than `angle`.
    pub edges: Vec<String>,
    /// As for [`Fillet::segments`].
    pub segments: u32,
}

fn positive(v: f64, name: &'static str) -> Result<f64, BevelError> {
    if !v.is_finite() {
        Err(BevelError::NotFinite(name))
    } else if v <= 0.0 {
        Err(BevelError::NotPositive(name))
    } else {
        Ok(v)
    }
}

/// The miters, checked: a patch miter needs a reflex corner.
fn miters(inner: Miter, outer: Miter, spread: f64) -> Result<Miters, BevelError> {
    if inner == Miter::Patch {
        return Err(BevelError::Invalid(
            "inner_miter takes Sharp or Arc: a patch miter needs a reflex corner",
        ));
    }
    let spread = if spread == 0.0 {
        0.0
    } else {
        positive(spread, "spread")?
    };
    Ok(Miters {
        inner,
        outer,
        spread,
    })
}

/// The edges named, or the real edges turning more than `min_turn`.
fn select(s: &Solid, names: &[String], min_turn: f64) -> Result<Vec<EdgeKey>, BevelError> {
    if !names.is_empty() {
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        return s
            .edges(&refs)
            .map_err(|e| BevelError::NoSuchEdge(e.missing, e.available));
    }
    let topo = bevel::topology(s);
    Ok(s.real_edges()
        .into_iter()
        .filter(
            |&(a, b)| match (topo.dir.get(&(a, b)), topo.dir.get(&(b, a))) {
                (Some(&ta), Some(&tb)) => bevel::turn(s, ta, tb) > min_turn,
                _ => false,
            },
        )
        .collect())
}

/// The bevel, its UVs carried when the solid has some (`uvs.rs`).
fn run(s: &Solid, spec: &Spec) -> Result<Solid, BevelError> {
    if s.uvs.is_none() {
        return bevel::bevel(s, spec);
    }
    let mut bare = s.clone();
    bare.uvs = None;
    let out = bevel::bevel(&bare, spec)?;
    uvs::carry(s, out, spec.kind.name())
}

/// Rounds the named edges with a superellipse profile (a circle by
/// default). New faces are named `fillet`.
pub fn fillet(s: &Solid, d: &Fillet) -> Result<Solid, BevelError> {
    if s.skin.is_some() {
        return Err(BevelError::Skinned);
    }
    if s.morphs.is_some() {
        let bare = Solid {
            morphs: None,
            ..s.clone()
        };
        let mut out = fillet(&bare, d)?;
        out.morphs = shape_mesh::carried(&[s], &out);
        return Ok(out);
    }
    let r = positive(d.radius, "radius")?;
    let profile = if d.profile == 0.0 { 0.5 } else { d.profile };
    if !(profile.is_finite() && profile > 0.0 && profile < 1.0) {
        return Err(BevelError::NotPositive("profile (between 0 and 1)"));
    }
    let edges = select(s, &d.edges, trig::to_radians(10.0))?;
    run(
        s,
        &Spec {
            edges,
            offsets: Offsets::Equal(r),
            segments: d.segments,
            exponent: 4.0 * profile,
            kind: Kind::Fillet,
            miters: miters(d.inner_miter, d.outer_miter, d.spread)?,
            loop_slide: d.loop_slide,
            checked: None,
        },
    )
}

/// Bevels the named edges flat. New faces are named `chamfer`.
pub fn chamfer(s: &Solid, d: &Chamfer) -> Result<Solid, BevelError> {
    if s.skin.is_some() {
        return Err(BevelError::Skinned);
    }
    if s.morphs.is_some() {
        let bare = Solid {
            morphs: None,
            ..s.clone()
        };
        let mut out = chamfer(&bare, d)?;
        out.morphs = shape_mesh::carried(&[s], &out);
        return Ok(out);
    }
    let d1 = positive(d.distance, "distance")?;
    let offsets = if d.angle != 0.0 {
        let a = positive(d.angle, "angle")?;
        if a >= trig::PI {
            return Err(BevelError::NotPositive("π - angle"));
        }
        Offsets::Angle(d1, a)
    } else if d.distance2 != 0.0 {
        Offsets::Two(d1, positive(d.distance2, "distance2")?)
    } else {
        Offsets::Equal(d1)
    };
    let edges = select(s, &d.edges, trig::to_radians(10.0))?;
    run(
        s,
        &Spec {
            edges,
            offsets,
            segments: d.segments,
            exponent: 1.0,
            kind: Kind::Chamfer,
            miters: miters(d.inner_miter, d.outer_miter, d.spread)?,
            loop_slide: d.loop_slide,
            checked: None,
        },
    )
}

/// A fillet of `radius` on every edge sharper than `angle` (or on the
/// named ones). A solid with no such edge comes back unchanged.
pub fn soften(s: &Solid, d: &Soften) -> Result<Solid, BevelError> {
    if s.skin.is_some() {
        return Err(BevelError::Skinned);
    }
    if s.morphs.is_some() {
        let bare = Solid {
            morphs: None,
            ..s.clone()
        };
        let mut out = soften(&bare, d)?;
        out.morphs = shape_mesh::carried(&[s], &out);
        return Ok(out);
    }
    let r = positive(d.radius, "radius")?;
    let angle = if d.angle == 0.0 {
        trig::to_radians(30.0)
    } else {
        positive(d.angle, "angle")?
    };
    let edges = select(s, &d.edges, angle)?;
    if edges.is_empty() {
        return Ok(s.clone());
    }
    run(
        s,
        &Spec {
            edges,
            offsets: Offsets::Equal(r),
            segments: d.segments,
            exponent: 2.0,
            kind: Kind::Fillet,
            miters: Miters::default(),
            loop_slide: false,
            checked: None,
        },
    )
}

/// How much of a set of triangles reads as quads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct QuadPairs {
    /// Triangles measured.
    pub triangles: usize,
    /// Triangles paired into quads.
    pub paired: usize,
    /// The largest fold between the two halves of a quad, radians (0 for
    /// planar quads).
    pub max_fold: f64,
}

/// Pairs triangles of the set into quads: two triangles sharing an edge,
/// on the same surface, whose union is a convex quad. Candidates are
/// taken greedily, the most rectangular first (the smallest sum of
/// |corner − 90°|, then the smallest fold, then triangle order), so a
/// quad grid split along its diagonals pairs back into its quads.
/// Reports the pairs and their worst fold.
pub fn quad_pairs(s: &Solid, triangles: &[u32]) -> QuadPairs {
    let set: std::collections::BTreeSet<u32> = triangles.iter().copied().collect();
    let mut by_edge: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for &i in &set {
        let t = s.triangles[i as usize];
        for k in 0..3 {
            by_edge.insert((t[k], t[(k + 1) % 3]), i);
        }
    }
    let p = |v: u32| s.positions[v as usize];
    let normal = |a: u32, b: u32, c: u32| (p(b) - p(a)).cross(p(c) - p(a)).normalize_or_zero();
    let angle = |a: DVec3, b: DVec3| {
        trig::acos(
            a.normalize_or_zero()
                .dot(b.normalize_or_zero())
                .clamp(-1.0, 1.0),
        )
    };
    let mut cands: Vec<(f64, f64, u32, u32)> = Vec::new();
    for &i in &set {
        let t = s.triangles[i as usize];
        for k in 0..3 {
            let (x, y, z) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
            let Some(&o) = by_edge.get(&(y, x)) else {
                continue;
            };
            if o <= i || s.surfaces[o as usize] != s.surfaces[i as usize] {
                continue;
            }
            let u = s.triangles[o as usize];
            let w = u.iter().copied().find(|&v| v != x && v != y).unwrap_or(x);
            // The quad x → w → y → z; convex when the other diagonal
            // splits it the same way round.
            let (n1, n2) = (normal(x, y, z), normal(y, x, w));
            if normal(x, w, z).dot(n1 + n2) <= 0.0 || normal(w, y, z).dot(n1 + n2) <= 0.0 {
                continue;
            }
            let ring = [x, w, y, z];
            let skew: f64 = (0..4)
                .map(|c| {
                    let (a, b, d) = (ring[(c + 3) % 4], ring[c], ring[(c + 1) % 4]);
                    (angle(p(a) - p(b), p(d) - p(b)) - trig::PI / 2.0).abs()
                })
                .sum();
            cands.push((skew, angle(n1, n2), i, o));
        }
    }
    cands.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.cmp(&b.2))
            .then(a.3.cmp(&b.3))
    });
    let mut taken = std::collections::BTreeSet::new();
    let mut q = QuadPairs {
        triangles: set.len(),
        ..Default::default()
    };
    for (_, fold, i, o) in cands {
        if taken.contains(&i) || taken.contains(&o) {
            continue;
        }
        taken.insert(i);
        taken.insert(o);
        q.paired += 2;
        q.max_fold = q.max_fold.max(fold);
    }
    q
}

#[cfg(test)]
mod tests;
