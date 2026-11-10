//! Named generators: solids built from [`shape_2d::Profile`]s, with
//! sand3d's names on caps, walls and edges (`docs/design.md` §4).
//!
//! | generator | faces | edges |
//! |---|---|---|
//! | [`extrude`] | `bottom`, `top`, `side` (`inner` on hole walls too) | `bottom`, `top`, `side` (vertical, at corners) |
//! | [`revolve`] | `side`; `start`, `end` when not a full turn | `rim` (at corners), `start`, `end` |
//! | primitives | as the generator they are made with | |
//!
//! Turned solids are about +y by default; [`Axis::Z`] turns them about +z
//! instead, and [`sphere_desc`], [`torus_desc`] and [`dome_desc`] then rest
//! on z = 0 (sand3d's). More generators: [`crate::loft`], [`crate::body`],
//! [`crate::wire`], [`crate::parts`] (gear solid, knurl).
//!
//! Every generator returns a canonical [`Solid`] (`Solid::canonicalize`).

use shape_2d::curve::segments;
use shape_2d::{Circle, Contour, Profile, ProfileError, Rect, circle, rect};
use shape_core::{DVec2, DVec3, trig};
use shape_mesh::Solid;

/// Why a generator could not build its solid.
#[derive(Clone, Debug, PartialEq)]
pub enum GenError {
    /// The profile was invalid.
    Profile(ProfileError),
    /// A size that must be positive was not (the field named).
    NotPositive(&'static str),
    /// A value was NaN or infinite (the field named).
    NotFinite(&'static str),
    /// A revolved profile crosses the axis (x < 0 somewhere).
    CrossesAxis,
    /// The inputs cannot make a solid; the message says where and why.
    Invalid(String),
}

impl From<ProfileError> for GenError {
    fn from(e: ProfileError) -> Self {
        GenError::Profile(e)
    }
}

impl std::fmt::Display for GenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenError::Profile(e) => write!(f, "profile: {e}"),
            GenError::NotPositive(n) => write!(f, "{n} must be positive"),
            GenError::NotFinite(n) => write!(f, "{n} must be finite"),
            GenError::CrossesAxis => f.write_str("a revolved profile must lie at x >= 0"),
            GenError::Invalid(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for GenError {}

pub(crate) fn positive(v: f64, name: &'static str) -> Result<f64, GenError> {
    if !v.is_finite() {
        Err(GenError::NotFinite(name))
    } else if v <= 0.0 {
        Err(GenError::NotPositive(name))
    } else {
        Ok(v)
    }
}

pub(crate) fn finite(v: f64, name: &'static str) -> Result<f64, GenError> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(GenError::NotFinite(name))
    }
}

/// Most segments a turn a `segments` field takes.
pub const MAX_SEGMENTS: u32 = 1024;

/// Checks a `segments` field: 0 (the chord rule) or 3 to [`MAX_SEGMENTS`].
pub(crate) fn segments_field(n: u32, name: &'static str) -> Result<u32, GenError> {
    if n == 0 || (3..=MAX_SEGMENTS).contains(&n) {
        Ok(n)
    } else {
        Err(GenError::Invalid(format!(
            "{name} is {n}: 0 for the chord rule, or 3 to {MAX_SEGMENTS} a turn"
        )))
    }
}

/// Segments for an arc of `radius` sweeping `angle`: the chord rule with
/// `per_turn` 0, else `per_turn`'s share of a turn (at least 1).
pub(crate) fn count(per_turn: u32, radius: f64, angle: f64) -> u32 {
    if per_turn == 0 {
        segments(radius, angle)
    } else {
        let fraction = (angle.abs() / trig::TAU).min(1.0);
        ((per_turn as f64 * fraction).ceil() as u32).max(1)
    }
}

/// Faces out, then canonical.
pub(crate) fn finish(mut s: Solid) -> Result<Solid, GenError> {
    if s.measure().volume < 0.0 {
        for t in &mut s.triangles {
            t.swap(1, 2);
        }
    }
    s.canonicalize()
        .map_err(|_| GenError::NotFinite("positions"))?;
    Ok(s)
}

/// The runs of a contour between its corners: `(first, last)` point
/// indices, `last` possibly wrapping past the end. A contour with no
/// corner is one run that closes on itself.
pub(crate) fn runs(c: &Contour) -> Vec<(usize, usize)> {
    let n = c.points.len();
    let corners: Vec<usize> = (0..n).filter(|&i| c.sharp[i]).collect();
    if corners.is_empty() {
        return vec![(0, n)];
    }
    (0..corners.len())
        .map(|k| {
            let a = corners[k];
            let b = corners[(k + 1) % corners.len()];
            (a, if b > a { b } else { b + n })
        })
        .collect()
}

/// Extrusion along +z.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Extrude {
    /// Height along +z.
    pub height: f64,
    /// Turn of the top relative to the bottom, radians, about z.
    pub twist: f64,
    /// Shrink of the top: its scale is `1 - taper` (0 keeps it, 1 makes
    /// an apex, negative widens it).
    pub taper: f64,
    /// Centre the solid on z = 0 instead of starting there.
    pub both_sides: bool,
}

/// A profile extruded along +z: `bottom` at z = 0, `top` at `height`.
pub fn extrude(profile: &Profile, d: &Extrude) -> Result<Solid, GenError> {
    let h = positive(d.height, "height")?;
    let twist = finite(d.twist, "twist")?;
    let taper = finite(d.taper, "taper")?;
    if taper > 1.0 {
        return Err(GenError::NotPositive("1 - taper"));
    }
    if profile.regions.is_empty() {
        return Err(ProfileError::Empty.into());
    }
    let reach = profile
        .contours()
        .flat_map(|c| &c.points)
        .fold(0.0f64, |m, p| m.max(p.length()));
    let layers = if twist == 0.0 {
        1
    } else {
        segments(reach, twist)
    };
    let z0 = if d.both_sides { -h / 2.0 } else { 0.0 };
    let at = |p: DVec2, layer: u32| {
        let t = layer as f64 / layers as f64;
        let (s, c) = trig::sin_cos(twist * t);
        let k = 1.0 - taper * t;
        DVec3::new((c * p.x - s * p.y) * k, (s * p.x + c * p.y) * k, z0 + h * t)
    };
    let mut s = Solid::default();
    // Caps.
    let fill = profile.triangulate()?;
    let bottom: Vec<u32> = fill
        .points
        .iter()
        .map(|p| s.push_vertex(at(*p, 0)))
        .collect();
    let top: Vec<u32> = fill
        .points
        .iter()
        .map(|p| s.push_vertex(at(*p, layers)))
        .collect();
    for t in &fill.triangles {
        s.push_triangle(
            [
                bottom[t[0] as usize],
                bottom[t[2] as usize],
                bottom[t[1] as usize],
            ],
            0,
            &["bottom"],
        );
        s.push_triangle(
            [top[t[0] as usize], top[t[1] as usize], top[t[2] as usize]],
            1,
            &["top"],
        );
    }
    // Walls, one surface per run between corners.
    let mut surface = 2;
    for region in &profile.regions {
        for (ci, c) in std::iter::once(&region.outer)
            .chain(&region.holes)
            .enumerate()
        {
            let tags: &[&str] = if ci == 0 {
                &["side"]
            } else {
                &["side", "inner"]
            };
            let n = c.points.len();
            let ring: Vec<Vec<u32>> = (0..=layers)
                .map(|l| c.points.iter().map(|p| s.push_vertex(at(*p, l))).collect())
                .collect();
            for (a, b) in runs(c) {
                for i in a..b {
                    let (i0, i1) = (i % n, (i + 1) % n);
                    for l in 0..layers as usize {
                        let (p, q) = (&ring[l], &ring[l + 1]);
                        s.push_triangle([p[i0], p[i1], q[i1]], surface, tags);
                        s.push_triangle([p[i0], q[i1], q[i0]], surface, tags);
                    }
                }
                surface += 1;
            }
            for i in 0..n {
                s.tag_edge(ring[0][i], ring[0][(i + 1) % n], &["bottom"]);
                s.tag_edge(
                    ring[layers as usize][i],
                    ring[layers as usize][(i + 1) % n],
                    &["top"],
                );
                if c.sharp[i] {
                    for l in 0..layers as usize {
                        s.tag_edge(ring[l][i], ring[l + 1][i], &["side"]);
                    }
                }
            }
        }
    }
    finish(s)
}

/// The axis a turned solid turns about.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    /// +y, three.js's lathe axis: the profile's y stays y.
    #[default]
    Y,
    /// +z, sand3d's: the profile's y becomes z, and the round primitives
    /// rest on z = 0.
    Z,
}

/// A solid made about +y, turned a quarter about +x so +y goes to +z:
/// `(x, y, z) → (x, −z, y)`, exact.
pub(crate) fn y_to_z(s: &mut Solid, lift: f64) -> Result<(), GenError> {
    s.map_positions(|p| DVec3::new(p.x, -p.z, p.y + lift), false);
    s.canonicalize()
        .map_err(|_| GenError::NotFinite("positions"))
}

/// Revolution about +y (or +z).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Revolve {
    /// Angle swept, radians; 0 for a full turn.
    pub angle: f64,
    /// The axis; +y by default.
    pub axis: Axis,
    /// Segments a full turn (a part turn gets its share); 0 for the chord
    /// rule ([`shape_2d::curve::segments`]).
    pub segments: u32,
}

/// A profile in the xy plane (x ≥ 0, x the radius) turned about +y,
/// counter-clockwise seen from +y, starting at +x. A partial turn has
/// `start` and `end` caps. With [`Axis::Z`] the profile's y is the
/// height along +z and the turn is counter-clockwise seen from +z, from
/// +x (sand3d's `revolve`).
pub fn revolve(profile: &Profile, d: &Revolve) -> Result<Solid, GenError> {
    let angle = finite(d.angle, "angle")?;
    let per_turn = segments_field(d.segments, "segments")?;
    let full = angle == 0.0 || angle.abs() >= trig::TAU;
    let sweep = if full { trig::TAU } else { angle };
    if profile.regions.is_empty() {
        return Err(ProfileError::Empty.into());
    }
    if profile
        .contours()
        .flat_map(|c| &c.points)
        .any(|p| p.x < 0.0)
    {
        return Err(GenError::CrossesAxis);
    }
    let reach = profile
        .contours()
        .flat_map(|c| &c.points)
        .fold(0.0f64, |m, p| m.max(p.x));
    let steps = count(per_turn, reach, sweep);
    let at = |p: DVec2, k: u32| {
        // The last column of a full turn is the first, exactly.
        let k = if full && k == steps { 0 } else { k };
        let (s, c) = trig::sin_cos(sweep * (k as f64 / steps as f64));
        DVec3::new(p.x * c, p.y, -p.x * s)
    };
    let mut s = Solid::default();
    let mut surface = 0;
    for c in profile.contours() {
        let n = c.points.len();
        let cols: Vec<Vec<u32>> = (0..=steps)
            .map(|k| c.points.iter().map(|p| s.push_vertex(at(*p, k))).collect())
            .collect();
        for (a, b) in runs(c) {
            for i in a..b {
                let (i0, i1) = (i % n, (i + 1) % n);
                for k in 0..steps as usize {
                    let (p, q) = (&cols[k], &cols[k + 1]);
                    s.push_triangle([p[i0], q[i0], q[i1]], surface, &["side"]);
                    s.push_triangle([p[i0], q[i1], p[i1]], surface, &["side"]);
                }
            }
            surface += 1;
        }
        for i in 0..n {
            if c.sharp[i] {
                for k in 0..steps as usize {
                    s.tag_edge(cols[k][i], cols[k + 1][i], &["rim"]);
                }
            }
            if !full {
                s.tag_edge(cols[0][i], cols[0][(i + 1) % n], &["start"]);
                s.tag_edge(
                    cols[steps as usize][i],
                    cols[steps as usize][(i + 1) % n],
                    &["end"],
                );
            }
        }
    }
    if !full {
        let fill = profile.triangulate()?;
        let first: Vec<u32> = fill
            .points
            .iter()
            .map(|p| s.push_vertex(at(*p, 0)))
            .collect();
        let last: Vec<u32> = fill
            .points
            .iter()
            .map(|p| s.push_vertex(at(*p, steps)))
            .collect();
        for t in &fill.triangles {
            s.push_triangle(t.map(|i| first[i as usize]), surface, &["start"]);
            s.push_triangle(
                [
                    last[t[0] as usize],
                    last[t[2] as usize],
                    last[t[1] as usize],
                ],
                surface + 1,
                &["end"],
            );
        }
    }
    let mut s = finish(s)?;
    if d.axis == Axis::Z {
        y_to_z(&mut s, 0.0)?;
    }
    Ok(s)
}

// ── primitives ──────────────────────────────────────────────────────────────

/// A box.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoxDesc {
    /// Size along x.
    pub width: f64,
    /// Size along y.
    pub depth: f64,
    /// Size along z.
    pub height: f64,
}

/// A box centred on the origin in x and y, from z = 0 up. Its walls are
/// also named `right` (+x), `left` (−x), `back` (+y) and `front` (−y).
pub fn cuboid(d: &BoxDesc) -> Result<Solid, GenError> {
    let p = rect(&Rect {
        width: d.width,
        height: d.depth,
        radius: 0.0,
    })?;
    let mut s = extrude(
        &p,
        &Extrude {
            height: d.height,
            ..Default::default()
        },
    )?;
    let (w, dd) = (d.width / 2.0, d.depth / 2.0);
    let names: Vec<(u32, &str)> = s
        .triangles
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let [a, b, c] = t.map(|v| s.positions[v as usize]);
            let side = |f: &dyn Fn(DVec3) -> bool| f(a) && f(b) && f(c);
            let name = if side(&|p| p.x == w) {
                "right"
            } else if side(&|p| p.x == -w) {
                "left"
            } else if side(&|p| p.y == dd) {
                "back"
            } else if side(&|p| p.y == -dd) {
                "front"
            } else {
                return None;
            };
            Some((i as u32, name))
        })
        .collect();
    for (i, n) in names {
        let id = s.names.intern(n);
        let tags = &mut s.face_tags[i as usize];
        tags.push(id);
        tags.sort_unstable();
    }
    s.canonicalize()
        .map_err(|_| GenError::NotFinite("positions"))?;
    Ok(s)
}

/// A cylinder or cone.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CylinderDesc {
    /// Bottom radius.
    pub radius: f64,
    /// Top radius; 0 keeps `radius` (a cylinder). Use [`cone`] for an apex.
    pub radius_top: f64,
    /// Height along +z.
    pub height: f64,
    /// Segments round; 0 for the chord rule.
    pub segments: u32,
}

/// A cylinder (or truncated cone) on z = 0 about the z axis.
pub fn cylinder(d: &CylinderDesc) -> Result<Solid, GenError> {
    let r = positive(d.radius, "radius")?;
    let top = if d.radius_top == 0.0 {
        r
    } else {
        positive(d.radius_top, "radius_top")?
    };
    let base = circle(&Circle {
        radius: r,
        segments: count(
            segments_field(d.segments, "segments")?,
            r.max(top),
            trig::TAU,
        ),
    })?;
    extrude(
        &base,
        &Extrude {
            height: d.height,
            taper: 1.0 - top / r,
            ..Default::default()
        },
    )
}

/// A cone with its apex on the z axis at `height`.
pub fn cone(radius: f64, height: f64) -> Result<Solid, GenError> {
    cone_desc(&ConeDesc {
        radius,
        height,
        segments: 0,
    })
}

/// A cone.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ConeDesc {
    /// Base radius, on z = 0.
    pub radius: f64,
    /// Height of the apex along +z.
    pub height: f64,
    /// Segments round; 0 for the chord rule.
    pub segments: u32,
}

/// A cone from its desc (as [`cone`]).
pub fn cone_desc(d: &ConeDesc) -> Result<Solid, GenError> {
    let (radius, height) = (d.radius, d.height);
    let p = circle(&Circle {
        radius: positive(radius, "radius")?,
        segments: segments_field(d.segments, "segments")?,
    })?;
    extrude(
        &p,
        &Extrude {
            height,
            taper: 1.0,
            ..Default::default()
        },
    )
}

/// A tube: a cylinder with a coaxial bore (walls `side` and `side`+`inner`).
pub fn tube(outer: f64, inner: f64, height: f64) -> Result<Solid, GenError> {
    tube_desc(&TubeDesc {
        outer,
        inner,
        height,
        segments: 0,
    })
}

/// A tube.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TubeDesc {
    /// Outer radius.
    pub outer: f64,
    /// Bore radius, below `outer`.
    pub inner: f64,
    /// Height along +z.
    pub height: f64,
    /// Segments round (both walls); 0 for the chord rule.
    pub segments: u32,
}

/// A tube from its desc (as [`tube`]).
pub fn tube_desc(d: &TubeDesc) -> Result<Solid, GenError> {
    let (outer, inner, height) = (d.outer, d.inner, d.height);
    let i = positive(inner, "inner")?;
    if i >= positive(outer, "outer")? {
        return Err(GenError::NotPositive("outer - inner"));
    }
    let n = count(segments_field(d.segments, "segments")?, outer, trig::TAU);
    let p = circle(&Circle {
        radius: outer,
        segments: n,
    })?
    .subtract(&circle(&Circle {
        radius: i,
        segments: n,
    })?);
    extrude(
        &p,
        &Extrude {
            height,
            ..Default::default()
        },
    )
}

/// A sphere centred on the origin.
pub fn sphere(radius: f64) -> Result<Solid, GenError> {
    sphere_n(radius, 0)
}

/// A sphere with `per_turn` segments round (0: the chord rule) and half
/// as many from pole to pole.
fn sphere_n(radius: f64, per_turn: u32) -> Result<Solid, GenError> {
    let r = positive(radius, "radius")?;
    let n = count(per_turn, r, trig::PI).max(2);
    let mut half: Vec<DVec2> = (0..=n)
        .map(|k| {
            let (s, c) = trig::sin_cos(-trig::PI / 2.0 + trig::PI * (k as f64 / n as f64));
            DVec2::new(r * c, r * s)
        })
        .collect();
    half[0].x = 0.0;
    half[n as usize].x = 0.0;
    let p = Profile::from_contour(Contour::smooth(half));
    revolve(
        &p,
        &Revolve {
            segments: per_turn,
            ..Default::default()
        },
    )
}

/// A dome: the upper half of a sphere, flat `bottom` on y = 0.
pub fn dome(radius: f64) -> Result<Solid, GenError> {
    dome_n(radius, 0)
}

fn dome_n(radius: f64, per_turn: u32) -> Result<Solid, GenError> {
    let r = positive(radius, "radius")?;
    let n = count(per_turn, r, trig::PI / 2.0);
    let mut pts: Vec<DVec2> = (0..=n)
        .map(|k| {
            let (s, c) = trig::sin_cos(trig::PI / 2.0 * (k as f64 / n as f64));
            DVec2::new(r * c, r * s)
        })
        .collect();
    pts[n as usize].x = 0.0;
    pts.push(DVec2::ZERO);
    let mut sharp = vec![false; pts.len()];
    sharp[0] = true;
    sharp[n as usize] = true;
    sharp[n as usize + 1] = true;
    let mut s = revolve(
        &Profile::from_contour(Contour { points: pts, sharp }),
        &Revolve {
            segments: per_turn,
            ..Default::default()
        },
    )?;
    // The flat disc on y = 0 is the dome's bottom.
    let bottom = s.names.intern("bottom");
    for (i, t) in s.triangles.iter().enumerate() {
        if t.iter().all(|&v| s.positions[v as usize].y == 0.0) {
            s.face_tags[i] = vec![bottom];
        }
    }
    s.canonicalize()
        .map_err(|_| GenError::NotFinite("positions"))?;
    Ok(s)
}

/// A torus about the y axis.
pub fn torus(radius: f64, tube_radius: f64) -> Result<Solid, GenError> {
    torus_y(&TorusDesc {
        radius,
        tube_radius,
        ..Default::default()
    })
}

/// A torus about the y axis from its desc, ignoring its `axis`.
fn torus_y(d: &TorusDesc) -> Result<Solid, GenError> {
    let (r, t) = (
        positive(d.radius, "radius")?,
        positive(d.tube_radius, "tube_radius")?,
    );
    if t >= r {
        return Err(GenError::NotPositive("radius - tube_radius"));
    }
    let p = circle(&Circle {
        radius: t,
        segments: segments_field(d.tube_segments, "tube_segments")?,
    })?
    .translate(DVec2::new(r, 0.0));
    revolve(
        &p,
        &Revolve {
            angle: d.angle,
            segments: d.segments,
            ..Default::default()
        },
    )
}

/// A sphere.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SphereDesc {
    /// Radius.
    pub radius: f64,
    /// [`Axis::Y`]: centred on the origin, poles on y (as [`sphere`]).
    /// [`Axis::Z`]: poles on z, resting on z = 0 (sand3d's).
    pub axis: Axis,
    /// Segments round the poles' axis, and half as many from pole to
    /// pole; 0 for the chord rule.
    pub segments: u32,
}

/// A sphere from its desc.
pub fn sphere_desc(d: &SphereDesc) -> Result<Solid, GenError> {
    let mut s = sphere_n(d.radius, segments_field(d.segments, "segments")?)?;
    if d.axis == Axis::Z {
        y_to_z(&mut s, d.radius)?;
    }
    Ok(s)
}

/// A torus.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TorusDesc {
    /// Radius of the tube's centre line.
    pub radius: f64,
    /// Radius of the tube.
    pub tube_radius: f64,
    /// [`Axis::Y`]: about y, centred on the origin (as [`torus`]).
    /// [`Axis::Z`]: about z, resting on z = 0 (sand3d's).
    pub axis: Axis,
    /// The part of a turn the tube runs, radians, from +x as [`revolve`]
    /// turns; 0 for a whole ring. A part ring has flat `start` and `end`
    /// caps.
    pub angle: f64,
    /// Segments round the ring's axis (a part ring gets its share); 0 for
    /// the chord rule.
    pub segments: u32,
    /// Segments round the tube; 0 for the chord rule.
    pub tube_segments: u32,
}

/// A torus from its desc.
pub fn torus_desc(d: &TorusDesc) -> Result<Solid, GenError> {
    let mut s = torus_y(d)?;
    if d.axis == Axis::Z {
        y_to_z(&mut s, d.tube_radius)?;
    }
    Ok(s)
}

/// A dome: a spherical cap on a flat `bottom`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DomeDesc {
    /// Radius of the base.
    pub radius: f64,
    /// Height of the cap; 0 for a half sphere (`radius` high). Above
    /// `radius`, more than half a sphere.
    pub height: f64,
    /// [`Axis::Y`]: base on y = 0, rising up y (as [`dome`]).
    /// [`Axis::Z`]: base on z = 0, rising up z (sand3d's).
    pub axis: Axis,
    /// Segments round, and a quarter as many from the rim of a half
    /// sphere to its top (a cap its share); 0 for the chord rule.
    pub segments: u32,
}

/// A dome from its desc: sand3d's `dome(d, h)` with `radius = d / 2`.
pub fn dome_desc(d: &DomeDesc) -> Result<Solid, GenError> {
    let per_turn = segments_field(d.segments, "segments")?;
    let mut s = if d.height == 0.0 {
        dome_n(d.radius, per_turn)?
    } else {
        cap(
            positive(d.radius, "radius")?,
            positive(d.height, "height")?,
            per_turn,
        )?
    };
    if d.axis == Axis::Z {
        y_to_z(&mut s, 0.0)?;
    }
    Ok(s)
}

/// A spherical cap `a` in base radius and `h` high, about +y on y = 0.
fn cap(a: f64, h: f64, per_turn: u32) -> Result<Solid, GenError> {
    let r = (a * a + h * h) / (2.0 * h);
    let from = trig::atan2(r - h, a);
    let sweep = trig::PI / 2.0 - from;
    let n = count(per_turn, r, sweep);
    let mut pts = vec![DVec2::ZERO];
    pts.extend((0..=n).map(|k| {
        let (s, c) = trig::sin_cos(from + sweep * (k as f64 / n as f64));
        DVec2::new(r * c, h - r + r * s)
    }));
    pts[1] = DVec2::new(a, 0.0);
    pts[n as usize + 1] = DVec2::new(0.0, h);
    let mut sharp = vec![false; pts.len()];
    sharp[0] = true;
    sharp[1] = true;
    sharp[n as usize + 1] = true;
    let mut s = revolve(
        &Profile::from_contour(Contour { points: pts, sharp }),
        &Revolve {
            segments: per_turn,
            ..Default::default()
        },
    )?;
    let bottom = s.names.intern("bottom");
    for (i, t) in s.triangles.iter().enumerate() {
        if t.iter().all(|&v| s.positions[v as usize].y == 0.0) {
            s.face_tags[i] = vec![bottom];
        }
    }
    s.canonicalize()
        .map_err(|_| GenError::NotFinite("positions"))?;
    Ok(s)
}

/// A wedge: a right triangle (legs `width` along x, `height` along y)
/// extruded `depth` along z.
pub fn wedge(width: f64, height: f64, depth: f64) -> Result<Solid, GenError> {
    let (w, h) = (positive(width, "width")?, positive(height, "height")?);
    let p = Profile::from_contour(Contour::corners(vec![
        DVec2::ZERO,
        DVec2::new(w, 0.0),
        DVec2::new(0.0, h),
    ]));
    extrude(
        &p,
        &Extrude {
            height: depth,
            ..Default::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn closed(s: &Solid) -> f64 {
        let m = s.measure();
        assert!(m.closed, "open: {m:?}");
        assert_eq!(m.degenerate_triangles, 0);
        m.volume
    }

    #[test]
    fn a_box_has_its_volume_and_names() {
        let s = cuboid(&BoxDesc {
            width: 4.0,
            depth: 3.0,
            height: 2.0,
        })
        .unwrap();
        assert!((closed(&s) - 24.0).abs() < 1e-12);
        assert_eq!(s.faces(&["top"]).unwrap().len(), 2);
        assert_eq!(s.faces(&["side"]).unwrap().len(), 8);
        assert_eq!(s.faces(&["front"]).unwrap().len(), 2);
        assert_eq!(s.edges(&["top"]).unwrap().len(), 4);
        assert_eq!(s.edges(&["side"]).unwrap().len(), 4);
        // Every real edge is named, and only real edges are.
        assert_eq!(s.real_edges().len(), 12);
        assert_eq!(s.edge_tags.len(), 12);
    }

    #[test]
    fn a_tube_names_its_bore_and_is_closed() {
        let s = tube(10.0, 6.0, 5.0).unwrap();
        let v = closed(&s);
        let want = trig::PI * (100.0 - 36.0) * 5.0;
        assert!((v - want).abs() / want < 2e-2, "{v}");
        assert!(!s.faces(&["inner"]).unwrap().is_empty());
        // A round wall has no vertical corner edges.
        assert!(s.edges(&["side"]).is_err());
    }

    #[test]
    fn twisted_and_tapered_extrusions_are_closed() {
        let p = rect(&Rect {
            width: 4.0,
            height: 2.0,
            radius: 0.0,
        })
        .unwrap();
        let twisted = extrude(
            &p,
            &Extrude {
                height: 10.0,
                twist: trig::PI / 2.0,
                taper: 0.5,
                both_sides: true,
            },
        )
        .unwrap();
        closed(&twisted);
        assert_eq!(twisted.measure().bounds_min.z, -5.0);
        let s = extrude(
            &p,
            &Extrude {
                height: 10.0,
                taper: 0.5,
                ..Default::default()
            },
        )
        .unwrap();
        closed(&s);
        let m = s.measure();
        // A frustum's volume with a scaled square cross-section: h·A·(1 + k + k²)/3.
        let want = 10.0 * 8.0 * (1.0 + 0.5 + 0.25) / 3.0;
        assert!((m.volume - want).abs() / want < 1e-9, "{}", m.volume);
    }

    #[test]
    fn revolved_solids_are_closed() {
        let v = closed(&sphere(5.0).unwrap());
        assert!((v - 4.0 / 3.0 * trig::PI * 125.0).abs() / v < 3e-2, "{v}");
        closed(&torus(5.0, 1.0).unwrap());
        closed(&dome(3.0).unwrap());
        closed(&cone(2.0, 4.0).unwrap());
        closed(
            &cylinder(&CylinderDesc {
                radius: 2.0,
                radius_top: 3.0,
                height: 1.0,
                ..Default::default()
            })
            .unwrap(),
        );
        closed(&wedge(1.0, 2.0, 3.0).unwrap());
        let quarter = rect(&Rect {
            width: 2.0,
            height: 2.0,
            radius: 0.0,
        })
        .unwrap()
        .translate(DVec2::new(3.0, 0.0));
        let part = revolve(
            &quarter,
            &Revolve {
                angle: trig::PI / 2.0,
                ..Default::default()
            },
        )
        .unwrap();
        closed(&part);
        assert!(!part.faces(&["start"]).unwrap().is_empty());
        assert!(!part.edges(&["end"]).unwrap().is_empty());
    }

    #[test]
    fn round_solids_turn_about_z_and_rest_on_it() {
        let m = sphere_desc(&SphereDesc {
            radius: 5.0,
            axis: Axis::Z,
            ..Default::default()
        })
        .unwrap()
        .measure();
        assert!(m.closed);
        assert_eq!((m.bounds_min.z, m.bounds_max.z), (0.0, 10.0));
        // Same solid as about y, turned (summed in another order).
        let v = sphere(5.0).unwrap().measure().volume;
        assert!((m.volume - v).abs() < 1e-12 * v);
        let t = torus_desc(&TorusDesc {
            radius: 8.0,
            tube_radius: 2.0,
            axis: Axis::Z,
            ..Default::default()
        })
        .unwrap()
        .measure();
        assert!(t.closed);
        assert_eq!((t.bounds_min.z, t.bounds_max.z), (0.0, 4.0));
        let d = dome_desc(&DomeDesc {
            radius: 3.0,
            axis: Axis::Z,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(d.measure().bounds_min.z, 0.0);
        // The flat bottom faces -z.
        for i in d.faces(&["bottom"]).unwrap() {
            let [a, b, c] = d.triangles[i as usize].map(|v| d.positions[v as usize]);
            assert!((b - a).cross(c - a).z < 0.0);
        }
        // revolve about z: profile y is height.
        let ring = rect(&Rect {
            width: 2.0,
            height: 4.0,
            radius: 0.0,
        })
        .unwrap()
        .translate(DVec2::new(5.0, 2.0));
        let r = revolve(
            &ring,
            &Revolve {
                axis: Axis::Z,
                ..Default::default()
            },
        )
        .unwrap()
        .measure();
        assert_eq!((r.bounds_min.z, r.bounds_max.z), (0.0, 4.0));
    }

    #[test]
    fn a_spherical_cap_has_its_volume() {
        // V = π h² (3R − h) / 3, R = (a² + h²) / 2h; for a = 4, h = 2, R = 5.
        for (a, h) in [(4.0, 2.0), (3.0, 3.0), (3.0, 5.0)] {
            let s = dome_desc(&DomeDesc {
                radius: a,
                height: h,
                axis: Axis::Z,
                ..Default::default()
            })
            .unwrap();
            let r = (a * a + h * h) / (2.0 * h);
            let want = trig::PI * h * h * (3.0 * r - h) / 3.0;
            let v = closed(&s);
            // The chord rule's deficit: 24 a turn around, 3 to 6 segments
            // along a shallow arc.
            assert!((v - want).abs() / want < 4e-2, "{a} {h}: {v} {want}");
            let m = s.measure();
            assert!((m.bounds_max.z - h).abs() < 1e-12);
            assert!(!s.faces(&["bottom"]).unwrap().is_empty());
        }
    }

    #[test]
    fn bad_input_is_an_error_value() {
        let p = rect(&Rect {
            width: 1.0,
            height: 1.0,
            radius: 0.0,
        })
        .unwrap();
        assert_eq!(
            extrude(&p, &Extrude::default()),
            Err(GenError::NotPositive("height"))
        );
        assert_eq!(revolve(&p, &Revolve::default()), Err(GenError::CrossesAxis));
    }

    #[test]
    fn a_part_torus_is_its_share_of_the_ring() {
        let d = TorusDesc {
            radius: 10.0,
            tube_radius: 2.0,
            segments: 32,
            tube_segments: 16,
            ..Default::default()
        };
        let whole = torus_desc(&d).unwrap().measure();
        let quarter = torus_desc(&TorusDesc {
            angle: trig::PI / 2.0,
            ..d
        })
        .unwrap()
        .measure();
        assert!(quarter.closed);
        // 8 of the ring's 32 steps, exactly the same sections.
        assert!((quarter.volume * 4.0 - whole.volume).abs() < 1e-9 * whole.volume);
        let m = torus_desc(&TorusDesc { segments: 2, ..d });
        assert!(matches!(m, Err(GenError::Invalid(_))));
    }
}
