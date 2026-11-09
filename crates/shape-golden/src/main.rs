//! Prints `<case> <sha256>` for every golden case, sorted by name.
//!
//! The gate (`scripts/check.sh`) runs this on every target, natively and
//! as wasm32-wasip1 under wasmtime, and compares the output with
//! `golden/hashes.txt` byte for byte. Each crate adds its operations as
//! cases here.

mod bevel;
mod cases;
mod gen2;
mod geom;
mod mcp;
mod meshtools;
mod model;
mod selfunion;
mod skin;
mod solids;
mod tools;

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut failed = false;
    let mut cases = cases::all();
    cases.sort_by(|a, b| a.name.cmp(b.name));
    for case in cases {
        let mut hash = shape_core::OutputHash::new();
        match (case.run)(&mut hash) {
            Ok(()) => println!("{} {}", case.name, hash.finish_hex()),
            Err(err) => {
                eprintln!("{}: {err}", case.name);
                println!("{} error", case.name);
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
