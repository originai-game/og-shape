//! `shape-mcp`: an MCP server over og-shape, on stdio
//! (`docs/research.md` §12).
//!
//! Entry: [`serve`], which reads JSON-RPC 2.0 messages one per line and
//! answers each request on its own line, as MCP's stdio transport says.
//! It speaks `initialize`, `ping`, `tools/list`, `tools/call`,
//! `resources/list`, `resources/read` and `resources/templates/list`;
//! notifications are taken and not answered. Long calls send
//! `notifications/progress` between steps and honour
//! `notifications/cancelled` ([`serve`] says how).
//!
//! - [`tools`]: build, preview, measure, export, import, repair,
//!   decimate, unwrap, place, process and run_typescript, each calling the
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

use std::collections::{BTreeSet, VecDeque};
use std::io::{BufRead, Read, Write};
use std::sync::mpsc::{Receiver, Sender};

use shape_cli::Failure;

use serde_json::{Map, Value, json};

pub use tools::Progress;
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
(read resource og-shape://guides/shape.md first, then og-shape://guides/program.md \
for the JSON form or og-shape://guides/typescript.md for TypeScript), then `build` or `run_typescript` it, \
`preview` it and look, `measure` it, and `export` GLB, OBJ or STL. Meshes from elsewhere \
(glb, gltf, obj, fbx) go through `import`, `repair`, `decimate`, `unwrap`, `place` (up, \
real size, base pivot), or the whole chain with `process` (og-shape://guides/meshtools.md). Paths are relative to the \
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

fn dispatch(
    server: &Server,
    method: &str,
    p: &Map<String, Value>,
    progress: &mut dyn Progress,
) -> Result<Value, RpcError> {
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
            tools::call_with(server, name, &args, progress)
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

/// A message, read: a request (with an id), a notification, a response
/// to us, or a line that is not a message (answered with this error).
enum Message {
    Request {
        id: Value,
        method: String,
        params: Result<Map<String, Value>, RpcError>,
    },
    Notification {
        method: String,
        params: Map<String, Value>,
    },
    Ignored,
    Bad(String),
}

fn parse(line: &str) -> Message {
    let msg: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Message::Bad(response(
                &Value::Null,
                Err(rpc_error(-32700, format!("not JSON: {e}"))),
            ));
        }
    };
    let Some(msg) = msg.as_object() else {
        return Message::Bad(response(
            &Value::Null,
            Err(rpc_error(-32600, "a message is one JSON object")),
        ));
    };
    let method = msg.get("method").and_then(Value::as_str);
    if method.is_none() && (msg.contains_key("result") || msg.contains_key("error")) {
        // A response to us: nothing to answer.
        return Message::Ignored;
    }
    match (msg.get("id").cloned(), method) {
        (None, Some(method)) => Message::Notification {
            method: method.to_string(),
            params: params(msg).unwrap_or_default(),
        },
        (None, None) => Message::Ignored,
        (Some(id), None) => Message::Bad(response(
            &id,
            Err(rpc_error(-32600, "a request has a `method`")),
        )),
        (Some(id), Some(method)) => Message::Request {
            id,
            method: method.to_string(),
            params: params(msg),
        },
    }
}

/// Answers one message (one line of JSON): the response line, or None
/// for a notification. No progress is sent and nothing can cancel it;
/// [`serve`] does both.
pub fn handle(server: &Server, line: &str) -> Option<String> {
    match parse(line) {
        Message::Request { id, method, params } => Some(response(
            &id,
            params.and_then(|p| dispatch(server, &method, &p, &mut tools::Silent)),
        )),
        Message::Bad(answer) => Some(answer),
        Message::Notification { .. } | Message::Ignored => None,
    }
}

/// A line from the reader thread.
enum Line {
    Text(String),
    /// A line over the cap, skipped.
    TooLong(u64),
}

/// The reader thread: reads lines until `input` ends, and sends each.
fn read_lines(mut input: impl BufRead, cap: u64, tx: Sender<std::io::Result<Line>>) {
    loop {
        let mut line = Vec::new();
        let n = match input.by_ref().take(cap).read_until(b'\n', &mut line) {
            Ok(n) => n,
            Err(e) => {
                let _ = tx.send(Err(e));
                return;
            }
        };
        if n == 0 {
            return;
        }
        let sent = if n as u64 == cap && !line.ends_with(b"\n") {
            // Skip to the end of the oversized line.
            loop {
                let mut rest = Vec::new();
                match input.by_ref().take(cap).read_until(b'\n', &mut rest) {
                    Ok(m) if m > 0 && !rest.ends_with(b"\n") => continue,
                    _ => break,
                }
            }
            tx.send(Ok(Line::TooLong(cap)))
        } else {
            tx.send(Ok(Line::Text(
                String::from_utf8_lossy(&line).trim().to_string(),
            )))
        };
        if sent.is_err() {
            return;
        }
    }
}

/// The key of a request id, to match `notifications/cancelled`.
fn id_key(id: &Value) -> String {
    id.to_string()
}

/// The serving side of a session: lines from the reader thread, lines
/// taken while a call ran (answered after it), and the ids of requests
/// cancelled before they ran.
struct Session<W: Write> {
    rx: Receiver<std::io::Result<Line>>,
    out: W,
    waiting: VecDeque<Line>,
    cancelled: BTreeSet<String>,
}

impl<W: Write> Session<W> {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        writeln!(self.out, "{line}")?;
        self.out.flush()
    }

    /// Takes every line read so far without waiting: cancellations are
    /// noted, everything else kept for later; while a call runs
    /// (`in_call`), pings are answered at once.
    fn drain(&mut self, in_call: bool) -> std::io::Result<()> {
        while let Ok(line) = self.rx.try_recv() {
            let line = line?;
            if let Line::Text(text) = &line {
                match parse(text) {
                    Message::Notification { method, params }
                        if method == "notifications/cancelled" =>
                    {
                        if let Some(id) = params.get("requestId") {
                            self.cancelled.insert(id_key(id));
                        }
                        continue;
                    }
                    Message::Request { id, method, .. } if in_call && method == "ping" => {
                        self.send(&response(&id, Ok(json!({}))))?;
                        continue;
                    }
                    _ => {}
                }
            }
            self.waiting.push_back(line);
        }
        Ok(())
    }

    fn next(&mut self) -> Option<std::io::Result<Line>> {
        if let Some(line) = self.waiting.pop_front() {
            return Some(Ok(line));
        }
        self.rx.recv().ok()
    }

    fn run(&mut self, server: &Server) -> std::io::Result<()> {
        while let Some(line) = self.next() {
            let text = match line? {
                Line::TooLong(cap) => {
                    let answer = response(
                        &Value::Null,
                        Err(rpc_error(-32600, format!("a message over {cap} bytes"))),
                    );
                    self.send(&answer)?;
                    continue;
                }
                Line::Text(t) => t,
            };
            if text.is_empty() {
                continue;
            }
            let (id, method, params) = match parse(&text) {
                Message::Request { id, method, params } => (id, method, params),
                Message::Bad(answer) => {
                    self.send(&answer)?;
                    continue;
                }
                // A cancellation of a request already answered, or any
                // other notification: nothing to do.
                Message::Notification { .. } | Message::Ignored => continue,
            };
            // A cancellation read before the request ran: no answer.
            self.drain(false)?;
            if self.cancelled.remove(&id_key(&id)) {
                continue;
            }
            let token = params
                .as_ref()
                .ok()
                .and_then(|p| p.get("_meta"))
                .and_then(|m| m.get("progressToken"))
                .cloned();
            let mut call = Call {
                session: self,
                id: id_key(&id),
                token,
                cancelled: false,
                io: None,
            };
            let result = params.and_then(|p| dispatch(server, &method, &p, &mut call));
            let (cancelled, io) = (call.cancelled, call.io.take());
            if let Some(e) = io {
                return Err(e);
            }
            self.cancelled.remove(&id_key(&id));
            // A cancelled request is not answered (MCP: cancellation).
            if !cancelled {
                self.send(&response(&id, result))?;
            }
        }
        Ok(())
    }
}

/// One request being answered: its progress token, and whether it was
/// cancelled.
struct Call<'a, W: Write> {
    session: &'a mut Session<W>,
    id: String,
    token: Option<Value>,
    cancelled: bool,
    io: Option<std::io::Error>,
}

impl<W: Write> Call<'_, W> {
    fn check(&mut self) -> Result<(), Failure> {
        if let Err(e) = self.session.drain(true) {
            self.io = Some(e);
            return Err(Failure::Failed("the client's input failed".into()));
        }
        if self.session.cancelled.contains(&self.id) {
            self.cancelled = true;
            return Err(Failure::Failed("cancelled by the client".into()));
        }
        Ok(())
    }
}

impl<W: Write> Progress for Call<'_, W> {
    fn step(&mut self, k: usize, total: usize, what: &str) -> Result<(), Failure> {
        self.check()?;
        if let Some(token) = &self.token {
            let note = json!({
                "jsonrpc": "2.0",
                "method": "notifications/progress",
                "params": { "progressToken": token, "progress": k, "total": total, "message": what },
            });
            if let Err(e) = self.session.send(&note.to_string()) {
                self.io = Some(e);
                return Err(Failure::Failed("the client's output failed".into()));
            }
        }
        Ok(())
    }

    fn poll(&mut self) -> Result<(), Failure> {
        self.check()
    }
}

/// Serves until `input` ends: each line one message, each answer one
/// line on `output`. A line over the read cap (plus 1 MiB for the
/// envelope) is skipped and answered with an error.
///
/// A thread reads `input` (the kernel stays on this one), so that
/// between the steps of a long call the server sees what came in: a
/// `notifications/cancelled` for the call stops it before its next step
/// and it is not answered; a `ping` is answered at once; anything else
/// waits for the call to end. A request with `_meta.progressToken` gets
/// `notifications/progress` before each step of `process` and
/// `run_typescript`.
pub fn serve(
    input: impl BufRead + Send + 'static,
    output: impl Write,
    server: &Server,
) -> std::io::Result<()> {
    let cap = server.ws.max_bytes().saturating_add(1 << 20);
    let (tx, rx) = std::sync::mpsc::channel();
    // Not joined: it may sit in a read of the client's input after the
    // session ends, and goes with the process. The one thread is the
    // server's, which only reads lines: every kernel call stays on this
    // thread, one at a time.
    #[allow(clippy::disallowed_methods)]
    std::thread::spawn(move || read_lines(input, cap, tx));
    Session {
        rx,
        out: output,
        waiting: VecDeque::new(),
        cancelled: BTreeSet::new(),
    }
    .run(server)
}
