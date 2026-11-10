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
//! | `uvs` | | |
//! | `box` | `name`, `width`, `depth`, `height`, `on`? | `id` |
//! | `cylinder` | `name`, `radius`, `radiusTop`?, `height`, `segments`?, `on`? | `id` |
//! | `cone` | `name`, `radius`, `height`, `segments`?, `on`? | `id` |
//! | `sphere` | `name`, `radius`, `axis`?, `segments`?, `on`? | `id` |
//! | `dome` | `name`, `radius`, `height`?, `axis`?, `segments`?, `on`? | `id` |
//! | `tube` | `name`, `outer`, `inner`, `height`, `segments`?, `on`? | `id` |
//! | `torus` | `name`, `radius`, `tube`, `angle`?, `axis`?, `segments`?, `tubeSegments`?, `on`? | `id` |
//! | `wedge` | `name`, `width`, `height`, `depth`, `on`? | `id` |
//! | `extrude` | `name`, `profile`, `height` or `until` (`{part, face}`), `twist`?, `taper`?, `bothSides`?, `on`? | `id` |
//! | `revolve` | `name`, `profile` (x the radius), `angle`?, `axis`?, `segments`?, `on`? | `id` |
//! | `sweep` | `name`, `profile`, `path` (a path), `up`?, `twist`?, `scaleEnd`?, `on`? | `id` |
//! | `loft` | `name`, `sections`: `[{profile, z}]`, `samples`?, `steps`?, `curve`? (`"auto"`, `"straight"`, `"catmullRom"`, `"spline"`), `on`? | `id` |
//! | `thread` | `name`, `radius`, `pitch`, `length`, `depth`, `on`? | `id` |
//! | `wire` | `name`, `path` (a path), `diameter`, `closed`?, `fillet`?, `fillets`? (points-only paths), `segments`?, `on`? | `id` |
//! | `body` | `name`, `plan`, `top`, `bottom`, `n`? (views: a number, `[[y, v]…]`, or `{points, smooth}`), `on`? | `id` |
//! | `knurl` | `name`, `diameter`, `height`, `pitch`?, `straight`?, `on`? | `id` |
//! | `gear` | `name`, `teeth`, `module`, `width`, `pressureAngle`?, `bore`?, `hub`?, `hubHeight`?, `on`? | `id` |
//! | `at`, `transform` | `id`, `frame` | |
//! | `group` | `name`, `children` | `id` |
//! | `assembly` | `name`, `children`, `anchors`? (`{name: frame}`) | `id` |
//! | `attach` | `to`, `name`, `children` | `id` |
//! | `copy` | `id`, `name` | `id` |
//! | `mirror` | `id`, `plane` (a frame), `name` | `id` |
//! | `cut`, `join`, `intersect` | `target`, `tool`, `check`? | `voids`, `touchingEdges`, `touching`, `facesWithoutUvs` |
//! | `merge` | `target`, `tool` | |
//! | `delete` | `id` | |
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
//! | `material` | `ids`, `name`?, `color`?, `metalness`?, `roughness`?, `emissive`?, `opacity`?, `map`?, `mapSize`? | |
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
//! `rotate` (`profile`, `angle`), `scale` (`profile`, `x`, `y`; 0 keeps 1),
//! `superellipse` (`rx`, `ry`, `exponent`, `segments`), `spline`
//! (`points`, closed), `path` (a 2D path, closed), `stroke` (`path`,
//! `width`, `join`, `roundCaps`, `closed`), `hull` (a list), `mirror`
//! (`profile`, `axis`: `"y"` or `"x"`), `text` (`text`, `font`, `size`,
//! `spacing`).
//!
//! A path is a list: a start point, then pieces from where the last
//! ended: a point (a line), `{"arc": {"through", "to"}}`,
//! `{"tangentArc": {"to"}}`, `{"spline": {"points"}}`, flattened by the
//! curve rule (`shape_2d::curve`, [`shape_gen::path3`]). In 3D (`sweep`,
//! `wire`) it may instead be `{"helix": {"radius", "pitch", "turns"}}`.
//!
//! Every generator also takes `uvs`? (a boolean; the `uvs` operation's
//! choice by default): the part's solid gets UVs in the model's unit
//! (`shape_gen::solids::textured`). `uvs` asks for them on every part
//! made after it, and is refused once a part without them exists. A
//! material's `map` names an image the caller supplies
//! ([`Session::texture`], [`textures_named`]); `mapSize` is the units it
//! spans. A part with UVs adds `uv` to the measures.
//!
//! A `text` profile names a font; the caller supplies its bytes under that
//! name ([`Session::font`], [`run_with`]); [`fonts_named`] lists the names
//! a program uses. No font ships with the kernel.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};
use shape_2d::Profile;
use shape_bevel::{Chamfer, Fillet, Soften};
use shape_bool::BoolOp;
use shape_core::{DVec2, DVec3, Frame};
use shape_edit::{Dent, Hole, Keep, Pocket, Shell};
use shape_gen::body::{Body, View};
use shape_gen::loft::LoftCurve;
use shape_gen::parts::{GearDesc, Knurl};
use shape_gen::path3::{Flat, Path3, Piece};
use shape_gen::paths::{Helix, Loft, Sweep, Thread};
use shape_gen::solids::{
    Axis, BoxDesc, ConeDesc, CylinderDesc, DomeDesc, Extrude, Revolve, SphereDesc, TorusDesc,
    TubeDesc,
};
use shape_gen::wire::Wire;
use shape_tools::{JointKind, Material, Mimic};

use crate::error::ModelError;
use crate::model::{Combine, Id, JointDesc, Model, Unit};
use crate::place::{self, Path};

/// A model and the program that built it.
#[derive(Clone, Debug, Default)]
pub struct Session {
    model: Model,
    ops: Vec<Value>,
    fonts: Fonts,
    textures: Fonts,
}

/// Fonts a caller supplies, by the name `text` profiles give them: a
/// font file's bytes (TrueType or OpenType).
pub type Fonts = BTreeMap<String, Vec<u8>>;

/// Replays a program (`{"name": …, "ops": [...]}`) into a model.
pub fn run(program: &str) -> Result<Model, ModelError> {
    Ok(Session::replay(program)?.model)
}

/// [`run`], with the fonts its `text` profiles name ([`fonts_named`]).
pub fn run_with(program: &str, fonts: &Fonts) -> Result<Model, ModelError> {
    Ok(Session::replay_with(program, fonts)?.model)
}

/// The font names a program's `text` profiles give, sorted, each once:
/// what a caller reads (the CLI and MCP read each as a path relative to
/// the program) and passes to [`run_with`].
pub fn fonts_named(program: &str) -> Result<Vec<String>, ModelError> {
    let v: Value =
        serde_json::from_str(program).map_err(|e| ModelError::Program(format!("not JSON: {e}")))?;
    fn walk(v: &Value, out: &mut std::collections::BTreeSet<String>) {
        match v {
            Value::Object(o) => {
                if let Some(f) = o
                    .get("text")
                    .and_then(|t| t.get("font"))
                    .and_then(Value::as_str)
                {
                    out.insert(f.to_string());
                }
                o.values().for_each(|x| walk(x, out));
            }
            Value::Array(l) => l.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = std::collections::BTreeSet::new();
    walk(v.get("ops").unwrap_or(&Value::Null), &mut out);
    Ok(out.into_iter().collect())
}

/// [`run_with`], with the textures its materials' `map`s name
/// ([`textures_named`]).
pub fn run_files(program: &str, fonts: &Fonts, textures: &Fonts) -> Result<Model, ModelError> {
    Ok(Session::replay_files(program, fonts, textures)?.model)
}

/// The texture names a program's materials give (`map`), sorted, each
/// once: what a caller reads (the CLI and MCP read each as a path
/// relative to the program) and passes to [`run_files`].
pub fn textures_named(program: &str) -> Result<Vec<String>, ModelError> {
    let v: Value =
        serde_json::from_str(program).map_err(|e| ModelError::Program(format!("not JSON: {e}")))?;
    let mut out = std::collections::BTreeSet::new();
    for op in v.get("ops").and_then(Value::as_array).into_iter().flatten() {
        if op.get("op").and_then(Value::as_str) == Some("material")
            && let Some(f) = op.get("map").and_then(Value::as_str)
        {
            out.insert(f.to_string());
        }
    }
    Ok(out.into_iter().collect())
}

impl Session {
    /// An empty model named `name`.
    pub fn new(name: &str) -> Session {
        Session {
            model: Model::new(name),
            ops: Vec::new(),
            fonts: Fonts::new(),
            textures: Fonts::new(),
        }
    }

    /// Supplies a font under `name`, the name `text` profiles give it.
    /// The bytes are read when a profile uses them; the program records
    /// the name only.
    pub fn font(&mut self, name: &str, bytes: Vec<u8>) {
        self.fonts.insert(name.to_string(), bytes);
    }

    /// The fonts supplied so far.
    pub fn fonts(&self) -> &Fonts {
        &self.fonts
    }

    /// Supplies an image (PNG or JPEG) under `name`, the name a
    /// material's `map` gives it. The program records the name only.
    pub fn texture(&mut self, name: &str, bytes: Vec<u8>) {
        self.textures.insert(name.to_string(), bytes);
    }

    /// The textures supplied so far.
    pub fn textures(&self) -> &Fonts {
        &self.textures
    }

    /// [`Session::replay_with`], with textures too ([`Session::texture`]).
    pub fn replay_files(
        program: &str,
        fonts: &Fonts,
        textures: &Fonts,
    ) -> Result<Session, ModelError> {
        let v: Value = serde_json::from_str(program)
            .map_err(|e| ModelError::Program(format!("not JSON: {e}")))?;
        let name = v.get("name").and_then(Value::as_str).unwrap_or("model");
        let ops = v
            .get("ops")
            .and_then(Value::as_array)
            .ok_or_else(|| ModelError::Program("no \"ops\" list".into()))?;
        let mut s = Session::new(name);
        s.fonts = fonts.clone();
        s.textures = textures.clone();
        for (i, op) in ops.iter().enumerate() {
            s.apply_at(op, i)?;
        }
        Ok(s)
    }

    /// Replays a program into a session that goes on recording.
    pub fn replay(program: &str) -> Result<Session, ModelError> {
        Session::replay_with(program, &Fonts::new())
    }

    /// [`Session::replay`] with fonts supplied ([`Session::font`]).
    pub fn replay_with(program: &str, fonts: &Fonts) -> Result<Session, ModelError> {
        Session::replay_files(program, fonts, &Fonts::new())
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
        let a = Args {
            o,
            op: verb,
            fonts: &self.fonts,
            textures: &self.textures,
        };
        let result = check_fields(o, verb).and_then(|()| dispatch(&mut self.model, &a));
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
}

fn dispatch(m: &mut Model, a: &Args) -> Result<(Value, bool), ModelError> {
    {
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
            "measure" => match a.opt("posed") {
                None | Some(Value::Bool(false)) => read(measures_value(m)?),
                Some(Value::Bool(true)) => read(posed_value(m)?),
                Some(_) => Err(a.bad("posed", "true or false")),
            },
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
            "uvs" => {
                m.ask_uvs()?;
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
                let uvs = match a.opt("uvs") {
                    None => m.wants_uvs(),
                    Some(_) => a.bool_or("uvs", false)?,
                };
                let solid = if a.opt("until").is_some() {
                    extrude_until(m, a, &on.unwrap_or(Frame::IDENTITY), uvs)?
                } else {
                    generate(a, uvs)?
                };
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
            "cut" | "join" | "intersect" => {
                let op = match a.op {
                    "cut" => BoolOp::Cut,
                    "join" => BoolOp::Join,
                    _ => BoolOp::Intersect,
                };
                let r = m.combine(
                    a.id("target")?,
                    a.id("tool")?,
                    &Combine {
                        op,
                        check: a.bool_or("check", false)?,
                    },
                )?;
                Ok((
                    json!({
                        "voids": r.voids,
                        "touchingEdges": r.touching_edges,
                        "touching": r.touching,
                        "facesWithoutUvs": r.faces_without_uvs,
                    }),
                    true,
                ))
            }
            "merge" => {
                m.merge(a.id("target")?, a.id("tool")?)?;
                Ok((none(), true))
            }
            "delete" => {
                m.delete(a.id("id")?)?;
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
                    profile: a.profile("profile")?,
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
                    map: match a.opt("map") {
                        None => None,
                        Some(_) => Some(a.texture("map")?),
                    },
                };
                if a.opt("mapSize").is_some() && mat.map.is_none() {
                    return Err(a.bad("mapSize", "with a `map` only"));
                }
                m.material_mapped(&a.ids("ids")?, &mat, a.num_or("mapSize", 1.0)?)?;
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
    ("measure", &["posed"]),
    ("units", &["unit"]),
    ("uvs", &[]),
    ("box", &["name", "width", "depth", "height", "uvs", "on"]),
    (
        "cylinder",
        &[
            "name",
            "radius",
            "radiusTop",
            "height",
            "segments",
            "uvs",
            "on",
        ],
    ),
    (
        "cone",
        &["name", "radius", "height", "segments", "uvs", "on"],
    ),
    (
        "sphere",
        &["name", "radius", "axis", "segments", "uvs", "on"],
    ),
    (
        "dome",
        &["name", "radius", "height", "axis", "segments", "uvs", "on"],
    ),
    (
        "tube",
        &["name", "outer", "inner", "height", "segments", "uvs", "on"],
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
            "uvs",
            "on",
        ],
    ),
    ("wedge", &["name", "width", "height", "depth", "uvs", "on"]),
    (
        "extrude",
        &[
            "name",
            "profile",
            "height",
            "twist",
            "taper",
            "bothSides",
            "until",
            "uvs",
            "on",
        ],
    ),
    (
        "revolve",
        &["name", "profile", "angle", "axis", "segments", "uvs", "on"],
    ),
    (
        "sweep",
        &[
            "name", "profile", "path", "up", "twist", "scaleEnd", "uvs", "on",
        ],
    ),
    (
        "loft",
        &["name", "sections", "samples", "steps", "curve", "uvs", "on"],
    ),
    (
        "thread",
        &["name", "radius", "pitch", "length", "depth", "uvs", "on"],
    ),
    (
        "wire",
        &[
            "name", "path", "diameter", "closed", "fillet", "fillets", "segments", "uvs", "on",
        ],
    ),
    ("body", &["name", "plan", "top", "bottom", "n", "uvs", "on"]),
    (
        "knurl",
        &[
            "name", "diameter", "height", "pitch", "straight", "uvs", "on",
        ],
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
            "uvs",
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
    ("cut", &["target", "tool", "check"]),
    ("join", &["target", "tool", "check"]),
    ("intersect", &["target", "tool", "check"]),
    ("merge", &["target", "tool"]),
    ("delete", &["id"]),
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
            "map",
            "mapSize",
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

/// The generator's solid, with UVs (`shape_gen::solids::textured`) when
/// `uvs`.
fn generate(a: &Args, uvs: bool) -> Result<shape_mesh::Solid, ModelError> {
    use shape_gen::solids as g;
    use shape_gen::solids::Generator as G;
    let segments = || a.u32_or("segments", 0);
    // Either the generator alone or `textured` of it.
    macro_rules! pick {
        ($gen:expr, $plain:expr) => {
            if uvs { g::textured(&$gen)? } else { $plain? }
        };
    }
    Ok(match a.op {
        "box" => {
            let d = BoxDesc {
                width: a.num("width")?,
                depth: a.num("depth")?,
                height: a.num("height")?,
            };
            pick!(G::Box(d), g::cuboid(&d))
        }
        "cylinder" => {
            let d = CylinderDesc {
                radius: a.num("radius")?,
                radius_top: a.num_or("radiusTop", 0.0)?,
                height: a.num("height")?,
                segments: segments()?,
            };
            pick!(G::Cylinder(d), g::cylinder(&d))
        }
        "cone" => {
            let d = ConeDesc {
                radius: a.num("radius")?,
                height: a.num("height")?,
                segments: segments()?,
            };
            pick!(G::Cone(d), g::cone_desc(&d))
        }
        "sphere" => {
            let d = SphereDesc {
                radius: a.num("radius")?,
                axis: a.axis()?,
                segments: segments()?,
            };
            pick!(G::Sphere(d), g::sphere_desc(&d))
        }
        "dome" => {
            let d = DomeDesc {
                radius: a.num("radius")?,
                height: a.num_or("height", 0.0)?,
                axis: a.axis()?,
                segments: segments()?,
            };
            pick!(G::Dome(d), g::dome_desc(&d))
        }
        "tube" => {
            let d = TubeDesc {
                outer: a.num("outer")?,
                inner: a.num("inner")?,
                height: a.num("height")?,
                segments: segments()?,
            };
            pick!(G::Tube(d), g::tube_desc(&d))
        }
        "torus" => {
            let d = TorusDesc {
                radius: a.num("radius")?,
                tube_radius: a.num("tube")?,
                axis: a.axis()?,
                angle: a.num_or("angle", 0.0)?,
                segments: segments()?,
                tube_segments: a.u32_or("tubeSegments", 0)?,
            };
            pick!(G::Torus(d), g::torus_desc(&d))
        }
        "wedge" => {
            let (w, h, d) = (a.num("width")?, a.num("height")?, a.num("depth")?);
            pick!(G::Wedge(w, h, d), g::wedge(w, h, d))
        }
        "extrude" => {
            let p = a.profile("profile")?;
            let d = Extrude {
                height: a.num("height")?,
                twist: a.num_or("twist", 0.0)?,
                taper: a.num_or("taper", 0.0)?,
                both_sides: a.bool_or("bothSides", false)?,
            };
            pick!(G::Extrude(&p, d), g::extrude(&p, &d))
        }
        "revolve" => {
            let p = a.profile("profile")?;
            let d = Revolve {
                angle: a.num_or("angle", 0.0)?,
                axis: a.axis()?,
                segments: segments()?,
            };
            pick!(G::Revolve(&p, d), g::revolve(&p, &d))
        }
        "sweep" => {
            let p = a.profile("profile")?;
            let d = Sweep {
                path: a.route("path")?.points,
                up: match a.opt("up") {
                    Some(_) => a.vec3("up")?,
                    None => DVec3::ZERO,
                },
                twist: a.num_or("twist", 0.0)?,
                scale_end: a.num_or("scaleEnd", 0.0)?,
            };
            pick!(G::Sweep(&p, &d), shape_gen::paths::sweep(&p, &d))
        }
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
                    profile(p, a.fonts).map_err(|e| a.inside(&format!("{within}.profile"), e))?,
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
            crate::checked::loft_with(
                &Loft {
                    sections,
                    samples: a.u32_or("samples", 0)?,
                    steps: a.u32_or("steps", 0)?,
                    curve,
                },
                uvs,
            )?
        }
        "thread" => {
            let d = Thread {
                radius: a.num("radius")?,
                pitch: a.num("pitch")?,
                length: a.num("length")?,
                depth: a.num("depth")?,
            };
            pick!(G::Thread(d), shape_gen::paths::thread(&d))
        }
        "wire" => {
            let route = a.route("path")?;
            let fillet = a.num_or("fillet", 0.0)?;
            let curved = route.corner.iter().any(|c| !c);
            let fillets = match a.opt("fillets") {
                Some(_) if curved => {
                    return Err(a.bad(
                        "fillets",
                        "one radius a point, so a path of points only; with arcs, splines or a \
                         helix give `fillet`, which bends the corners between pieces",
                    ));
                }
                Some(_) => a.nums("fillets")?,
                // Inside a curve the path is already round: no bend there.
                None if curved && fillet != 0.0 => route
                    .corner
                    .iter()
                    .map(|&c| if c { fillet } else { 0.0 })
                    .collect(),
                None => Vec::new(),
            };
            let d = Wire {
                path: route.points,
                diameter: a.num("diameter")?,
                closed: a.bool_or("closed", false)?,
                fillet: if fillets.is_empty() { fillet } else { 0.0 },
                fillets,
                segments: segments()?,
            };
            pick!(G::Wire(&d), shape_gen::wire::wire(&d))
        }
        "body" => {
            let d = Body {
                plan: a.view("plan")?,
                top: a.view("top")?,
                bottom: a.view("bottom")?,
                n: match a.opt("n") {
                    Some(_) => a.view("n")?,
                    None => View::default(),
                },
            };
            pick!(G::Body(&d), shape_gen::body::body(&d))
        }
        "knurl" => {
            let d = Knurl {
                diameter: a.num("diameter")?,
                height: a.num("height")?,
                pitch: a.num_or("pitch", 0.0)?,
                straight: a.bool_or("straight", false)?,
            };
            pick!(G::Knurl(d), shape_gen::parts::knurl(&d))
        }
        "gear" => {
            let d = GearDesc {
                teeth: a.u32_or("teeth", 0)?,
                module: a.num("module")?,
                pressure_angle: a.num_or("pressureAngle", 0.0)?,
                width: a.num("width")?,
                bore: a.num_or("bore", 0.0)?,
                hub: a.num_or("hub", 0.0)?,
                hub_height: a.num_or("hubHeight", 0.0)?,
            };
            pick!(G::Gear(d), shape_gen::parts::gear_solid(&d))
        }
        _ => return Err(ModelError::Program(format!("no generator {:?}", a.op))),
    })
}

/// An extrusion up to a face of a part (`until: {part, face}`): its
/// height runs from the profile's plane to the face's plane along the
/// extrusion's +z. A face parallel to the profile gives a flat top at
/// that height; a tilted one, a top cut along its plane (named `top`,
/// its edges `top` too). `on` is the extrusion's frame. Refused: `height`
/// with it, or `twist`, `taper`, `bothSides`; a face that is not flat; a
/// plane the extrusion would meet at or below its profile.
fn extrude_until(
    m: &Model,
    a: &Args,
    on: &Frame,
    uvs: bool,
) -> Result<shape_mesh::Solid, ModelError> {
    for k in ["height", "twist", "taper", "bothSides"] {
        if a.opt(k).is_some() {
            return Err(a.bad(k, "not with `until`, which sets the height"));
        }
    }
    let u = a.get("until")?;
    let o = u
        .as_object()
        .ok_or_else(|| a.bad("until", "{part, face}"))?;
    only(o, "until", &["part", "face"])?;
    let part = o
        .get("part")
        .and_then(id_of)
        .ok_or_else(|| a.bad("until.part", "a part's id"))?;
    let face = o
        .get("face")
        .and_then(Value::as_str)
        .ok_or_else(|| a.bad("until.face", "a face name"))?;
    let plane = on.inverse().compose(&m.face(part, face)?);
    let (q, n) = (plane.origin, plane.z);
    let prof = a.profile("profile")?;
    if n.z == 0.0 {
        return Err(a.bad(
            "until",
            "a face the extrusion meets: this one is parallel to the extrusion's direction",
        ));
    }
    // Where each profile point's line along +z meets the plane.
    let t = |p: DVec2| (q - DVec3::new(p.x, p.y, 0.0)).dot(n) / n.z;
    let heights: Vec<f64> = prof
        .contours()
        .flat_map(|c| &c.points)
        .map(|&p| t(p))
        .collect();
    let lo = heights.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = heights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !(lo > 0.0 && hi.is_finite()) {
        return Err(a.bad(
            "until",
            &format!(
                "a face whose plane lies above the whole profile; it meets the extrusion {lo:.4} \
                 from the profile's plane"
            ),
        ));
    }
    let flat = n.x == 0.0 && n.y == 0.0;
    let height = if flat { q.z } else { hi + hi.max(1.0) };
    let d = shape_gen::solids::Extrude {
        height,
        ..Default::default()
    };
    // With UVs, the tilted top (the split's cut) is planar in its own
    // plane's x and y (`shape_edit::split`).
    let s = if uvs {
        shape_gen::solids::textured(&shape_gen::solids::Generator::Extrude(&prof, d))?
    } else {
        shape_gen::solids::extrude(&prof, &d)?
    };
    if flat {
        return Ok(s);
    }
    // Keep the side of the plane the profile is on.
    let cut = Frame::facing(q, n).ok_or_else(|| a.bad("until", "a face with a normal"))?;
    let p0 = prof.regions[0].outer.points[0];
    let keep = if (DVec3::new(p0.x, p0.y, 0.0) - q).dot(n) < 0.0 {
        shape_edit::Keep::Below
    } else {
        shape_edit::Keep::Above
    };
    let mut s = shape_edit::split(&s, &cut, keep)?;
    let top = s.names.intern("top");
    let made: Vec<u32> = s
        .names
        .all()
        .filter(|x| x.starts_with("split."))
        .filter_map(|x| s.names.get(x))
        .collect();
    for tags in &mut s.face_tags {
        if tags.iter().any(|x| made.contains(x)) && !tags.contains(&top) {
            tags.push(top);
            tags.sort_unstable();
        }
    }
    if let Some(c) = s.names.get("cut") {
        for tags in s.edge_tags.values_mut() {
            if tags.contains(&c) && !tags.contains(&top) {
                tags.push(top);
                tags.sort_unstable();
            }
        }
    }
    s.canonicalize()
        .map_err(|_| ModelError::BadValue("the extrusion's positions".into()))?;
    Ok(s)
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
    ("superellipse", &["rx", "ry", "exponent", "segments"]),
    ("spline", &["points"]),
    ("path", &[]),
    ("stroke", &["path", "width", "join", "roundCaps", "closed"]),
    ("hull", &[]),
    ("mirror", &["profile", "axis"]),
    ("text", &["text", "font", "size", "spacing"]),
];

/// A 2D point `[x, y]`.
fn xy(v: &Value) -> Option<DVec2> {
    match v.as_array().map(|l| &l[..]) {
        Some([x, y]) => Some(DVec2::new(x.as_f64()?, y.as_f64()?)),
        _ => None,
    }
}

/// A path's pieces before they are built, in `D` dimensions.
enum RawPiece<P> {
    Line(P),
    Arc(P, P),
    TangentArc(P),
    Spline(Vec<P>),
}

/// What a path list holds, for the errors.
const PATH_WANT: &str = "a list: a start point, then points (straight to them), \
    {\"arc\": {\"through\", \"to\"}}, {\"tangentArc\": {\"to\"}} or \
    {\"spline\": {\"points\"}}";

/// A path list: its start and its pieces, points read by `point`
/// (`[x, y]` or `[x, y, z]`). Errors name `at[i]`.
fn raw_path<P>(
    v: &Value,
    at: &str,
    point: impl Fn(&Value) -> Option<P>,
    want: &str,
) -> Result<(P, Vec<RawPiece<P>>), ModelError> {
    let bad = |w: &str, what: &str| ModelError::BadField(w.into(), what.into());
    let l = v.as_array().ok_or_else(|| bad(at, PATH_WANT))?;
    let (first, rest) = l
        .split_first()
        .ok_or_else(|| bad(at, "at least a start point"))?;
    let start =
        point(first).ok_or_else(|| bad(&format!("{at}[0]"), &format!("the start, {want}")))?;
    let mut pieces = Vec::new();
    for (j, item) in rest.iter().enumerate() {
        let w = format!("{at}[{}]", j + 1);
        let pt = |o: &Map<String, Value>, k: &str| {
            o.get(k)
                .and_then(&point)
                .ok_or_else(|| bad(&format!("{w}.{k}"), want))
        };
        let piece = match item {
            Value::Array(_) => RawPiece::Line(point(item).ok_or_else(|| bad(&w, want))?),
            Value::Object(o) if o.len() == 1 => {
                let Some((k, body)) = o.iter().next() else {
                    return Err(bad(&w, PATH_WANT));
                };
                let fields: &[&str] = match k.as_str() {
                    "arc" => &["through", "to"],
                    "tangentArc" => &["to"],
                    "spline" => &["points"],
                    _ => return Err(bad(&w, PATH_WANT)),
                };
                let b = body
                    .as_object()
                    .ok_or_else(|| bad(&format!("{w}.{k}"), "an object"))?;
                only(b, &format!("{w}.{k}"), fields)?;
                let w = format!("{w}.{k}");
                match k.as_str() {
                    "arc" => RawPiece::Arc(pt(b, "through")?, pt(b, "to")?),
                    "tangentArc" => RawPiece::TangentArc(pt(b, "to")?),
                    _ => RawPiece::Spline(
                        b.get("points")
                            .and_then(Value::as_array)
                            .filter(|l| !l.is_empty())
                            .and_then(|l| l.iter().map(&point).collect::<Option<Vec<_>>>())
                            .ok_or_else(|| {
                                bad(&format!("{w}.points"), &format!("a list of {want}"))
                            })?,
                    ),
                }
            }
            _ => return Err(bad(&w, PATH_WANT)),
        };
        pieces.push(piece);
    }
    Ok((start, pieces))
}

/// A 2D path list as a [`shape_2d::Path`] (not closed). A three-point arc
/// on one line and a tangent arc with no direction before it are refused.
fn path2(v: &Value, at: &str) -> Result<shape_2d::Path, ModelError> {
    let (start, pieces) = raw_path(v, at, xy, "[x, y]")?;
    let mut p = shape_2d::Path::new(start.x, start.y);
    for (j, piece) in pieces.into_iter().enumerate() {
        let w = format!("{at}[{}]", j + 1);
        let last = *p.points().last().unwrap_or(&start);
        p = match piece {
            RawPiece::Line(q) => p.line_to(q.x, q.y),
            RawPiece::Arc(t, q) => {
                if shape_core::predicates::orient2d(last, t, q).is_eq() {
                    return Err(ModelError::BadField(
                        format!("{w}.arc"),
                        "three points not on one line (the arc's start, through, to)".into(),
                    ));
                }
                p.arc_to(t.x, t.y, q.x, q.y)
            }
            RawPiece::TangentArc(q) => {
                if p.heading().is_none() {
                    return Err(ModelError::BadField(
                        format!("{w}.tangentArc"),
                        "a piece before it, to leave tangent to".into(),
                    ));
                }
                p.tangent_arc_to(q.x, q.y)
            }
            RawPiece::Spline(l) => p.spline_through(&l),
        };
    }
    Ok(p)
}

/// A 3D path: a list of points and pieces (flattened by the curve rule),
/// or `{"helix": {"radius", "pitch", "turns"}}` about +z from
/// `(radius, 0, 0)`.
fn path3(v: &Value, at: &str) -> Result<Flat, ModelError> {
    if let Some(o) = v.as_object() {
        only(o, at, &["helix"])?;
        let h = o.get("helix").and_then(Value::as_object).ok_or_else(|| {
            ModelError::BadField(at.into(), "{\"helix\": {radius, pitch, turns}}".into())
        })?;
        only(h, &format!("{at}.helix"), &["radius", "pitch", "turns"])?;
        let n = |k: &str| {
            h.get(k)
                .and_then(Value::as_f64)
                .ok_or_else(|| ModelError::BadField(format!("{at}.helix.{k}"), "a number".into()))
        };
        return Ok(shape_gen::path3::helix(&Helix {
            radius: n("radius")?,
            pitch: n("pitch")?,
            turns: n("turns")?,
        })?);
    }
    let (start, pieces) = raw_path(v, at, vec3_of, "[x, y, z]")?;
    let pieces = pieces
        .into_iter()
        .map(|p| match p {
            RawPiece::Line(q) => Piece::Line(q),
            RawPiece::Arc(through, to) => Piece::Arc { through, to },
            RawPiece::TangentArc(q) => Piece::TangentArc(q),
            RawPiece::Spline(l) => Piece::Spline(l),
        })
        .collect();
    Ok(shape_gen::path3::flatten(&Path3 { start, pieces })?)
}

/// A profile from its JSON. Errors name the field inside the profile
/// (`rect.width`); the caller says where the profile is.
fn profile(v: &Value, fonts: &Fonts) -> Result<Profile, ModelError> {
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
    // A segment count: 0 for the chord rule, else at least `least`.
    let count = |key: &str, least: u32| -> Result<u32, ModelError> {
        let n = u(key)?;
        if n != 0 && n < least {
            return Err(bad(key, &format!("0 (the chord rule) or at least {least}")));
        }
        Ok(n)
    };
    let one = |f: f64| if f == 0.0 { 1.0 } else { f };
    let inner = |key: &str| -> Result<Profile, ModelError> {
        let p = v.get(key).ok_or_else(|| bad(key, "missing"))?;
        profile(p, fonts).map_err(|e| nest(&format!("{k}.{key}"), e))
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
            segments: count("segments", 3)?,
        })?,
        "ellipse" => shape_2d::ellipse(&shape_2d::Ellipse {
            rx: f("rx")?,
            ry: f("ry")?,
            segments: count("segments", 3)?,
        })?,
        "superellipse" => shape_2d::superellipse(&shape_2d::Superellipse {
            rx: f("rx")?,
            ry: f("ry")?,
            exponent: f("exponent")?,
            segments: count("segments", 4)?,
        })?,
        "spline" => shape_2d::spline(&shape_2d::Spline {
            points: v
                .get("points")
                .and_then(Value::as_array)
                .and_then(|l| l.iter().map(xy).collect::<Option<Vec<_>>>())
                .ok_or_else(|| bad("points", "a list of [x, y]"))?,
        })?,
        "path" => path2(v, "path")?.close()?,
        "stroke" => {
            let p = path2(v.get("path").unwrap_or(&Value::Null), "stroke.path")?;
            let join = match v.get("join").map(Value::as_str) {
                None | Some(Some("round")) => shape_2d::Join::Round,
                Some(Some("miter")) => shape_2d::Join::Miter,
                Some(Some("bevel")) => shape_2d::Join::Bevel,
                Some(_) => return Err(bad("join", "\"round\", \"miter\" or \"bevel\"")),
            };
            let flag = |key: &str| match v.get(key) {
                None | Some(Value::Null) => Ok(false),
                Some(b) => b.as_bool().ok_or_else(|| bad(key, "true or false")),
            };
            shape_2d::stroke(
                p.points(),
                &shape_2d::StrokeDesc {
                    width: f("width")?,
                    join,
                    round_caps: flag("roundCaps")?,
                    closed: flag("closed")?,
                },
            )?
        }
        "hull" => {
            let l = list()?;
            let mut all = Profile::default();
            for (i, q) in l.iter().enumerate() {
                let q = profile(q, fonts).map_err(|e| nest(&format!("hull[{i}]"), e))?;
                all.regions.extend(q.regions);
            }
            all.hull()?
        }
        "mirror" => match v.get("axis").map(Value::as_str) {
            None | Some(Some("y")) => inner("profile")?.mirror(),
            Some(Some("x")) => inner("profile")?.scale(1.0, -1.0),
            Some(_) => {
                return Err(bad(
                    "axis",
                    "\"y\" (across the y axis: x becomes -x) or \"x\" (y becomes -y)",
                ));
            }
        },
        "text" => {
            let name = v
                .get("font")
                .and_then(Value::as_str)
                .ok_or_else(|| bad("font", "the name of a font the caller supplies"))?;
            let font = fonts.get(name).ok_or_else(|| {
                let have: Vec<&str> = fonts.keys().map(String::as_str).collect();
                bad(
                    "font",
                    &format!(
                        "a font supplied by the caller; {name:?} was not (supplied: {})",
                        if have.is_empty() {
                            "none".to_string()
                        } else {
                            have.join(", ")
                        }
                    ),
                )
            })?;
            shape_2d::text(&shape_2d::Text {
                text: v
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| bad("text", "a string"))?
                    .into(),
                font: font.clone(),
                size: f("size")?,
                spacing: f("spacing")?,
            })?
        }
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
            let item = |i: usize| profile(&l[i], fonts).map_err(|e| nest(&format!("{k}[{i}]"), e));
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

/// [`measures_json`] with one field added, `interference`: per pose, the
/// rest first and then each of `poses` (joint values; every joint not
/// named at rest), the pairs of parts that pass through each other, touch,
/// or hold one wholly inside ([`Model::interference`]):
///
/// `{"pose": {joint: value}, "pairs": [{"a", "b", "crossings",
/// "touching", "inside" (a part's name or null), "boundsMin",
/// "boundsMax"}]}`
///
/// Counts only: no threshold says what is too much.
pub fn report_json(m: &Model, poses: &[Vec<(String, f64)>]) -> Result<String, ModelError> {
    let mut v = measures_value(m)?;
    let mut list = vec![interference_value(m, &[])?];
    for p in poses {
        list.push(interference_value(m, p)?);
    }
    if let Some(o) = v.as_object_mut() {
        o.insert("interference".into(), Value::Array(list));
    }
    Ok(v.to_string())
}

/// One pose's entry of [`report_json`]'s `interference`.
fn interference_value(m: &Model, pose: &[(String, f64)]) -> Result<Value, ModelError> {
    let v3 = |d: DVec3| json!([d.x, d.y, d.z]);
    let pairs: Vec<Value> = m
        .interference(pose)?
        .into_iter()
        .map(|x| {
            json!({
                "a": x.a,
                "b": x.b,
                "crossings": x.crossings,
                "touching": x.touching,
                "inside": if x.inside.is_empty() { Value::Null } else { Value::String(x.inside) },
                "boundsMin": v3(x.bounds_min),
                "boundsMax": v3(x.bounds_max),
            })
        })
        .collect();
    let pose: Map<String, Value> = pose.iter().map(|(k, x)| (k.clone(), json!(x))).collect();
    Ok(json!({ "pose": pose, "pairs": pairs }))
}

/// The model as posed (`docs/research.md` §20): [`measures_json`]'s
/// fields measured at the model's pose (each joint at its value), and
///
/// - `pose`: every joint that can be posed, with its value;
/// - `posed`: whether any value is not 0 (OBJ and STL are then written
///   posed);
/// - `joints`: per joint, at rest, `{"name", "child", "parent", "value",
///   "pivotToParent", "pivotToChild", "parentMoved", "parentTurned"}`
///   ([`Model::seating`]): how far the pivot lies from the parent's and
///   the child's surfaces (null for no triangles), and how far the parent
///   has moved and turned (radians) off the pivot since the joint was
///   declared;
/// - `interference`: the pairs at the pose, as [`report_json`] gives
///   them, each with `asBuilt`: true when both parts move by the very
///   same motion and were compared where built, false when compared
///   posed (flush faces under different joints can then read as
///   crossing by rounding).
///
/// Measurements only, with no thresholds.
pub fn posed_json(m: &Model) -> Result<String, ModelError> {
    Ok(posed_value(m)?.to_string())
}

fn posed_value(m: &Model) -> Result<Value, ModelError> {
    let pose = m.pose_values();
    let mut v = parts_value(m, &m.measure_posed()?);
    let v3 = |d: DVec3| json!([d.x, d.y, d.z]);
    let joints: Vec<Value> = m
        .seating()?
        .into_iter()
        .map(|j| {
            json!({
                "name": j.joint,
                "child": j.child,
                "parent": j.parent,
                "value": j.value,
                "pivotToParent": j.pivot_to_parent,
                "pivotToChild": j.pivot_to_child,
                "parentMoved": j.parent_moved,
                "parentTurned": j.parent_turned,
            })
        })
        .collect();
    let pairs: Vec<Value> = m
        .interference(&pose)?
        .into_iter()
        .map(|x| {
            json!({
                "a": x.a,
                "b": x.b,
                "crossings": x.crossings,
                "touching": x.touching,
                "inside": if x.inside.is_empty() { Value::Null } else { Value::String(x.inside) },
                "boundsMin": v3(x.bounds_min),
                "boundsMax": v3(x.bounds_max),
                "asBuilt": x.as_built,
            })
        })
        .collect();
    let values: Map<String, Value> = pose.iter().map(|(k, x)| (k.clone(), json!(x))).collect();
    if let Some(o) = v.as_object_mut() {
        o.insert("pose".into(), Value::Object(values));
        o.insert("posed".into(), Value::Bool(m.posed()));
        o.insert("joints".into(), Value::Array(joints));
        o.insert("interference".into(), Value::Array(pairs));
    }
    Ok(v)
}

fn measures_value(m: &Model) -> Result<Value, ModelError> {
    Ok(parts_value(m, &m.measure()?))
}

fn parts_value(m: &Model, parts: &[crate::PartMeasures]) -> Value {
    let v3 = |d: DVec3| json!([d.x, d.y, d.z]);
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    let (mut tris, mut verts, mut area, mut volume) = (0usize, 0usize, 0.0, 0.0);
    let mut list = Vec::new();
    let (mut coverage, mut overlap): (Option<f64>, Option<f64>) = (None, None);
    for p in parts {
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
        // UVs only on a part that has them, so other reports are as they
        // were: measured as exported (divided by the map's size).
        if let Some(u) = m.uv_measures(&p.name)
            && let Some(Value::Object(o)) = list.last_mut()
        {
            coverage = Some(coverage.map_or(u.coverage, |c| c.min(u.coverage)));
            overlap = Some(overlap.map_or(u.overlap, |c| c.max(u.overlap)));
            o.insert(
                "uv".into(),
                json!({
                    "area": u.area,
                    "coverage": u.coverage,
                    "overlap": u.overlap,
                    "overlappingPairs": u.overlapping_pairs,
                    "flipped": u.flipped,
                    "zeroArea": u.zero_area,
                    "outside": u.outside,
                }),
            );
        }
    }
    let empty = tris == 0;
    let unit = match m.unit() {
        Unit::Millimetre => "mm",
        Unit::Centimetre => "cm",
        Unit::Metre => "m",
    };
    json!({
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
        // Over the parts with UVs: the least coverage, the most overlap
        // (each part's UVs are its own map's); null when none has UVs.
        "uvCoverage": coverage.map_or(Value::Null, Value::from),
        "uvOverlap": overlap.map_or(Value::Null, Value::from),
        "textureBytes": 0,
    })
}

/// One operation's fields.
struct Args<'a> {
    o: &'a Map<String, Value>,
    op: &'a str,
    fonts: &'a Fonts,
    textures: &'a Fonts,
}

impl Args<'_> {
    /// A profile field.
    fn profile(&self, k: &str) -> Result<Profile, ModelError> {
        profile(self.get(k)?, self.fonts).map_err(|e| nest(k, e))
    }

    /// A texture field: the name of an image the caller supplied
    /// ([`Session::texture`]), PNG or JPEG.
    fn texture(&self, k: &str) -> Result<shape_tools::Texture, ModelError> {
        let name = self.str(k)?;
        let bytes = self.textures.get(name).ok_or_else(|| {
            self.bad(
                k,
                &format!(
                    "the name of an image the caller supplies; {name:?} was not supplied (the \
                     CLI and MCP read it as a path relative to the program)"
                ),
            )
        })?;
        let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            "image/jpeg"
        } else {
            return Err(self.bad(k, &format!("a PNG or JPEG image; {name:?} is neither")));
        };
        Ok(shape_tools::Texture {
            name: name.to_string(),
            mime: mime.into(),
            bytes: bytes.clone(),
        })
    }

    /// A path field: points and pieces, or a helix ([`path3`]).
    fn route(&self, k: &str) -> Result<Flat, ModelError> {
        path3(self.get(k)?, k)
    }

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
