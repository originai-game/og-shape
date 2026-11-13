//! The MCP tools: each a name, a description, a JSON schema for its
//! arguments and a handler that calls the `shape_cli` command of the same
//! name.
//!
//! Entry: [`list`] (for `tools/list`) and [`call`] (for `tools/call`).
//! Results are MCP content: JSON reports as text, previews as PNG image
//! content. A tool that fails answers `isError: true` with the kernel's
//! message; arguments of the wrong type, or not in the schema, fail the
//! same way. Paths are the workspace's ([`crate::workspace`]).

use std::path::PathBuf;

use serde_json::{Map, Value, json};
use shape_cli::build::report as build_report;
use shape_cli::collide::{CollideDesc, collide};
use shape_cli::compress::{CompressDesc, compress};
use shape_cli::decimate::{DecimateDesc, decimate};
use shape_cli::export::{export, written};
use shape_cli::import::{ImportDesc, import};
use shape_cli::measure::{MeasureDesc, measure, measure_posed};
use shape_cli::place::{PlaceDesc, length, pivot, place};
use shape_cli::preview::{PreviewDesc, image_path, render};
use shape_cli::process::{ProcessDesc, STEPS, process_steps};
use shape_cli::repair::{RepairDesc, repair};
use shape_cli::report::{array, object, string};
use shape_cli::source::{Source, load, load_uvs};
use shape_cli::unwrap::{UnwrapDesc, unwrap};
use shape_cli::{Failure, Files, Input, failed};
use shape_tools::{Decimate, Floaters, Import, Place, Preview, Repair, Size, Unwrap, UvView};

use crate::Server;
use crate::ts::{TsSource, run_polled as run_ts};

/// A tool's handler: the server, its arguments, and whom to tell of its steps.
type Handler = fn(&Server, &Args, &mut dyn Progress) -> Result<Vec<Value>, Failure>;

/// One tool.
struct Tool {
    name: &'static str,
    description: &'static str,
    schema: fn() -> Value,
    handler: Handler,
}

const TOOLS: [Tool; 13] = [
    Tool {
        name: "build",
        description: "Build a model program (the JSON form of resource og-shape://guides/program.md) \
            and report its parts, joints, booleans (voids left, edges where operands touch, faces \
            left without UVs, crossing triangle pairs left) and \
            measures (counts, bounds, area, volume, closed, non-manifold edges; no thresholds). \
            Give `program` inline or a `.json` `path` in the workspace; fonts that `text` \
            profiles name are read relative to the program (the workspace root for an inline \
            one).",
        schema: build_schema,
        handler: build_tool,
    },
    Tool {
        name: "preview",
        description: "Render a turntable of a program or a mesh file with the kernel's CPU \
            rasterizer: one PNG with the views side by side, at `pose` if given, with each \
            part's base-colour map and vertex colours. Look at it after every change. `each`: \
            one PNG a view with its camera (for a texturing step); `uv`: the UV layout of each \
            material over its map or a checker instead.",
        schema: preview_schema,
        handler: preview_tool,
    },
    Tool {
        name: "measure",
        description: "Measures of a program (every part and the whole) or a mesh file (every part \
            placed, and the parts joined) as JSON, with no thresholds; resource \
            og-shape://guides/meshtools.md says what to look for.",
        schema: measure_schema,
        handler: measure_tool,
    },
    Tool {
        name: "export",
        description: "Write a program (posed, with joints and materials in GLB) or a mesh file's \
            parts as GLB, OBJ or STL, by the extension of `out`, a path in the workspace.",
        schema: export_schema,
        handler: export_tool,
    },
    Tool {
        name: "import",
        description: "Report what a mesh file (glb, gltf, obj, fbx) holds: parts, materials, \
            triangles, UVs, bones, images, and what the importer skipped. With `out`, also write \
            the parts placed as GLB (skins kept), OBJ or STL.",
        schema: import_schema,
        handler: import_tool,
    },
    Tool {
        name: "repair",
        description: "Join a mesh file's parts and repair them: weld, drop degenerate and \
            duplicate triangles and floaters, orient, close holes, fuse interpenetrating shells. \
            Reports what each step changed and the measures after. Every option's zero value does \
            the least; og-shape://guides/meshtools.md gives settings that worked.",
        schema: repair_schema,
        handler: repair_tool,
    },
    Tool {
        name: "decimate",
        description: "Join a mesh file's parts and decimate them (quadric error; closed stays \
            closed) to a `ratio` or a triangle `target`, or into `lods` levels each from the one \
            before. Reports each level's error (mm) and measures.",
        schema: decimate_schema,
        handler: decimate_tool,
    },
    Tool {
        name: "unwrap",
        description: "Join a mesh file's parts and UV-unwrap them (charts, planar projection, \
            packing). Reports the charts and the UV measures; `out` (.glb) gets the UVs.",
        schema: unwrap_schema,
        handler: unwrap_tool,
    },
    Tool {
        name: "collide",
        description: "Colliders: cover each part of a program or a mesh file with convex hulls, \
            the shape a physics engine tests against. One hull a part by default; `hulls` cuts a \
            part with hollows (a cup, an L, a chair) into up to that many pieces, each with its \
            own hull, until every piece's concavity is within `concavity` (mm; how deep the \
            part's surface lies under its hull). `max_vertices` thins each hull. Reports per \
            part each hull's vertices, triangles, volume, concavity and `outside` (how far the \
            part pokes out of a thinned hull); `out` gets the hulls alone, named \
            <part>.collider.<k>. Decimate a dense mesh first: the cost grows with its points.",
        schema: collide_schema,
        handler: collide_tool,
    },
    Tool {
        name: "compress",
        description: "Compress a GLB's geometry and animation with meshopt \
            (`EXT_meshopt_compression`), losslessly: the file reads back to the same vertices, \
            triangles, skins and clips, and a viewer needs the meshopt decoder. Images are left \
            as they are. Reports the buffer views compressed and their bytes before and after. \
            Do it last, on the file you ship.",
        schema: compress_schema,
        handler: compress_tool,
    },
    Tool {
        name: "place",
        description: "Put a mesh file where a game wants it: turn it so the file's `up` axis is \
            up and its `front` axis faces front (exact axis turns, then `turn` degrees about up), \
            scale it uniformly to a real size (`longest`, `height`, `width` or `depth`, a length \
            with its unit such as \"0.9m\", or a `scale` factor), and move its origin (`pivot`: \
            \"base\" to stand on z = 0, \"center\", or a point). Skins, UVs and textures are \
            kept. Reports what was done and, before and after, the extents, pivot, the area \
            facing each way and where the mass sits; it does not guess which way is up.",
        schema: place_schema,
        handler: place_tool,
    },
    Tool {
        name: "process",
        description: "The mesh tools' chain on one file: import → join → repair → decimate to \
            `budget` triangles → place (only if `place` is given) → unwrap → textures → measure \
            → export (`out`, a plain GLB with UVs, vertex colours and the material). One report \
            of every step. A mesh that carries UVs keeps them and its material's maps, and is \
            not unwrapped unless `fresh_uvs` is set; `texture_max` shrinks maps over that many \
            texels. `bake` (texels) keeps the look through fresh UVs: the repaired mesh's maps, \
            vertex colours and shading normals are baked into new maps on the decimated mesh's \
            packed atlas (the report's `bake` says what was baked and what found no source).",
        schema: process_schema,
        handler: process_tool,
    },
    Tool {
        name: "run_typescript",
        description: "Run a TypeScript program through the og-shape binding under bun \
            (resource og-shape://guides/typescript.md): its default export takes `og` and \
            returns the Shape it built. The program it recorded is replayed by the native kernel, \
            which gives the report and measures; the program JSON comes back too, for `build`, \
            `preview` and `export` (which take `pose` to look at a pose without changing it), \
            and its `console.log` output as `logs`. The program runs with the server's \
            permissions.",
        schema: ts_schema,
        handler: ts_tool,
    },
];

/// The `tools/list` result.
pub fn list() -> Value {
    let tools: Vec<Value> = TOOLS
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": (t.schema)(),
            })
        })
        .collect();
    json!({ "tools": tools })
}

/// What a long tool tells its caller between steps, and how it learns
/// that the call was cancelled. The kernel is never interrupted: a tool
/// asks only between kernel calls (`process`'s steps), or while it waits
/// on bun (`run_typescript`).
pub trait Progress {
    /// Step `k` (from 0) of `total`, named `what`, is about to run. An
    /// error stops the tool there and is its result.
    fn step(&mut self, k: usize, total: usize, what: &str) -> Result<(), Failure>;
    /// Asked about every 10 ms while the tool waits on a child process;
    /// an error stops the child and is the tool's result.
    fn poll(&mut self) -> Result<(), Failure>;
}

/// Progress nobody hears: no call is ever cancelled.
pub struct Silent;

impl Progress for Silent {
    fn step(&mut self, _: usize, _: usize, _: &str) -> Result<(), Failure> {
        Ok(())
    }
    fn poll(&mut self) -> Result<(), Failure> {
        Ok(())
    }
}

/// Runs a tool: its `tools/call` result, or None for a tool that does
/// not exist.
pub fn call(server: &Server, name: &str, args: &Map<String, Value>) -> Option<Value> {
    call_with(server, name, args, &mut Silent)
}

/// As [`call`], telling `progress` of each step of a long tool.
pub fn call_with(
    server: &Server,
    name: &str,
    args: &Map<String, Value>,
    progress: &mut dyn Progress,
) -> Option<Value> {
    let tool = TOOLS.iter().find(|t| t.name == name)?;
    let schema = (tool.schema)();
    let result = check_keys(&schema, args)
        .and_then(|()| (tool.handler)(server, &Args { o: args }, progress));
    Some(match result {
        Ok(content) => json!({ "content": content, "isError": false }),
        Err(e) => json!({
            "content": [text(format!("error: {e}"))],
            "isError": true,
        }),
    })
}

fn check_keys(schema: &Value, args: &Map<String, Value>) -> Result<(), Failure> {
    let props = schema["properties"].as_object();
    for k in args.keys() {
        if !props.is_some_and(|p| p.contains_key(k)) {
            return Err(Failure::Usage(format!("no argument {k:?}")));
        }
    }
    for k in schema["required"].as_array().into_iter().flatten() {
        let k = k.as_str().unwrap_or_default();
        if !args.contains_key(k) {
            return Err(Failure::Usage(format!("{k:?} is required")));
        }
    }
    Ok(())
}

fn text(s: String) -> Value {
    json!({ "type": "text", "text": s })
}

// ── arguments ──

struct Args<'a> {
    o: &'a Map<String, Value>,
}

fn bad(k: &str, what: &str) -> Failure {
    Failure::Usage(format!("{k}: {what}"))
}

impl Args<'_> {
    fn num(&self, k: &str) -> Result<f64, Failure> {
        match self.o.get(k) {
            None | Some(Value::Null) => Ok(0.0),
            Some(v) => v.as_f64().ok_or_else(|| bad(k, "a number")),
        }
    }

    fn opt_num(&self, k: &str) -> Result<Option<f64>, Failure> {
        match self.o.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(_) => self.num(k).map(Some),
        }
    }

    fn uint(&self, k: &str) -> Result<u64, Failure> {
        match self.o.get(k) {
            None | Some(Value::Null) => Ok(0),
            Some(v) => v.as_u64().ok_or_else(|| bad(k, "a whole number ≥ 0")),
        }
    }

    fn u32(&self, k: &str) -> Result<u32, Failure> {
        u32::try_from(self.uint(k)?).map_err(|_| bad(k, "too large"))
    }

    fn flag(&self, k: &str) -> Result<bool, Failure> {
        match self.o.get(k) {
            None | Some(Value::Null) => Ok(false),
            Some(v) => v.as_bool().ok_or_else(|| bad(k, "true or false")),
        }
    }

    fn string(&self, k: &str) -> Result<Option<&str>, Failure> {
        match self.o.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v.as_str().map(Some).ok_or_else(|| bad(k, "a string")),
        }
    }

    fn path(&self, k: &str) -> Result<Option<PathBuf>, Failure> {
        Ok(self.string(k)?.map(PathBuf::from))
    }

    fn sub(&self, k: &str) -> Result<Option<Args<'_>>, Failure> {
        match self.o.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Object(o)) => Ok(Some(Args { o })),
            Some(_) => Err(bad(k, "an object")),
        }
    }

    fn import(&self) -> Result<Import, Failure> {
        Ok(Import {
            unit_mm: self.num("unit_mm")?,
            z_up: self.flag("z_up")?,
            up: self.axis("up")?,
            front: self.axis("front")?,
        })
    }

    fn axis(&self, k: &str) -> Result<Option<shape_tools::Axis>, Failure> {
        self.string(k)?
            .map(|v| shape_cli::args::axis(k, v))
            .transpose()
    }

    /// The placement: `turn`, one size, `pivot`.
    fn place(&self) -> Result<Place, Failure> {
        let mut sizes = Vec::new();
        for (k, size) in [
            ("longest", Size::Longest as fn(f64) -> Size),
            ("height", Size::Height),
            ("width", Size::Width),
            ("depth", Size::Depth),
        ] {
            if let Some(v) = self.string(k)? {
                sizes.push(size(length(k, v)?));
            }
        }
        if let Some(f) = self.opt_num("scale")? {
            sizes.push(Size::Scale(f));
        }
        if sizes.len() > 1 {
            return Err(Failure::Usage(
                "one size: longest, height, width, depth or scale".into(),
            ));
        }
        Ok(Place {
            turn: self.num("turn")?,
            size: sizes.pop().unwrap_or_default(),
            pivot: self
                .string("pivot")?
                .map(pivot)
                .transpose()?
                .unwrap_or_default(),
            ..Default::default()
        })
    }

    /// `program` (an object or JSON text) or `path`.
    fn input(&self, server: &Server) -> Result<Input, Failure> {
        let program = match self.o.get("program") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s.clone()),
            Some(v @ Value::Object(_)) => Some(v.to_string()),
            Some(_) => return Err(bad("program", "an object or its JSON text")),
        };
        match (program, self.path("path")?) {
            (Some(p), None) => {
                if p.len() as u64 > server.ws.max_bytes() {
                    return Err(bad("program", "over the size cap (--max-bytes)"));
                }
                Ok(Input::Program(p))
            }
            (None, Some(p)) => Ok(Input::Path(p)),
            (Some(_), Some(_)) => Err(Failure::Usage("give `program` or `path`, not both".into())),
            (None, None) => Err(Failure::Usage("give `program` or `path`".into())),
        }
    }

    fn mesh_input(&self) -> Result<Input, Failure> {
        self.path("path")?
            .map(Input::Path)
            .ok_or_else(|| Failure::Usage("give the mesh file's `path`".into()))
    }

    fn repair(&self) -> Result<Repair, Failure> {
        let floaters = match self.sub("floaters")? {
            None => Floaters::default(),
            Some(f) => Floaters {
                size: f.num("size")?,
                fraction: f.num("fraction")?,
            },
        };
        Ok(Repair {
            weld: self.num("weld")?,
            weld_all: self.flag("weld_all")?,
            floaters,
            non_manifold: self.flag("non_manifold")?,
            max_hole_edges: self.uint("max_hole_edges")? as usize,
            thicken: self.num("thicken")?,
            fuse: self.flag("fuse")?,
            min_edge: self.num("min_edge")?,
            check_intersections: self.flag("check_intersections")?,
        })
    }

    fn unwrap(&self) -> Result<Unwrap, Failure> {
        Ok(Unwrap {
            angle: self.num("angle")?,
            resolution: self.u32("resolution")?,
            padding: self.num("padding")?,
            shared: self.flag("shared")?,
            conformal: self.flag("conformal")?,
        })
    }
}

// ── schemas ──

fn schema(props: Vec<(&str, Value)>, required: &[&str]) -> Value {
    let mut p = Map::new();
    for (k, v) in props {
        p.insert(k.to_string(), v);
    }
    json!({
        "type": "object",
        "properties": p,
        "required": required,
        "additionalProperties": false,
    })
}

fn num(description: &str) -> Value {
    json!({ "type": "number", "description": description })
}

fn int(description: &str) -> Value {
    json!({ "type": "integer", "minimum": 0, "description": description })
}

fn boolean(description: &str) -> Value {
    json!({ "type": "boolean", "description": description })
}

fn path(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn program_props() -> Vec<(&'static str, Value)> {
    vec![
        (
            "program",
            json!({
                "type": ["object", "string"],
                "description": "A program, {\"name\": …, \"ops\": [{\"op\": …}, …]} \
                    (og-shape://guides/program.md), as an object or its JSON text.",
            }),
        ),
        (
            "path",
            path(
                "Instead of `program`: a file in the workspace, a program (.json) or a mesh \
                 (.glb, .gltf, .obj, .fbx).",
            ),
        ),
        (
            "uvs",
            boolean(
                "A program: every part it makes gets UVs, as if it began with the `uvs` \
                 operation (og-shape://guides/program.md). Textures a material's `map` names \
                 are read relative to the program.",
            ),
        ),
    ]
}

/// `pose`: joint values set on a program for this call only.
fn pose_prop() -> (&'static str, Value) {
    (
        "pose",
        json!({
            "type": "object",
            "additionalProperties": { "type": "number" },
            "description": "A program only: joints to pose for this call, {\"lid_hinge\": 1.2} \
                (radians, or mm for a slide), on top of the program's own `pose` ops; the \
                program is not changed.",
        }),
    )
}

fn obj_props() -> Vec<(&'static str, Value)> {
    vec![
        (
            "unit_mm",
            num("OBJ only: millimetres per file unit; 0 reads metres (1000), as glTF."),
        ),
        (
            "z_up",
            boolean("OBJ and glTF: the file is Z up and faces -Y already (up +z, front -y)."),
        ),
        (
            "up",
            axis_prop(
                "OBJ and glTF: the file's axis that is the object's up; default +y (glTF's).",
            ),
        ),
        (
            "front",
            axis_prop(
                "OBJ and glTF: the file's axis the object faces; default +z (glTF's), or what \
                 the smallest turn from `up` gives.",
            ),
        ),
    ]
}

fn axis_prop(description: &str) -> Value {
    json!({
        "type": "string",
        "enum": ["+x", "-x", "+y", "-y", "+z", "-z"],
        "description": description,
    })
}

fn place_props() -> Vec<(&'static str, Value)> {
    let len = |what: &str| {
        json!({
            "type": "string",
            "description": format!("Scale uniformly so the {what} is this length, with its unit: \"0.9m\", \"90cm\", \"900mm\". One size at most."),
        })
    };
    vec![
        (
            "turn",
            num("Then turn about up, degrees, counter-clockwise seen from above (90s are exact)."),
        ),
        ("longest", len("box's longest side")),
        ("height", len("box's height (Z, after the turn)")),
        ("width", len("box's width (X, after the turn)")),
        (
            "depth",
            len("box's depth (Y, front to back, after the turn)"),
        ),
        ("scale", num("Instead of a length: scale by this factor.")),
        (
            "pivot",
            json!({
                "type": "string",
                "description": "Where the origin goes: \"base\" (the centre of the box's bottom \
                    face: stands on z = 0), \"center\", or a point of the input \"x,y,z\" with \
                    a unit (\"0,0,-480mm\", in the coordinates `measure` reports). Absent keeps it.",
            }),
        ),
    ]
}

fn mesh_props(out: &str) -> Vec<(&'static str, Value)> {
    let mut v = vec![
        (
            "path",
            path("The mesh file in the workspace: .glb, .gltf, .obj or .fbx."),
        ),
        ("out", path(out)),
    ];
    v.extend(obj_props());
    v
}

fn repair_props() -> Vec<(&'static str, Value)> {
    vec![
        (
            "weld",
            num("Weld vertices on open edges closer than this, mm (0: exact duplicates only)."),
        ),
        (
            "weld_all",
            boolean("Weld every vertex within `weld`, not only those on open edges."),
        ),
        (
            "floaters",
            json!({
                "type": "object",
                "description": "Drop small pieces: below `size` mm across, or below `fraction` \
                    of the largest piece's diagonal.",
                "properties": {
                    "size": num("mm"),
                    "fraction": num("of the largest piece's diagonal"),
                },
                "additionalProperties": false,
            }),
        ),
        (
            "non_manifold",
            boolean(
                "Split edges with more than two faces into manifold sheets, by the faces' order \
                 round the edge; then drop the pieces that come apart and are floaters.",
            ),
        ),
        (
            "max_hole_edges",
            int("Close holes whose rim has at most this many edges (0: none)."),
        ),
        (
            "thicken",
            num(
                "Give every piece still open a second sheet this far under it, mm, and a rim \
                 along its open edges, so a sheet (a leaf, a cape) encloses a volume (0: none).",
            ),
        ),
        (
            "fuse",
            boolean("Join closed, interpenetrating shells by self-union, if the union is closed."),
        ),
        (
            "min_edge",
            num("Collapse edges shorter than this, mm, where the mesh stays manifold."),
        ),
        (
            "check_intersections",
            boolean("Count self-intersecting triangle pairs at the end (exact)."),
        ),
    ]
}

fn unwrap_props() -> Vec<(&'static str, Value)> {
    vec![
        (
            "angle",
            num("Largest angle, degrees, between a chart's faces and its projection; 0 is 66."),
        ),
        (
            "resolution",
            int("Texels across the atlas the padding is measured in; 0 is 1024."),
        ),
        ("padding", num("Texels between charts; 0 is 2.")),
        (
            "shared",
            boolean(
                "Charts share projection directions chosen over the whole mesh (as Blender's \
                 Smart UV Project chooses them): all that faces one way is projected one way.",
            ),
        ),
        (
            "conformal",
            boolean(
                "Flatten charts that bend keeping their angles (least squares conformal maps) \
                 where that neither folds nor stretches an area past twice or under half: fewer, \
                 larger charts on curved surfaces, at the surface's true area.",
            ),
        ),
    ]
}

fn build_schema() -> Value {
    schema(program_props(), &[])
}

fn preview_schema() -> Value {
    let mut p = program_props();
    p.extend([
        ("views", int("Views round the turntable; 0 is 4.")),
        ("width", int("Pixels across a view; 0 is 256.")),
        ("height", int("Pixels down a view; 0 is the width.")),
        ("columns", int("Views a row; 0 puts them all in one row.")),
        (
            "elevation",
            num("Radians above the horizon, over 0 and up to pi/2; absent is 25 degrees."),
        ),
        (
            "azimuth",
            num("Radians from the front (the kernel's -Y side)."),
        ),
        ("no_edges", boolean("Do not draw edges.")),
        ("no_ground", boolean("Do not draw the ground shadow.")),
        (
            "fov",
            num(
                "Perspective: vertical field of view in radians, over 0 and up to pi/3; absent \
                 is orthographic.",
            ),
        ),
        (
            "unlit",
            boolean(
                "Colour only: no light (with no_edges and no_ground, a flat colour \
                 pass).",
            ),
        ),
        (
            "each",
            boolean(
                "One PNG a view instead of one sheet; the report gives each view's camera \
                     (position, target, up, fov or ortho_height; scene frame and glTF frame).",
            ),
        ),
        (
            "clip",
            json!({"type": "string", "description": "A skinned mesh file's clip to pose it by \
                (its names are in `import`'s `clips`); with `time` or `frames` and no `clip`, \
                the file's first. Without any of the three, the bind pose."}),
        ),
        (
            "morph",
            json!({"type": "string", "description": "Morph targets of a mesh file set to                 weights: `NAME=WEIGHT[,NAME=WEIGHT…]` (the names are in `import`'s parts'                 `morphs`), over the file's rest weights and a clip's."}),
        ),
        (
            "time",
            num("Seconds into the clip, from its first key (held past its ends)."),
        ),
        (
            "frames",
            int(
                "This many frames of the clip, evenly from its start to its end (the end left \
                 out), one PNG each with the camera still; at most 720.",
            ),
        ),
        (
            "uv",
            boolean(
                "Draw the UV layout of each material instead: chart outlines over its map, \
                     or over a checker. The report gives uv_min and tiles: pixel (x, y) is \
                     u = uv_min[0] + tiles (x + 0.5) / size, v = uv_min[1] + tiles (size - y - \
                     0.5) / size.",
            ),
        ),
        (
            "uv_size",
            int("With `uv`: pixels across and down; 0 is 512."),
        ),
        (
            "out",
            path(
                "Also save the PNG at this path in the workspace; several images go to \
                  <stem>.<k>.png beside it.",
            ),
        ),
        pose_prop(),
    ]);
    p.extend(obj_props());
    schema(p, &[])
}

fn measure_schema() -> Value {
    let mut p = program_props();
    p.extend([
        (
            "intersections",
            boolean("Meshes: count self-intersecting triangle pairs (exact; slower)."),
        ),
        (
            "posed",
            boolean(
                "A program at its pose (its `pose` ops and `pose` here): bounds at the pose, \
                 how each joint holds (`joints`) and the parts that meet (`interference`). \
                 Implied by `pose`.",
            ),
        ),
        pose_prop(),
        (
            "axes",
            boolean(
                "Meshes: add `axes` to each part and the whole file: extents, longest and \
                 shortest axis, the area facing each way, where the mass sits in the box.",
            ),
        ),
    ]);
    p.extend(obj_props());
    schema(p, &[])
}

fn export_schema() -> Value {
    let mut p = program_props();
    p.push((
        "out",
        path(
            "Where to write, in the workspace: .glb, .obj, .stl, or .urdf (the description, \
             with each part's STL beside it as <stem>_<part>.stl; links at rest, joints with \
             their axes, limits and mimics; no inertial, and limit effort and velocity are 0), \
             or .usda (a USD stage in the kernel's space, Z up in millimetres: a mesh a part \
             with normals and UVs, UsdPreviewSurface materials, PNG and JPEG maps beside it; \
             no joints, skins, morph targets or clips).",
        ),
    ));
    p.push(pose_prop());
    p.extend(obj_props());
    schema(p, &["out"])
}

fn import_schema() -> Value {
    schema(
        mesh_props("Also write the parts placed here: .glb, .obj or .stl."),
        &["path"],
    )
}

fn repair_schema() -> Value {
    let mut p = mesh_props("Write the repaired mesh here: .glb (plain), .obj or .stl.");
    p.extend(repair_props());
    schema(p, &["path"])
}

fn decimate_schema() -> Value {
    let mut p = mesh_props(
        "Write the result here: .glb (plain), .obj or .stl; with `lods`, as \
         <stem>.lod1.<ext>, <stem>.lod2.<ext> ….",
    );
    p.extend([
        (
            "ratio",
            num("Keep this fraction of the triangles (0 < ratio <= 1)."),
        ),
        ("target", int("Keep at most this many triangles.")),
        (
            "max_error",
            num("Refuse collapses that move the surface more than this, mm; 0 is no limit."),
        ),
        (
            "lock_border",
            boolean("Never move vertices on open borders."),
        ),
        ("merge_charts", merge_charts_prop()),
        ("cross_color_borders", cross_color_borders_prop()),
        (
            "lods",
            int(
                "Make this many levels, each decimated from the one before: level k keeps \
                 ratio^k of the triangles, or the target and then half the level before; 0 is one.",
            ),
        ),
        (
            "lod_ratios",
            json!({
                "type": "array",
                "items": {"type": "number"},
                "description": "One level a number, each decimated from the one before: its share \
                    of the input's triangles (over 0, at most 1, falling), as [0.5, 0.2, 0.05]. \
                    Instead of `lods`, `ratio` and `target`.",
            }),
        ),
    ]);
    schema(p, &["path"])
}

fn unwrap_schema() -> Value {
    let mut p = mesh_props("Write a plain GLB with the UVs here (.glb; .obj and .stl carry none).");
    p.extend(unwrap_props());
    schema(p, &["path"])
}

fn collide_schema() -> Value {
    let mut p = program_props();
    p.push((
        "out",
        path("Write the hulls here, in the workspace: .glb, .obj or .stl."),
    ));
    p.extend(obj_props());
    p.extend([
        (
            "hulls",
            int("The most hulls a part gets; 0 and 1 make one (its convex hull)."),
        ),
        (
            "concavity",
            num(
                "Stop cutting a part when its surface lies at most this deep under its hulls \
                 (mm); 0 cuts until `hulls` is reached or every piece is convex.",
            ),
        ),
        (
            "max_vertices",
            int("The most vertices a hull may have (at least 4); 0 keeps every corner."),
        ),
    ]);
    schema(p, &[])
}

fn compress_schema() -> Value {
    schema(
        vec![
            ("path", path("The GLB file, in the workspace.")),
            (
                "out",
                path("Write the compressed GLB here, in the workspace."),
            ),
        ],
        &["path", "out"],
    )
}

fn place_schema() -> Value {
    let mut p = mesh_props(
        "Write the parts placed here: .glb (skins, UVs and textures kept), .obj or .stl.",
    );
    p.extend(place_props());
    schema(p, &["path"])
}

fn cross_color_borders_prop() -> Value {
    boolean(
        "Collapse across hard vertex-colour borders as anywhere (default false: a border is a \
         feature line, as a UV seam is). For colours sampled per vertex, where a border marks \
         no feature; a border then moves and blurs by up to an edge.",
    )
}

fn merge_charts_prop() -> Value {
    num(
        "UV islands that meet along an edge and agree there up to a rigid move in UV space \
         (within this distance, UV units) are one chart for decimation's seam rule; 0 is off.",
    )
}

fn process_schema() -> Value {
    let mut p = mesh_props("Write the result here: .glb (plain, with UVs), .obj or .stl.");
    p.extend([
        (
            "budget",
            int("Decimate to at most this many triangles; 0 does not decimate."),
        ),
        (
            "ratio",
            num(
                "Decimate to this share of the triangles the repair leaves (over 0, at most \
                 1), instead of `budget`.",
            ),
        ),
        ("merge_charts", merge_charts_prop()),
        ("cross_color_borders", cross_color_borders_prop()),
        (
            "repair",
            json!({
                "type": "object",
                "description": "The repair, as the `repair` tool takes it (minus path and out).",
                "properties": Value::Object(repair_props().into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
                "additionalProperties": false,
            }),
        ),
        (
            "unwrap",
            json!({
                "type": "object",
                "description": "The unwrap, as the `unwrap` tool takes it.",
                "properties": Value::Object(unwrap_props().into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
                "additionalProperties": false,
            }),
        ),
        (
            "place",
            json!({
                "type": "object",
                "description": "Place the mesh after decimation, as the `place` tool takes \
                    it (minus path, out and the file's axes, which are `up` and `front` here). \
                    Absent: not placed, and no `place` in the report.",
                "properties": Value::Object(place_props().into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
                "additionalProperties": false,
            }),
        ),
        (
            "intersections",
            boolean("Count self-intersecting triangle pairs in the final measures (exact)."),
        ),
        (
            "fresh_uvs",
            boolean("Unwrap even a mesh that carries UVs from its file (its maps are then not written)."),
        ),
        (
            "texture_max",
            int("Shrink every map kept whose longest side is over this many texels (box filter, \
                 written as PNG); the report gives each map's size before and after. 0 keeps them."),
        ),
        (
            "ktx2",
            boolean("Write every map kept as KTX2 (UASTC blocks with mip levels, Zstandard; \
                     `KHR_texture_basisu`), after any shrinking: a quarter of the texture memory \
                     on a GPU, and seconds of work per 1024² map."),
        ),
        (
            "bake",
            int("Bake the repaired mesh's look into new maps this many texels across, on fresh \
                 UVs packed into one atlas: decimate without UVs or vertex colours, pack, and \
                 bake base colour (times vertex colours), metallic-roughness, emissive, \
                 occlusion and normals (from the source's normal map, or its shape). The GLB \
                 gets the baked maps, the factors and TANGENT. 0 does not bake."),
        ),
        (
            "bake_distance",
            num("How far from the decimated surface a baked texel's source point may be, in \
                 the mesh's units (mm); 0 is 2 % of the bounding-box diagonal."),
        ),
    ]);
    schema(p, &["path"])
}

fn ts_schema() -> Value {
    schema(
        vec![
            (
                "source",
                json!({
                    "type": "string",
                    "description": "The program's TypeScript: `export default function (og) { \
                        const s = og.model({ name: \"…\" }); …; return s; }`.",
                }),
            ),
            (
                "path",
                path(
                    "Instead of `source`: a .ts file already in the workspace (relative imports \
                     work); the server writes no .ts file.",
                ),
            ),
            (
                "program_out",
                path("Save the recorded program (JSON) at this path in the workspace."),
            ),
        ],
        &[],
    )
}

// ── handlers ──

fn loaded(server: &Server, a: &Args) -> Result<Source, Failure> {
    let mut source = load_uvs(&server.ws, &a.input(server)?, &a.import()?, a.flag("uvs")?)?;
    if let Some(pose) = a.sub("pose")? {
        let Source::Program(model) = &mut source else {
            return Err(bad("pose", "a program's joints; a mesh file has none"));
        };
        for (joint, value) in pose.o {
            let v = value
                .as_f64()
                .ok_or_else(|| bad("pose", "joint values are numbers"))?;
            model.pose(joint, v).map_err(failed)?;
        }
    }
    Ok(source)
}

fn build_tool(server: &Server, a: &Args, _p: &mut dyn Progress) -> Result<Vec<Value>, Failure> {
    let source = loaded(server, a)?;
    let Source::Program(model) = &source else {
        return Err(Failure::Usage(
            "build takes a program; `import` and `measure` read a mesh file".into(),
        ));
    };
    let report = build_report(model)?;
    let measures = measure(&source, &MeasureDesc::default())?;
    Ok(vec![text(object(&[
        ("build", report),
        ("measures", measures),
    ]))])
}

/// The width and height a PNG's header gives.
fn png_size(png: &[u8]) -> (u32, u32) {
    let be = |i: usize| {
        png.get(i..i + 4)
            .map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    (be(16), be(20))
}

fn preview_tool(server: &Server, a: &Args, _p: &mut dyn Progress) -> Result<Vec<Value>, Failure> {
    let elevation = a.opt_num("elevation")?;
    if elevation.is_some_and(|e| !(e > 0.0 && e <= std::f64::consts::FRAC_PI_2)) {
        return Err(bad(
            "elevation",
            "radians above the horizon, over 0 and up to pi/2 (1.5708); no view from below",
        ));
    }
    let fov = a.opt_num("fov")?;
    if fov.is_some_and(|f| !(f > 0.0 && f <= std::f64::consts::FRAC_PI_3)) {
        return Err(bad(
            "fov",
            "radians of vertical field of view, over 0 and up to pi/3 (1.0472)",
        ));
    }
    let uv = a.flag("uv")?;
    if !uv && a.u32("uv_size")? != 0 {
        return Err(bad("uv_size", "goes with `uv`"));
    }
    let d = PreviewDesc {
        preview: Preview {
            width: a.u32("width")?,
            height: a.u32("height")?,
            views: a.u32("views")?,
            elevation,
            azimuth: a.num("azimuth")?,
            no_edges: a.flag("no_edges")?,
            no_ground: a.flag("no_ground")?,
            fov,
            unlit: a.flag("unlit")?,
            frame: None,
        },
        columns: a.u32("columns")?,
        each: a.flag("each")?,
        uv: uv.then(|| UvView {
            size: a.u32("uv_size").unwrap_or(0),
        }),
        // `time` or `frames` alone take the file's first clip.
        clip: match a.string("clip")? {
            Some(c) => Some(c.to_string()),
            None if a.opt_num("time")?.is_some() || a.u32("frames")? > 0 => Some(String::new()),
            None => None,
        },
        morph: shape_cli::preview::morph_weights(a.string("morph")?.unwrap_or(""))?,
        time: a.num("time")?,
        frames: a.u32("frames")?,
    };
    let rendered = render(&loaded(server, a)?, &d)?;
    let out = a.path("out")?;
    let n = rendered.images.len();
    let mut content = Vec::with_capacity(n + 1);
    let mut listed = Vec::with_capacity(n);
    for (k, (png, shows)) in rendered.images.iter().enumerate() {
        let output = match &out {
            None => "null".to_string(),
            Some(out) => {
                let path = image_path(out, k, n);
                server.ws.write(&path, png)?;
                written(&path, png.len())
            }
        };
        let (w, h) = png_size(png);
        listed.push(vec![
            ("width", w.to_string()),
            ("height", h.to_string()),
            ("bytes", png.len().to_string()),
            ("output", output),
            ("shows", shows.clone()),
        ]);
        content.push(json!({
            "type": "image",
            "data": crate::base64::encode(png),
            "mimeType": "image/png",
        }));
    }
    // The first image's fields at the top, as before `each` and `uv`.
    let mut fields: Vec<(&str, String)> =
        listed.first().map(|f| f[..4].to_vec()).unwrap_or_default();
    fields.push((
        "images",
        array(&listed.iter().map(|f| object(f)).collect::<Vec<_>>()),
    ));
    fields.push(("undrawn", rendered.undrawn));
    content.push(text(object(&fields)));
    Ok(content)
}

fn measure_tool(server: &Server, a: &Args, _p: &mut dyn Progress) -> Result<Vec<Value>, Failure> {
    let d = MeasureDesc {
        intersections: a.flag("intersections")?,
        axes: a.flag("axes")?,
    };
    let source = loaded(server, a)?;
    if a.flag("posed")? || a.sub("pose")?.is_some() {
        return Ok(vec![text(measure_posed(&source)?)]);
    }
    Ok(vec![text(measure(&source, &d)?)])
}

fn export_tool(server: &Server, a: &Args, _p: &mut dyn Progress) -> Result<Vec<Value>, Failure> {
    let out = a
        .path("out")?
        .ok_or_else(|| Failure::Usage("give `out`".into()))?;
    Ok(vec![text(export(&server.ws, &loaded(server, a)?, &out)?)])
}

fn import_tool(server: &Server, a: &Args, _p: &mut dyn Progress) -> Result<Vec<Value>, Failure> {
    let d = ImportDesc {
        input: a.mesh_input()?,
        import: a.import()?,
        out: a.path("out")?,
    };
    Ok(vec![text(import(&server.ws, &d)?)])
}

fn repair_tool(server: &Server, a: &Args, _p: &mut dyn Progress) -> Result<Vec<Value>, Failure> {
    let d = RepairDesc {
        input: a.mesh_input()?,
        import: a.import()?,
        repair: a.repair()?,
        out: a.path("out")?,
    };
    Ok(vec![text(repair(&server.ws, &d)?)])
}

fn decimate_tool(server: &Server, a: &Args, _p: &mut dyn Progress) -> Result<Vec<Value>, Failure> {
    let d = DecimateDesc {
        input: a.mesh_input()?,
        import: a.import()?,
        decimate: Decimate {
            ratio: a.num("ratio")?,
            target: a.uint("target")? as usize,
            max_error: a.num("max_error")?,
            lock_border: a.flag("lock_border")?,
            merge_charts: a.num("merge_charts")?,
            cross_color_borders: a.flag("cross_color_borders")?,
            ..Default::default()
        },
        lods: a.u32("lods")?,
        lod_ratios: match a.o.get("lod_ratios") {
            None | Some(Value::Null) => Vec::new(),
            Some(v) => v
                .as_array()
                .and_then(|l| l.iter().map(Value::as_f64).collect::<Option<Vec<f64>>>())
                .ok_or_else(|| bad("lod_ratios", "a list of numbers"))?,
        },
        out: a.path("out")?,
    };
    Ok(vec![text(decimate(&server.ws, &d)?)])
}

fn unwrap_tool(server: &Server, a: &Args, _p: &mut dyn Progress) -> Result<Vec<Value>, Failure> {
    let d = UnwrapDesc {
        input: a.mesh_input()?,
        import: a.import()?,
        unwrap: a.unwrap()?,
        out: a.path("out")?,
    };
    Ok(vec![text(unwrap(&server.ws, &d)?)])
}

fn collide_tool(server: &Server, a: &Args, _p: &mut dyn Progress) -> Result<Vec<Value>, Failure> {
    let d = CollideDesc {
        input: a.input(server)?,
        import: a.import()?,
        collide: shape_tools::Collide {
            max_hulls: a.uint("hulls")? as u32,
            concavity: a.num("concavity")?,
            max_vertices: a.uint("max_vertices")? as u32,
        },
        out: a.path("out")?,
    };
    Ok(vec![text(collide(&server.ws, &d)?)])
}

fn compress_tool(server: &Server, a: &Args, _p: &mut dyn Progress) -> Result<Vec<Value>, Failure> {
    let d = CompressDesc {
        input: a.mesh_input()?,
        out: a
            .path("out")?
            .ok_or_else(|| Failure::Usage("give `out`, where to write the GLB".into()))?,
    };
    Ok(vec![text(compress(&server.ws, &d)?)])
}

fn place_tool(server: &Server, a: &Args, _p: &mut dyn Progress) -> Result<Vec<Value>, Failure> {
    let d = PlaceDesc {
        input: a.mesh_input()?,
        import: a.import()?,
        place: a.place()?,
        out: a.path("out")?,
    };
    Ok(vec![text(place(&server.ws, &d)?)])
}

fn process_tool(server: &Server, a: &Args, p: &mut dyn Progress) -> Result<Vec<Value>, Failure> {
    let empty = Map::new();
    let none = Args { o: &empty };
    let d = ProcessDesc {
        input: a.mesh_input()?,
        import: a.import()?,
        place: a.sub("place")?.as_ref().unwrap_or(&none).place()?,
        repair: a.sub("repair")?.as_ref().unwrap_or(&none).repair()?,
        budget: a.uint("budget")? as usize,
        ratio: a.num("ratio")?,
        merge_charts: a.num("merge_charts")?,
        cross_color_borders: a.flag("cross_color_borders")?,
        unwrap: a.sub("unwrap")?.as_ref().unwrap_or(&none).unwrap()?,
        intersections: a.flag("intersections")?,
        fresh_uvs: a.flag("fresh_uvs")?,
        texture_max: a.u32("texture_max")?,
        ktx2: a.flag("ktx2")?,
        bake: a.u32("bake")?,
        bake_distance: a.num("bake_distance")?,
        out: a.path("out")?,
    };
    let total = STEPS.len();
    Ok(vec![text(process_steps(&server.ws, &d, &mut |k| {
        p.step(k, total, STEPS[k])
    })?)])
}

fn ts_tool(server: &Server, a: &Args, p: &mut dyn Progress) -> Result<Vec<Value>, Failure> {
    let src = match (a.string("source")?, a.path("path")?) {
        (Some(s), None) => TsSource::Text(s.to_string()),
        (None, Some(p)) => TsSource::Path(p),
        (Some(_), Some(_)) => {
            return Err(Failure::Usage("give `source` or `path`, not both".into()));
        }
        (None, None) => return Err(Failure::Usage("give `source` or `path`".into())),
    };
    p.step(0, 2, "run the program under bun")?;
    let run = run_ts(&server.ts, &server.ws, &src, &mut || p.poll())?;
    p.step(1, 2, "replay it natively and measure")?;
    // Replayed natively: the report and measures are the native kernel's.
    let source = load(
        &server.ws,
        &Input::Program(run.program.clone()),
        &Import::default(),
    )?;
    let Source::Program(model) = &source else {
        unreachable!("a program input loads a program")
    };
    let report = build_report(model)?;
    let measures = measure(&source, &MeasureDesc::default())?;
    let program_out = match a.path("program_out")? {
        None => "null".to_string(),
        Some(out) => {
            server.ws.write(&out, run.program.as_bytes())?;
            written(&out, run.program.len())
        }
    };
    Ok(vec![text(object(&[
        ("build", report),
        ("measures", measures),
        ("program", run.program),
        ("program_out", program_out),
        ("logs", string(&run.logs)),
    ]))])
}
