//! Golden cases for the front ends (`shape-cli`'s commands, which the
//! `shape` binary and the MCP server `shape-mcp` both call): their JSON
//! reports and files are the kernel's, so they are the same bytes on
//! every target.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use shape_cli::process::{ProcessDesc, process};
use shape_cli::{Failure, Files, Input};
use shape_core::{HashError, OutputHash};
use shape_tools::{Floaters, Repair};

use crate::cases::Case;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "mcp.process.ball_defects",
            run: process_case,
        },
        Case {
            name: "mcp.process.look",
            run: look_case,
        },
    ]
}

fn fail(e: impl std::fmt::Display) -> HashError {
    eprintln!("mcp: {e}");
    HashError::NaN
}

/// One file in memory (wasmtime gives the golden run no directory), and
/// the files written.
struct Memory {
    input: (PathBuf, Vec<u8>),
    written: RefCell<Vec<(PathBuf, Vec<u8>)>>,
}

impl Files for Memory {
    fn read(&self, path: &Path) -> Result<Vec<u8>, Failure> {
        if path == self.input.0 {
            Ok(self.input.1.clone())
        } else {
            Err(Failure::Failed(format!("{}: no such file", path.display())))
        }
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), Failure> {
        self.written
            .borrow_mut()
            .push((path.to_path_buf(), bytes.to_vec()));
        Ok(())
    }
}

/// The `process` chain on the committed mesh with defects: its report
/// and the plain GLB it writes.
fn process_case(h: &mut OutputHash) -> Result<(), HashError> {
    let files = Memory {
        input: (
            PathBuf::from("ball_defects.obj"),
            include_bytes!("../../../golden/meshes/ball_defects.obj").to_vec(),
        ),
        written: RefCell::new(Vec::new()),
    };
    let report = process(
        &files,
        &ProcessDesc {
            input: Input::Path(PathBuf::from("ball_defects.obj")),
            repair: Repair {
                weld: 0.001,
                floaters: Floaters {
                    fraction: 0.05,
                    ..Default::default()
                },
                max_hole_edges: 64,
                ..Default::default()
            },
            budget: 1000,
            intersections: true,
            out: Some(PathBuf::from("out.glb")),
            ..Default::default()
        },
    )
    .map_err(fail)?;
    h.str(&report);
    for (path, bytes) in files.written.borrow().iter() {
        h.str(&path.display().to_string());
        h.bytes(bytes);
    }
    Ok(())
}

/// The `process` chain on a coloured, textured sphere with a material in
/// a GLB: kept UVs, maps shrunk to 2 texels, the report and the GLB; then
/// again with fresh UVs (every map dropped, and said so) to OBJ (colours,
/// UVs and material not written, and said so).
fn look_case(h: &mut OutputHash) -> Result<(), HashError> {
    let s = crate::look::painted_sphere()?;
    let mut part = shape_tools::Part::new("ball", &s);
    part.material = crate::look::material();
    let glb = shape_tools::glb(&shape_tools::Scene {
        name: "look".into(),
        parts: vec![part],
    })
    .map_err(fail)?;
    for (fresh_uvs, out) in [(false, "out.glb"), (true, "out.obj")] {
        let files = Memory {
            input: (PathBuf::from("ball.glb"), glb.clone()),
            written: RefCell::new(Vec::new()),
        };
        let report = process(
            &files,
            &ProcessDesc {
                input: Input::Path(PathBuf::from("ball.glb")),
                repair: Repair {
                    weld: 0.001,
                    ..Default::default()
                },
                budget: 300,
                fresh_uvs,
                texture_max: 2,
                out: Some(PathBuf::from(out)),
                ..Default::default()
            },
        )
        .map_err(fail)?;
        h.str(&report);
        for (path, bytes) in files.written.borrow().iter() {
            h.str(&path.display().to_string());
            h.bytes(bytes);
        }
    }
    Ok(())
}
