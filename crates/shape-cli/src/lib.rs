//! The `shape` commands as functions (`docs/design.md` §5 and §7), for
//! two front ends: the command line (`src/main.rs`) and the MCP server
//! (`crates/shape-mcp`). Every result is the kernel's; this crate only
//! reads inputs, calls the kernel and writes its outputs.
//!
//! Entry: [`source::load`] reads a [`Source`]: a model program
//! (`shape_model::program`) or a mesh file (glb, gltf, obj, fbx), through
//! a [`Files`] that does the I/O (plain files for the command line, a
//! confined workspace for the server). Each command is one module:
//!
//! - [`build`]: the built model's parts and joints.
//! - [`measure`]: measures as JSON, with no thresholds.
//! - [`preview`]: the CPU turntable as one PNG.
//! - [`export`]: GLB, OBJ or STL.
//! - [`import`]: what a mesh file holds, and what was skipped.
//! - [`repair`], [`decimate`], [`unwrap`]: the mesh tools on a mesh file.
//! - [`place`]: a mesh file turned, sized and moved to a base pivot.
//! - [`process`]: the chain import → repair → decimate → unwrap →
//!   measure → export under a triangle budget.
//!
//! Reports are JSON with keys in a fixed order ([`report`]); errors are
//! [`Failure`] values. [`args`] reads the command line.

pub mod args;
pub mod build;
pub mod collide;
pub mod compress;
pub mod decimate;
pub mod export;
pub mod import;
pub mod measure;
pub mod place;
pub mod preview;
pub mod process;
pub mod repair;
pub mod report;
pub mod source;
pub mod unwrap;

pub use source::{Files, Input, Local, Mesh, Source};

/// Why a command did not run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The command was asked wrongly (a missing or unknown option).
    Usage(String),
    /// The command ran and failed: the kernel's error, or an I/O error.
    Failed(String),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Usage(w) | Failure::Failed(w) => f.write_str(w),
        }
    }
}

impl std::error::Error for Failure {}

/// A kernel error as a failure.
pub fn failed(e: impl std::fmt::Display) -> Failure {
    Failure::Failed(e.to_string())
}
