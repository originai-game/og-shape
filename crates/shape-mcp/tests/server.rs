//! The MCP server driven over stdio: JSON-RPC lines in, lines out, through
//! `shape_mcp::serve` in-process (and once through the binary).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};
use shape_mcp::{Server, TsConfig, Workspace};

const MESH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../golden/meshes/ball_defects.obj"
);
const BINDING: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../shape-wasm/ts");

/// A fresh workspace under the temp directory, with the committed mesh.
fn workspace(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("shape-mcp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("meshes")).unwrap();
    std::fs::copy(MESH, root.join("meshes/ball.obj")).unwrap();
    root
}

fn server(root: &Path, ts: TsConfig) -> Server {
    Server {
        ws: Workspace::new(root, 1 << 22).unwrap(),
        ts,
    }
}

/// Sends requests (ids 1, 2, …) after an initialize and its notification;
/// the responses by id.
fn session(s: &Server, calls: &[(&str, Value)]) -> Vec<Value> {
    let mut input = String::new();
    let init = json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "test", "version": "0"}}});
    input.push_str(&format!("{init}\n"));
    input.push_str("{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n");
    for (i, (method, params)) in calls.iter().enumerate() {
        let m = json!({"jsonrpc": "2.0", "id": i + 1, "method": method, "params": params});
        input.push_str(&format!("{m}\n"));
    }
    let mut out = Vec::new();
    shape_mcp::serve(std::io::Cursor::new(input.into_bytes()), &mut out, s).unwrap();
    let lines: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).expect("one JSON message a line"))
        .collect();
    // One answer per request, none for the notification.
    assert_eq!(lines.len(), calls.len() + 1);
    assert_eq!(lines[0]["result"]["protocolVersion"], "2025-06-18");
    for (i, l) in lines.iter().enumerate() {
        assert_eq!(l["id"], i);
        assert_eq!(l["jsonrpc"], "2.0");
    }
    lines[1..].to_vec()
}

fn call(name: &str, arguments: Value) -> (&'static str, Value) {
    ("tools/call", json!({"name": name, "arguments": arguments}))
}

/// A tool's JSON report (its first text content), which must not be an
/// error.
fn report(r: &Value) -> Value {
    assert_eq!(r["result"]["isError"], false, "{r}");
    let text = r["result"]["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "text")
        .unwrap()["text"]
        .as_str()
        .unwrap();
    serde_json::from_str(text).unwrap()
}

fn error_text(r: &Value) -> String {
    assert_eq!(r["result"]["isError"], true, "{r}");
    r["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_string()
}

const PLATE: &str = r#"{"name":"plate","ops":[
  {"op":"box","name":"plate","width":80,"depth":40,"height":6},
  {"op":"cylinder","name":"bore","radius":5,"height":20,"on":{"origin":[0,0,-5],"x":[1,0,0],"y":[0,1,0],"z":[0,0,1]}},
  {"op":"cut","target":0,"tool":1}]}"#;

#[test]
fn tools_resources_and_the_workspace() {
    let root = workspace("main");
    let s = server(&root, TsConfig::default());
    let program: Value = serde_json::from_str(PLATE).unwrap();
    let r = session(
        &s,
        &[
            ("tools/list", json!({})),
            ("resources/list", json!({})),
            (
                "resources/read",
                json!({"uri": "og-shape://guides/program.md"}),
            ),
            call("build", json!({"program": program})),
            call(
                "preview",
                json!({"program": PLATE, "width": 48, "views": 2, "out": "out/plate.png"}),
            ),
            call(
                "process",
                json!({"path": "meshes/ball.obj", "budget": 1000, "out": "out/ball.glb",
                       "repair": {"weld": 0.001, "floaters": {"fraction": 0.05}}}),
            ),
            call(
                "export",
                json!({"program": program, "out": "../escape.glb"}),
            ),
            call("measure", json!({"path": "/etc/hostname"})),
            call(
                "repair",
                json!({"path": "meshes/ball.obj", "colour": "red"}),
            ),
            ("tools/call", json!({"name": "no_such_tool"})),
            ("no/such/method", json!({})),
        ],
    );

    let tools: Vec<&str> = r[0]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            assert_eq!(t["inputSchema"]["type"], "object");
            t["name"].as_str().unwrap()
        })
        .collect();
    for t in [
        "build",
        "preview",
        "measure",
        "export",
        "import",
        "repair",
        "decimate",
        "unwrap",
        "process",
        "run_typescript",
    ] {
        assert!(tools.contains(&t), "{t}");
    }

    let uris: Vec<&str> = r[1]["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["uri"].as_str().unwrap())
        .collect();
    assert_eq!(
        uris,
        [
            "og-shape://guides/shape.md",
            "og-shape://guides/program.md",
            "og-shape://guides/typescript.md",
            "og-shape://guides/meshtools.md"
        ]
    );
    let guide = r[2]["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(guide.contains("\"op\""));

    // build: the native kernel's report and measures.
    let b = report(&r[3]);
    let model = shape_model::program::run(PLATE).unwrap();
    assert_eq!(
        b["measures"],
        serde_json::from_str::<Value>(&shape_model::program::measures_json(&model).unwrap())
            .unwrap()
    );
    assert_eq!(b["build"]["parts"][0]["name"], "plate");
    assert_eq!(b["measures"]["parts"][0]["closed"], true);

    // preview: a PNG as image content, saved in the workspace too.
    let content = r[4]["result"]["content"].as_array().unwrap();
    assert_eq!(content[0]["type"], "image");
    assert_eq!(content[0]["mimeType"], "image/png");
    assert!(
        content[0]["data"]
            .as_str()
            .unwrap()
            .starts_with("iVBORw0KGgo")
    );
    let p = report(&r[4]);
    assert_eq!(
        (p["width"].as_u64(), p["height"].as_u64()),
        (Some(96), Some(48))
    );
    let png = std::fs::read(root.join("out/plate.png")).unwrap();
    assert!(png.starts_with(b"\x89PNG"));
    assert_eq!(p["bytes"], png.len());

    // process on the committed mesh: repaired, within budget, unwrapped.
    let c = report(&r[5]);
    assert_eq!(c["repair"]["flipped"], 79);
    assert_eq!(c["repair"]["floaters"], 1);
    assert_eq!(c["measures"]["triangles"], 1000);
    assert_eq!(c["measures"]["closed"], true);
    assert_eq!(c["measures"]["uv"]["overlapping_pairs"], 0);
    assert!(
        std::fs::read(root.join("out/ball.glb"))
            .unwrap()
            .starts_with(b"glTF")
    );

    // Outside the root: refused, nothing written.
    assert!(error_text(&r[6]).contains("outside the workspace"));
    assert!(!root.parent().unwrap().join("escape.glb").exists());
    assert!(error_text(&r[7]).contains("refused"));
    // An argument not in the schema.
    assert!(error_text(&r[8]).contains("colour"));
    // Protocol errors.
    assert_eq!(r[9]["error"]["code"], -32602);
    assert_eq!(r[10]["error"]["code"], -32601);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn mesh_tools_and_the_size_cap() {
    let root = workspace("mesh");
    let s = server(&root, TsConfig::default());
    let r = session(
        &s,
        &[
            call("import", json!({"path": "meshes/ball.obj"})),
            call(
                "decimate",
                json!({"path": "meshes/ball.obj", "ratio": 0.5, "lods": 2, "out": "low.glb"}),
            ),
            call("unwrap", json!({"path": "low.lod1.glb", "out": "uv.glb"})),
            call("measure", json!({"path": "uv.glb"})),
        ],
    );
    assert_eq!(report(&r[0])["import"]["triangles"], 1984);
    let d = report(&r[1]);
    assert_eq!(d["levels"].as_array().unwrap().len(), 2);
    assert!(root.join("low.lod2.glb").exists());
    assert!(report(&r[2])["unwrap"]["charts"].as_u64().unwrap() > 0);
    assert_eq!(
        report(&r[3])["parts"][0]["measures"]["uv"]["overlapping_pairs"],
        0
    );

    // A file over the cap is refused before it is read.
    let small = Server {
        ws: Workspace::new(&root, 1000).unwrap(),
        ts: TsConfig::default(),
    };
    let r = session(
        &small,
        &[call("import", json!({"path": "meshes/ball.obj"}))],
    );
    assert!(error_text(&r[0]).contains("over the cap"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn typescript_without_bun_is_an_error_value() {
    let root = workspace("nobun");
    // A binding that looks built, and a bun that is not there.
    let binding = root.join("binding");
    std::fs::create_dir_all(binding.join("src")).unwrap();
    std::fs::create_dir_all(binding.join("pkg")).unwrap();
    std::fs::write(binding.join("src/index.ts"), "").unwrap();
    std::fs::write(binding.join("pkg/shape_wasm_bg.wasm"), "").unwrap();
    let ts = TsConfig {
        enabled: true,
        bun: root.join("no-such-bun"),
        binding,
        timeout: Duration::from_secs(10),
    };
    let r = session(
        &server(&root, ts),
        &[call(
            "run_typescript",
            json!({"source": "export default (og) => og.model({ name: \"x\" });"}),
        )],
    );
    assert!(error_text(&r[0]).contains("bun was not found"));
    let off = session(
        &server(&root, TsConfig::default()),
        &[call("run_typescript", json!({"source": "x"}))],
    );
    assert!(error_text(&off[0]).contains("--no-typescript"));
    let _ = std::fs::remove_dir_all(&root);
}

/// With bun and the built binding (the gate's binding step), a TypeScript
/// program's recorded program is replayed natively: its measures are the
/// native kernel's. Skipped, and said so, when either is missing.
#[test]
fn typescript_program_replays_natively() {
    let built = Path::new(BINDING).join("pkg/shape_wasm_bg.wasm").is_file();
    let bun = Command::new("bun").arg("--version").output().is_ok();
    if !built || !bun {
        eprintln!("typescript_program_replays_natively: NOT RUN (needs bun and the built binding)");
        return;
    }
    let root = workspace("ts");
    let ts = TsConfig {
        enabled: true,
        bun: PathBuf::new(),
        binding: PathBuf::from(BINDING),
        timeout: Duration::from_secs(60),
    };
    let source = r#"
export default function (og) {
  const s = og.model({ name: "plate" });
  const plate = s.box({ name: "plate", width: 80, depth: 40, height: 6 });
  s.cut(plate, s.cylinder({ name: "bore", radius: 5, height: 20, on: og.world.move(0, 0, -5) }));
  console.log("faces", s.names(plate).faces.join(" "));
  return s;
}"#;
    let r = session(
        &server(&root, ts),
        &[
            call(
                "run_typescript",
                json!({"source": source, "program_out": "plate.json"}),
            ),
            call(
                "run_typescript",
                json!({"source": "export default () => 1;"}),
            ),
        ],
    );
    let t = report(&r[0]);
    assert!(t["logs"].as_str().unwrap().contains("faces"));
    let program = std::fs::read_to_string(root.join("plate.json")).unwrap();
    assert_eq!(
        t["program"],
        serde_json::from_str::<Value>(&program).unwrap()
    );
    let model = shape_model::program::run(&program).unwrap();
    assert_eq!(
        t["measures"],
        serde_json::from_str::<Value>(&shape_model::program::measures_json(&model).unwrap())
            .unwrap()
    );
    assert!(error_text(&r[1]).contains("must return the Shape"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_binary_speaks_on_stdio() {
    let root = workspace("bin");
    let mut child = Command::new(env!("CARGO_BIN_EXE_shape-mcp"))
        .args(["--root", root.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":7,"method":"initialize","params":{{"protocolVersion":"1999-01-01"}}}}"#
    )
    .unwrap();
    writeln!(stdin, "not json").unwrap();
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":8,"method":"ping"}}"#).unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let lines: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 3);
    // An unknown version is answered with the newest.
    assert_eq!(
        lines[0]["result"]["protocolVersion"],
        shape_mcp::PROTOCOL_VERSIONS[0]
    );
    assert_eq!(lines[1]["error"]["code"], -32700);
    assert_eq!(lines[2]["id"], 8);
    let _ = std::fs::remove_dir_all(&root);
}

/// The fenced block of `lang` in a guide.
fn example(guide: &str, lang: &str) -> String {
    let start = guide.find(&format!("```{lang}\n")).unwrap() + lang.len() + 4;
    guide[start..start + guide[start..].find("```").unwrap()].to_string()
}

/// The guides' examples build: the JSON one natively, the TypeScript one
/// through bun when it is there (as above).
#[test]
fn guide_examples_build() {
    let root = workspace("guides");
    let program = example(include_str!("../../../guides/program.md"), "json");
    let r = session(
        &server(&root, TsConfig::default()),
        &[call("build", json!({ "program": program }))],
    );
    assert_eq!(report(&r[0])["measures"]["parts"][0]["closed"], true);

    let built = Path::new(BINDING).join("pkg/shape_wasm_bg.wasm").is_file();
    if built && Command::new("bun").arg("--version").output().is_ok() {
        let ts = TsConfig {
            enabled: true,
            binding: PathBuf::from(BINDING),
            ..TsConfig::default()
        };
        let source = example(include_str!("../../../guides/typescript.md"), "ts");
        let r = session(
            &server(&root, ts),
            &[call("run_typescript", json!({ "source": source }))],
        );
        let t = report(&r[0]);
        assert_eq!(t["measures"]["parts"][0]["closed"], true);
        assert_eq!(t["measures"]["parts"][0]["components"], 1);
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn fonts_a_program_names_stay_in_the_workspace() {
    let root = workspace("fonts");
    let s = server(&root, TsConfig::default());
    let text = |font: &str| {
        json!({"name": "t", "ops": [{"op": "extrude", "name": "t", "height": 1,
            "profile": {"text": {"text": "A", "font": font, "size": 5}}}]})
    };
    let r = session(
        &s,
        &[
            call("build", json!({"program": text("../outside.ttf")})),
            call("build", json!({"program": text("meshes/ball.obj")})),
        ],
    );
    assert!(
        error_text(&r[0]).contains("outside the workspace"),
        "{}",
        r[0]
    );
    // Inside the workspace it is read (and is not a font).
    assert!(error_text(&r[1]).contains("not a TrueType"), "{}", r[1]);
    let _ = std::fs::remove_dir_all(&root);
}
