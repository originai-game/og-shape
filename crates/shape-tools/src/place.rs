//! Placing a mesh from elsewhere where a game wants it: which way is up
//! and which way it faces, how large it is, and where its origin sits
//! (`docs/research.md` §31).
//!
//! Entry: [`place`] with a [`Place`]. Three steps, each optional, in this
//! order:
//!
//! 1. **Turn.** [`Place::up`] and [`Place::front`] name the input axes
//!    that are the object's up and its front; the turn takes them to the
//!    kernel's up (+Z) and front (−Y, the side `preview` looks at first).
//!    It is one of the 24 axis-aligned turns: a permutation of the
//!    coordinates with signs, so it rounds nothing. [`Place::turn`] then
//!    turns about the new up, in degrees (counter-clockwise seen from
//!    above); a multiple of 90 is exact too, any other angle goes through
//!    `shape_core::trig`.
//! 2. **Size.** [`Size`]: the longest side, the height, the width or the
//!    depth of the turned box to a length, or a factor. Always uniform.
//! 3. **Pivot.** [`Pivot`]: the centre of the box's bottom face (after
//!    the turn, so "bottom" is the new −Z), the box's centre, or a point
//!    of the input, moved to the origin.
//!
//! Every vertex moves; UVs, face names and surfaces stay as they are, and
//! normals follow (a solid stores none: they are made from the positions
//! on export). A skinned solid keeps its skin: bone rest frames turn and
//! move with the mesh and keep unit axes; a scale moves their origins
//! only, as import takes an armature's scale out of its bones
//! (`docs/research.md` §11.5); inverse bind matrices become
//! `S · IBM · A⁻¹` for the placement `A` and the scale `S`, so a bone's
//! rest frame times its inverse bind matrix is what it was.
//!
//! [`axis_measures`] reports the numbers an agent reads to choose up and
//! front (extents, pivot, the area facing each way, where the mass
//! sits). It reports them and never guesses.

use core::fmt;

use shape_core::trig;
use shape_core::{DVec3, Frame};
use shape_mesh::skin::Mat4;
use shape_mesh::{Bone, Skin, Solid, edge_key};
use std::collections::BTreeMap;

use crate::import::Affine;
use crate::json::Json;
use crate::measure::weld_exact;

/// One of the six axis directions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Axis {
    /// +X.
    PosX,
    /// −X.
    NegX,
    /// +Y.
    PosY,
    /// −Y.
    NegY,
    /// +Z.
    PosZ,
    /// −Z.
    NegZ,
}

impl Axis {
    /// All six, in the order `+x -x +y -y +z -z`.
    pub const ALL: [Axis; 6] = [
        Axis::PosX,
        Axis::NegX,
        Axis::PosY,
        Axis::NegY,
        Axis::PosZ,
        Axis::NegZ,
    ];

    /// Reads `+x`, `-x`, `x` (= `+x`), … in either case.
    pub fn parse(s: &str) -> Option<Axis> {
        let s = s.trim().to_ascii_lowercase();
        let (neg, name) = match s.as_bytes() {
            [b'+', n] => (false, *n),
            [b'-', n] => (true, *n),
            [n] => (false, *n),
            _ => return None,
        };
        let i = match name {
            b'x' => 0,
            b'y' => 1,
            b'z' => 2,
            _ => return None,
        };
        Some(Axis::of(i, neg))
    }

    /// `+x`, `-x`, … as [`Axis::parse`] reads it.
    pub fn name(self) -> &'static str {
        ["+x", "-x", "+y", "-y", "+z", "-z"][self as usize]
    }

    /// The coordinate it runs along: 0, 1 or 2.
    pub fn index(self) -> usize {
        self as usize / 2
    }

    /// Whether it points down its coordinate.
    pub fn negative(self) -> bool {
        self as usize % 2 == 1
    }

    /// The opposite direction.
    pub fn opposite(self) -> Axis {
        Axis::of(self.index(), !self.negative())
    }

    fn of(index: usize, negative: bool) -> Axis {
        Axis::ALL[index * 2 + usize::from(negative)]
    }

    /// The unit vector.
    pub fn vector(self) -> DVec3 {
        let mut v = [0.0; 3];
        v[self.index()] = if self.negative() { -1.0 } else { 1.0 };
        DVec3::from_array(v)
    }

    /// `self × other` when both are axes at right angles.
    fn cross(self, other: Axis) -> Option<Axis> {
        let (a, b) = (self.index(), other.index());
        if a == b {
            return None;
        }
        let c = 3 - a - b;
        // e_a × e_b = +e_c when (a, b, c) is a cyclic order.
        let cyclic = (a + 1) % 3 == b;
        Some(Axis::of(c, cyclic == (self.negative() != other.negative())))
    }
}

impl fmt::Display for Axis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// An axis-aligned turn: output coordinate `k` is input coordinate
/// `from[k]`, negated where `neg[k]`. Exact: it moves and negates
/// numbers, and rounds none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AxisTurn {
    from: [usize; 3],
    neg: [bool; 3],
}

impl AxisTurn {
    pub(crate) const IDENTITY: AxisTurn = AxisTurn {
        from: [0, 1, 2],
        neg: [false; 3],
    };

    /// The turn that takes input `up` to +Z and input `front` to −Y.
    /// Either may be left out: with only `up`, the smallest turn that
    /// stands `up` up; with only `front`, the smallest that brings it to
    /// −Y; with neither, no turn.
    pub(crate) fn new(up: Option<Axis>, front: Option<Axis>) -> Result<AxisTurn, String> {
        use Axis::*;
        let (up, front) = match (up, front) {
            (None, None) => return Ok(AxisTurn::IDENTITY),
            (Some(u), Some(f)) => (u, f),
            (Some(u), None) => (
                u,
                match u {
                    PosX | NegX | PosZ => NegY,
                    NegZ => PosY,
                    PosY => PosZ,
                    NegY => NegZ,
                },
            ),
            (None, Some(f)) => (
                match f {
                    PosX | NegX | PosY | NegY => PosZ,
                    PosZ => PosY,
                    NegZ => NegY,
                },
                f,
            ),
        };
        let Some(side) = up.cross(front) else {
            return Err(format!(
                "up {up} and front {front} lie along one axis; give two axes at right angles"
            ));
        };
        // The images: up → +Z, front → −Y, up × front → +X (Z × −Y = X),
        // so the turn keeps handedness.
        let mut t = AxisTurn::IDENTITY;
        for (input, output) in [(side, PosX), (front, NegY), (up, PosZ)] {
            t.from[output.index()] = input.index();
            t.neg[output.index()] = input.negative() != output.negative();
        }
        Ok(t)
    }

    /// The turn applied to a point or a direction.
    pub(crate) fn apply(&self, v: DVec3) -> DVec3 {
        let a = v.to_array();
        let c = |k: usize| {
            if self.neg[k] {
                -a[self.from[k]]
            } else {
                a[self.from[k]]
            }
        };
        DVec3::new(c(0), c(1), c(2))
    }

    /// Where input axis `a` goes.
    pub(crate) fn image(&self, a: Axis) -> Axis {
        let k = (0..3).find(|&k| self.from[k] == a.index()).unwrap_or(0);
        Axis::of(k, a.negative() != self.neg[k])
    }

    /// `self ∘ other`: `other` first.
    pub(crate) fn then(&self, other: &AxisTurn) -> AxisTurn {
        let mut t = AxisTurn::IDENTITY;
        for k in 0..3 {
            t.from[k] = other.from[self.from[k]];
            t.neg[k] = self.neg[k] != other.neg[self.from[k]];
        }
        t
    }

    /// The turn applied after an affine map (to its columns and its
    /// translation): `self ∘ a`, exact.
    pub(crate) fn after(&self, a: &Affine) -> Affine {
        Affine {
            x: self.apply(a.x),
            y: self.apply(a.y),
            z: self.apply(a.z),
            t: self.apply(a.t),
        }
    }

    /// The turn back.
    pub(crate) fn inverse(&self) -> AxisTurn {
        let mut t = AxisTurn::IDENTITY;
        for k in 0..3 {
            t.from[self.from[k]] = k;
            t.neg[self.from[k]] = self.neg[k];
        }
        t
    }

    /// The quarter turns about Z, counter-clockwise: 0 to 3.
    fn quarter(n: u32) -> AxisTurn {
        // (x, y) ↦ (−y, x) once.
        let once = AxisTurn {
            from: [1, 0, 2],
            neg: [true, false, false],
        };
        (0..n % 4).fold(AxisTurn::IDENTITY, |t, _| once.then(&t))
    }
}

/// How large to make the mesh. Every size is uniform: the mesh is never
/// stretched. Lengths are in millimetres, of the box after the turn. The
/// zero value keeps the size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Size {
    /// Keep the size.
    #[default]
    Keep,
    /// The longest side of the box to this length, mm.
    Longest(f64),
    /// The box's extent along Z (up) to this length, mm.
    Height(f64),
    /// Its extent along X (side to side) to this length, mm.
    Width(f64),
    /// Its extent along Y (front to back) to this length, mm.
    Depth(f64),
    /// Scale by this factor.
    Scale(f64),
}

/// Where the origin goes. The zero value keeps it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Pivot {
    /// Keep the origin where the turn and the scale put it.
    #[default]
    Keep,
    /// The centre of the box's bottom face (lowest Z after the turn): a
    /// prop or a character then stands on z = 0.
    Base,
    /// The centre of the box.
    Center,
    /// This point of the input (its coordinates before the turn, mm).
    Point(DVec3),
}

/// How to place a mesh. The zero value changes nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Place {
    /// The input axis that is the object's up; it becomes +Z.
    pub up: Option<Axis>,
    /// The input axis the object faces; it becomes −Y.
    pub front: Option<Axis>,
    /// Then a turn about the up axis, degrees, counter-clockwise seen
    /// from above.
    pub turn: f64,
    /// The size.
    pub size: Size,
    /// The origin.
    pub pivot: Pivot,
}

/// Why a mesh could not be placed.
#[derive(Clone, Debug, PartialEq)]
pub enum PlaceError {
    /// Up and front along one axis.
    Axes(String),
    /// A size, factor, turn or point that is not a finite positive
    /// number, or a box with no extent to scale.
    Size(String),
    /// No vertices.
    Empty,
}

impl fmt::Display for PlaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlaceError::Axes(w) | PlaceError::Size(w) => f.write_str(w),
            PlaceError::Empty => f.write_str("nothing to place: the mesh has no vertices"),
        }
    }
}

impl std::error::Error for PlaceError {}

/// What [`place`] did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlaceReport {
    /// The input axes taken as up and front (as given, or as the
    /// smallest turn takes them), if a turn was asked.
    pub up: Option<Axis>,
    /// The front.
    pub front: Option<Axis>,
    /// Where input +X, +Y and +Z went, if anything turned: by the whole
    /// turn when [`Place::turn`] is a multiple of 90, else by the turn
    /// before it.
    pub axes: [Option<Axis>; 3],
    /// The turn about up, degrees.
    pub turn: f64,
    /// The scale factor (1 when kept).
    pub scale: f64,
    /// The translation applied after the turn and the scale, mm.
    pub moved: DVec3,
    /// The input's measures.
    pub before: AxisMeasures,
    /// The output's.
    pub after: AxisMeasures,
}

/// The whole placement: `p ↦ scale · yaw(axes(p)) + t`.
#[derive(Clone, Copy, Debug)]
struct Motion {
    axes: AxisTurn,
    /// cos and sin of a turn about Z that is not a quarter turn.
    yaw: Option<(f64, f64)>,
    scale: f64,
    t: DVec3,
}

impl Motion {
    fn rotate(&self, v: DVec3) -> DVec3 {
        let v = self.axes.apply(v);
        match self.yaw {
            None => v,
            Some((c, s)) => DVec3::new(c * v.x - s * v.y, s * v.x + c * v.y, v.z),
        }
    }

    fn point(&self, p: DVec3) -> DVec3 {
        let mut q = self.rotate(p);
        if self.scale != 1.0 {
            q *= self.scale;
        }
        if self.t != DVec3::ZERO {
            q += self.t;
        }
        q
    }

    /// The 4×4 matrix of the inverse placement, column-major.
    fn inverse_matrix(&self) -> Mat4 {
        // The rotation's columns are its images of the unit axes; its
        // inverse is its transpose. A⁻¹(q) = Rᵀ (q − t) / s.
        let cols = [DVec3::X, DVec3::Y, DVec3::Z].map(|e| self.rotate(e));
        let mut m = [0.0; 16];
        for c in 0..3 {
            for r in 0..3 {
                // Row r of Rᵀ is column r of R.
                m[4 * c + r] = cols[r].to_array()[c] / self.scale;
            }
        }
        for r in 0..3 {
            m[12 + r] = -(0..3)
                .map(|c| m[4 * c + r] * self.t.to_array()[c])
                .sum::<f64>();
        }
        m[15] = 1.0;
        m
    }
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

fn moved(s: &Solid, m: &Motion) -> Solid {
    let mut out = s.clone();
    for p in &mut out.positions {
        *p = m.point(*p);
    }
    if let Some(skin) = &s.skin {
        let inverse = m.inverse_matrix();
        let mut scale = [0.0; 16];
        for k in 0..3 {
            scale[5 * k] = m.scale;
        }
        scale[15] = 1.0;
        out.skin = Some(Skin {
            bones: skin
                .bones
                .iter()
                .map(|b| Bone {
                    name: b.name.clone(),
                    parent: b.parent,
                    rest: Frame {
                        origin: m.point(b.rest.origin),
                        x: m.rotate(b.rest.x),
                        y: m.rotate(b.rest.y),
                        z: m.rotate(b.rest.z),
                    },
                    inverse_bind: mat_mul(&mat_mul(&scale, &b.inverse_bind), &inverse),
                })
                .collect(),
            weights: skin.weights.clone(),
        });
    }
    out
}

/// A solid turned by an axis turn, exactly: its positions, and its skin
/// as [`place`] turns one. Import uses it for a skinned part.
pub(crate) fn turned(s: &Solid, t: &AxisTurn) -> Solid {
    moved(
        s,
        &Motion {
            axes: *t,
            yaw: None,
            scale: 1.0,
            t: DVec3::ZERO,
        },
    )
}

fn bounds(parts: &[Solid], f: impl Fn(DVec3) -> DVec3) -> Option<(DVec3, DVec3)> {
    let mut it = parts.iter().flat_map(|s| s.positions.iter()).map(|&p| f(p));
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
}

/// Places `parts` (one model, in one space) by `d`: every part by the
/// same turn, scale and translation, the box taken over them all. The
/// parts come back in order.
pub fn place(parts: &[Solid], d: &Place) -> Result<(Vec<Solid>, PlaceReport), PlaceError> {
    let axes = AxisTurn::new(d.up, d.front).map_err(PlaceError::Axes)?;
    if !d.turn.is_finite() {
        return Err(PlaceError::Size(format!(
            "turn {} is not a finite angle",
            d.turn
        )));
    }
    // A whole number of quarter turns stays exact.
    let quarters = d.turn / 90.0;
    let (axes_all, yaw) = if quarters == quarters.round() {
        let n = quarters.round().rem_euclid(4.0) as u32;
        (AxisTurn::quarter(n).then(&axes), None)
    } else {
        let (s, c) = trig::sin_cos(trig::to_radians(d.turn));
        (axes, Some((c, s)))
    };
    let mut m = Motion {
        axes: axes_all,
        yaw,
        scale: 1.0,
        t: DVec3::ZERO,
    };
    let (lo, hi) = bounds(parts, |p| m.rotate(p)).ok_or(PlaceError::Empty)?;
    let size = hi - lo;
    let to = |what: &str, target: f64, now: f64| -> Result<f64, PlaceError> {
        if !(target > 0.0 && target.is_finite()) {
            return Err(PlaceError::Size(format!(
                "{what} {target}: give a length above 0"
            )));
        }
        if now <= 0.0 {
            return Err(PlaceError::Size(format!(
                "the mesh has no {what} to scale (0 mm)"
            )));
        }
        Ok(target / now)
    };
    m.scale = match d.size {
        Size::Keep => 1.0,
        Size::Longest(l) => to("longest side", l, size.max_element())?,
        Size::Height(l) => to("height", l, size.z)?,
        Size::Width(l) => to("width", l, size.x)?,
        Size::Depth(l) => to("depth", l, size.y)?,
        Size::Scale(f) => {
            if !(f > 0.0 && f.is_finite()) {
                return Err(PlaceError::Size(format!(
                    "scale {f}: give a factor above 0"
                )));
            }
            f
        }
    };
    let (lo, hi) = (lo * m.scale, hi * m.scale);
    m.t = match d.pivot {
        Pivot::Keep => DVec3::ZERO,
        Pivot::Base => -DVec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, lo.z),
        Pivot::Center => -(lo + hi) / 2.0,
        Pivot::Point(p) => {
            if !p.is_finite() {
                return Err(PlaceError::Size("the pivot point is not finite".into()));
            }
            -(m.rotate(p) * m.scale)
        }
    };
    // Exactly 0 rather than −0 in the report.
    m.t += DVec3::ZERO;
    let out: Vec<Solid> = parts.iter().map(|s| moved(s, &m)).collect();
    let turned = d.up.is_some() || d.front.is_some();
    let report = PlaceReport {
        up: turned.then(|| axes_input(&axes, Axis::PosZ)),
        front: turned.then(|| axes_input(&axes, Axis::NegY)),
        axes: if turned || d.turn != 0.0 {
            [Axis::PosX, Axis::PosY, Axis::PosZ].map(|a| Some(axes_all.image(a)))
        } else {
            [None; 3]
        },
        turn: d.turn,
        scale: m.scale,
        moved: m.t,
        before: axis_measures(parts),
        after: axis_measures(&out),
    };
    Ok((out, report))
}

/// The input axis an axis turn sends to `output`.
fn axes_input(t: &AxisTurn, output: Axis) -> Axis {
    Axis::ALL
        .into_iter()
        .find(|&a| t.image(a) == output)
        .unwrap_or(output)
}

/// The numbers that tell which way a mesh stands, with no verdict
/// (`guides/meshtools.md`): its box, where its origin sits, which way
/// its surface faces, and where its mass sits in the box.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AxisMeasures {
    /// The box's extents along X, Y and Z, mm.
    pub size: DVec3,
    /// The origin less the centre of the box's bottom face (lowest Z), mm
    /// (as `MeshMeasures::pivot_from_base`).
    pub pivot_from_base: DVec3,
    /// The axis the box is longest along: 0, 1 or 2 (the first of equals).
    pub longest: usize,
    /// The axis it is shortest along.
    pub shortest: usize,
    /// The fraction of the surface's area facing each way, in the order
    /// of [`Axis::ALL`] (`+x -x +y -y +z -z`): each triangle counts for
    /// the axis its normal leans along most (the first of equals). They
    /// sum to 1 (0 for a mesh with no area).
    pub facing: [f64; 6],
    /// Where the mass sits in the box, per axis: 0 at the low side, 1 at
    /// the high side, 0.5 in the middle.
    pub mass: DVec3,
    /// Whether [`AxisMeasures::mass`] is the centre of volume (every part
    /// closed, with a volume above 0) or of area.
    pub mass_of_volume: bool,
}

/// [`AxisMeasures`] of `parts` taken together.
pub fn axis_measures(parts: &[Solid]) -> AxisMeasures {
    let mut m = AxisMeasures::default();
    let Some((lo, hi)) = bounds(parts, |p| p) else {
        return m;
    };
    m.size = hi - lo;
    let mid = (lo + hi) / 2.0;
    m.pivot_from_base = DVec3::ZERO - DVec3::new(mid.x, mid.y, lo.z);
    let s = m.size.to_array();
    for k in 1..3 {
        if s[k] > s[m.longest] {
            m.longest = k;
        }
        if s[k] < s[m.shortest] {
            m.shortest = k;
        }
    }
    let (mut area, mut area_moment) = (0.0, DVec3::ZERO);
    let (mut volume, mut volume_moment) = (0.0, DVec3::ZERO);
    let mut closed = true;
    for part in parts {
        let mut part_volume = 0.0;
        let mut part_moment = DVec3::ZERO;
        for t in &part.triangles {
            // About the box's centre, for fewer bits lost.
            let [a, b, c] = t.map(|v| part.positions[v as usize] - mid);
            let n = (b - a).cross(c - a);
            let twice = n.length();
            area += twice / 2.0;
            area_moment += (a + b + c) * (twice / 6.0);
            let w = n.abs().to_array();
            let k = if w[0] >= w[1] && w[0] >= w[2] {
                0
            } else if w[1] >= w[2] {
                1
            } else {
                2
            };
            let neg = n.to_array()[k] < 0.0;
            m.facing[2 * k + usize::from(neg)] += twice / 2.0;
            let v = a.dot(b.cross(c)) / 6.0;
            part_volume += v;
            part_moment += (a + b + c) * (v / 4.0);
        }
        closed &= part_volume > 0.0 && is_closed(part);
        volume += part_volume;
        volume_moment += part_moment;
    }
    if area > 0.0 {
        for f in &mut m.facing {
            *f /= area;
        }
    }
    let centre = if closed && volume > 0.0 {
        m.mass_of_volume = true;
        volume_moment / volume
    } else if area > 0.0 {
        area_moment / area
    } else {
        DVec3::ZERO
    } + mid;
    let rel = |k: usize| {
        let (l, h, c) = (lo.to_array()[k], hi.to_array()[k], centre.to_array()[k]);
        if h > l { (c - l) / (h - l) } else { 0.5 }
    };
    m.mass = DVec3::new(rel(0), rel(1), rel(2));
    m
}

/// Every edge (positions welded exactly) has two triangles running it
/// opposite ways.
fn is_closed(s: &Solid) -> bool {
    let (_, tris) = weld_exact(s);
    if tris.is_empty() {
        return false;
    }
    let mut edges: BTreeMap<(u32, u32), (u32, u32)> = BTreeMap::new();
    for t in &tris {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            if a == b {
                continue;
            }
            let e = edges.entry(edge_key(a, b)).or_default();
            if a < b {
                e.0 += 1;
            } else {
                e.1 += 1;
            }
        }
    }
    edges.values().all(|&n| n == (1, 1))
}

fn vec3(v: DVec3) -> Json {
    Json::Arr(v.to_array().map(num).to_vec())
}

fn num(v: f64) -> Json {
    if v.is_finite() {
        Json::Num(v)
    } else {
        Json::Null
    }
}

fn axis_name(k: usize) -> Json {
    Json::Str(["x", "y", "z"][k].into())
}

impl AxisMeasures {
    pub(crate) fn json(&self) -> Json {
        Json::Obj(vec![
            ("size", vec3(self.size)),
            ("pivot_from_base", vec3(self.pivot_from_base)),
            ("longest", axis_name(self.longest)),
            ("shortest", axis_name(self.shortest)),
            (
                "facing",
                Json::Obj(
                    Axis::ALL
                        .iter()
                        .zip(self.facing)
                        .map(|(a, f)| (a.name(), num(f)))
                        .collect(),
                ),
            ),
            (
                "mass",
                Json::Obj(vec![
                    (
                        "of",
                        Json::Str(
                            if self.mass_of_volume {
                                "volume"
                            } else {
                                "area"
                            }
                            .into(),
                        ),
                    ),
                    ("in_box", vec3(self.mass)),
                ]),
            ),
        ])
    }

    /// The measures as one line of JSON, lengths in mm: `size`,
    /// `pivot_from_base`, `longest`, `shortest`, `facing` (`+x` … `-z`)
    /// and `mass` (`of`: `volume` or `area`; `in_box`: 0 to 1 per axis).
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.json().write(&mut out);
        out
    }
}

impl PlaceReport {
    /// The report as one line of JSON, lengths in mm: `up`, `front` and
    /// `axes` (where input `+x`, `+y`, `+z` went; null without a turn),
    /// `turn` (degrees), `scale`, `moved`, and the axis measures `before`
    /// and `after`.
    pub fn to_json(&self) -> String {
        let axis = |a: Option<Axis>| a.map_or(Json::Null, |a| Json::Str(a.name().into()));
        let axes = if self.axes.iter().all(Option::is_some) {
            Json::Obj(vec![
                ("+x", axis(self.axes[0])),
                ("+y", axis(self.axes[1])),
                ("+z", axis(self.axes[2])),
            ])
        } else {
            Json::Null
        };
        let doc = Json::Obj(vec![
            ("units", Json::Str("mm".into())),
            ("up", axis(self.up)),
            ("front", axis(self.front)),
            ("axes", axes),
            ("turn", num(self.turn)),
            ("scale", num(self.scale)),
            ("moved", vec3(self.moved)),
            ("before", self.before.json()),
            ("after", self.after.json()),
        ]);
        let mut out = String::new();
        doc.write(&mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shape_gen::solids::{BoxDesc, cuboid};

    fn block() -> Solid {
        // 10 × 20 × 40 mm, low corner at (1, 2, 3).
        let mut s = cuboid(&BoxDesc {
            width: 10.0,
            depth: 20.0,
            height: 40.0,
        })
        .unwrap();
        let lo = s.positions.iter().fold(DVec3::INFINITY, |a, &p| a.min(p));
        s.map_positions(|p| p - lo + DVec3::new(1.0, 2.0, 3.0), false);
        s
    }

    #[test]
    fn the_24_turns_are_proper_and_send_up_and_front_home() {
        let mut seen = Vec::new();
        for up in Axis::ALL {
            for front in Axis::ALL {
                let Ok(t) = AxisTurn::new(Some(up), Some(front)) else {
                    assert_eq!(up.index(), front.index());
                    continue;
                };
                assert_eq!(t.image(up), Axis::PosZ);
                assert_eq!(t.image(front), Axis::NegY);
                let [x, y, z] = [DVec3::X, DVec3::Y, DVec3::Z].map(|e| t.apply(e));
                assert_eq!(x.dot(y.cross(z)), 1.0, "{up} {front}");
                seen.push(t);
            }
        }
        seen.sort_by_key(|t| (t.from, t.neg));
        seen.dedup();
        assert_eq!(seen.len(), 24);
        // One axis alone: the smallest turn.
        let t = AxisTurn::new(Some(Axis::PosY), None).unwrap();
        assert_eq!(t.image(Axis::PosX), Axis::PosX);
        let t = AxisTurn::new(Some(Axis::PosZ), None).unwrap();
        assert_eq!(t, AxisTurn::IDENTITY);
    }

    #[test]
    fn base_pivot_and_height_stand_a_lying_block_up() {
        // Lying with its up along +y: stand it up, 100 mm tall, on its base.
        let (out, r) = place(
            &[block()],
            &Place {
                up: Some(Axis::PosY),
                size: Size::Height(100.0),
                pivot: Pivot::Base,
                ..Default::default()
            },
        )
        .unwrap();
        let (lo, hi) = bounds(&out, |p| p).unwrap();
        assert_eq!(hi.z - lo.z, 100.0);
        assert_eq!(lo.z, 0.0);
        assert_eq!(lo.x + hi.x, 0.0);
        assert_eq!(lo.y + hi.y, 0.0);
        assert_eq!(r.scale, 5.0);
        assert_eq!(r.after.pivot_from_base, DVec3::ZERO);
        // The block's 20 mm (y) became its height, 40 mm (z) its depth.
        assert_eq!(r.before.size, DVec3::new(10.0, 20.0, 40.0));
        assert_eq!(r.after.size, DVec3::new(50.0, 200.0, 100.0));
        assert!(r.before.mass_of_volume);
        assert_eq!(r.after.mass, DVec3::splat(0.5));
    }

    #[test]
    fn quarter_turns_are_exact_and_other_angles_turn() {
        let s = block();
        let (a, _) = place(
            std::slice::from_ref(&s),
            &Place {
                turn: 450.0,
                ..Default::default()
            },
        )
        .unwrap();
        for (p, q) in s.positions.iter().zip(&a[0].positions) {
            assert_eq!(*q, DVec3::new(-p.y, p.x, p.z));
        }
        let (b, _) = place(
            &[s],
            &Place {
                turn: 30.0,
                ..Default::default()
            },
        )
        .unwrap();
        let p = b[0].positions[0];
        assert!(p.is_finite());
    }

    fn skinned_block() -> Solid {
        let mut s = block();
        let rest = Frame {
            origin: DVec3::new(5.0, 10.0, 20.0),
            x: DVec3::X,
            y: DVec3::Y,
            z: DVec3::Z,
        };
        s.skin = Some(Skin {
            bones: vec![Bone::new("root", None, rest)],
            weights: vec![shape_mesh::Weights::single(0); s.positions.len()],
        });
        s
    }

    #[test]
    fn a_skin_turns_scales_and_moves_with_its_mesh() {
        let s = skinned_block();
        let (out, _) = place(
            std::slice::from_ref(&s),
            &Place {
                up: Some(Axis::NegX),
                front: Some(Axis::PosZ),
                size: Size::Scale(2.0),
                pivot: Pivot::Base,
                ..Default::default()
            },
        )
        .unwrap();
        let skin = out[0].skin.as_ref().unwrap();
        skin.check(out[0].positions.len()).unwrap();
        let b = &skin.bones[0];
        // rest · IBM is still the identity, and every vertex in bone
        // coordinates is where it was, scaled.
        let id = mat_mul(&shape_mesh::skin::frame_matrix(&b.rest), &b.inverse_bind);
        for (k, v) in id.iter().enumerate() {
            let want = if k % 5 == 0 { 1.0 } else { 0.0 };
            assert!((v - want).abs() < 1e-12, "{id:?}");
        }
        let local = |m: &Mat4, p: DVec3| {
            DVec3::new(
                m[0] * p.x + m[4] * p.y + m[8] * p.z + m[12],
                m[1] * p.x + m[5] * p.y + m[9] * p.z + m[13],
                m[2] * p.x + m[6] * p.y + m[10] * p.z + m[14],
            )
        };
        let old = &s.skin.as_ref().unwrap().bones[0].inverse_bind;
        for (p, q) in s.positions.iter().zip(&out[0].positions) {
            let d = local(&b.inverse_bind, *q) - local(old, *p) * 2.0;
            assert!(d.length() < 1e-9);
        }
    }

    #[test]
    fn facing_and_mass_are_measured() {
        let m = axis_measures(&[block()]);
        // Faces 20×40 (±x), 10×40 (±y), 10×20 (±z) of 2800 mm².
        let f = m.facing;
        assert!((f[0] - 800.0 / 2800.0).abs() < 1e-12 && f[0] == f[1]);
        assert!((f[4] - 200.0 / 2800.0).abs() < 1e-12);
        assert_eq!((m.longest, m.shortest), (2, 0));
        assert_eq!(m.pivot_from_base, -DVec3::new(6.0, 12.0, 3.0));
        // Open (its top dropped): the centre of area, lower down.
        let mut open = block();
        let top = (0..open.triangles.len())
            .filter(|&i| {
                open.triangles[i]
                    .iter()
                    .all(|&v| open.positions[v as usize].z == 43.0)
            })
            .collect::<Vec<_>>();
        for i in top.into_iter().rev() {
            open.triangles.remove(i);
            open.surfaces.remove(i);
            open.face_tags.remove(i);
        }
        let m = axis_measures(&[open]);
        assert!(!m.mass_of_volume);
        assert!(m.mass.z < 0.5);
        assert_eq!(m.facing[4], 0.0);
    }

    #[test]
    fn import_reads_the_files_own_axes() {
        use crate::{Import, import, no_files};
        let obj = "v 1 2 3\nv 4 5 6\nv 7 8 10\nf 1 2 3\n";
        let read = |d: Import| {
            import(obj.as_bytes(), &d, &no_files).map(|m| m.parts[0].solid.positions.clone())
        };
        let z_up = read(Import {
            z_up: true,
            ..Default::default()
        })
        .unwrap();
        let named = read(Import {
            up: Some(Axis::PosZ),
            front: Some(Axis::NegY),
            ..Default::default()
        });
        assert_eq!(named.unwrap(), z_up);
        // Up +y is the default; front +z follows.
        let y_up = read(Import {
            up: Some(Axis::PosY),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(y_up, read(Import::default()).unwrap());
        // A frame neither reads: up along -x, facing +y.
        let other = read(Import {
            up: Some(Axis::NegX),
            front: Some(Axis::PosY),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(other[0], DVec3::new(-3000.0, -2000.0, -1000.0));
        assert!(
            read(Import {
                z_up: true,
                up: Some(Axis::PosZ),
                ..Default::default()
            })
            .is_err()
        );
        // glTF: a skinned file read Z up keeps a skin that checks, its
        // mesh and bones turned from the Y-up reading.
        let glb = crate::glb(&crate::Scene {
            name: "legs".into(),
            parts: vec![crate::Part::new("legs", &skinned_block())],
        })
        .unwrap();
        let plain = import(&glb, &Import::default(), &no_files).unwrap();
        let z_up = Import {
            up: Some(Axis::PosZ),
            ..Default::default()
        };
        let turned = import(&glb, &z_up, &no_files).unwrap();
        let (a, b) = (&plain.parts[0].solid, &turned.parts[0].solid);
        b.skin.as_ref().unwrap().check(b.positions.len()).unwrap();
        let c = AxisTurn::IDENTITY.then(&AxisTurn::new(Some(Axis::PosY), None).unwrap().inverse());
        for (p, q) in a.positions.iter().zip(&b.positions) {
            assert_eq!(c.apply(*p), *q);
        }
        let (ra, rb) = (
            &a.skin.as_ref().unwrap().bones[0].rest,
            &b.skin.as_ref().unwrap().bones[0].rest,
        );
        assert_eq!(c.apply(ra.origin), rb.origin);
        assert_eq!(c.apply(ra.z), rb.z);
    }

    #[test]
    fn errors_are_values() {
        let s = [block()];
        let bad = |d: Place| place(&s, &d).unwrap_err();
        assert!(matches!(
            bad(Place {
                up: Some(Axis::PosZ),
                front: Some(Axis::NegZ),
                ..Default::default()
            }),
            PlaceError::Axes(_)
        ));
        assert!(matches!(
            bad(Place {
                size: Size::Height(-1.0),
                ..Default::default()
            }),
            PlaceError::Size(_)
        ));
        assert_eq!(
            place(&[Solid::default()], &Place::default()).unwrap_err(),
            PlaceError::Empty
        );
    }
}
