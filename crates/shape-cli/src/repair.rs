//! `shape repair <mesh> [-o out]`: the file's parts joined and repaired
//! (`shape_tools::repair`), as JSON: what each step changed and the
//! measures after. With `-o`, writes the repaired solid (a plain GLB,
//! OBJ or STL). The zero value of every option does the least; the
//! settings that worked on the acceptance meshes are in
//! `guides/meshtools.md`.

use std::path::PathBuf;

use shape_tools::{Floaters, Import, Measure, Repair};

use crate::args::{Args, IMPORT_OPTIONS};
use crate::export::write_solid;
use crate::report::{self, object};
use crate::source::{Input, Local, load_mesh};
use crate::{Failure, Files, failed};

/// What to repair. The zero value has no input.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RepairDesc {
    /// The mesh file.
    pub input: Input,
    /// How to read the mesh file: its unit (OBJ) and axes.
    pub import: Import,
    /// The repair.
    pub repair: Repair,
    /// Where to write the result, if anywhere.
    pub out: Option<PathBuf>,
}

/// The repair report: `triangles_in`, `repair`, `measures`, `output`.
pub fn repair(files: &dyn Files, d: &RepairDesc) -> Result<String, Failure> {
    let mesh = load_mesh(files, &d.input, &d.import)?;
    let joined = mesh.joined()?;
    let triangles_in = joined.triangles.len();
    let (fixed, r) = shape_tools::repair_owned(joined, &d.repair).map_err(failed)?;
    let m = shape_tools::measure(&fixed, &Measure::default());
    let (material, defaults, dropped) = mesh.material();
    let output = write_solid(files, &mesh.name, &fixed, Some(&material), d.out.as_deref())?;
    Ok(object(&[
        ("triangles_in", triangles_in.to_string()),
        ("repair", report::repair(&r)),
        ("measures", m.to_json()),
        ("material", report::material(&material, &defaults, &dropped)),
        ("output", output),
    ]))
}

/// The repair options of the command line.
pub const OPTIONS: [&str; 9] = [
    "--weld",
    "--weld-all",
    "--floaters",
    "--floater-size",
    "--non-manifold",
    "--holes",
    "--thicken",
    "--fuse",
    "--min-edge",
];

/// [`OPTIONS`] read into a repair desc.
pub fn options(a: &Args) -> Result<Repair, Failure> {
    Ok(Repair {
        weld: a.number("--weld")?.unwrap_or(0.0),
        weld_all: a.switch("--weld-all"),
        floaters: Floaters {
            size: a.number("--floater-size")?.unwrap_or(0.0),
            fraction: a.number("--floaters")?.unwrap_or(0.0),
        },
        non_manifold: a.switch("--non-manifold"),
        max_hole_edges: a.number("--holes")?.unwrap_or(0),
        thicken: a.number("--thicken")?.unwrap_or(0.0),
        fuse: a.switch("--fuse"),
        min_edge: a.number("--min-edge")?.unwrap_or(0.0),
        check_intersections: a.switch("--intersections"),
    })
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    let mut known = vec!["-o", "--out", "--intersections"];
    known.extend(IMPORT_OPTIONS);
    known.extend(OPTIONS);
    a.only(&known)?;
    repair(
        &Local,
        &RepairDesc {
            input: a.input()?,
            import: a.import()?,
            repair: options(a)?,
            out: a.out_opt()?,
        },
    )
}
