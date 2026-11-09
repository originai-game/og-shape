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
use shape_cli::decimate::{DecimateDesc, decimate};
use shape_cli::export::{export, written};
use shape_cli::import::{ImportDesc, import};
use shape_cli::measure::{MeasureDesc, measure};
use shape_cli::preview::{PreviewDesc, render};
use shape_cli::process::{ProcessDesc, process};
use shape_cli::repair::{RepairDesc, repair};
use shape_cli::report::{object, string};
use shape_cli::source::{Source, load};
use shape_cli::unwrap::{UnwrapDesc, unwrap};
use shape_cli::{Failure, Files, Input};
use shape_tools::{Decimate, Floaters, Import, Preview, Repair, Unwrap};

use crate::Server;
use crate::ts::{TsSource, run as run_ts};

/// One tool.
struct Tool {
    name: &'static str,
    description: &'static str,
    schema: fn() -> Value,
    handler: fn(&Server, &Args) -> Result<Vec<Value>, Failure>,
}

const TOOLS: [Tool; 10] = [
    Tool {
        name: "build",
        description: "Build a model program (the JSON form of resource og-shape://guides/program.md) \
            and report its parts, joints and measures (counts, bounds, area, volume, closed, \
            non-manifold edges; no thresholds). Give `program` inline or a `.json` `path` in the \
            workspace.",
        schema: build_schema,
        handler: build_tool,
    },
    Tool {
        name: "preview",
        description: "Render a turntable of a program or a mesh file with the kernel's CPU \
            rasterizer: one PNG with the views side by side. Look at it after every change.",
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
        name: "process",
        description: "The mesh tools' chain on one file: import → join → repair → decimate to \
            `budget` triangles → unwrap → measure → export (`out`, a plain GLB with UVs). One \
            report of every step.",
        schema: process_schema,
        handler: process_tool,
    },
    Tool {
        name: "run_typescript",
        description: "Run a TypeScript program through the og-shape binding under bun \
            (resource og-shape://guides/typescript.md): its default export takes `og` and \
            returns the Shape it built. The program it recorded is replayed by the native kernel, \
            which gives the report and measures; the program JSON comes back too, for `build`, \
            `preview` and `export`. The program runs with the server's permissions.",
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

/// Runs a tool: its `tools/call` result, or None for a tool that does
/// not exist.
pub fn call(server: &Server, name: &str, args: &Map<String, Value>) -> Option<Value> {
    let tool = TOOLS.iter().find(|t| t.name == name)?;
    let schema = (tool.schema)();
    let result = check_keys(&schema, args).and_then(|()| (tool.handler)(server, &Args { o: args }));
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
            max_hole_edges: self.uint("max_hole_edges")? as usize,
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
    ]
}

fn obj_props() -> Vec<(&'static str, Value)> {
    vec![
        (
            "unit_mm",
            num("OBJ only: millimetres per file unit; 0 reads metres (1000), as glTF."),
        ),
        ("z_up", boolean("OBJ only: the file is Z up already.")),
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
            "max_hole_edges",
            int("Close holes whose rim has at most this many edges (0: none)."),
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
            num("Radians above the horizon; absent is 25 degrees."),
        ),
        (
            "azimuth",
            num("Radians from the front (the kernel's -Y side)."),
        ),
        ("no_edges", boolean("Do not draw edges.")),
        ("no_ground", boolean("Do not draw the ground shadow.")),
        (
            "out",
            path("Also save the PNG at this path in the workspace."),
        ),
    ]);
    p.extend(obj_props());
    schema(p, &[])
}

fn measure_schema() -> Value {
    let mut p = program_props();
    p.push((
        "intersections",
        boolean("Meshes: count self-intersecting triangle pairs (exact; slower)."),
    ));
    p.extend(obj_props());
    schema(p, &[])
}

fn export_schema() -> Value {
    let mut p = program_props();
    p.push((
        "out",
        path("Where to write, in the workspace: .glb, .obj or .stl."),
    ));
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
        (
            "lods",
            int("Make this many levels, each decimated from the one before; 0 is one."),
        ),
    ]);
    schema(p, &["path"])
}

fn unwrap_schema() -> Value {
    let mut p = mesh_props("Write a plain GLB with the UVs here (.glb; .obj and .stl carry none).");
    p.extend(unwrap_props());
    schema(p, &["path"])
}

fn process_schema() -> Value {
    let mut p = mesh_props("Write the result here: .glb (plain, with UVs), .obj or .stl.");
    p.extend([
        (
            "budget",
            int("Decimate to at most this many triangles; 0 does not decimate."),
        ),
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
            "intersections",
            boolean("Count self-intersecting triangle pairs in the final measures (exact)."),
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
                path("Instead of `source`: a .ts file in the workspace (relative imports work)."),
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
    load(&server.ws, &a.input(server)?, &a.import()?)
}

fn build_tool(server: &Server, a: &Args) -> Result<Vec<Value>, Failure> {
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

fn preview_tool(server: &Server, a: &Args) -> Result<Vec<Value>, Failure> {
    let d = PreviewDesc {
        preview: Preview {
            width: a.u32("width")?,
            height: a.u32("height")?,
            views: a.u32("views")?,
            elevation: a.opt_num("elevation")?,
            azimuth: a.num("azimuth")?,
            no_edges: a.flag("no_edges")?,
            no_ground: a.flag("no_ground")?,
        },
        columns: a.u32("columns")?,
    };
    let png = render(&loaded(server, a)?, &d)?;
    let output = match a.path("out")? {
        None => "null".to_string(),
        Some(out) => {
            server.ws.write(&out, &png)?;
            written(&out, png.len())
        }
    };
    let (w, h) = png_size(&png);
    Ok(vec![
        json!({
            "type": "image",
            "data": crate::base64::encode(&png),
            "mimeType": "image/png",
        }),
        text(object(&[
            ("width", w.to_string()),
            ("height", h.to_string()),
            ("bytes", png.len().to_string()),
            ("output", output),
        ])),
    ])
}

fn measure_tool(server: &Server, a: &Args) -> Result<Vec<Value>, Failure> {
    let d = MeasureDesc {
        intersections: a.flag("intersections")?,
    };
    Ok(vec![text(measure(&loaded(server, a)?, &d)?)])
}

fn export_tool(server: &Server, a: &Args) -> Result<Vec<Value>, Failure> {
    let out = a
        .path("out")?
        .ok_or_else(|| Failure::Usage("give `out`".into()))?;
    Ok(vec![text(export(&server.ws, &loaded(server, a)?, &out)?)])
}

fn import_tool(server: &Server, a: &Args) -> Result<Vec<Value>, Failure> {
    let d = ImportDesc {
        input: a.mesh_input()?,
        import: a.import()?,
        out: a.path("out")?,
    };
    Ok(vec![text(import(&server.ws, &d)?)])
}

fn repair_tool(server: &Server, a: &Args) -> Result<Vec<Value>, Failure> {
    let d = RepairDesc {
        input: a.mesh_input()?,
        import: a.import()?,
        repair: a.repair()?,
        out: a.path("out")?,
    };
    Ok(vec![text(repair(&server.ws, &d)?)])
}

fn decimate_tool(server: &Server, a: &Args) -> Result<Vec<Value>, Failure> {
    let d = DecimateDesc {
        input: a.mesh_input()?,
        import: a.import()?,
        decimate: Decimate {
            ratio: a.num("ratio")?,
            target: a.uint("target")? as usize,
            max_error: a.num("max_error")?,
            lock_border: a.flag("lock_border")?,
        },
        lods: a.u32("lods")?,
        out: a.path("out")?,
    };
    Ok(vec![text(decimate(&server.ws, &d)?)])
}

fn unwrap_tool(server: &Server, a: &Args) -> Result<Vec<Value>, Failure> {
    let d = UnwrapDesc {
        input: a.mesh_input()?,
        import: a.import()?,
        unwrap: a.unwrap()?,
        out: a.path("out")?,
    };
    Ok(vec![text(unwrap(&server.ws, &d)?)])
}

fn process_tool(server: &Server, a: &Args) -> Result<Vec<Value>, Failure> {
    let empty = Map::new();
    let none = Args { o: &empty };
    let d = ProcessDesc {
        input: a.mesh_input()?,
        import: a.import()?,
        repair: a.sub("repair")?.as_ref().unwrap_or(&none).repair()?,
        budget: a.uint("budget")? as usize,
        unwrap: a.sub("unwrap")?.as_ref().unwrap_or(&none).unwrap()?,
        intersections: a.flag("intersections")?,
        out: a.path("out")?,
    };
    Ok(vec![text(process(&server.ws, &d)?)])
}

fn ts_tool(server: &Server, a: &Args) -> Result<Vec<Value>, Failure> {
    let src = match (a.string("source")?, a.path("path")?) {
        (Some(s), None) => TsSource::Text(s.to_string()),
        (None, Some(p)) => TsSource::Path(p),
        (Some(_), Some(_)) => {
            return Err(Failure::Usage("give `source` or `path`, not both".into()));
        }
        (None, None) => return Err(Failure::Usage("give `source` or `path`".into())),
    };
    let run = run_ts(&server.ts, &server.ws, &src)?;
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
