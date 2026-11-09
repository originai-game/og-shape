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
//! Every operation, and every object inside one (a profile, a section, a
//! view, a frame), takes only the fields listed; any other field is an
//! error naming it and the fields it takes. A failing operation answers
//! [`ModelError::At`]: its index in the program, its verb, and the part it
//! made or worked on.
//!
//! Operations, with their fields (lengths in the model's unit, angles in
//! radians, frames as `{"origin": [x, y, z], "x": […], "y": […], "z": […]}`,
//! parts by the `id` a constructor answered, turned solids' `axis` `"+Y"`
//! (the default) or `"+Z"`, `segments` a turn with 0 for the chord rule):
//!
//! | op | fields | answers |
//! |---|---|---|
//! | `units` | `unit`: `"mm"`, `"cm"`, `"m"` | |
//! | `box` | `name`, `width`, `depth`, `height`, `on`? | `id` |
//! | `cylinder` | `name`, `radius`, `radiusTop`?, `height`, `segments`?, `on`? | `id` |
//! | `cone` | `name`, `radius`, `height`, `segments`?, `on`? | `id` |
//! | `sphere` | `name`, `radius`, `axis`?, `segments`?, `on`? | `id` |
//! | `dome` | `name`, `radius`, `height`?, `axis`?, `segments`?, `on`? | `id` |
//! | `tube` | `name`, `outer`, `inner`, `height`, `segments`?, `on`? | `id` |
//! | `torus` | `name`, `radius`, `tube`, `angle`?, `axis`?, `segments`?, `tubeSegments`?, `on`? | `id` |
//! | `wedge` | `name`, `width`, `height`, `depth`, `on`? | `id` |
//! | `extrude` | `name`, `profile`, `height`, `twist`?, `taper`?, `bothSides`?, `on`? | `id` |
//! | `revolve` | `name`, `profile` (x the radius), `angle`?, `axis`?, `segments`?, `on`? | `id` |
//! | `sweep` | `name`, `profile`, `path` (points), `up`?, `twist`?, `scaleEnd`?, `on`? | `id` |
//! | `loft` | `name`, `sections`: `[{profile, z}]`, `samples`?, `steps`?, `curve`? (`"auto"`, `"straight"`, `"catmullRom"`, `"spline"`), `on`? | `id` |
//! | `thread` | `name`, `radius`, `pitch`, `length`, `depth`, `on`? | `id` |
//! | `wire` | `name`, `path` (points), `diameter`, `closed`?, `fillet`?, `fillets`?, `on`? | `id` |
//! | `body` | `name`, `plan`, `top`, `bottom`, `n`? (views: a number, `[[y, v]…]`, or `{points, smooth}`), `on`? | `id` |
//! | `knurl` | `name`, `diameter`, `height`, `pitch`?, `straight`?, `on`? | `id` |
//! | `gear` | `name`, `teeth`, `module`, `width`, `pressureAngle`?, `bore`?, `hub`?, `hubHeight`?, `on`? | `id` |
//! | `at`, `transform` | `id`, `frame` | |
//! | `group` | `name`, `children` | `id` |
//! | `assembly` | `name`, `children`, `anchors`? (`{name: frame}`) | `id` |
//! | `attach` | `to`, `name`, `children` | `id` |
//! | `copy` | `id`, `name` | `id` |
//! | `mirror` | `id`, `plane` (a frame), `name` | `id` |
//! | `cut`, `join`, `intersect` | `target`, `tool` | |
//! | `seat` | `host`, `part` | |
//! | `scale` | `id`, `x`?, `y`?, `z`? (factors along its own axes; 0 keeps 1) | |
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
//! `ellipse`, `slot`, `polygon` (`sides`, `radius` or `flats`), `poly`
//! (`points`: `[x, y]` or `{x, y, radius, bulge}`), `gear`, `union` /
//! `subtract` / `intersect` (a list, the first operated on by the rest),
//! `offset` (`profile`, `distance`), `translate` (`profile`, `x`, `y`),
//! `rotate` (`profile`, `angle`), `scale` (`profile`, `x`, `y`; 0 keeps 1).

use serde_json::{Map, Value, json};
use shape_2d::Profile;
use shape_bevel::{Chamfer, Fillet, Soften};
use shape_core::{DVec2, DVec3, Frame};
use shape_edit::{Dent, Hole, Keep, Pocket, Shell};
use shape_gen::body::{Body, View};
use shape_gen::loft::LoftCurve;
use shape_gen::parts::{GearDesc, Knurl};
use shape_gen::paths::{Loft, Sweep, Thread};
use shape_gen::solids::{
    Axis, BoxDesc, ConeDesc, CylinderDesc, DomeDesc, Extrude, Revolve, SphereDesc, TorusDesc,
    TubeDesc,
};
use shape_gen::wire::Wire;
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
            s.apply_at(op, i)?;
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

    /// Applies one operation; answers a JSON object. An error names the
    /// operation, the index it would be recorded at, and its part
    /// ([`ModelError::At`]).
    pub fn apply_value(&mut self, op: &Value) -> Result<Value, ModelError> {
        let index = self.ops.len();
        self.apply_at(op, index)
    }

    fn apply_at(&mut self, op: &Value, index: usize) -> Result<Value, ModelError> {
        let verb = op.get("op").and_then(Value::as_str).unwrap_or("");
        let at = |error: ModelError, part: Option<String>| ModelError::At {
            index,
            op: verb.into(),
            reader: is_reader(verb),
            part,
            error: Box::new(error),
        };
        let o = op.as_object().ok_or_else(|| {
            at(
                ModelError::Program("an operation is an object".into()),
                None,
            )
        })?;
        if verb.is_empty() {
            return Err(at(
                ModelError::Program("an operation has an \"op\" string".into()),
                None,
            ));
        }
        let a = Args { o, op: verb };
        let result = check_fields(o, verb).and_then(|()| self.dispatch(&a));
        match result {
            Ok((answer, recorded)) => {
                if recorded {
                    self.ops.push(op.clone());
                }
                Ok(answer)
            }
            Err(e) => Err(at(e, self.part_of(o))),
        }
    }

    /// The part an operation made or worked on, by name.
    fn part_of(&self, o: &Map<String, Value>) -> Option<String> {
        for k in ["id", "target", "host", "child", "to", "ids"] {
            let first = match o.get(k) {
                Some(Value::Array(l)) => l.first(),
                v => v,
            };
            if let Some(name) = first.and_then(id_of).and_then(|i| self.model.label(i)) {
                return Some(name.into());
            }
        }
        o.get("name").and_then(Value::as_str).map(String::from)
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
            | "extrude" | "revolve" | "sweep" | "loft" | "thread" | "wire" | "body" | "knurl"
            | "gear" => {
                // Everything is read and checked before the part is added,
                // so a failing operation leaves the model as it was.
                let on = match a.opt("on") {
                    Some(_) => {
                        let f = a.frame("on")?;
                        if !f.is_rigid(1e-9) {
                            return Err(a.bad("on", "a frame: finite, orthonormal, right-handed"));
                        }
                        Some(f)
                    }
                    None => None,
                };
                let solid = generate(a)?;
                let i = m.add(a.str("name")?, solid)?;
                if let Some(f) = on {
                    m.at(i, &f)?;
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
            "scale" => {
                let f = |k: &str| -> Result<f64, ModelError> {
                    let v = a.num_or(k, 0.0)?;
                    Ok(if v == 0.0 { 1.0 } else { v })
                };
                m.scale(a.id("id")?, DVec3::new(f("x")?, f("y")?, f("z")?))?;
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
                    profile: profile(a.get("profile")?).map_err(|e| a.inside("profile", e))?,
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
                    ..Default::default()
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
                    ..Default::default()
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
                        only(o, "mimic", &["joint", "multiplier"])?;
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
            other => Err(ModelError::Program(format!(
                "no operation {other:?}; operations: {}",
                OPS.iter().map(|(o, _)| *o).collect::<Vec<_>>().join(", ")
            ))),
        }
    }
}

/// Readers: answered, not recorded.
const READERS: &[&str] = &[
    "frame",
    "offset",
    "translate",
    "turn",
    "compose",
    "bbox",
    "face",
    "edge",
    "local",
    "anchor",
    "placement",
    "grid",
    "ring",
    "along",
    "snap",
    "names",
    "measure",
];

fn is_reader(op: &str) -> bool {
    READERS.contains(&op)
}

/// Every operation and the fields it takes.
const OPS: &[(&str, &[&str])] = &[
    ("frame", &["origin", "normal", "up"]),
    ("offset", &["frame", "distance"]),
    ("translate", &["frame", "x", "y", "z"]),
    ("turn", &["frame", "rx", "ry", "rz"]),
    ("compose", &["frame", "local"]),
    ("bbox", &["ids", "sides"]),
    ("face", &["id", "name"]),
    ("edge", &["id", "name"]),
    ("local", &["id", "frame"]),
    ("anchor", &["id", "name"]),
    ("placement", &["id"]),
    ("grid", &["nx", "ny", "px", "py", "stagger", "on"]),
    ("ring", &["n", "radius", "start", "angle", "on"]),
    ("along", &["path", "pitch", "count"]),
    ("snap", &["ids", "points", "direction"]),
    ("names", &["id"]),
    ("measure", &[]),
    ("units", &["unit"]),
    ("box", &["name", "width", "depth", "height", "on"]),
    (
        "cylinder",
        &["name", "radius", "radiusTop", "height", "segments", "on"],
    ),
    ("cone", &["name", "radius", "height", "segments", "on"]),
    ("sphere", &["name", "radius", "axis", "segments", "on"]),
    (
        "dome",
        &["name", "radius", "height", "axis", "segments", "on"],
    ),
    (
        "tube",
        &["name", "outer", "inner", "height", "segments", "on"],
    ),
    (
        "torus",
        &[
            "name",
            "radius",
            "tube",
            "angle",
            "axis",
            "segments",
            "tubeSegments",
            "on",
        ],
    ),
    ("wedge", &["name", "width", "height", "depth", "on"]),
    (
        "extrude",
        &[
            "name",
            "profile",
            "height",
            "twist",
            "taper",
            "bothSides",
            "on",
        ],
    ),
    (
        "revolve",
        &["name", "profile", "angle", "axis", "segments", "on"],
    ),
    (
        "sweep",
        &["name", "profile", "path", "up", "twist", "scaleEnd", "on"],
    ),
    (
        "loft",
        &["name", "sections", "samples", "steps", "curve", "on"],
    ),
    (
        "thread",
        &["name", "radius", "pitch", "length", "depth", "on"],
    ),
    (
        "wire",
        &[
            "name", "path", "diameter", "closed", "fillet", "fillets", "on",
        ],
    ),
    ("body", &["name", "plan", "top", "bottom", "n", "on"]),
    (
        "knurl",
        &["name", "diameter", "height", "pitch", "straight", "on"],
    ),
    (
        "gear",
        &[
            "name",
            "teeth",
            "module",
            "pressureAngle",
            "width",
            "bore",
            "hub",
            "hubHeight",
            "on",
        ],
    ),
    ("at", &["id", "frame"]),
    ("transform", &["id", "frame"]),
    ("group", &["name", "children"]),
    ("assembly", &["name", "children", "anchors"]),
    ("attach", &["to", "name", "children"]),
    ("copy", &["id", "name"]),
    ("mirror", &["id", "plane", "name"]),
    ("cut", &["target", "tool"]),
    ("join", &["target", "tool"]),
    ("intersect", &["target", "tool"]),
    ("seat", &["host", "part"]),
    ("scale", &["id", "x", "y", "z"]),
    (
        "hole",
        &[
            "id",
            "diameter",
            "at",
            "depth",
            "counterbore",
            "countersink",
        ],
    ),
    ("pocket", &["id", "profile", "depth", "at"]),
    ("split", &["id", "frame", "keep"]),
    ("shell", &["id", "thickness", "open"]),
    ("dent", &["id", "at", "radius", "depth"]),
    ("fillet", &["id", "radius", "edges", "segments", "profile"]),
    (
        "chamfer",
        &["id", "distance", "distance2", "angle", "edges", "segments"],
    ),
    ("soften", &["id", "radius", "angle", "edges", "segments"]),
    (
        "material",
        &[
            "ids",
            "name",
            "color",
            "metalness",
            "roughness",
            "emissive",
            "opacity",
        ],
    ),
    (
        "joint",
        &[
            "child", "parent", "name", "kind", "on", "axis", "range", "mimic",
        ],
    ),
    ("pose", &["joint", "value"]),
];

/// Every field of `o` must be in `accepted`.
fn only(o: &Map<String, Value>, within: &str, accepted: &[&str]) -> Result<(), ModelError> {
    match o.keys().find(|k| !accepted.contains(&k.as_str())) {
        None => Ok(()),
        Some(k) => Err(ModelError::UnknownField {
            within: within.into(),
            field: k.clone(),
            accepted: accepted.iter().map(|s| String::from(*s)).collect(),
        }),
    }
}

/// An operation's own fields (besides `op`) must be the ones it takes.
fn check_fields(o: &Map<String, Value>, op: &str) -> Result<(), ModelError> {
    let Some((_, fields)) = OPS.iter().find(|(name, _)| *name == op) else {
        // `dispatch` names the unknown operation.
        return Ok(());
    };
    match o
        .keys()
        .find(|k| *k != "op" && !fields.contains(&k.as_str()))
    {
        None => Ok(()),
        Some(k) => Err(ModelError::UnknownField {
            within: op.into(),
            field: k.clone(),
            accepted: fields.iter().map(|s| String::from(*s)).collect(),
        }),
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
    let segments = || a.u32_or("segments", 0);
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
            segments: segments()?,
        })?,
        "cone" => g::cone_desc(&ConeDesc {
            radius: a.num("radius")?,
            height: a.num("height")?,
            segments: segments()?,
        })?,
        "sphere" => g::sphere_desc(&SphereDesc {
            radius: a.num("radius")?,
            axis: a.axis()?,
            segments: segments()?,
        })?,
        "dome" => g::dome_desc(&DomeDesc {
            radius: a.num("radius")?,
            height: a.num_or("height", 0.0)?,
            axis: a.axis()?,
            segments: segments()?,
        })?,
        "tube" => g::tube_desc(&TubeDesc {
            outer: a.num("outer")?,
            inner: a.num("inner")?,
            height: a.num("height")?,
            segments: segments()?,
        })?,
        "torus" => g::torus_desc(&TorusDesc {
            radius: a.num("radius")?,
            tube_radius: a.num("tube")?,
            axis: a.axis()?,
            angle: a.num_or("angle", 0.0)?,
            segments: segments()?,
            tube_segments: a.u32_or("tubeSegments", 0)?,
        })?,
        "wedge" => g::wedge(a.num("width")?, a.num("height")?, a.num("depth")?)?,
        "extrude" => g::extrude(
            &profile(a.get("profile")?).map_err(|e| a.inside("profile", e))?,
            &Extrude {
                height: a.num("height")?,
                twist: a.num_or("twist", 0.0)?,
                taper: a.num_or("taper", 0.0)?,
                both_sides: a.bool_or("bothSides", false)?,
            },
        )?,
        "revolve" => g::revolve(
            &profile(a.get("profile")?).map_err(|e| a.inside("profile", e))?,
            &Revolve {
                angle: a.num_or("angle", 0.0)?,
                axis: a.axis()?,
                segments: segments()?,
            },
        )?,
        "sweep" => shape_gen::paths::sweep(
            &profile(a.get("profile")?).map_err(|e| a.inside("profile", e))?,
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
            for (i, sec) in list.iter().enumerate() {
                let within = format!("sections[{i}]");
                let o = sec
                    .as_object()
                    .ok_or_else(|| a.bad(&within, "{profile, z}"))?;
                only(o, &within, &["profile", "z"])?;
                let p = o
                    .get("profile")
                    .ok_or_else(|| a.bad(&format!("{within}.profile"), "missing"))?;
                let z = o
                    .get("z")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| a.bad(&format!("{within}.z"), "a number"))?;
                sections.push((
                    profile(p).map_err(|e| a.inside(&format!("{within}.profile"), e))?,
                    z,
                ));
            }
            let curve = match a.opt("curve").map(|v| v.as_str()) {
                None => LoftCurve::Auto,
                Some(Some("auto")) => LoftCurve::Auto,
                Some(Some("straight")) => LoftCurve::Straight,
                Some(Some("catmullRom")) => LoftCurve::CatmullRom,
                Some(Some("spline")) => LoftCurve::Spline,
                Some(_) => {
                    return Err(a.bad(
                        "curve",
                        "\"auto\", \"straight\", \"catmullRom\" or \"spline\"",
                    ));
                }
            };
            shape_gen::paths::loft(&Loft {
                sections,
                samples: a.u32_or("samples", 0)?,
                steps: a.u32_or("steps", 0)?,
                curve,
            })?
        }
        "thread" => shape_gen::paths::thread(&Thread {
            radius: a.num("radius")?,
            pitch: a.num("pitch")?,
            length: a.num("length")?,
            depth: a.num("depth")?,
        })?,
        "wire" => shape_gen::wire::wire(&Wire {
            path: a.points("path")?,
            diameter: a.num("diameter")?,
            closed: a.bool_or("closed", false)?,
            fillet: a.num_or("fillet", 0.0)?,
            fillets: match a.opt("fillets") {
                Some(_) => a.nums("fillets")?,
                None => Vec::new(),
            },
        })?,
        "body" => shape_gen::body::body(&Body {
            plan: a.view("plan")?,
            top: a.view("top")?,
            bottom: a.view("bottom")?,
            n: match a.opt("n") {
                Some(_) => a.view("n")?,
                None => View::default(),
            },
        })?,
        "knurl" => shape_gen::parts::knurl(&Knurl {
            diameter: a.num("diameter")?,
            height: a.num("height")?,
            pitch: a.num_or("pitch", 0.0)?,
            straight: a.bool_or("straight", false)?,
        })?,
        "gear" => shape_gen::parts::gear_solid(&GearDesc {
            teeth: a.u32_or("teeth", 0)?,
            module: a.num("module")?,
            pressure_angle: a.num_or("pressureAngle", 0.0)?,
            width: a.num("width")?,
            bore: a.num_or("bore", 0.0)?,
            hub: a.num_or("hub", 0.0)?,
            hub_height: a.num_or("hubHeight", 0.0)?,
        })?,
        _ => return Err(ModelError::Program(format!("no generator {:?}", a.op))),
    })
}

/// The fields each profile takes.
const PROFILES: &[(&str, &[&str])] = &[
    ("rect", &["width", "height", "radius"]),
    ("circle", &["radius", "segments"]),
    ("ellipse", &["rx", "ry", "segments"]),
    ("slot", &["length", "width"]),
    ("polygon", &["sides", "radius", "flats"]),
    ("poly", &["points"]),
    ("gear", &["teeth", "module", "pressureAngle"]),
    ("union", &[]),
    ("subtract", &[]),
    ("intersect", &[]),
    ("offset", &["profile", "distance"]),
    ("translate", &["profile", "x", "y"]),
    ("rotate", &["profile", "angle"]),
    ("scale", &["profile", "x", "y"]),
];

/// A profile from its JSON. Errors name the field inside the profile
/// (`rect.width`); the caller says where the profile is.
fn profile(v: &Value) -> Result<Profile, ModelError> {
    let kinds = || {
        PROFILES
            .iter()
            .map(|(k, _)| *k)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let o = v.as_object().filter(|o| o.len() == 1).ok_or_else(|| {
        ModelError::BadField(
            String::new(),
            format!("an object with one key: {}", kinds()),
        )
    })?;
    let (k, v) = o
        .iter()
        .next()
        .ok_or_else(|| ModelError::BadField(String::new(), "empty".into()))?;
    let bad = |key: &str, want: &str| ModelError::BadField(format!("{k}.{key}"), want.into());
    let Some((_, fields)) = PROFILES.iter().find(|(name, _)| name == k) else {
        return Err(ModelError::BadField(
            String::new(),
            format!("no profile {k:?}; profiles: {}", kinds()),
        ));
    };
    if !fields.is_empty() {
        let fo = v
            .as_object()
            .ok_or_else(|| ModelError::BadField(k.clone(), "an object".into()))?;
        only(fo, k, fields)?;
    }
    let f = |key: &str| -> Result<f64, ModelError> {
        match v.get(key) {
            None | Some(Value::Null) => Ok(0.0),
            Some(x) => x.as_f64().ok_or_else(|| bad(key, "a number")),
        }
    };
    let u = |key: &str| -> Result<u32, ModelError> {
        match v.get(key) {
            None | Some(Value::Null) => Ok(0),
            Some(x) => x
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| bad(key, "a whole number")),
        }
    };
    let one = |f: f64| if f == 0.0 { 1.0 } else { f };
    let inner = |key: &str| -> Result<Profile, ModelError> {
        let p = v.get(key).ok_or_else(|| bad(key, "missing"))?;
        profile(p).map_err(|e| nest(&format!("{k}.{key}"), e))
    };
    let list = || {
        v.as_array()
            .filter(|l| !l.is_empty())
            .ok_or_else(|| ModelError::BadField(k.clone(), "a list of profiles".into()))
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
            flats: f("flats")?,
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
                .ok_or_else(|| bad("points", "a list of [x, y] or {x, y, radius, bulge}"))?;
            let mut points = Vec::new();
            for (i, p) in pts.iter().enumerate() {
                let want = "[x, y] or {x, y, radius?, bulge?}";
                let num = |o: &Map<String, Value>, key: &str| -> Result<f64, ModelError> {
                    match o.get(key) {
                        None | Some(Value::Null) => Ok(0.0),
                        Some(x) => x
                            .as_f64()
                            .ok_or_else(|| bad(&format!("points[{i}].{key}"), "a number")),
                    }
                };
                let point = match p {
                    Value::Array(xy) => match xy[..] {
                        [ref x, ref y] => shape_2d::PolyPoint {
                            x: x.as_f64()
                                .ok_or_else(|| bad(&format!("points[{i}]"), want))?,
                            y: y.as_f64()
                                .ok_or_else(|| bad(&format!("points[{i}]"), want))?,
                            ..Default::default()
                        },
                        _ => return Err(bad(&format!("points[{i}]"), want)),
                    },
                    Value::Object(o) => {
                        only(
                            o,
                            &format!("poly.points[{i}]"),
                            &["x", "y", "radius", "bulge"],
                        )?;
                        if !(o.contains_key("x") && o.contains_key("y")) {
                            return Err(bad(&format!("points[{i}]"), want));
                        }
                        shape_2d::PolyPoint {
                            x: num(o, "x")?,
                            y: num(o, "y")?,
                            radius: num(o, "radius")?,
                            bulge: num(o, "bulge")?,
                        }
                    }
                    _ => return Err(bad(&format!("points[{i}]"), want)),
                };
                points.push(point);
            }
            shape_2d::poly(&shape_2d::Poly { points })?
        }
        "union" | "subtract" | "intersect" => {
            let l = list()?;
            let item = |i: usize| profile(&l[i]).map_err(|e| nest(&format!("{k}[{i}]"), e));
            let mut acc = item(0)?;
            for i in 1..l.len() {
                let q = item(i)?;
                acc = match k.as_str() {
                    "union" => acc.union(&q),
                    "subtract" => acc.subtract(&q),
                    _ => acc.intersect(&q),
                };
            }
            acc
        }
        "offset" => inner("profile")?.offset(f("distance")?, shape_2d::Join::Round)?,
        "translate" => inner("profile")?.translate(DVec2::new(f("x")?, f("y")?)),
        "rotate" => inner("profile")?.rotate(f("angle")?),
        "scale" => {
            let (x, y) = (one(f("x")?), one(f("y")?));
            if !(x.is_finite() && y.is_finite() && x > 0.0 && y > 0.0) {
                return Err(bad(
                    "x, y",
                    "positive factors (0 keeps 1); mirror with a negative is not taken",
                ));
            }
            inner("profile")?.scale(x, y)
        }
        _ => unreachable!("every profile kind is matched"),
    })
}

/// `e` from inside the field `at`: field names get `at.` in front.
fn nest(at: &str, e: ModelError) -> ModelError {
    match e {
        ModelError::BadField(f, w) if f.is_empty() => ModelError::BadField(at.into(), w),
        ModelError::BadField(f, w) => ModelError::BadField(format!("{at}.{f}"), w),
        ModelError::UnknownField {
            within,
            field,
            accepted,
        } => ModelError::UnknownField {
            within: format!("{at}: {within}"),
            field,
            accepted,
        },
        ModelError::Profile(p) => ModelError::BadField(at.into(), p.to_string()),
        e => e,
    }
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
    let o = v.as_object().ok_or("a frame is {origin, x, y, z}")?;
    if let Some(k) = o
        .keys()
        .find(|k| !["origin", "x", "y", "z"].contains(&k.as_str()))
    {
        return Err(format!(
            "a frame has no field {k:?}; it takes: origin, x, y, z"
        ));
    }
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
        ModelError::BadField(field.into(), want.into())
    }

    /// An error from inside the field `k` (a profile): named under it.
    fn inside(&self, k: &str, e: ModelError) -> ModelError {
        nest(k, e)
    }

    /// A turned solid's axis: `"+Y"` (the default) or `"+Z"`.
    fn axis(&self) -> Result<Axis, ModelError> {
        match self.opt("axis").map(Value::as_str) {
            None | Some(Some("+Y")) => Ok(Axis::Y),
            Some(Some("+Z")) => Ok(Axis::Z),
            Some(_) => Err(self.bad("axis", "\"+Y\" or \"+Z\"")),
        }
    }

    /// A [`View`] of a body: a number, a list of `[y, value]` points
    /// (straight between them), or `{points, smooth}`.
    fn view(&self, k: &str) -> Result<View, ModelError> {
        let want = "a number, [[y, value], …] or {points, smooth}";
        let pairs = |l: &Vec<Value>| -> Result<View, ModelError> {
            l.iter()
                .map(|p| match p.as_array().map(|xy| &xy[..]) {
                    Some([y, v]) => Some(DVec2::new(y.as_f64()?, v.as_f64()?)),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()
                .map(|points| View {
                    points,
                    ..Default::default()
                })
                .ok_or_else(|| self.bad(k, want))
        };
        match self.get(k)? {
            Value::Number(n) => Ok(View::constant(n.as_f64().unwrap_or(f64::NAN))),
            Value::Array(l) => pairs(l),
            Value::Object(o) => {
                only(o, k, &["points", "smooth"])?;
                let mut v = pairs(
                    o.get("points")
                        .and_then(Value::as_array)
                        .ok_or_else(|| self.bad(&format!("{k}.points"), "[[y, value], …]"))?,
                )?;
                v.smooth = match o.get("smooth") {
                    None | Some(Value::Null) => false,
                    Some(b) => b
                        .as_bool()
                        .ok_or_else(|| self.bad(&format!("{k}.smooth"), "true or false"))?,
                };
                Ok(v)
            }
            _ => Err(self.bad(k, want)),
        }
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
        if let Some(o) = v.as_object() {
            only(o, k, &["points", "closed"])?;
        }
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
