//! Runs an og-shape TypeScript program under bun and captures the program
//! it recorded.
//!
//! Entry: [`run`]. The author's file exports a default function that
//! takes the loaded kernel (`og`) and returns the `Shape` it built
//! (`guides/typescript.md`). `ts/run.ts` (embedded here) loads the
//! binding (`crates/shape-wasm/ts`), calls that function and writes
//! `shape.program()`: the operations the program applied, the same JSON
//! `shape build` replays. The server then replays that program natively,
//! so the report, measures, previews and files all come from the native
//! kernel; bun's process only decides what the operations are. What the
//! program prints is returned as its logs.
//!
//! bun is spawned in the workspace root with the server's own
//! permissions: the program is code, and it is not sandboxed. A server
//! started with `--no-typescript` refuses this tool.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use shape_cli::{Failure, Files};

use crate::workspace::Workspace;

/// How to run TypeScript. The zero value is turned off.
#[derive(Clone, Debug, Default)]
pub struct TsConfig {
    /// Whether the tool runs at all.
    pub enabled: bool,
    /// The bun executable; "" is `bun` on the PATH.
    pub bun: PathBuf,
    /// The binding's directory (`crates/shape-wasm/ts`), with `src/index.ts`
    /// and the built `pkg/`.
    pub binding: PathBuf,
    /// How long a program may run; zero is two minutes.
    pub timeout: Duration,
}

/// The program, as text or as a file in the workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsSource {
    /// The program's source.
    Text(String),
    /// A `.ts` file in the workspace (its relative imports work).
    Path(PathBuf),
}

/// What a run gave.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TsRun {
    /// The recorded program (`{"name", "ops"}`).
    pub program: String,
    /// What the program printed (stdout, then stderr), capped.
    pub logs: String,
}

const RUNNER: &str = include_str!("../ts/run.ts");
const LOG_CAP: usize = 16 * 1024;

fn scratch() -> Result<PathBuf, Failure> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "shape-mcp-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir)
        .map_err(|e| Failure::Failed(format!("{}: {e}", dir.display())))?;
    Ok(dir)
}

fn logs_of(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_default();
    let mut s = String::from_utf8_lossy(&bytes[..bytes.len().min(LOG_CAP)]).into_owned();
    if bytes.len() > LOG_CAP {
        s.push_str(&format!("\n… {} more bytes", bytes.len() - LOG_CAP));
    }
    s
}

/// Runs the program; its recorded program and logs, or why not.
pub fn run(cfg: &TsConfig, ws: &Workspace, src: &TsSource) -> Result<TsRun, Failure> {
    if !cfg.enabled {
        return Err(Failure::Failed(
            "run_typescript is turned off: the server was started with --no-typescript".into(),
        ));
    }
    let index = cfg.binding.join("src/index.ts");
    let wasm = cfg.binding.join("pkg/shape_wasm_bg.wasm");
    if !index.is_file() || !wasm.is_file() {
        return Err(Failure::Failed(format!(
            "the TypeScript binding is not built at {}: run crates/shape-wasm/build.sh \
             (needs wasm-bindgen-cli 0.2.130), or start the server with --binding <dir>",
            cfg.binding.display()
        )));
    }
    let dir = scratch()?;
    let result = run_in(cfg, ws, src, &index, &dir);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn run_in(
    cfg: &TsConfig,
    ws: &Workspace,
    src: &TsSource,
    index: &Path,
    dir: &Path,
) -> Result<TsRun, Failure> {
    let io = |p: &Path, e: std::io::Error| Failure::Failed(format!("{}: {e}", p.display()));
    let runner = dir.join("run.ts");
    std::fs::write(&runner, RUNNER).map_err(|e| io(&runner, e))?;
    let program = match src {
        TsSource::Text(text) => {
            if text.len() as u64 > ws.max_bytes() {
                return Err(Failure::Failed(format!(
                    "the program is {} bytes, over the cap of {} (--max-bytes)",
                    text.len(),
                    ws.max_bytes()
                )));
            }
            let p = dir.join("program.ts");
            std::fs::write(&p, text).map_err(|e| io(&p, e))?;
            p
        }
        TsSource::Path(p) => {
            ws.read(p)?; // confined, and within the cap
            ws.resolve(p)?
        }
    };
    let out = dir.join("program.json");
    let (stdout, stderr) = (dir.join("stdout.txt"), dir.join("stderr.txt"));
    let file = |p: &Path| std::fs::File::create(p).map_err(|e| io(p, e));
    let bun = if cfg.bun.as_os_str().is_empty() {
        PathBuf::from("bun")
    } else {
        cfg.bun.clone()
    };
    let mut child = Command::new(&bun)
        .arg(&runner)
        .arg(index)
        .arg(&program)
        .arg(&out)
        .current_dir(ws.root())
        .stdin(Stdio::null())
        .stdout(file(&stdout)?)
        .stderr(file(&stderr)?)
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Failure::Failed(format!(
                    "bun was not found ({}): install it from https://bun.sh, or start \
                     the server with --bun <path>",
                    bun.display()
                ))
            } else {
                Failure::Failed(format!("{}: {e}", bun.display()))
            }
        })?;
    let timeout = if cfg.timeout.is_zero() {
        Duration::from_secs(120)
    } else {
        cfg.timeout
    };
    let start = Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| io(&bun, e))? {
            Some(status) => break status,
            None if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Failure::Failed(format!(
                    "the program ran over {} s and was stopped",
                    timeout.as_secs()
                )));
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    let logs = format!("{}{}", logs_of(&stdout), logs_of(&stderr));
    if !status.success() {
        // bun prints a source excerpt before the message; lead with it.
        let errors: Vec<&str> = logs
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("error:") || l.starts_with("Error:"))
            .collect();
        return Err(Failure::Failed(format!(
            "the program failed ({status}): {}\n--- logs ---\n{logs}",
            errors.join("\n")
        )));
    }
    let program = std::fs::read_to_string(&out).map_err(|e| io(&out, e))?;
    Ok(TsRun { program, logs })
}
