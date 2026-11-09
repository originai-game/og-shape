//! The model API as JSON operations: what a binding (TypeScript over
//! wasm, the CLI) sends, one operation at a time.
//!
//! Entry: [`Session`]. [`Session::apply`] takes one operation, an object
//! `{"op": "<name>", …fields}` with camelCase fields, and answers a JSON
//! object. Operations that change the model are recorded;
//! [`Session::program`] gives them back as a program,
//! `{"name": …, "ops": [...]}`, and [`run`] replays one. Readers (frames,
//! boxes, faces, measures) are not recorded: the values they gave are
//! written into the operations that used them, so a replay needs no
//! readers and gives the same bytes.
//!
//! Operations, with their fields (lengths in the model's unit, angles in
//! radians, frames as `{"origin": [x, y, z], "x": […], "y": […], "z": […]}`,
//! parts by the `id` a constructor answered):
//!
//! | op | fields | answers |
//! |---|---|---|
//! | `units` | `unit`: `"mm"`, `"cm"`, `"m"` | |
//! | `box` | `name`, `width`, `depth`, `height`, `on`? | `id` |
//! | `cylinder` | `name`, `radius`, `radiusTop`?, `height`, `on`? | `id` |
//! | `cone`, `sphere`, `dome` | `name`, `radius`, `height` (cone), `on`? | `id` |
//! | `tube` | `name`, `outer`, `inner`, `height`, `on`? | `id` |
//! | `torus` | `name`, `radius`, `tube`, `on`? | `id` |
//! | `wedge` | `name`, `width`, `height`, `depth`, `on`? | `id` |
//! | `extrude` | `name`, `profile`, `height`, `twist`?, `taper`?, `bothSides`?, `on`? | `id` |
//! | `revolve` | `name`, `profile` (x the radius, about +y), `angle`?, `on`? | `id` |
//! | `sweep` | `name`, `profile`, `path` (points), `up`?, `twist`?, `scaleEnd`? | `id` |
//! | `loft` | `name`, `sections`: `[{profile, z}]`, `samples`?, `steps`?, `on`? | `id` |
//! | `thread` | `name`, `radius`, `pitch`, `length`, `depth`, `on`? | `id` |
//! | `at`, `transform` | `id`, `frame` | |
//! | `group` | `name`, `children` | `id` |
//! | `assembly` | `name`, `children`, `anchors`? (`{name: frame}`) | `id` |
//! | `attach` | `to`, `name`, `children` | `id` |
//! | `copy` | `id`, `name` | `id` |
//! | `mirror` | `id`, `plane` (a frame), `name` | `id` |
//! | `cut`, `join`, `intersect` | `target`, `tool` | |
//! | `seat` | `host`, `part` | |
//! | `hole` | `id`, `diameter`, `at` (frames), `depth`?, `counterbore`? `[d, depth]`, `countersink`? `[d, angle]` | |
//! | `pocket` | `id`, `profile`, `depth`?, `at`? (frames) | |
//! | `split` | `id`, `frame`, `keep`? (`"above"`, `"below"`) | |
//! | `shell` | `id`, `thickness`, `open`? | |
//! | `dent` | `id`, `at` (points), `radius`, `depth` | |
//! | `fillet` | `id`, `radius`, `edges`?, `segments`?, `profile`? | |
//! | `chamfer` | `id`, `distance`, `distance2`?, `angle`?, `edges`?, `segments`? | |
//! | `soften` | `id`, `radius`, `angle`?, `edges`?, `segments`? | |
//! | `material` | `ids`, `name`?, `color`?, `metalness`?, `roughness`?, `emissive`?, `opacity`? | |
//! | `joint` | `child`, `parent`, `name`, `kind`?, `on`, `axis`?, `range`?, `mimic`? (`{joint, multiplier}`) | |
//! | `pose` | `joint`, `value` | |
//!
//! Readers, not recorded: `frame` (`origin`, `normal`, `up`?: a frame with
//! [`Frame::facing`]'s axes, or with Y toward `up`), `offset` (`frame`,
//! `distance`), `translate` (`frame`, `x`, `y`, `z`), `turn` (`frame`,
//! `rx`, `ry`, `rz`), `compose` (`frame`, `local`), `bbox` (`ids`,
//! `sides`), `face` (`id`, `name`), `edge` (`id`, `name`: a `path`),
//! `local` (`id`, `frame`), `anchor` (`id`, `name`), `placement` (`id`),
//! `grid` (`nx`, `ny`?, `px`?, `py`?, `stagger`?, `on`?), `ring` (`n`,
//! `radius`, `start`?, `angle`?, `on`?), `along` (`path`, `pitch`?,
//! `count`?), `snap` (`ids`, `points`, `direction`?), `names` (`id`),
//! `measure`. Profiles are objects with one key: `rect`, `circle`,
//! `ellipse`, `slot`, `polygon`, `poly` (`points`: `[x, y]` or
//! `{x, y, radius}`), `gear`, `union` / `subtract` / `intersect` (a list,
//! the first operated on by the rest), `offset` (`profile`, `distance`),
//! `translate` (`profile`, `x`, `y`), `rotate` (`profile`, `angle`).

use serde_json::{Map, Value, json};
use shape_2d::Profile;
use shape_bevel::{Chamfer, Fillet, Soften};
use shape_core::{DVec2, DVec3, Frame};
use shape_edit::{Dent, Hole, Keep, Pocket, Shell};
use shape_gen::paths::{Loft, Sweep, Thread};
use shape_gen::solids::{BoxDesc, CylinderDesc, Extrude, Revolve};
use shape_tools::{JointKind, Material, Mimic};

use crate::error::ModelError;
use crate::model::{Id, JointDesc, Model, Unit};
use crate::place::{self, Path};

/// A model and the program that built it.
#[derive(Clone, Debug, Default)]
pub struct Session {
    model: Model,
    ops: Vec<Value>,
}

/// Replays a program (`{"name": …, "ops": [...]}`) into a model.
pub fn run(program: &str) -> Result<Model, ModelError> {
    Ok(Session::replay(program)?.model)
}

impl Session {
    /// An empty model named `name`.
    pub fn new(name: &str) -> Session {
        Session {
            model: Model::new(name),
            ops: Vec::new(),
        }
    }

    /// Replays a program into a session that goes on recording.
    pub fn replay(program: &str) -> Result<Session, ModelError> {
        let v: Value = serde_json::from_str(program)
            .map_err(|e| ModelError::Program(format!("not JSON: {e}")))?;
        let name = v.get("name").and_then(Value::as_str).unwrap_or("model");
        let ops = v
            .get("ops")
            .and_then(Value::as_array)
            .ok_or_else(|| ModelError::Program("no \"ops\" list".into()))?;
        let mut s = Session::new(name);
        for (i, op) in ops.iter().enumerate() {
            s.apply_value(op)
                .map_err(|e| ModelError::Program(format!("op {i}: {e}")))?;
        }
        Ok(s)
    }

    /// The model so far.
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// The operations that changed the model, as a program [`run`] replays.
    pub fn program(&self) -> String {
        json!({"name": self.model.name(), "ops": self.ops}).to_string()
    }

    /// Applies one operation given as JSON text; answers JSON text.
    pub fn apply(&mut self, op: &str) -> Result<String, ModelError> {
        let v: Value =
            serde_json::from_str(op).map_err(|e| ModelError::Program(format!("not JSON: {e}")))?;
        Ok(self.apply_value(&v)?.to_string())
    }

    /// Applies one operation; answers a JSON object.
    pub fn apply_value(&mut self, op: &Value) -> Result<Value, ModelError> {
        let o = op
            .as_object()
            .ok_or_else(|| ModelError::Program("an operation is an object".into()))?;
        let name = o
            .get("op")
            .and_then(Value::as_str)
            .ok_or_else(|| ModelError::Program("an operation has an \"op\" string".into()))?;
        let a = Args { o, op: name };
        let (answer, recorded) = self.dispatch(&a)?;
        if recorded {
            self.ops.push(op.clone());
        }
        Ok(answer)
    }

    fn dispatch(&mut self, a: &Args) -> Result<(Value, bool), ModelError> {
        let m = &mut self.model;
        let none = || Value::Object(Map::new());
        let read = |v: Value| Ok((v, false));
        let id = |i: Id| json!({ "id": i.0 });
        match a.op {
            // ── readers ──
            "frame" => {
                let origin = a.vec3("origin")?;
                let normal = a.vec3("normal")?;
                let f = match a.opt("up") {
                    Some(_) => Frame::with_up(origin, normal, a.vec3("up")?),
                    None => Frame::facing(origin, normal),
                }
                .ok_or_else(|| a.bad("normal", "zero, or along up"))?;
                read(frame_json(&f))
            }
            "offset" => read(frame_json(&a.frame("frame")?.offset(a.num("distance")?))),
            "translate" => read(frame_json(&a.frame("frame")?.translate(
                a.num_or("x", 0.0)?,
                a.num_or("y", 0.0)?,
                a.num_or("z", 0.0)?,
            ))),
            "turn" => read(frame_json(&a.frame("frame")?.turn(
                a.num_or("rx", 0.0)?,
                a.num_or("ry", 0.0)?,
                a.num_or("rz", 0.0)?,
            ))),
            "compose" => read(frame_json(&a.frame("frame")?.compose(&a.frame("local")?))),
            "bbox" => read(frame_json(&m.bbox(&a.ids("ids")?, a.str("sides")?)?)),
            "face" => read(frame_json(&m.face(a.id("id")?, a.str("name")?)?)),
            "edge" => read(path_json(&m.edge(a.id("id")?, a.str("name")?)?)),
            "local" => read(frame_json(&m.local(a.id("id")?, &a.frame("frame")?)?)),
            "anchor" => read(frame_json(&m.anchor(a.id("id")?, a.str("name")?)?)),
            "placement" => read(frame_json(&m.placement(a.id("id")?)?)),
            "grid" => read(frames_json(&place::grid(
                a.u32_or("nx", 1)?,
                a.u32_or("ny", 1)?,
                a.num_or("px", 0.0)?,
                a.num_or("py", 0.0)?,
                a.bool_or("stagger", false)?,
                &a.frame_or("on")?,
            ))),
            "ring" => read(frames_json(&place::ring(
                a.u32_or("n", 0)?,
                a.num("radius")?,
                a.num_or("start", 0.0)?,
                a.num_or("angle", 0.0)?,
                &a.frame_or("on")?,
            ))),
            "along" => read(frames_json(&place::along(
                &a.path("path")?,
                a.num_or("pitch", 0.0)?,
                a.u32_or("count", 0)?,
            )?)),
            "snap" => {
                let dir = match a.opt("direction") {
                    Some(_) => Some(a.vec3("direction")?),
                    None => None,
                };
                read(frames_json(&m.snap(
                    &a.ids("ids")?,
                    &a.points("points")?,
                    dir,
                )?))
            }
            "names" => {
                let (faces, edges) = m.names(a.id("id")?)?;
                read(json!({ "faces": faces, "edges": edges }))
            }
            "measure" => read(measures_value(m)?),
            // ── the model ──
            "units" => {
                let u = match a.str("unit")? {
                    "mm" => Unit::Millimetre,
                    "cm" => Unit::Centimetre,
                    "m" => Unit::Metre,
                    _ => return Err(a.bad("unit", "\"mm\", \"cm\" or \"m\"")),
                };
                m.units(u)?;
                Ok((none(), true))
            }
            "box" | "cylinder" | "cone" | "sphere" | "dome" | "tube" | "torus" | "wedge"
            | "extrude" | "revolve" | "sweep" | "loft" | "thread" => {
                let solid = generate(a)?;
                let i = m.add(a.str("name")?, solid)?;
                if a.opt("on").is_some() {
                    m.at(i, &a.frame("on")?)?;
                }
                Ok((id(i), true))
            }
            "at" => {
                m.at(a.id("id")?, &a.frame("frame")?)?;
                Ok((none(), true))
            }
            "transform" => {
                m.transform(a.id("id")?, &a.frame("frame")?)?;
                Ok((none(), true))
            }
            "group" => Ok((id(m.group(a.str("name")?, &a.ids("children")?)?), true)),
            "assembly" => {
                let mut anchors = Vec::new();
                if let Some(v) = a.opt("anchors") {
                    let o = v
                        .as_object()
                        .ok_or_else(|| a.bad("anchors", "an object of frames"))?;
                    for (k, f) in o {
                        anchors.push((k.clone(), frame_of(f).map_err(|w| a.bad("anchors", &w))?));
                    }
                }
                Ok((
                    id(m.assembly(a.str("name")?, &a.ids("children")?, &anchors)?),
                    true,
                ))
            }
            "attach" => Ok((
                id(m.attach(a.id("to")?, a.str("name")?, &a.ids("children")?)?),
                true,
            )),
            "copy" => Ok((id(m.copy(a.id("id")?, a.str("name")?)?), true)),
            "mirror" => Ok((
                id(m.mirror(a.id("id")?, &a.frame("plane")?, a.str("name")?)?),
                true,
            )),
            "cut" => {
                m.cut(a.id("target")?, a.id("tool")?)?;
                Ok((none(), true))
            }
            "join" => {
                m.join(a.id("target")?, a.id("tool")?)?;
                Ok((none(), true))
            }
            "intersect" => {
                m.intersect(a.id("target")?, a.id("tool")?)?;
                Ok((none(), true))
            }
            "seat" => {
                m.seat(a.id("host")?, a.id("part")?)?;
                Ok((none(), true))
            }
            "hole" => {
                let pair = |k: &str| -> Result<(f64, f64), ModelError> {
                    match a.opt(k) {
                        None => Ok((0.0, 0.0)),
                        Some(_) => {
                            let v = a.nums(k)?;
                            match v[..] {
                                [x, y] => Ok((x, y)),
                                _ => Err(a.bad(k, "two numbers")),
                            }
                        }
                    }
                };
                let d = Hole {
                    diameter: a.num("diameter")?,
                    at: a.frames("at")?,
                    depth: a.num_or("depth", 0.0)?,
                    counterbore: pair("counterbore")?,
                    countersink: pair("countersink")?,
                };
                m.hole(a.id("id")?, &d)?;
                Ok((none(), true))
            }
            "pocket" => {
                let d = Pocket {
                    profile: profile(a, a.get("profile")?)?,
                    depth: a.num_or("depth", 0.0)?,
                    at: match a.opt("at") {
                        Some(_) => a.frames("at")?,
                        None => Vec::new(),
                    },
                };
                m.pocket(a.id("id")?, &d)?;
                Ok((none(), true))
            }
            "split" => {
                let keep = match a.opt("keep").and_then(Value::as_str).unwrap_or("above") {
                    "above" => Keep::Above,
                    "below" => Keep::Below,
                    _ => return Err(a.bad("keep", "\"above\" or \"below\"")),
                };
                m.split(a.id("id")?, &a.frame("frame")?, keep)?;
                Ok((none(), true))
            }
            "shell" => {
                let d = Shell {
                    thickness: a.num("thickness")?,
                    open: a.names_or("open")?,
                };
                m.shell(a.id("id")?, &d)?;
                Ok((none(), true))
            }
            "dent" => {
                let d = Dent {
                    at: a.points("at")?,
                    radius: a.num("radius")?,
                    depth: a.num("depth")?,
                };
                m.dent(a.id("id")?, &d)?;
                Ok((none(), true))
            }
            "fillet" => {
                let d = Fillet {
                    radius: a.num("radius")?,
                    edges: a.names_or("edges")?,
                    segments: a.u32_or("segments", 0)?,
                    profile: a.num_or("profile", 0.0)?,
                };
                m.fillet(a.id("id")?, &d)?;
                Ok((none(), true))
            }
            "chamfer" => {
                let d = Chamfer {
                    distance: a.num("distance")?,
                    distance2: a.num_or("distance2", 0.0)?,
                    angle: a.num_or("angle", 0.0)?,
                    edges: a.names_or("edges")?,
                    segments: a.u32_or("segments", 0)?,
                };
                m.chamfer(a.id("id")?, &d)?;
                Ok((none(), true))
            }
            "soften" => {
                let d = Soften {
                    radius: a.num("radius")?,
                    angle: a.num_or("angle", 0.0)?,
                    edges: a.names_or("edges")?,
                    segments: a.u32_or("segments", 0)?,
                };
                m.soften(a.id("id")?, &d)?;
                Ok((none(), true))
            }
            "material" => {
                let colour = |k: &str| -> Result<Option<[f64; 3]>, ModelError> {
                    match a.opt(k) {
                        None => Ok(None),
                        Some(Value::String(s)) => {
                            hex(s).map(Some).ok_or_else(|| a.bad(k, "\"#rrggbb\""))
                        }
                        Some(_) => match a.nums(k)?[..] {
                            [r, g, b] => Ok(Some([r, g, b])),
                            _ => Err(a.bad(k, "\"#rrggbb\" or [r, g, b]")),
                        },
                    }
                };
                let opt_num = |k: &str| -> Result<Option<f64>, ModelError> {
                    match a.opt(k) {
                        None => Ok(None),
                        Some(_) => a.num(k).map(Some),
                    }
                };
                let mat = Material {
                    name: a.opt("name").and_then(Value::as_str).unwrap_or("").into(),
                    color: colour("color")?,
                    metalness: a.num_or("metalness", 0.0)?,
                    roughness: opt_num("roughness")?,
                    emissive: colour("emissive")?.unwrap_or([0.0; 3]),
                    opacity: opt_num("opacity")?,
                };
                m.material(&a.ids("ids")?, &mat)?;
                Ok((none(), true))
            }
            "joint" => {
                let kind = match a.opt("kind").and_then(Value::as_str).unwrap_or("revolute") {
                    "revolute" => JointKind::Revolute,
                    "continuous" => JointKind::Continuous,
                    "prismatic" => JointKind::Prismatic,
                    "fixed" => JointKind::Fixed,
                    "floating" => JointKind::Floating,
                    _ => {
                        return Err(
                            a.bad("kind", "revolute, continuous, prismatic, fixed or floating")
                        );
                    }
                };
                let range = match a.opt("range") {
                    None | Some(Value::Null) => None,
                    Some(_) => match a.nums("range")?[..] {
                        [lo, hi] => Some((lo, hi)),
                        _ => return Err(a.bad("range", "[lower, upper]")),
                    },
                };
                let mimic = match a.opt("mimic") {
                    None | Some(Value::Null) => None,
                    Some(v) => {
                        let o = v
                            .as_object()
                            .ok_or_else(|| a.bad("mimic", "{joint, multiplier}"))?;
                        Some(Mimic {
                            joint: o
                                .get("joint")
                                .and_then(Value::as_str)
                                .ok_or_else(|| a.bad("mimic", "{joint, multiplier}"))?
                                .into(),
                            multiplier: o.get("multiplier").and_then(Value::as_f64).unwrap_or(1.0),
                        })
                    }
                };
                let d = JointDesc {
                    name: a.str("name")?.into(),
                    kind,
                    on: a.frame("on")?,
                    axis: match a.opt("axis") {
                        Some(Value::String(s)) => {
                            place::direction(s).ok_or_else(|| a.bad("axis", "\"+X\"…"))?
                        }
                        Some(_) => a.vec3("axis")?,
                        None => DVec3::ZERO,
                    },
                    range,
                    mimic,
                };
                m.joint(a.id("child")?, a.id("parent")?, &d)?;
                Ok((none(), true))
            }
            "pose" => {
                m.pose(a.str("joint")?, a.num("value")?)?;
                Ok((none(), true))
            }
            other => Err(ModelError::Program(format!("no operation {other:?}"))),
        }
    }
}

/// `"#rrggbb"` as sRGB components in 0..=1.
fn hex(s: &str) -> Option<[f64; 3]> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let c = |i: usize| {
        u8::from_str_radix(&h[i..i + 2], 16)
            .ok()
            .map(|b| f64::from(b) / 255.0)
    };
    Some([c(0)?, c(2)?, c(4)?])
}

fn generate(a: &Args) -> Result<shape_mesh::Solid, ModelError> {
    use shape_gen::solids as g;
    Ok(match a.op {
        "box" => g::cuboid(&BoxDesc {
            width: a.num("width")?,
            depth: a.num("depth")?,
            height: a.num("height")?,
        })?,
        "cylinder" => g::cylinder(&CylinderDesc {
            radius: a.num("radius")?,
            radius_top: a.num_or("radiusTop", 0.0)?,
            height: a.num("height")?,
        })?,
        "cone" => g::cone(a.num("radius")?, a.num("height")?)?,
        "sphere" => g::sphere(a.num("radius")?)?,
        "dome" => g::dome(a.num("radius")?)?,
        "tube" => g::tube(a.num("outer")?, a.num("inner")?, a.num("height")?)?,
        "torus" => g::torus(a.num("radius")?, a.num("tube")?)?,
        "wedge" => g::wedge(a.num("width")?, a.num("height")?, a.num("depth")?)?,
        "extrude" => g::extrude(
            &profile(a, a.get("profile")?)?,
            &Extrude {
                height: a.num("height")?,
                twist: a.num_or("twist", 0.0)?,
                taper: a.num_or("taper", 0.0)?,
                both_sides: a.bool_or("bothSides", false)?,
            },
        )?,
        "revolve" => g::revolve(
            &profile(a, a.get("profile")?)?,
            &Revolve {
                angle: a.num_or("angle", 0.0)?,
            },
        )?,
        "sweep" => shape_gen::paths::sweep(
            &profile(a, a.get("profile")?)?,
            &Sweep {
                path: a.points("path")?,
                up: match a.opt("up") {
                    Some(_) => a.vec3("up")?,
                    None => DVec3::ZERO,
                },
                twist: a.num_or("twist", 0.0)?,
                scale_end: a.num_or("scaleEnd", 0.0)?,
            },
        )?,
        "loft" => {
            let list = a
                .get("sections")?
                .as_array()
                .ok_or_else(|| a.bad("sections", "a list of {profile, z}"))?;
            let mut sections = Vec::new();
            for s in list {
                let p = s
                    .get("profile")
                    .ok_or_else(|| a.bad("sections", "a list of {profile, z}"))?;
                let z = s
                    .get("z")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| a.bad("sections", "a list of {profile, z}"))?;
                sections.push((profile(a, p)?, z));
            }
            shape_gen::paths::loft(&Loft {
                sections,
                samples: a.u32_or("samples", 0)?,
                steps: a.u32_or("steps", 0)?,
            })?
        }
        "thread" => shape_gen::paths::thread(&Thread {
            radius: a.num("radius")?,
            pitch: a.num("pitch")?,
            length: a.num("length")?,
            depth: a.num("depth")?,
        })?,
        _ => return Err(ModelError::Program(format!("no generator {:?}", a.op))),
    })
}

fn profile(a: &Args, v: &Value) -> Result<Profile, ModelError> {
    let bad = |w: &str| a.bad("profile", w);
    let o = v
        .as_object()
        .filter(|o| o.len() == 1)
        .ok_or_else(|| bad("an object with one key: rect, circle, …"))?;
    let (k, v) = o.iter().next().ok_or_else(|| bad("empty"))?;
    let f = |key: &str| -> Result<f64, ModelError> {
        match v.get(key) {
            None => Ok(0.0),
            Some(x) => x
                .as_f64()
                .ok_or_else(|| bad(&format!("{k}.{key} is a number"))),
        }
    };
    let u = |key: &str| -> Result<u32, ModelError> { Ok(f(key)? as u32) };
    let list = || {
        v.as_array()
            .filter(|l| !l.is_empty())
            .ok_or_else(|| bad(&format!("{k} takes a list of profiles")))
    };
    Ok(match k.as_str() {
        "rect" => shape_2d::rect(&shape_2d::Rect {
            width: f("width")?,
            height: f("height")?,
            radius: f("radius")?,
        })?,
        "circle" => shape_2d::circle(&shape_2d::Circle {
            radius: f("radius")?,
            segments: u("segments")?,
        })?,
        "ellipse" => shape_2d::ellipse(&shape_2d::Ellipse {
            rx: f("rx")?,
            ry: f("ry")?,
            segments: u("segments")?,
        })?,
        "slot" => shape_2d::slot(&shape_2d::Slot {
            length: f("length")?,
            width: f("width")?,
        })?,
        "polygon" => shape_2d::polygon(&shape_2d::Polygon {
            sides: u("sides")?,
            radius: f("radius")?,
        })?,
        "gear" => shape_2d::gear(&shape_2d::Gear {
            teeth: u("teeth")?,
            module: f("module")?,
            pressure_angle: f("pressureAngle")?,
        })?,
        "poly" => {
            let pts = v
                .get("points")
                .and_then(Value::as_array)
                .ok_or_else(|| bad("poly.points"))?;
            let mut points = Vec::new();
            for p in pts {
                let (x, y, r) = match p {
                    Value::Array(xy) if xy.len() == 2 => {
                        (xy[0].as_f64(), xy[1].as_f64(), Some(0.0))
                    }
                    Value::Object(o) => (
                        o.get("x").and_then(Value::as_f64),
                        o.get("y").and_then(Value::as_f64),
                        Some(o.get("radius").and_then(Value::as_f64).unwrap_or(0.0)),
                    ),
                    _ => (None, None, None),
                };
                match (x, y, r) {
                    (Some(x), Some(y), Some(radius)) => {
                        points.push(shape_2d::PolyPoint { x, y, radius })
                    }
                    _ => return Err(bad("poly.points: [x, y] or {x, y, radius}")),
                }
            }
            shape_2d::poly(&shape_2d::Poly { points })?
        }
        "union" | "subtract" | "intersect" => {
            let l = list()?;
            let mut acc = profile(a, &l[0])?;
            for q in &l[1..] {
                let q = profile(a, q)?;
                acc = match k.as_str() {
                    "union" => acc.union(&q),
                    "subtract" => acc.subtract(&q),
                    _ => acc.intersect(&q),
                };
            }
            acc
        }
        "offset" => profile(a, v.get("profile").ok_or_else(|| bad("offset.profile"))?)?
            .offset(f("distance")?, shape_2d::Join::Round)?,
        "translate" => profile(a, v.get("profile").ok_or_else(|| bad("translate.profile"))?)?
            .translate(DVec2::new(f("x")?, f("y")?)),
        "rotate" => {
            profile(a, v.get("profile").ok_or_else(|| bad("rotate.profile"))?)?.rotate(f("angle")?)
        }
        other => return Err(bad(&format!("no profile {other:?}"))),
    })
}

/// A frame as JSON.
pub fn frame_json(f: &Frame) -> Value {
    let v = |d: DVec3| json!([d.x, d.y, d.z]);
    json!({ "origin": v(f.origin), "x": v(f.x), "y": v(f.y), "z": v(f.z) })
}

fn frames_json(fs: &[Frame]) -> Value {
    json!({ "frames": fs.iter().map(frame_json).collect::<Vec<_>>() })
}

fn path_json(p: &Path) -> Value {
    json!({
        "points": p.points.iter().map(|d| json!([d.x, d.y, d.z])).collect::<Vec<_>>(),
        "closed": p.closed,
    })
}

fn vec3_of(v: &Value) -> Option<DVec3> {
    let l = v.as_array()?;
    match l[..] {
        [ref x, ref y, ref z] => Some(DVec3::new(x.as_f64()?, y.as_f64()?, z.as_f64()?)),
        _ => None,
    }
}

fn frame_of(v: &Value) -> Result<Frame, String> {
    let g = |k: &str| {
        v.get(k)
            .and_then(vec3_of)
            .ok_or_else(|| format!("a frame has {k}: [x, y, z]"))
    };
    Ok(Frame {
        origin: g("origin")?,
        x: g("x")?,
        y: g("y")?,
        z: g("z")?,
    })
}

/// Measures of every part (`docs/design.md` §7) as JSON, in the model's
/// unit, with no thresholds.
pub fn measures_json(m: &Model) -> Result<String, ModelError> {
    Ok(measures_value(m)?.to_string())
}

fn measures_value(m: &Model) -> Result<Value, ModelError> {
    let parts = m.measure()?;
    let v3 = |d: DVec3| json!([d.x, d.y, d.z]);
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    let (mut tris, mut verts, mut area, mut volume) = (0usize, 0usize, 0.0, 0.0);
    let mut list = Vec::new();
    for p in &parts {
        let x = &p.measures;
        if x.vertices > 0 {
            lo = lo.min(x.bounds_min);
            hi = hi.max(x.bounds_max);
        }
        tris += x.triangles;
        verts += x.vertices;
        area += x.area;
        volume += x.volume;
        list.push(json!({
            "name": p.name,
            "triangles": x.triangles,
            "vertices": x.vertices,
            "boundsMin": v3(x.bounds_min),
            "boundsMax": v3(x.bounds_max),
            "area": x.area,
            "volume": x.volume,
            "closed": x.closed,
            "boundaryEdges": x.boundary_edges,
            "nonManifoldEdges": x.non_manifold_edges,
            "degenerateTriangles": x.degenerate_triangles,
            "components": x.components,
        }));
    }
    let empty = tris == 0;
    let unit = match m.unit() {
        Unit::Millimetre => "mm",
        Unit::Centimetre => "cm",
        Unit::Metre => "m",
    };
    Ok(json!({
        "name": m.name(),
        "unit": unit,
        "parts": list,
        "joints": m.joint_names(),
        "triangles": tris,
        "vertices": verts,
        "boundsMin": if empty { Value::Null } else { v3(lo) },
        "boundsMax": if empty { Value::Null } else { v3(hi) },
        "pivot": v3(DVec3::ZERO),
        "area": area,
        "volume": volume,
        // No UVs and no textures yet (phase 5).
        "uvCoverage": Value::Null,
        "uvOverlap": Value::Null,
        "textureBytes": 0,
    }))
}

/// One operation's fields.
struct Args<'a> {
    o: &'a Map<String, Value>,
    op: &'a str,
}

impl Args<'_> {
    fn bad(&self, field: &str, want: &str) -> ModelError {
        ModelError::Program(format!("{}: {field}: {want}", self.op))
    }

    fn opt(&self, k: &str) -> Option<&Value> {
        self.o.get(k).filter(|v| !v.is_null())
    }

    fn get(&self, k: &str) -> Result<&Value, ModelError> {
        self.opt(k).ok_or_else(|| self.bad(k, "missing"))
    }

    fn num(&self, k: &str) -> Result<f64, ModelError> {
        self.get(k)?.as_f64().ok_or_else(|| self.bad(k, "a number"))
    }

    fn num_or(&self, k: &str, d: f64) -> Result<f64, ModelError> {
        if self.opt(k).is_some() {
            self.num(k)
        } else {
            Ok(d)
        }
    }

    fn u32_or(&self, k: &str, d: u32) -> Result<u32, ModelError> {
        match self.opt(k) {
            None => Ok(d),
            Some(v) => v
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| self.bad(k, "a whole number")),
        }
    }

    fn bool_or(&self, k: &str, d: bool) -> Result<bool, ModelError> {
        match self.opt(k) {
            None => Ok(d),
            Some(v) => v.as_bool().ok_or_else(|| self.bad(k, "true or false")),
        }
    }

    fn str(&self, k: &str) -> Result<&str, ModelError> {
        self.get(k)?.as_str().ok_or_else(|| self.bad(k, "a string"))
    }

    /// A list of names, or one string of names separated by spaces.
    fn names_or(&self, k: &str) -> Result<Vec<String>, ModelError> {
        match self.opt(k) {
            None => Ok(Vec::new()),
            Some(Value::String(s)) => Ok(s.split_whitespace().map(String::from).collect()),
            Some(Value::Array(l)) => l
                .iter()
                .map(|v| v.as_str().map(String::from))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| self.bad(k, "a list of names")),
            Some(_) => Err(self.bad(k, "names")),
        }
    }

    fn id(&self, k: &str) -> Result<Id, ModelError> {
        id_of(self.get(k)?).ok_or_else(|| self.bad(k, "a part's id"))
    }

    fn ids(&self, k: &str) -> Result<Vec<Id>, ModelError> {
        match self.get(k)? {
            Value::Array(l) => l
                .iter()
                .map(id_of)
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| self.bad(k, "a list of ids")),
            v => id_of(v).map(|i| vec![i]).ok_or_else(|| self.bad(k, "ids")),
        }
    }

    fn nums(&self, k: &str) -> Result<Vec<f64>, ModelError> {
        self.get(k)?
            .as_array()
            .and_then(|l| l.iter().map(Value::as_f64).collect::<Option<Vec<_>>>())
            .ok_or_else(|| self.bad(k, "a list of numbers"))
    }

    fn vec3(&self, k: &str) -> Result<DVec3, ModelError> {
        vec3_of(self.get(k)?).ok_or_else(|| self.bad(k, "[x, y, z]"))
    }

    fn points(&self, k: &str) -> Result<Vec<DVec3>, ModelError> {
        self.get(k)?
            .as_array()
            .and_then(|l| l.iter().map(vec3_of).collect::<Option<Vec<_>>>())
            .ok_or_else(|| self.bad(k, "a list of [x, y, z]"))
    }

    fn frame(&self, k: &str) -> Result<Frame, ModelError> {
        frame_of(self.get(k)?).map_err(|w| self.bad(k, &w))
    }

    fn frame_or(&self, k: &str) -> Result<Frame, ModelError> {
        if self.opt(k).is_some() {
            self.frame(k)
        } else {
            Ok(Frame::IDENTITY)
        }
    }

    fn frames(&self, k: &str) -> Result<Vec<Frame>, ModelError> {
        match self.get(k)? {
            Value::Array(l) => l
                .iter()
                .map(frame_of)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|w| self.bad(k, &w)),
            v => frame_of(v).map(|f| vec![f]).map_err(|w| self.bad(k, &w)),
        }
    }

    fn path(&self, k: &str) -> Result<Path, ModelError> {
        let v = self.get(k)?;
        let points = v
            .get("points")
            .and_then(Value::as_array)
            .and_then(|l| l.iter().map(vec3_of).collect::<Option<Vec<_>>>())
            .ok_or_else(|| self.bad(k, "{points, closed}"))?;
        Ok(Path {
            points,
            closed: v.get("closed").and_then(Value::as_bool).unwrap_or(false),
        })
    }
}

fn id_of(v: &Value) -> Option<Id> {
    match v {
        Value::Number(n) => n.as_u64().and_then(|n| u32::try_from(n).ok()).map(Id),
        Value::Object(o) => o.get("id").and_then(id_of),
        _ => None,
    }
}
