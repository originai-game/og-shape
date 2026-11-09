//! `shape`: the og-shape command line, for tests and for the platform
//! before the merge (`docs/design.md` §5 and §7).
//!
//! ```text
//! shape build   <program.json>
//! shape export  <program.json> -o <out.glb|out.obj|out.stl>
//! shape measure <program.json>
//! shape preview <program.json> -o <out.png> [--views N] [--width PX]
//!               [--height PX] [--columns N] [--elevation RAD]
//!               [--azimuth RAD] [--no-edges] [--no-ground]
//! ```
//!
//! A program is the JSON a binding records (`shape_model::program`).
//! Reports go to stdout as JSON with no thresholds; errors go to stderr
//! with exit status 1 (2 for a usage error). Each subcommand is one
//! module; repair, decimate and unwrap join them in phase 5.

mod args;
mod build;
mod export;
mod measure;
mod preview;

use std::process::ExitCode;

use args::{Args, Failure};

const USAGE: &str = "usage: shape <build|export|measure|preview> <program.json> [options]
  build   <program.json>                       report the built model as JSON
  export  <program.json> -o <out.glb|obj|stl>  write the posed model
  measure <program.json>                       measures as JSON
  preview <program.json> -o <out.png> [--views N] [--width PX] [--height PX]
          [--columns N] [--elevation RAD] [--azimuth RAD] [--no-edges] [--no-ground]";

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let result = match argv.split_first() {
        Some((cmd, rest)) => {
            let a = Args::new(rest);
            match cmd.as_str() {
                "build" => build::run(a),
                "export" => export::run(a),
                "measure" => measure::run(a),
                "preview" => preview::run(a),
                "help" | "-h" | "--help" => {
                    println!("{USAGE}");
                    return ExitCode::SUCCESS;
                }
                other => Err(Failure::Usage(format!("no command {other:?}"))),
            }
        }
        None => Err(Failure::Usage("no command".into())),
    };
    match result {
        Ok(out) => {
            if !out.is_empty() {
                println!("{out}");
            }
            ExitCode::SUCCESS
        }
        Err(Failure::Usage(why)) => {
            eprintln!("shape: {why}\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Failed(why)) => {
            eprintln!("shape: {why}");
            ExitCode::from(1)
        }
    }
}
