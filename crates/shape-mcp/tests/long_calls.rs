//! Long calls: progress notifications between steps, `ping` answered
//! while a call runs, and `notifications/cancelled` honoured before a
//! request runs, between `process`'s steps, and while bun runs.
//!
//! The client's input is gated so that each test is deterministic: the
//! lines after the gate are released when the server sends its first
//! progress notification, and the server's output waits until the reader
//! has passed them on.

use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use shape_mcp::{Server, TsConfig, Workspace};

const MESH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../golden/meshes/ball_defects.obj"
);
const BINDING: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../shape-wasm/ts");

#[derive(Default)]
struct State {
    /// Lines the reader may take.
    released: usize,
    /// Read calls so far.
    reads: usize,
}

type Gate = Arc<(Mutex<State>, Condvar)>;

/// The client's input: one line a read; lines from `gate_at` on wait
/// for the gate.
struct Input {
    lines: Vec<String>,
    next: usize,
    gate: Gate,
}

impl Read for Input {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let (m, cv) = &*self.gate;
        let mut s = m.lock().unwrap();
        s.reads += 1;
        cv.notify_all();
        while self.next < self.lines.len() && self.next >= s.released {
            s = cv.wait(s).unwrap();
        }
        let Some(line) = self.lines.get(self.next) else {
            return Ok(0);
        };
        let bytes = line.as_bytes();
        assert!(bytes.len() <= buf.len());
        buf[..bytes.len()].copy_from_slice(bytes);
        self.next += 1;
        Ok(bytes.len())
    }
}

/// The server's output. At the first progress notification it opens the
/// gate and waits until the reader has asked for `wait_reads` reads, so
/// every released line up to there is in the server's queue.
struct Output {
    bytes: Vec<u8>,
    gate: Gate,
    wait_reads: usize,
    opened: bool,
}

impl Write for Output {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let text = String::from_utf8_lossy(&self.bytes);
        if !self.opened && text.contains("notifications/progress") {
            self.opened = true;
            let (m, cv) = &*self.gate;
            let mut s = m.lock().unwrap();
            s.released = usize::MAX;
            cv.notify_all();
            while s.reads < self.wait_reads {
                s = cv.wait(s).unwrap();
            }
        }
        Ok(())
    }
}

/// Serves `lines` (each a message) with the gate before line `gate_at`;
/// the output opens it and waits for `wait_reads` reads. The messages
/// the server sent, in order.
fn serve(s: &Server, lines: &[Value], gate_at: usize, wait_reads: usize) -> Vec<Value> {
    let gate: Gate = Arc::new((
        Mutex::new(State {
            released: gate_at,
            reads: 0,
        }),
        Condvar::new(),
    ));
    let input = Input {
        lines: lines.iter().map(|l| format!("{l}\n")).collect(),
        next: 0,
        gate: gate.clone(),
    };
    let mut out = Output {
        bytes: Vec::new(),
        gate,
        wait_reads,
        opened: false,
    };
    shape_mcp::serve(BufReader::with_capacity(1 << 20, input), &mut out, s).unwrap();
    String::from_utf8(out.bytes)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn workspace(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("shape-mcp-long-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::copy(MESH, root.join("ball.obj")).unwrap();
    root
}

fn server(root: &Path) -> Server {
    Server {
        ws: Workspace::new(root, 1 << 22).unwrap(),
        ts: TsConfig {
            enabled: true,
            bun: PathBuf::new(),
            binding: PathBuf::from(BINDING),
            timeout: Duration::from_secs(60),
        },
    }
}

fn init() -> Value {
    json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "test", "version": "0"}}})
}

fn tool(id: u64, name: &str, arguments: Value, token: Option<Value>) -> Value {
    let mut params = json!({"name": name, "arguments": arguments});
    if let Some(t) = token {
        params["_meta"] = json!({"progressToken": t});
    }
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": params})
}

fn cancel(id: u64) -> Value {
    json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
        "params": {"requestId": id, "reason": "test"}})
}

fn ping(id: u64) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "ping"})
}

fn progress(out: &[Value]) -> Vec<(u64, String)> {
    out.iter()
        .filter(|m| m["method"] == "notifications/progress")
        .map(|m| {
            (
                m["params"]["progress"].as_u64().unwrap(),
                m["params"]["message"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn answered(out: &[Value]) -> Vec<u64> {
    out.iter().filter_map(|m| m["id"].as_u64()).collect()
}

#[test]
fn progress_ping_and_a_queued_cancellation() {
    let root = workspace("queued");
    let process = json!({"path": "ball.obj", "budget": 1000});
    let lines = [
        init(),
        tool(1, "process", process.clone(), Some(json!("p1"))),
        tool(2, "process", process, None),
        cancel(2),
        ping(3),
    ];
    // Everything after request 1 arrives while it runs (and the reader
    // reaches the end) before its second step.
    let out = serve(&server(&root), &lines, 2, lines.len() + 1);
    let steps: Vec<(u64, String)> = shape_cli::process::STEPS
        .iter()
        .enumerate()
        .map(|(k, s)| (k as u64, s.to_string()))
        .collect();
    assert_eq!(progress(&out), steps);
    for p in out
        .iter()
        .filter(|m| m["method"] == "notifications/progress")
    {
        assert_eq!(p["params"]["progressToken"], "p1");
        assert_eq!(p["params"]["total"], 6);
    }
    // The ping is answered while request 1 runs; request 2 never runs.
    assert_eq!(answered(&out), [0, 3, 1]);
    let r1 = out.iter().find(|m| m["id"] == 1).unwrap();
    assert_eq!(r1["result"]["isError"], false);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn process_cancelled_between_steps() {
    let root = workspace("between");
    let lines = [
        init(),
        tool(
            1,
            "process",
            json!({"path": "ball.obj", "budget": 1000, "out": "low.glb"}),
            Some(json!(7)),
        ),
        cancel(1),
        ping(2),
    ];
    // The cancellation is queued before the second step.
    let out = serve(&server(&root), &lines, 2, 4);
    assert_eq!(progress(&out), [(0, "import".to_string())]);
    // A cancelled request is not answered, and the chain stopped before
    // it wrote anything.
    assert_eq!(answered(&out), [0, 2]);
    assert!(!root.join("low.glb").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn typescript_cancelled_while_bun_runs() {
    let built = Path::new(BINDING).join("pkg/shape_wasm_bg.wasm").is_file();
    let bun = Command::new("bun").arg("--version").output().is_ok();
    if !built || !bun {
        eprintln!("typescript_cancelled_while_bun_runs: NOT RUN (needs bun and the built binding)");
        return;
    }
    let root = workspace("bun");
    let forever = "export default function (og) { for (;;) {} }";
    let lines = [
        init(),
        tool(
            1,
            "run_typescript",
            json!({"source": forever}),
            Some(json!("t")),
        ),
        cancel(1),
        ping(2),
    ];
    let start = Instant::now();
    let out = serve(&server(&root), &lines, 2, 4);
    // Stopped by the cancellation, well before the 60 s timeout.
    assert!(start.elapsed() < Duration::from_secs(30));
    assert_eq!(
        progress(&out),
        [(0, "run the program under bun".to_string())]
    );
    assert_eq!(answered(&out), [0, 2]);
    let _ = std::fs::remove_dir_all(&root);
}
