//! Edits on named solids, after sand3d's verbs (`docs/design.md` §2).
//!
//! Entry: the functions below, each taking a [`Solid`] and a zero-value
//! desc and returning a new canonical solid or an [`EditError`].
//!
//! | edit | how | names it adds |
//! |---|---|---|
//! | [`split`] | intersect or cut with a half-space | `cut`, `split.*` |
//! | [`hole`] | cut cylinders (and a counterbore or countersink) from the entry point | `hole` on the mouth edges, `hole.side`, `hole.bottom` |
//! | [`pocket`] | cut a profile extruded from a face | `pocket.bottom`, `pocket.side` |
//! | [`seat`] | cut the volume a part fills | `cut`, `<part>.*` |
//! | [`shell`] | cut a solid offset inside the surface | `cut`, `shell.*` |
//! | [`dent`] | press the surface in, subdividing where needed | `dent` on moved faces |
//! | [`merge`] | append, no boolean | faces as given |

use std::collections::BTreeMap;

use robust::{Coord3D, orient3d};
use shape_2d::Profile;
use shape_bool::{BoolError, BoolOp, Boolean, boolean};
use shape_core::{DVec3, Frame};
use shape_gen::solids::{Extrude, GenError, extrude};
use shape_mesh::{Solid, edge_key};

/// Why an edit could not be honoured.
#[derive(Clone, Debug, PartialEq)]
pub enum EditError {
    /// The boolean underneath refused (see [`BoolError`]).
    Bool(BoolError),
    /// A tool could not be built.
    Gen(GenError),
    /// A size that must be positive was not (the field named).
    NotPositive(&'static str),
    /// An axis or normal of zero length (the field named).
    NoDirection(&'static str),
    /// The axis through a frame's origin, along its −z (into the part),
    /// meets no face: it runs past the part, or inside an opening through
    /// it (a through hole on the same axis).
    Misses {
        /// The frame's origin, as given.
        origin: DVec3,
        /// The axis's direction into the part (the frame's −z).
        direction: DVec3,
        /// The distance from the axis to the nearest edge of the solid.
        clearance: f64,
    },
    /// Nothing named so (the names that exist follow).
    NoSuchFace(Vec<String>, Vec<String>),
    /// A face to open is not flat: cut it flat first.
    NotFlat(String),
    /// The part shares no volume with the host.
    NoOverlap,
    /// The solid is skinned: edits do not carry skins yet, and never drop
    /// one silently.
    Skinned,
}

impl From<BoolError> for EditError {
    fn from(e: BoolError) -> Self {
        EditError::Bool(e)
    }
}

impl From<GenError> for EditError {
    fn from(e: GenError) -> Self {
        EditError::Gen(e)
    }
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::Bool(e) => write!(f, "{e}"),
            EditError::Gen(e) => write!(f, "{e}"),
            EditError::NotPositive(n) => write!(f, "{n} must be positive"),
            EditError::NoDirection(n) => write!(f, "{n} has no direction"),
            EditError::Misses {
                origin,
                direction,
                clearance,
            } => write!(
                f,
                "the axis through {origin} along {direction} meets no face of the solid: \
                 its nearest edge is {clearance} away from it (an axis inside a through \
                 hole meets nothing)"
            ),
            EditError::NoSuchFace(m, a) => write!(
                f,
                "no face is named {}; names: {}",
                m.join(", "),
                a.join(", ")
            ),
            EditError::NotFlat(n) => write!(f, "the faces {n} are not flat; cut them flat first"),
            EditError::NoOverlap => f.write_str("the part shares no volume with the host"),
            EditError::Skinned => f.write_str(
                "the solid is skinned; edits do not carry skins yet: edit first, then skin it",
            ),
        }
    }
}

impl std::error::Error for EditError {}

fn unskinned(solids: &[&Solid]) -> Result<(), EditError> {
    if solids.iter().any(|s| s.skin.is_some()) {
        Err(EditError::Skinned)
    } else {
        Ok(())
    }
}

/// Finite and above zero (NaN fails).
fn positive(v: f64) -> bool {
    v.is_finite() && v > 0.0
}

fn size(s: &Solid) -> f64 {
    let m = s.measure();
    (m.bounds_max - m.bounds_min).length().max(1.0)
}

fn placed(mut tool: Solid, frame: &Frame) -> Solid {
    tool.map_positions(|p| frame.point_to_world(p), false);
    tool
}

/// Where the line through `origin` along `-dir` first enters the solid
/// from the `dir` side: the hit farthest along `dir`. Whether the line
/// meets a triangle is decided exactly: the line, through `origin` and
/// `origin - dir`, meets it when `orient3d` of the line with each of its
/// edges has one sign or is zero, so a line through an edge or a corner
/// meets every triangle there (`docs/research.md` §13.6, item 13). A
/// triangle the line lies in is skipped. `Err` gives the distance from the
/// line to the nearest edge when it meets none.
fn entry(s: &Solid, origin: DVec3, dir: DVec3) -> Result<DVec3, f64> {
    let c3 = |p: DVec3| Coord3D {
        x: p.x,
        y: p.y,
        z: p.z,
    };
    let d = -dir;
    let (p0, p1) = (c3(origin), c3(origin + d));
    let mut best: Option<f64> = None;
    for t in &s.triangles {
        let [a, b, c] = t.map(|v| s.positions[v as usize]);
        let side = [(a, b), (b, c), (c, a)].map(|(x, y)| orient3d(p0, p1, c3(x), c3(y)));
        if side.iter().any(|&x| x > 0.0) && side.iter().any(|&x| x < 0.0) {
            continue;
        }
        // Distance along -dir from the triangle's plane equation, which is
        // exact for axis-aligned faces (the hit point lands on the face).
        let n = (b - a).cross(c - a);
        let nd = n.dot(d);
        if side == [0.0; 3] || nd == 0.0 {
            continue;
        }
        let dist = n.dot(a - origin) / nd;
        if best.is_none_or(|b| dist < b) {
            best = Some(dist);
        }
    }
    best.map(|dist| origin + d * dist)
        .ok_or_else(|| clearance(s, origin, dir))
}

/// The distance from the line through `origin` along `dir` to the nearest
/// edge of `s` (infinite for a solid with no triangles).
fn clearance(s: &Solid, origin: DVec3, dir: DVec3) -> f64 {
    let u = dir.normalize_or_zero();
    let off = |v: DVec3| v - u * v.dot(u);
    let mut near = f64::INFINITY;
    for t in &s.triangles {
        for k in 0..3 {
            let (a, b) = (
                s.positions[t[k] as usize],
                s.positions[t[(k + 1) % 3] as usize],
            );
            let (w, e) = (off(a - origin), off(b - a));
            let ee = e.dot(e);
            let f = if ee > 0.0 {
                (-w.dot(e) / ee).clamp(0.0, 1.0)
            } else {
                0.0
            };
            near = near.min((w + e * f).length());
        }
    }
    near
}

/// The entry point of frame `f`'s axis, or [`EditError::Misses`].
fn entry_of(s: &Solid, f: &Frame, axis: &Frame, span: f64) -> Result<DVec3, EditError> {
    entry(s, f.origin + axis.z * 2.0 * span, axis.z).map_err(|clearance| EditError::Misses {
        origin: f.origin,
        direction: -axis.z + DVec3::ZERO,
        clearance,
    })
}

// ── split ───────────────────────────────────────────────────────────────────

/// Which side of the plane [`split`] keeps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Keep {
    /// On the side the frame's normal (+z) points to.
    #[default]
    Above,
    /// The other side.
    Below,
}

/// The solid cut by the plane through `frame.origin` normal to `frame.z`.
pub fn split(s: &Solid, frame: &Frame, keep: Keep) -> Result<Solid, EditError> {
    unskinned(&[s])?;
    let l = 4.0 * size(s)
        + 4.0 * (frame.origin - s.positions.first().copied().unwrap_or_default()).length();
    let half = shape_gen::solids::cuboid(&shape_gen::solids::BoxDesc {
        width: 2.0 * l,
        depth: 2.0 * l,
        height: l,
    })?;
    let tool = placed(half, frame);
    let op = match keep {
        Keep::Above => BoolOp::Intersect,
        Keep::Below => BoolOp::Cut,
    };
    Ok(boolean(
        s,
        &tool,
        &Boolean {
            op,
            target: "",
            tool: "split",
        },
    )?)
}

// ── hole ────────────────────────────────────────────────────────────────────

/// Round holes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hole {
    /// Diameter.
    pub diameter: f64,
    /// Where: each frame's origin above the part, its +z pointing out of
    /// the material. The hole starts where its axis enters the part.
    pub at: Vec<Frame>,
    /// Depth from the entry; 0 for right through.
    pub depth: f64,
    /// Counterbore `(diameter, depth)`; zeros for none.
    pub counterbore: (f64, f64),
    /// Countersink `(diameter, full angle in radians)`; zeros for none.
    pub countersink: (f64, f64),
}

/// A frame looking down at `(x, y)` from above: what a plain point means.
pub fn above(x: f64, y: f64) -> Frame {
    Frame {
        origin: DVec3::new(x, y, 0.0),
        ..Frame::IDENTITY
    }
}

/// A tool along a frame's −z: from `over` above the entry to `depth` below.
fn bore(
    entry: DVec3,
    axis: &Frame,
    r_bottom: f64,
    r_top: f64,
    depth: f64,
    over: f64,
    n: u32,
) -> Result<Solid, EditError> {
    let circle = shape_2d::circle(&shape_2d::Circle {
        radius: r_bottom,
        segments: n,
    })
    .map_err(GenError::from)?;
    let c = extrude(
        &circle,
        &Extrude {
            height: depth + over,
            taper: 1.0 - r_top / r_bottom,
            ..Default::default()
        },
    )?;
    let f = Frame {
        origin: entry - axis.z * depth,
        ..*axis
    };
    Ok(placed(c, &f))
}

/// Holes of `d.diameter` into the solid. Mouth edges get `hole`; the bore
/// `hole.side`, a blind floor `hole.bottom`.
pub fn hole(s: &Solid, d: &Hole) -> Result<Solid, EditError> {
    unskinned(&[s])?;
    if !positive(d.diameter) {
        return Err(EditError::NotPositive("diameter"));
    }
    let span = size(s);
    let over = span * 1e-3;
    let mut out = s.clone();
    for f in &d.at {
        let axis = Frame::from_normal(DVec3::ZERO, f.z, f.x).ok_or(EditError::NoDirection("at"))?;
        let e = entry_of(&out, f, &axis, span)?;
        let depth = if d.depth > 0.0 { d.depth } else { 4.0 * span };
        let r = d.diameter / 2.0;
        // One segment count for every tool of this hole, so a countersink's
        // narrow end is the bore's own polygon.
        let widest = r.max(d.counterbore.0 / 2.0).max(d.countersink.0 / 2.0);
        let n = shape_2d::curve::segments(widest, shape_core::trig::TAU);
        let mut tools = vec![bore(e, &axis, r, r, depth, over, n)?];
        let (cb_d, cb_depth) = d.counterbore;
        if cb_d > 0.0 && cb_depth > 0.0 {
            tools.push(bore(e, &axis, cb_d / 2.0, cb_d / 2.0, cb_depth, over, n)?);
        }
        let (cs_d, cs_angle) = d.countersink;
        if cs_d > 2.0 * r && cs_angle > 0.0 {
            // The cone runs on below the bore's radius (to 0.9 r), so it
            // crosses the bore wall instead of touching it along a ring.
            let slope = shape_core::trig::tan(cs_angle / 2.0);
            let h = (cs_d / 2.0 - 0.9 * r) / slope;
            tools.push(bore(
                e,
                &axis,
                0.9 * r,
                cs_d / 2.0 + over * slope,
                h,
                over,
                n,
            )?);
        }
        for t in tools {
            out = boolean(
                &out,
                &t,
                &Boolean {
                    op: BoolOp::Cut,
                    target: "",
                    tool: "hole",
                },
            )?;
        }
    }
    Ok(out)
}

// ── pocket ──────────────────────────────────────────────────────────────────

/// A profile cut in from a face.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pocket {
    /// The outline, in the frames' xy plane.
    pub profile: Profile,
    /// Depth from the entry; 0 for right through.
    pub depth: f64,
    /// Where: frames as for [`Hole::at`]; empty for one over the origin.
    pub at: Vec<Frame>,
}

/// The profile cut into the solid from where each frame's axis enters it.
/// Floor and walls are named `pocket.bottom` and `pocket.side`.
pub fn pocket(s: &Solid, d: &Pocket) -> Result<Solid, EditError> {
    unskinned(&[s])?;
    let span = size(s);
    let over = span * 1e-3;
    let frames = if d.at.is_empty() {
        vec![above(0.0, 0.0)]
    } else {
        d.at.clone()
    };
    let mut out = s.clone();
    for f in &frames {
        let axis = Frame::from_normal(DVec3::ZERO, f.z, f.x).ok_or(EditError::NoDirection("at"))?;
        let e = entry_of(&out, f, &axis, span)?;
        let depth = if d.depth > 0.0 { d.depth } else { 4.0 * span };
        let tool = extrude(
            &d.profile,
            &Extrude {
                height: depth + over,
                ..Default::default()
            },
        )?;
        let frame = Frame {
            origin: e - axis.z * depth,
            ..axis
        };
        out = boolean(
            &out,
            &placed(tool, &frame),
            &Boolean {
                op: BoolOp::Cut,
                target: "",
                tool: "pocket",
            },
        )?;
    }
    Ok(out)
}

// ── seat ────────────────────────────────────────────────────────────────────

/// The host with the volume `part` fills taken out; `part` is untouched.
/// Refused if they share no volume.
pub fn seat(host: &Solid, part: &Solid, part_name: &str) -> Result<Solid, EditError> {
    unskinned(&[host, part])?;
    let shared = boolean(
        host,
        part,
        &Boolean {
            op: BoolOp::Intersect,
            ..Default::default()
        },
    );
    match shared {
        Err(BoolError::Empty) => return Err(EditError::NoOverlap),
        Err(e) => return Err(e.into()),
        Ok(v) if v.measure().volume <= 0.0 => return Err(EditError::NoOverlap),
        Ok(_) => {}
    }
    Ok(boolean(
        host,
        part,
        &Boolean {
            op: BoolOp::Cut,
            target: "",
            tool: part_name,
        },
    )?)
}

// ── shell ───────────────────────────────────────────────────────────────────

/// Hollowing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shell {
    /// Wall thickness.
    pub thickness: f64,
    /// Faces to take away: names (`top`), or a direction (`+x`, `-y`, …)
    /// for the flat face furthest that way. Empty for a closed void.
    pub open: Vec<String>,
}

/// cos 1°: normals closer than this are one plane for the offset.
const SAME_PLANE_COS: f64 = 0.999_847_695_156_391_3;
/// cos 0.5°: how flat a face to open must be.
const FLAT_COS: f64 = 0.999_961_923_064_171_3;

fn tri_normal(s: &Solid, t: [u32; 3]) -> DVec3 {
    let [a, b, c] = t.map(|v| s.positions[v as usize]);
    (b - a).cross(c - a).normalize_or_zero()
}

/// Triangles to open: by name, or the flat face furthest along a direction.
fn opened(s: &Solid, open: &[String]) -> Result<Vec<bool>, EditError> {
    let mut mask = vec![false; s.triangles.len()];
    for term in open {
        let dir = match term.to_ascii_lowercase().as_str() {
            "+x" => Some(DVec3::X),
            "-x" => Some(DVec3::NEG_X),
            "+y" => Some(DVec3::Y),
            "-y" => Some(DVec3::NEG_Y),
            "+z" => Some(DVec3::Z),
            "-z" => Some(DVec3::NEG_Z),
            _ => None,
        };
        let picked: Vec<usize> = match dir {
            Some(dir) => {
                let far = s
                    .positions
                    .iter()
                    .map(|p| p.dot(dir))
                    .fold(f64::NEG_INFINITY, f64::max);
                let tol = size(s) * 1e-9;
                (0..s.triangles.len())
                    .filter(|&i| {
                        let t = s.triangles[i];
                        tri_normal(s, t).dot(dir) > FLAT_COS
                            && t.iter()
                                .all(|&v| (s.positions[v as usize].dot(dir) - far).abs() <= tol)
                    })
                    .collect()
            }
            None => s
                .faces(&[term])
                .map_err(|e| EditError::NoSuchFace(e.missing, e.available))?
                .into_iter()
                .map(|i| i as usize)
                .collect(),
        };
        if picked.is_empty() {
            return Err(EditError::NotFlat(term.clone()));
        }
        let n0 = tri_normal(s, s.triangles[picked[0]]);
        if picked
            .iter()
            .any(|&i| tri_normal(s, s.triangles[i]).dot(n0) < FLAT_COS)
        {
            return Err(EditError::NotFlat(term.clone()));
        }
        for i in picked {
            mask[i] = true;
        }
    }
    Ok(mask)
}

/// The displacement `d` with `n·d = target` for every plane, in the least
/// squares sense over the distinct planes (1, 2 or 3+).
fn offset(planes: &[(DVec3, f64)]) -> DVec3 {
    // Cluster normals that are one plane; a cluster with an open face
    // keeps the open face's target.
    let mut clusters: Vec<(DVec3, f64, bool)> = Vec::new();
    for &(n, target) in planes {
        let open = target > 0.0;
        match clusters
            .iter_mut()
            .find(|c| c.0.normalize().dot(n) > SAME_PLANE_COS)
        {
            Some(c) => {
                c.0 += n;
                if open {
                    c.1 = target;
                    c.2 = true;
                }
            }
            None => clusters.push((n, target, open)),
        }
    }
    let cs: Vec<(DVec3, f64)> = clusters.iter().map(|c| (c.0.normalize(), c.1)).collect();
    match cs.len() {
        0 => DVec3::ZERO,
        1 => cs[0].0 * cs[0].1,
        2 => {
            // d = a n1 + b n2 with n1·d = t1, n2·d = t2.
            let (n1, t1, n2, t2) = (cs[0].0, cs[0].1, cs[1].0, cs[1].1);
            let k = n1.dot(n2);
            let det = 1.0 - k * k;
            if det.abs() < 1e-12 {
                return n1 * t1;
            }
            let a = (t1 - k * t2) / det;
            let b = (t2 - k * t1) / det;
            n1 * a + n2 * b
        }
        _ => {
            // Normal equations (Σ n nᵀ) d = Σ t n, by Cramer's rule.
            let mut m = [[0.0f64; 3]; 3];
            let mut r = [0.0f64; 3];
            for &(n, t) in &cs {
                let n = n.to_array();
                for i in 0..3 {
                    r[i] += t * n[i];
                    for j in 0..3 {
                        m[i][j] += n[i] * n[j];
                    }
                }
            }
            let det3 = |m: &[[f64; 3]; 3]| {
                m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
                    - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                    + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
            };
            let det = det3(&m);
            if det.abs() < 1e-12 {
                return cs.iter().map(|c| c.0 * c.1).sum::<DVec3>() / cs.len() as f64;
            }
            let col = |k: usize| {
                let mut mk = m;
                for i in 0..3 {
                    mk[i][k] = r[i];
                }
                det3(&mk) / det
            };
            DVec3::new(col(0), col(1), col(2))
        }
    }
}

/// Hollows the solid, leaving walls `thickness` thick. Each vertex of an
/// inner solid moves by the least-squares fit over the planes it touches:
/// `thickness` in from kept faces, `thickness` out through opened ones (so
/// the cut opens them); then the inner solid is cut away. Inner faces get
/// `cut` and `shell.*`.
pub fn shell(s: &Solid, d: &Shell) -> Result<Solid, EditError> {
    unskinned(&[s])?;
    let t = d.thickness;
    if !positive(t) {
        return Err(EditError::NotPositive("thickness"));
    }
    let open = opened(s, &d.open)?;
    let mut planes: Vec<Vec<(DVec3, f64)>> = vec![Vec::new(); s.positions.len()];
    for (i, tri) in s.triangles.iter().enumerate() {
        let [a, b, c] = tri.map(|v| s.positions[v as usize]);
        let area_n = (b - a).cross(c - a);
        let target = if open[i] { t } else { -t };
        for &v in tri {
            planes[v as usize].push((area_n.normalize_or_zero(), target));
        }
    }
    let mut inner = Solid::default();
    for (v, p) in s.positions.iter().enumerate() {
        inner.push_vertex(*p + offset(&planes[v]));
    }
    for (i, tri) in s.triangles.iter().enumerate() {
        inner.push_triangle(*tri, s.surfaces[i], &["wall"]);
    }
    inner
        .canonicalize()
        .map_err(|_| EditError::NotPositive("thickness"))?;
    Ok(boolean(
        s,
        &inner,
        &Boolean {
            op: BoolOp::Cut,
            target: "",
            tool: "shell",
        },
    )?)
}

// ── dent ────────────────────────────────────────────────────────────────────

/// Round hollows pressed into the surface.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dent {
    /// Centres, on the surface.
    pub at: Vec<DVec3>,
    /// Radius of each hollow.
    pub radius: f64,
    /// Depth at the centre.
    pub depth: f64,
}

/// Presses the surface in at each point: vertices within `radius` move
/// against the local normal by `depth · (1 − (r/radius)²)²`. Triangles in
/// reach are first split (shared edge midpoints, so no cracks) until no
/// edge is longer than `radius / 4`. Moved faces get `dent`.
pub fn dent(s: &Solid, d: &Dent) -> Result<Solid, EditError> {
    unskinned(&[s])?;
    if !positive(d.radius) {
        return Err(EditError::NotPositive("radius"));
    }
    if !positive(d.depth) {
        return Err(EditError::NotPositive("depth"));
    }
    let mut out = s.clone();
    let max_edge = d.radius / 4.0;
    let near = |p: DVec3| d.at.iter().any(|c| (p - *c).length() < d.radius + max_edge);
    for _ in 0..12 {
        let mut split_at: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for t in &out.triangles {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                let (pa, pb) = (out.positions[a as usize], out.positions[b as usize]);
                if (pa - pb).length() > max_edge && (near(pa) || near(pb) || near((pa + pb) / 2.0))
                {
                    split_at.insert(edge_key(a, b), 0);
                }
            }
        }
        if split_at.is_empty() {
            break;
        }
        for (&(a, b), mid) in split_at.iter_mut() {
            *mid = out.push_vertex((out.positions[a as usize] + out.positions[b as usize]) / 2.0);
        }
        let mut next = Solid {
            positions: out.positions.clone(),
            edge_tags: BTreeMap::new(),
            names: Default::default(),
            ..Default::default()
        };
        for (i, t) in out.triangles.iter().enumerate() {
            let tags: Vec<&str> = out.face_tags[i]
                .iter()
                .map(|&x| out.names.name(x))
                .collect();
            let m = [0, 1, 2].map(|k| split_at.get(&edge_key(t[k], t[(k + 1) % 3])).copied());
            for tri in split_triangle(*t, m) {
                next.push_triangle(tri, out.surfaces[i], &tags);
            }
        }
        // Named edges: each half keeps the name.
        for (&(a, b), tags) in &out.edge_tags {
            let names: Vec<&str> = tags.iter().map(|&x| out.names.name(x)).collect();
            match split_at.get(&(a, b)) {
                Some(&m) => {
                    next.tag_edge(a, m, &names);
                    next.tag_edge(m, b, &names);
                }
                None => next.tag_edge(a, b, &names),
            }
        }
        out = next;
    }
    // Vertex normals (area-weighted) for the push direction.
    let mut normals = vec![DVec3::ZERO; out.positions.len()];
    for t in &out.triangles {
        let [a, b, c] = t.map(|v| out.positions[v as usize]);
        let n = (b - a).cross(c - a);
        for &v in t {
            normals[v as usize] += n;
        }
    }
    let mut moved = vec![false; out.positions.len()];
    for (v, p) in out.positions.clone().iter().enumerate() {
        let mut push = 0.0;
        for c in &d.at {
            let r = (*p - *c).length() / d.radius;
            if r < 1.0 {
                let f = 1.0 - r * r;
                push += d.depth * f * f;
            }
        }
        if push > 0.0 {
            out.positions[v] = *p - normals[v].normalize_or_zero() * push;
            moved[v] = true;
        }
    }
    let dent_tag = out.names.intern("dent");
    for (i, t) in out.triangles.iter().enumerate() {
        if t.iter().any(|&v| moved[v as usize]) && !out.face_tags[i].contains(&dent_tag) {
            out.face_tags[i].push(dent_tag);
            out.face_tags[i].sort_unstable();
        }
    }
    out.canonicalize()
        .map_err(|_| EditError::NotPositive("depth"))?;
    Ok(out)
}

/// A triangle with some edges split at the given midpoints, as 1 to 4
/// triangles with the same winding.
fn split_triangle(t: [u32; 3], m: [Option<u32>; 3]) -> Vec<[u32; 3]> {
    let [a, b, c] = t;
    match m {
        [None, None, None] => vec![t],
        [Some(ab), Some(bc), Some(ca)] => vec![[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]],
        _ => {
            // Rotate so the split edges come first, then fan.
            let k = (0..3)
                .find(|&k| m[k].is_some() && m[(k + 2) % 3].is_none())
                .unwrap_or(0);
            let v = [t[k], t[(k + 1) % 3], t[(k + 2) % 3]];
            let mm = [m[k], m[(k + 1) % 3], m[(k + 2) % 3]];
            match mm {
                [Some(x), None, None] => vec![[v[0], x, v[2]], [x, v[1], v[2]]],
                [Some(x), Some(y), None] => vec![[v[0], x, v[2]], [x, v[1], y], [x, y, v[2]]],
                _ => vec![t],
            }
        }
    }
}

// ── merge ───────────────────────────────────────────────────────────────────

/// One solid holding all the given ones, untouched: no boolean. Shells
/// that touch stay separate; a later boolean fuses them first. Skins
/// follow [`Solid::append`]: kept when every solid has the same skeleton,
/// otherwise reported by [`shape_mesh::Skin::check`].
pub fn merge(solids: &[&Solid]) -> Solid {
    let mut out = Solid::default();
    for s in solids {
        out.append(s);
    }
    let _ = out.canonicalize();
    out
}

#[cfg(test)]
mod tests;
