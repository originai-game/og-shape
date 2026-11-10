//! Golden cases for `place` (`docs/research.md` §31): a mesh turned to
//! stand up and face front, sized, and moved to its base; a skinned mesh
//! carried through; a file read in its own axes; and the command's
//! report and file.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use shape_cli::place::{PlaceDesc, place as place_command};
use shape_cli::process::{ProcessDesc, process};
use shape_cli::{Failure, Files, Input};
use shape_core::{DVec3, HashError, OutputHash};
use shape_gen::solids::{BoxDesc, cuboid, torus};
use shape_mesh::Solid;
use shape_tools::{Axis, Floaters, Import, Pivot, Place, Repair, Size, import, no_files, place};

use crate::cases::Case;

// Also `meshtools`'s, whose copy is private to it.
#[allow(dead_code, clippy::duplicate_mod)]
#[path = "../../shape-tools/tests/support/skinned.rs"]
mod skinned_support;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "place.turn",
            run: turn_case,
        },
        Case {
            name: "place.pivot",
            run: pivot_case,
        },
        Case {
            name: "place.size",
            run: size_case,
        },
        Case {
            name: "place.skinned",
            run: skinned_case,
        },
        Case {
            name: "place.import.axes",
            run: import_axes_case,
        },
        Case {
            name: "place.command",
            run: command_case,
        },
        Case {
            name: "place.process",
            run: process_case,
        },
    ]
}

fn fail(e: impl std::fmt::Display) -> HashError {
    eprintln!("place: {e}");
    HashError::NaN
}

/// A box with a ring on one side, off the origin: no symmetry hides a
/// wrong turn.
fn model() -> Result<Vec<Solid>, HashError> {
    let mut body = cuboid(&BoxDesc {
        width: 40.0,
        depth: 30.0,
        height: 20.0,
    })
    .map_err(fail)?;
    body.map_positions(|p| p + DVec3::new(7.0, -3.0, 11.0), false);
    let mut ring = torus(12.0, 3.0).map_err(fail)?;
    ring.map_positions(|p| p + DVec3::new(30.0, 5.0, 40.0), false);
    Ok(vec![body, ring])
}

fn run(h: &mut OutputHash, parts: &[Solid], d: &Place) -> Result<(), HashError> {
    match place(parts, d) {
        Ok((out, r)) => {
            for s in &out {
                s.hash_into(h)?;
            }
            h.str(&r.to_json());
        }
        Err(e) => h.str(&e.to_string()),
    }
    Ok(())
}

/// Axis turns (one axis given, both, and the refusal of two along one
/// axis), quarter turns about up, and a turn of 30°.
fn turn_case(h: &mut OutputHash) -> Result<(), HashError> {
    let parts = model()?;
    let axes = |up, front| Place {
        up,
        front,
        ..Default::default()
    };
    for d in [
        axes(Some(Axis::PosY), None),
        axes(None, Some(Axis::PosX)),
        axes(Some(Axis::NegZ), Some(Axis::PosX)),
        axes(Some(Axis::NegX), Some(Axis::PosY)),
        axes(Some(Axis::PosZ), Some(Axis::NegZ)),
        Place {
            turn: -270.0,
            ..Default::default()
        },
        Place {
            up: Some(Axis::PosY),
            turn: 30.0,
            ..Default::default()
        },
    ] {
        run(h, &parts, &d)?;
    }
    Ok(())
}

/// The origin to the base, the centre and a point, after a turn.
fn pivot_case(h: &mut OutputHash) -> Result<(), HashError> {
    let parts = model()?;
    for pivot in [
        Pivot::Base,
        Pivot::Center,
        Pivot::Point(DVec3::new(7.0, -3.0, 11.0)),
    ] {
        run(
            h,
            &parts,
            &Place {
                up: Some(Axis::NegY),
                pivot,
                ..Default::default()
            },
        )?;
    }
    Ok(())
}

/// Each size, and a size of nothing refused.
fn size_case(h: &mut OutputHash) -> Result<(), HashError> {
    let parts = model()?;
    for size in [
        Size::Longest(1000.0),
        Size::Height(1800.0),
        Size::Width(250.0),
        Size::Depth(90.0),
        Size::Scale(0.01),
        Size::Height(0.0),
    ] {
        run(
            h,
            &parts,
            &Place {
                size,
                pivot: Pivot::Base,
                ..Default::default()
            },
        )?;
    }
    Ok(())
}

/// A skinned tube laid on its side, stood up, scaled and moved: its
/// bones and inverse bind matrices with it.
fn skinned_case(h: &mut OutputHash) -> Result<(), HashError> {
    let tube = skinned_support::skinned_tube();
    run(
        h,
        &[tube],
        &Place {
            up: Some(Axis::PosX),
            front: Some(Axis::PosZ),
            size: Size::Height(1700.0),
            pivot: Pivot::Base,
            ..Default::default()
        },
    )
}

/// One OBJ read in a frame neither Y up nor Z up (up −z, front +x), Z
/// up through its words and its switch, and a GLB read Z up.
fn import_axes_case(h: &mut OutputHash) -> Result<(), HashError> {
    let obj = "o thing\nv 0 0 0\nv 1 0 0\nv 0 2 0\nv 0 0 3\nf 1 3 2\nf 1 2 4\nf 2 3 4\nf 1 4 3\n";
    let gltf = shape_tools::glb(&shape_tools::Scene {
        name: "m".into(),
        parts: model()?
            .iter()
            .enumerate()
            .map(|(k, s)| shape_tools::Part::new(["body", "ring"][k], s))
            .collect::<Vec<_>>(),
    })
    .map_err(fail)?;
    let reads = [
        Import {
            up: Some(Axis::NegZ),
            front: Some(Axis::PosX),
            ..Default::default()
        },
        Import {
            up: Some(Axis::PosZ),
            front: Some(Axis::NegY),
            ..Default::default()
        },
        Import {
            z_up: true,
            ..Default::default()
        },
        Import {
            z_up: true,
            front: Some(Axis::PosX),
            ..Default::default()
        },
    ];
    for d in &reads {
        match import(obj.as_bytes(), d, &no_files) {
            Ok(m) => m.parts[0].solid.hash_into(h)?,
            Err(e) => h.str(&e.to_string()),
        }
    }
    let m = import(&gltf, &reads[1], &no_files).map_err(fail)?;
    for p in &m.parts {
        let t = &p.transform;
        h.dvec3s(&[t.x, t.y, t.z, t.t])?;
        p.solid.hash_into(h)?;
    }
    Ok(())
}

/// One file in memory, and the files written.
struct Memory {
    input: (PathBuf, &'static [u8]),
    written: RefCell<Vec<(PathBuf, Vec<u8>)>>,
}

impl Files for Memory {
    fn read(&self, path: &Path) -> Result<Vec<u8>, Failure> {
        if path == self.input.0 {
            Ok(self.input.1.to_vec())
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

fn ball() -> Memory {
    Memory {
        input: (
            PathBuf::from("ball_defects.obj"),
            include_bytes!("../../../golden/meshes/ball_defects.obj"),
        ),
        written: RefCell::new(Vec::new()),
    }
}

fn hash_written(h: &mut OutputHash, files: &Memory) {
    for (path, bytes) in files.written.borrow().iter() {
        h.str(&path.display().to_string());
        h.bytes(bytes);
    }
}

/// `shape process` with a place step: placed after the repair drops the
/// floaters, so the output stands on its own base.
fn process_case(h: &mut OutputHash) -> Result<(), HashError> {
    let files = ball();
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
                ..Default::default()
            },
            budget: 500,
            place: Place {
                size: Size::Height(1000.0),
                pivot: Pivot::Base,
                ..Default::default()
            },
            out: Some(PathBuf::from("out.glb")),
            ..Default::default()
        },
    )
    .map_err(fail)?;
    h.str(&report);
    hash_written(h, &files);
    Ok(())
}

/// `shape place` on the committed OBJ, read Z up, sized and moved to
/// its base: its report and its GLB.
fn command_case(h: &mut OutputHash) -> Result<(), HashError> {
    let files = ball();
    let report = place_command(
        &files,
        &PlaceDesc {
            input: Input::Path(PathBuf::from("ball_defects.obj")),
            import: Import {
                up: Some(Axis::PosZ),
                ..Default::default()
            },
            place: Place {
                turn: 90.0,
                size: Size::Longest(250.0),
                pivot: Pivot::Base,
                ..Default::default()
            },
            out: Some(PathBuf::from("out.glb")),
        },
    )
    .map_err(fail)?;
    h.str(&report);
    hash_written(h, &files);
    Ok(())
}
