//! `shape-mcp`: the og-shape MCP server on stdio.
//!
//! ```text
//! shape-mcp --root <dir> [--max-bytes N] [--bun <path>] [--binding <dir>]
//!           [--ts-timeout SECONDS] [--no-typescript]
//! ```
//!
//! Every path a tool takes is confined to `--root`; files read and inline
//! programs are capped at `--max-bytes` (64 MiB by default). `--binding`
//! is the TypeScript binding's directory (default: this checkout's
//! `crates/shape-wasm/ts`), `--bun` the bun executable (default: `bun`
//! on the PATH). Register it with an MCP client as `guides/mcp.md` says.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use shape_mcp::{Server, TsConfig, Workspace};

const USAGE: &str = "usage: shape-mcp --root <dir> [--max-bytes N] [--bun <path>] \
[--binding <dir>] [--ts-timeout SECONDS] [--no-typescript]
  An MCP server (JSON-RPC 2.0 on stdin and stdout) over og-shape. Every path
  is confined to --root. See guides/mcp.md.";

fn parse(argv: &[String]) -> Result<Server, String> {
    let mut root = None;
    let mut max_bytes: u64 = 64 << 20;
    let mut ts = TsConfig {
        enabled: true,
        bun: PathBuf::new(),
        binding: PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../shape-wasm/ts")),
        timeout: Duration::from_secs(120),
    };
    let mut i = 0;
    while i < argv.len() {
        let flag = argv[i].as_str();
        let value = || {
            argv.get(i + 1)
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag {
            "--root" => root = Some(PathBuf::from(value()?)),
            "--max-bytes" => {
                max_bytes = value()?
                    .parse()
                    .map_err(|_| "--max-bytes: a whole number".to_string())?;
            }
            "--bun" => ts.bun = PathBuf::from(value()?),
            "--binding" => ts.binding = PathBuf::from(value()?),
            "--ts-timeout" => {
                let s: u64 = value()?
                    .parse()
                    .map_err(|_| "--ts-timeout: whole seconds".to_string())?;
                ts.timeout = Duration::from_secs(s);
            }
            "--no-typescript" => {
                ts.enabled = false;
                i += 1;
                continue;
            }
            other => return Err(format!("unknown option {other}")),
        }
        i += 2;
    }
    let root = root.ok_or("--root <dir> is required")?;
    Ok(Server {
        ws: Workspace::new(&root, max_bytes)?,
        ts,
    })
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let server = match parse(&argv) {
        Ok(s) => s,
        Err(why) => {
            eprintln!("shape-mcp: {why}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    eprintln!("shape-mcp: serving {}", server.ws.root().display());
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    match shape_mcp::serve(stdin.lock(), stdout.lock(), &server) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("shape-mcp: {e}");
            ExitCode::from(1)
        }
    }
}
