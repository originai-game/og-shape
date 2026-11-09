//! Phase 0's boolean benchmark (`docs/plan.md`): og-geom's CSG against
//! manifold-rust, on sand3d's booleans, Thingi10K's closed meshes and
//! synthetic AI-mesh defects, plus a scaling sweep.
//!
//! ```text
//! cargo run --release -- [--thingi DIR] [--suite NAME]... [--grid N]
//!                        [--timeout SECS] [--out FILE] [--hashes FILE]
//! ```
//!
//! For each case and engine it runs union, intersect and subtract and
//! records one verdict:
//! - `ok`: all three returned, every output is closed, and the volume
//!   identities hold: V(A∪B) + V(A∩B) = V(A) + V(B) and V(A−B) = V(A) −
//!   V(A∩B), to 1e-6 of the operands' size (only the closure is checked
//!   where an input has no well-defined volume);
//! - `refused`: the engine returned an error value;
//! - `wrong`: it returned a mesh that is open or breaks an identity;
//! - `panic`, `timeout`.
//!
//! Output: one TSV row per case and engine, a summary per suite, and with
//! `--hashes` the output hash of every boolean, for comparing hosts.

mod engines;
mod mesh;
mod suites;

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use engines::{Engine, Op, run};
use suites::Case;

struct Args {
    thingi: Option<PathBuf>,
    suites: Vec<String>,
    grid: usize,
    timeout: u64,
    out: Option<PathBuf>,
    hashes: Option<PathBuf>,
    inputs: bool,
}

fn args() -> Args {
    let mut a = Args {
        thingi: None,
        suites: Vec::new(),
        grid: 32,
        timeout: 60,
        out: None,
        hashes: None,
        inputs: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut v = || it.next().unwrap_or_else(|| panic!("{k} needs a value"));
        match k.as_str() {
            "--thingi" => a.thingi = Some(v().into()),
            "--suite" => a.suites.push(v()),
            "--grid" => a.grid = v().parse().expect("--grid N"),
            "--timeout" => a.timeout = v().parse().expect("--timeout SECS"),
            "--out" => a.out = Some(v().into()),
            "--hashes" => a.hashes = Some(v().into()),
            "--inputs" => a.inputs = true,
            _ => panic!("unknown argument {k}"),
        }
    }
    a
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Verdict {
    Ok,
    Refused,
    Wrong,
    Panic,
    Timeout,
}

struct Row {
    verdict: Verdict,
    ms: f64,
    tris: usize,
    detail: String,
    hashes: Vec<(Op, String)>,
}

fn evaluate(case: &Case, engine: Engine) -> Row {
    let mut ms = 0.0;
    let mut vols = Vec::new();
    let mut detail = String::new();
    let mut tris = 0;
    let mut hashes = Vec::new();
    let mut verdict = Verdict::Ok;
    for op in Op::ALL {
        let r = run(engine, &case.a, &case.b, op);
        ms += r.ms;
        match r.result {
            Err(e) => {
                hashes.push((op, "error".to_string()));
                return Row {
                    verdict: Verdict::Refused,
                    ms,
                    tris,
                    detail: format!("{op:?}: {e}"),
                    hashes,
                };
            }
            Ok(m) => {
                hashes.push((op, format!("{:016x}", m.hash())));
                if op == Op::Union {
                    tris = m.tris();
                }
                if !m.closed() {
                    verdict = Verdict::Wrong;
                    let _ = write!(detail, "{op:?} open; ");
                } else if !m.manifold() {
                    let _ = write!(detail, "{op:?} non-manifold; ");
                }
                vols.push(m.volume());
            }
        }
    }
    if let (Some(va), Some(vb)) = (case.vol_a, case.vol_b) {
        let (u, i, s) = (vols[0], vols[1], vols[2]);
        let scale = va.abs().max(vb.abs()).max(1e-30);
        let e1 = (u + i - va - vb).abs() / scale;
        let e2 = (s - (va - i)).abs() / scale;
        if e1 > 1e-6 || e2 > 1e-6 {
            verdict = Verdict::Wrong;
            let _ = write!(detail, "identities off by {e1:.1e}, {e2:.1e}; ");
        }
    }
    Row {
        verdict,
        ms,
        tris,
        detail,
        hashes,
    }
}

/// Runs one case on one engine with a timeout and panic capture. On wasm32
/// there are no threads: no timeout, panics abort.
fn guarded(case: &Case, engine: Engine, timeout: u64) -> Row {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::{Arc, mpsc};
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new((
            case.a.clone(),
            case.b.clone(),
            case.vol_a,
            case.vol_b,
            case.name.clone(),
        ));
        let job = shared.clone();
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(move || {
                let c = Case {
                    suite: "",
                    name: job.4.clone(),
                    a: job.0.clone(),
                    b: job.1.clone(),
                    vol_a: job.2,
                    vol_b: job.3,
                };
                let r =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| evaluate(&c, engine)));
                let _ = tx.send(r);
            })
            .expect("spawn");
        match rx.recv_timeout(Duration::from_secs(timeout)) {
            Ok(Ok(row)) => row,
            Ok(Err(p)) => {
                let msg = p
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                Row {
                    verdict: Verdict::Panic,
                    ms: 0.0,
                    tris: 0,
                    detail: msg.lines().next().unwrap_or("").chars().take(120).collect(),
                    hashes: Vec::new(),
                }
            }
            Err(_) => Row {
                verdict: Verdict::Timeout,
                ms: timeout as f64 * 1000.0,
                tris: 0,
                detail: String::new(),
                hashes: Vec::new(),
            },
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = timeout;
        evaluate(case, engine)
    }
}

fn main() {
    let a = args();
    std::panic::set_hook(Box::new(|_| {}));
    let want = |s: &str| a.suites.is_empty() || a.suites.iter().any(|x| x == s);
    let mut cases = Vec::new();
    if want("sand3d") {
        cases.extend(suites::sand3d());
    }
    if want("thingi10k") {
        if let Some(dir) = &a.thingi {
            cases.extend(suites::thingi(dir));
        }
    }
    if want("synthetic") {
        cases.extend(suites::synthetic(a.grid));
    }
    if want("scaling") {
        cases.extend(suites::scaling());
    }
    if a.inputs {
        for c in &cases {
            let d = |t: &mesh::Tri| {
                format!(
                    "tris {} closed {} manifold {} degenerate {}",
                    t.tris(),
                    t.closed(),
                    t.manifold(),
                    t.degenerate()
                )
            };
            println!("{} {}: A {} | B {}", c.suite, c.name, d(&c.a), d(&c.b));
        }
        return;
    }
    let mut tsv =
        String::from("suite\tcase\ttris_a\ttris_b\tengine\tverdict\tms\ttris_out\tdetail\n");
    let mut hashes = String::new();
    // (suite, engine) -> verdict counts and total ms of ok cases.
    let mut summary: std::collections::BTreeMap<(String, &str), ([usize; 5], f64, usize)> =
        Default::default();
    for case in &cases {
        for engine in Engine::ALL {
            let row = guarded(case, engine, a.timeout);
            eprintln!(
                "{:<10} {:<34} {:<16} {:?} {:.1} ms {}",
                case.suite,
                case.name,
                engine.name(),
                row.verdict,
                row.ms,
                row.detail
            );
            let _ = writeln!(
                tsv,
                "{}\t{}\t{}\t{}\t{}\t{:?}\t{:.2}\t{}\t{}",
                case.suite,
                case.name,
                case.a.tris(),
                case.b.tris(),
                engine.name(),
                row.verdict,
                row.ms,
                row.tris,
                row.detail.trim()
            );
            for (op, h) in &row.hashes {
                let _ = writeln!(
                    hashes,
                    "{} {} {} {:?} {}",
                    case.suite,
                    case.name,
                    engine.name(),
                    op,
                    h
                );
            }
            let e = summary
                .entry((case.suite.to_string(), engine.name()))
                .or_default();
            e.0[row.verdict as usize] += 1;
            if row.verdict == Verdict::Ok {
                e.1 += row.ms;
                e.2 += 1;
            }
        }
    }
    println!("| suite | engine | ok | refused | wrong | panic | timeout | mean ms (ok) |");
    println!("|---|---|---|---|---|---|---|---|");
    for ((suite, engine), (n, ms, k)) in &summary {
        let mean = if *k > 0 {
            format!("{:.1}", ms / *k as f64)
        } else {
            "-".into()
        };
        println!(
            "| {suite} | {engine} | {} | {} | {} | {} | {} | {mean} |",
            n[0], n[1], n[2], n[3], n[4]
        );
    }
    if let Some(p) = &a.out {
        std::fs::write(p, tsv).expect("write --out");
    }
    if let Some(p) = &a.hashes {
        std::fs::write(p, hashes).expect("write --hashes");
    }
    #[cfg(not(target_arch = "wasm32"))]
    std::process::exit(0); // timed-out threads may still be running
}
