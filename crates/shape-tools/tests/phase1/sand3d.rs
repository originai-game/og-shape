//! The part of sand3d's vocabulary the phase 1 rebuilds use, over
//! og-shape's existing generators. No booleans and no bevels.
//!
//! sand3d (Apache-2.0, `Sandflow-ai/sand3d` at 84c980a) is millimetres,
//! Z up, like the kernel. Its conventions, kept here:
//! - `box`, `cylinder`, `tube` stand on z = 0, centred in x and y;
//!   `cylinder(d, h)` takes a diameter.
//! - `revolve` turns a profile of (radius, z) points about Z.
//! - `sphere` and `torus` rest on z = 0.
//! - `at(x, y, z, rx, ry, rz)` moves then turns (Blender's XYZ Euler, in
//!   degrees); `on` a frame puts that in the frame.
//! - A frame built from a normal alone has X = Z × normal (+X when the
//!   normal is ±Z), Y = normal × X.
//! - `bbox(ob, "+X-Y")` is a frame on the bounding box looking the first
//!   way named; `face(ob, "+X")` starts at the world origin projected on
//!   that face's plane.
//! - A joint's axis is a world direction (or the frame's normal); ranges
//!   are degrees for turns, millimetres for slides.

use shape_2d::{Circle, Contour, Gear, Profile, Rect, circle, gear, rect};
use shape_core::{DVec2, DVec3, Frame, trig};
use shape_gen::paths::{Loft, loft as gen_loft};
use shape_gen::solids::{
    BoxDesc, CylinderDesc, Extrude, Revolve, cuboid, cylinder as gen_cylinder,
    extrude as gen_extrude, revolve as gen_revolve, sphere as gen_sphere, torus as gen_torus,
};
use shape_mesh::Solid;
use shape_tools::{Joint, JointKind, Material, Part, Scene};

pub type Id = usize;

// ── frames ─────────────────────────────────────────────────────────────────

/// sand3d's `Frame(origin, normal)`.
pub fn frame(origin: [f64; 3], normal: [f64; 3]) -> Frame {
    let z = DVec3::from(normal).normalize();
    let x = DVec3::Z.cross(z);
    let x = if x.length() > 1e-6 {
        x.normalize()
    } else {
        DVec3::X
    };
    Frame {
        origin: origin.into(),
        x,
        y: z.cross(x),
        z,
    }
}

/// sand3d's `Frame(origin, normal, up)`.
pub fn frame_up(origin: [f64; 3], normal: [f64; 3], up: [f64; 3]) -> Frame {
    let z = DVec3::from(normal).normalize();
    let y = DVec3::from(up);
    let y = (y - z * y.dot(z)).normalize();
    Frame {
        origin: origin.into(),
        x: y.cross(z),
        y,
        z,
    }
}

/// sin and cos of `deg` degrees, snapped onto 0 and ±1 as sand3d snaps
/// matrix entries.
fn sin_cos_deg(deg: f64) -> (f64, f64) {
    let snap = |v: f64| {
        let r = v.round();
        if (v - r).abs() < 1e-12 { r } else { v }
    };
    let (s, c) = trig::sin_cos(trig::to_radians(deg));
    (snap(s), snap(c))
}

/// Blender's XYZ Euler rotation: about X, then Y, then Z.
pub fn euler(rx: f64, ry: f64, rz: f64) -> impl Fn(DVec3) -> DVec3 {
    let (sx, cx) = sin_cos_deg(rx);
    let (sy, cy) = sin_cos_deg(ry);
    let (sz, cz) = sin_cos_deg(rz);
    move |v: DVec3| {
        let v = DVec3::new(v.x, cx * v.y - sx * v.z, sx * v.y + cx * v.z);
        let v = DVec3::new(cy * v.x + sy * v.z, v.y, -sy * v.x + cy * v.z);
        DVec3::new(cz * v.x - sz * v.y, sz * v.x + cz * v.y, v.z)
    }
}

/// sand3d's `Frame.offset`, `.move` and `.turn`.
pub trait Moves {
    fn offset(&self, d: f64) -> Frame;
    fn mv(&self, x: f64, y: f64, z: f64) -> Frame;
    fn turn(&self, rx: f64, ry: f64, rz: f64) -> Frame;
}

impl Moves for Frame {
    fn offset(&self, d: f64) -> Frame {
        Frame {
            origin: self.origin + self.z * d,
            ..*self
        }
    }

    fn mv(&self, x: f64, y: f64, z: f64) -> Frame {
        Frame {
            origin: self.point_to_world(DVec3::new(x, y, z)),
            ..*self
        }
    }

    fn turn(&self, rx: f64, ry: f64, rz: f64) -> Frame {
        let e = euler(rx, ry, rz);
        let col = |v: DVec3| self.vector_to_world(e(v));
        let o = self.origin;
        frame_up([o.x, o.y, o.z], col(DVec3::Z).into(), col(DVec3::Y).into())
    }
}

// ── solids, in their own space ──────────────────────────────────────────────

fn ok<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    r.unwrap_or_else(|e| panic!("generator refused: {e:?}"))
}

/// `rect(w, h, r=…)`, centred.
pub fn rect_r(w: f64, h: f64, r: f64) -> Profile {
    ok(rect(&Rect {
        width: w,
        height: h,
        radius: r,
    }))
}

/// `circle(d)`.
pub fn circle_d(d: f64) -> Profile {
    ok(circle(&Circle {
        radius: d / 2.0,
        segments: 0,
    }))
}

/// `poly(points)`: every point a corner.
pub fn poly(points: &[[f64; 2]]) -> Profile {
    Profile::from_contour(Contour::corners(
        points.iter().map(|p| DVec2::new(p[0], p[1])).collect(),
    ))
}

/// `extrude(profile, h)` along +Z from z = 0.
pub fn extrude(p: &Profile, h: f64) -> Solid {
    ok(gen_extrude(
        p,
        &Extrude {
            height: h,
            ..Default::default()
        },
    ))
}

/// `box(w, d, h)`.
pub fn box_(w: f64, d: f64, h: f64) -> Solid {
    ok(cuboid(&BoxDesc {
        width: w,
        depth: d,
        height: h,
    }))
}

/// `cylinder(d, h)`.
pub fn cylinder(d: f64, h: f64) -> Solid {
    ok(gen_cylinder(&CylinderDesc {
        radius: d / 2.0,
        height: h,
        ..Default::default()
    }))
}

/// `tube(d, d_in, h)`.
pub fn tube(d: f64, d_in: f64, h: f64) -> Solid {
    ok(shape_gen::solids::tube(d / 2.0, d_in / 2.0, h))
}

/// The kernel turns about +Y; sand3d about +Z: a quarter turn about X.
fn y_axis_to_z(mut s: Solid) -> Solid {
    s.map_positions(|p| DVec3::new(p.x, -p.z, p.y), false);
    s
}

/// `revolve(points)`: a profile of (radius, z) corners about Z.
pub fn revolve(points: &[[f64; 2]]) -> Solid {
    y_axis_to_z(ok(gen_revolve(&poly(points), &Revolve::default())))
}

/// `sphere(d)`, resting on z = 0.
pub fn sphere(d: f64) -> Solid {
    let mut s = y_axis_to_z(ok(gen_sphere(d / 2.0)));
    s.map_positions(|p| p + DVec3::Z * (d / 2.0), false);
    s
}

/// `torus(d, d_wire)` about Z, resting on z = 0.
pub fn torus(d: f64, d_wire: f64) -> Solid {
    let mut s = y_axis_to_z(ok(gen_torus(d / 2.0, d_wire / 2.0)));
    s.map_positions(|p| p + DVec3::Z * (d_wire / 2.0), false);
    s
}

/// `loft(sections…)`: profiles stacked along +Z at the heights given,
/// each moved in its plane by `(dx, dy)`.
pub fn loft(sections: Vec<(Profile, f64, f64, f64)>) -> Solid {
    ok(gen_loft(&Loft {
        sections: sections
            .into_iter()
            .map(|(p, dx, dy, z)| (p.translate(DVec2::new(dx, dy)), z))
            .collect(),
        ..Default::default()
    }))
}

/// `gear(teeth, module, width, bore=…)`: an involute spur gear along +Z,
/// a tooth on +X, its bore cut in the profile.
pub fn spur(teeth: u32, module: f64, width: f64, bore: f64) -> Solid {
    let g = ok(gear(&Gear {
        teeth,
        module,
        pressure_angle: 0.0,
    }));
    extrude(&g.subtract(&circle_d(bore)), width)
}

/// Several solids as one, unfused (`merge`).
pub fn merge(solids: Vec<Solid>) -> Solid {
    let mut out = Solid::default();
    for s in &solids {
        out.append(s);
    }
    ok(out.canonicalize());
    out
}

// ── placement ───────────────────────────────────────────────────────────────

/// `at(s, x, y, z, rx, ry, rz)` in the world.
pub fn at(s: Solid, xyz: [f64; 3], rxyz: [f64; 3]) -> Solid {
    on_at(s, &Frame::IDENTITY, xyz, rxyz)
}

/// `at(s, x, y, z)`.
pub fn at_xyz(s: Solid, x: f64, y: f64, z: f64) -> Solid {
    at(s, [x, y, z], [0.0; 3])
}

/// `at(s, on=f)`.
pub fn on(s: Solid, f: &Frame) -> Solid {
    on_at(s, f, [0.0; 3], [0.0; 3])
}

/// `at(s, x, y, z, rx, ry, rz, on=f)`.
pub fn on_at(mut s: Solid, f: &Frame, xyz: [f64; 3], rxyz: [f64; 3]) -> Solid {
    let e = euler(rxyz[0], rxyz[1], rxyz[2]);
    let t = DVec3::from(xyz);
    s.map_positions(|p| f.point_to_world(t + e(p)), false);
    ok(s.canonicalize());
    s
}

/// `transform(s, mirror="x")`.
pub fn mirror_x(mut s: Solid) -> Solid {
    s.map_positions(|p| DVec3::new(-p.x, p.y, p.z), true);
    ok(s.canonicalize());
    s
}

// ── materials (sand3d's presets) ─────────────────────────────────────────────

/// `"#rrggbb"` as sRGB components.
pub fn hex(h: &str) -> [f64; 3] {
    let b = |i: usize| f64::from(u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0)) / 255.0;
    [b(1), b(3), b(5)]
}

/// sand3d's preset `kind`, its colour replaced where given.
pub fn material(kind: &str, color: Option<[f64; 3]>) -> Material {
    // (sRGB colour, roughness, metallic), from sand3d's materials.py.
    let (c, roughness, metalness) = match kind {
        "oak" => ("#b48a5a", 0.55, 0.0),
        "steel" => ("#a8abb0", 0.3, 1.0),
        "brushed_steel" => ("#b4b7bb", 0.35, 1.0),
        "aluminium" => ("#c9ccd0", 0.4, 1.0),
        "brass" => ("#c9a24a", 0.25, 1.0),
        "painted_metal" => ("#2b2d30", 0.4, 0.0),
        "plastic" => ("#d8d8d8", 0.5, 0.0),
        "glossy_plastic" => ("#d8d8d8", 0.12, 0.0),
        "rubber" => ("#202020", 0.9, 0.0),
        "concrete" => ("#9a9893", 0.85, 0.0),
        other => panic!("no preset {other:?} here"),
    };
    Material {
        name: match color {
            Some(rgb) => format!(
                "{kind} #{:02x}{:02x}{:02x}",
                (rgb[0] * 255.0).round() as u8,
                (rgb[1] * 255.0).round() as u8,
                (rgb[2] * 255.0).round() as u8
            ),
            None => kind.to_string(),
        },
        color: Some(color.unwrap_or_else(|| hex(c))),
        metalness,
        roughness: Some(roughness),
        ..Default::default()
    }
}

// ── the model ───────────────────────────────────────────────────────────────

/// A joint's axis: a world direction, or the pivot frame's normal.
#[derive(Clone, Copy)]
pub enum Axis {
    Normal,
    World(DVec3),
}

pub const PX: Axis = Axis::World(DVec3::X);
pub const NY: Axis = Axis::World(DVec3::NEG_Y);
pub const PY: Axis = Axis::World(DVec3::Y);
pub const PZ: Axis = Axis::World(DVec3::Z);
pub const NZ: Axis = Axis::World(DVec3::NEG_Z);

struct P {
    name: String,
    solid: Solid,
    material: Material,
    parent: Option<Id>,
    joint: Option<Joint>,
}

/// Parts in the world at rest, with materials and joints.
pub struct Model {
    pub name: String,
    parts: Vec<P>,
}

impl Model {
    pub fn new(name: &str) -> Model {
        Model {
            name: name.into(),
            parts: Vec::new(),
        }
    }

    /// A part already placed in the world. A name already taken gets
    /// Blender's suffix, `.001` and up, as a copy does in sand3d.
    pub fn add(&mut self, name: &str, solid: Solid) -> Id {
        let mut unique = name.to_string();
        let mut k = 0;
        while self.parts.iter().any(|p| p.name == unique) {
            k += 1;
            unique = format!("{name}.{k:03}");
        }
        self.parts.push(P {
            name: unique,
            solid,
            material: Material::default(),
            parent: None,
            joint: None,
        });
        self.parts.len() - 1
    }

    pub fn material(&mut self, ids: &[Id], m: &Material) {
        for &i in ids {
            self.parts[i].material = m.clone();
        }
    }

    fn bounds(&self, ids: &[Id]) -> (DVec3, DVec3) {
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        for &i in ids {
            for &p in &self.parts[i].solid.positions {
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
        (lo, hi)
    }

    /// `bbox(ids, "+X-Y")`.
    pub fn bbox(&self, ids: &[Id], which: &str) -> Frame {
        let (lo, hi) = self.bounds(ids);
        let mut origin = (lo + hi) * 0.5;
        let sides = sides(which);
        for &(axis, sign) in &sides {
            origin[axis] = if sign > 0.0 { hi[axis] } else { lo[axis] };
        }
        let mut normal = [0.0; 3];
        normal[sides[0].0] = sides[0].1;
        frame(origin.into(), normal)
    }

    /// `ob.face("+X")`: the world origin projected on the face's plane.
    pub fn face(&self, id: Id, which: &str) -> Frame {
        let (lo, hi) = self.bounds(&[id]);
        let (axis, sign) = sides(which)[0];
        let mut origin = [0.0; 3];
        origin[axis] = if sign > 0.0 { hi[axis] } else { lo[axis] };
        let mut normal = [0.0; 3];
        normal[axis] = sign;
        frame(origin, normal)
    }

    /// `joint(name, child, on, kind, axis, range)`: `child[0]` carries the
    /// joint and the rest of the group hangs on it; `parent` is the part
    /// `on` was read on. Ranges: degrees for turns, millimetres for
    /// slides.
    #[allow(clippy::too_many_arguments)]
    pub fn joint(
        &mut self,
        name: &str,
        child: &[Id],
        on: Frame,
        parent: Id,
        kind: JointKind,
        axis: Axis,
        range: Option<(f64, f64)>,
    ) {
        let axis = match axis {
            Axis::Normal => DVec3::Z,
            Axis::World(d) => on.vector_to_local(d),
        };
        let range = range.map(|(lo, hi)| match kind {
            JointKind::Prismatic => (lo, hi),
            _ => (trig::to_radians(lo), trig::to_radians(hi)),
        });
        let root = child[0];
        for &c in &child[1..] {
            if self.parts[c].joint.is_none() {
                self.parts[c].parent = Some(root);
            }
        }
        let p = &mut self.parts[root];
        assert!(p.joint.is_none(), "{} already hangs on a joint", p.name);
        p.parent = Some(parent);
        p.joint = Some(Joint {
            name: name.into(),
            kind,
            pivot: on,
            axis,
            range,
            ..Default::default()
        });
    }

    /// `follows=leader, ratio=…` on joint `name`.
    pub fn follows(&mut self, name: &str, leader: &str, ratio: f64) {
        for p in &mut self.parts {
            if let Some(j) = &mut p.joint
                && j.name == name
            {
                j.mimic = Some(shape_tools::Mimic {
                    joint: leader.into(),
                    multiplier: ratio,
                });
                return;
            }
        }
        panic!("no joint {name}");
    }

    /// `pose(…, value)`: degrees for turns, millimetres for slides.
    pub fn pose(&mut self, joint: &str, value: f64) {
        for p in &mut self.parts {
            if let Some(j) = &mut p.joint
                && j.name == joint
            {
                j.value = match j.kind {
                    JointKind::Prismatic => value,
                    _ => trig::to_radians(value),
                };
                return;
            }
        }
        panic!("no joint {joint}");
    }

    pub fn solids(&self) -> impl Iterator<Item = (&str, &Solid)> {
        self.parts.iter().map(|p| (p.name.as_str(), &p.solid))
    }

    /// The export scene: parents before children.
    pub fn scene(&self) -> Scene<'_> {
        let mut order = Vec::with_capacity(self.parts.len());
        let mut done = vec![false; self.parts.len()];
        while order.len() < self.parts.len() {
            let before = order.len();
            for (i, p) in self.parts.iter().enumerate() {
                if !done[i] && p.parent.is_none_or(|q| done[q]) {
                    done[i] = true;
                    order.push(i);
                }
            }
            assert!(order.len() > before, "a cycle among the parts' parents");
        }
        Scene {
            name: self.name.clone(),
            parts: order
                .into_iter()
                .map(|i| {
                    let p = &self.parts[i];
                    Part {
                        material: p.material.clone(),
                        parent: p
                            .parent
                            .map(|q| self.parts[q].name.clone())
                            .unwrap_or_default(),
                        joint: p.joint.clone(),
                        ..Part::new(&p.name, &p.solid)
                    }
                })
                .collect(),
        }
    }
}

/// "+X-Y" → [(0, +1), (1, −1)]; "top" is "+Z", "bottom" "-Z".
fn sides(which: &str) -> Vec<(usize, f64)> {
    let which = match which {
        "top" => "+Z",
        "bottom" => "-Z",
        w => w,
    };
    let b = which.as_bytes();
    (0..b.len() / 2)
        .map(|k| {
            let sign = if b[2 * k] == b'-' { -1.0 } else { 1.0 };
            let axis = match b[2 * k + 1] {
                b'X' => 0,
                b'Y' => 1,
                b'Z' => 2,
                c => panic!("not an axis: {}", c as char),
            };
            (axis, sign)
        })
        .collect()
}
