//! `shape-mcp`: an MCP server over og-shape, on stdio
//! (`docs/research.md` §12).
//!
//! Entry: [`serve`], which reads JSON-RPC 2.0 messages one per line and
//! answers each request on its own line, as MCP's stdio transport says.
//! It speaks `initialize`, `ping`, `tools/list`, `tools/call`,
//! `resources/list`, `resources/read` and `resources/templates/list`;
//! notifications are taken and not answered.
//!
//! - [`tools`]: build, preview, measure, export, import, repair,
//!   decimate, unwrap, process and run_typescript, each calling the
//!   `shape_cli` command of the same name.
//! - [`resources`]: the authoring guides (`guides/`).
//! - [`Workspace`]: every path is confined to the root given at start,
//!   and files read are capped.
//! - [`ts`]: TypeScript programs under bun, replayed natively.
//!
//! The server is a front end, not kernel: it does I/O and spawns bun, but
//! every report, image and file it returns comes from the deterministic
//! kernel. The protocol layer is written here over serde_json
//! (`docs/research.md` §12 says why no SDK).

mod base64;
pub mod resources;
pub mod tools;
pub mod ts;
mod workspace;

use std::io::{BufRead, Read, Write};

use serde_json::{Map, Value, json};

pub use ts::{TsConfig, TsSource};
pub use workspace::Workspace;

/// The protocol versions the server speaks, newest first. It answers a
/// client's version if it is one of these, else the newest.
pub const PROTOCOL_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// What the server works on: its workspace and how it runs TypeScript.
#[derive(Clone, Debug)]
pub struct Server {
    /// The root every path is confined to, and the read cap.
    pub ws: Workspace,
    /// The `run_typescript` tool's settings.
    pub ts: TsConfig,
}

const INSTRUCTIONS: &str = "og-shape: 3D modeling for LLMs. Write a model as a program \
(read resource og-shape://guides/program.md for the JSON form, or \
og-shape://guides/typescript.md for TypeScript), then `build` or `run_typescript` it, \
`preview` it and look, `measure` it, and `export` GLB, OBJ or STL. Meshes from elsewhere \
(glb, gltf, obj, fbx) go through `import`, `repair`, `decimate`, `unwrap`, or the whole \
chain with `process` (og-shape://guides/meshtools.md). Paths are relative to the \
workspace root. Tools report measurements, never verdicts: you judge them.";

/// A JSON-RPC error.
struct RpcError {
    code: i64,
    message: String,
}

fn rpc_error(code: i64, message: impl Into<String>) -> RpcError {
    RpcError {
        code,
        message: message.into(),
    }
}

fn response(id: &Value, result: Result<Value, RpcError>) -> String {
    match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err(e) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": e.code, "message": e.message },
        }),
    }
    .to_string()
}

fn params(msg: &Map<String, Value>) -> Result<Map<String, Value>, RpcError> {
    match msg.get("params") {
        None | Some(Value::Null) => Ok(Map::new()),
        Some(Value::Object(o)) => Ok(o.clone()),
        Some(_) => Err(rpc_error(-32602, "params must be an object")),
    }
}

fn dispatch(server: &Server, method: &str, p: &Map<String, Value>) -> Result<Value, RpcError> {
    match method {
        "initialize" => {
            let asked = p.get("protocolVersion").and_then(Value::as_str);
            let version = asked
                .filter(|v| PROTOCOL_VERSIONS.contains(v))
                .unwrap_or(PROTOCOL_VERSIONS[0]);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "listChanged": false, "subscribe": false },
                },
                "serverInfo": {
                    "name": "og-shape",
                    "title": "og-shape: 3D modeling for LLMs",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "instructions": INSTRUCTIONS,
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools::list()),
        "tools/call" => {
            let name = p
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| rpc_error(-32602, "tools/call needs a tool `name`"))?;
            let args = match p.get("arguments") {
                None | Some(Value::Null) => Map::new(),
                Some(Value::Object(o)) => o.clone(),
                Some(_) => return Err(rpc_error(-32602, "`arguments` must be an object")),
            };
            tools::call(server, name, &args)
                .ok_or_else(|| rpc_error(-32602, format!("no tool {name:?}")))
        }
        "resources/list" => Ok(resources::list()),
        "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
        "resources/read" => {
            let uri = p
                .get("uri")
                .and_then(Value::as_str)
                .ok_or_else(|| rpc_error(-32602, "resources/read needs a `uri`"))?;
            // -32002: resource not found (the MCP specification).
            resources::read(uri).ok_or_else(|| rpc_error(-32002, format!("no resource {uri:?}")))
        }
        other => Err(rpc_error(-32601, format!("no method {other:?}"))),
    }
}

/// Answers one message (one line of JSON): the response line, or None
/// for a notification.
pub fn handle(server: &Server, line: &str) -> Option<String> {
    let msg: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(response(
                &Value::Null,
                Err(rpc_error(-32700, format!("not JSON: {e}"))),
            ));
        }
    };
    let Some(msg) = msg.as_object() else {
        return Some(response(
            &Value::Null,
            Err(rpc_error(-32600, "a message is one JSON object")),
        ));
    };
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str);
    if method.is_none() && (msg.contains_key("result") || msg.contains_key("error")) {
        // A response to us: nothing to answer.
        return None;
    }
    match (id, method) {
        // A notification.
        (None, _) => None,
        (Some(id), None) => Some(response(
            &id,
            Err(rpc_error(-32600, "a request has a `method`")),
        )),
        (Some(id), Some(method)) => {
            let result = params(msg).and_then(|p| dispatch(server, method, &p));
            Some(response(&id, result))
        }
    }
}

/// Serves until `input` ends: each line one message, each answer one
/// line on `output`. A line over the read cap (plus 1 MiB for the
/// envelope) is skipped and answered with an error.
pub fn serve(
    mut input: impl BufRead,
    mut output: impl Write,
    server: &Server,
) -> std::io::Result<()> {
    let cap = server.ws.max_bytes().saturating_add(1 << 20);
    loop {
        let mut line = Vec::new();
        let n = input.by_ref().take(cap).read_until(b'\n', &mut line)?;
        if n == 0 {
            return Ok(());
        }
        if n as u64 == cap && !line.ends_with(b"\n") {
            // Skip to the end of the oversized line.
            loop {
                let mut rest = Vec::new();
                let m = input.by_ref().take(cap).read_until(b'\n', &mut rest)?;
                if m == 0 || rest.ends_with(b"\n") {
                    break;
                }
            }
            let answer = response(
                &Value::Null,
                Err(rpc_error(-32600, format!("a message over {cap} bytes"))),
            );
            writeln!(output, "{answer}")?;
            output.flush()?;
            continue;
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(answer) = handle(server, text) {
            writeln!(output, "{answer}")?;
            output.flush()?;
        }
    }
}
