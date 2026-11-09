//! `shape process <mesh> --budget N [-o out.glb]`: the mesh tools'
//! chain on one file (`guides/meshtools.md`): import → join → repair →
//! decimate to the triangle budget → unwrap → measure → export, as one
//! JSON report of every step. The output is a plain GLB with the UVs
//! (one node, one mesh), or OBJ or STL without them. Repair and unwrap
//! take their zero values unless set, as in `shape repair` and
//! `shape unwrap`.

use std::path::PathBuf;

use shape_tools::{Decimate, Import, Measure, Repair, Unwrap};

use crate::args::{Args, IMPORT_OPTIONS};
use crate::export::write_solid;
use crate::report::{self, object};
use crate::source::{Input, Local, load_mesh};
use crate::{Failure, Files, failed};

/// The chain. The zero value has no input and no budget (no
/// decimation).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProcessDesc {
    /// The mesh file.
    pub input: Input,
    /// How to read an OBJ file.
    pub import: Import,
    /// The repair.
    pub repair: Repair,
    /// Decimate to at most this many triangles; 0 skips decimation, and
    /// so does a mesh already within it.
    pub budget: usize,
    /// The unwrap.
    pub unwrap: Unwrap,
    /// Count self-intersecting triangle pairs in the final measures.
    pub intersections: bool,
    /// Where to write the result, if anywhere.
    pub out: Option<PathBuf>,
}

/// The chain's report: `import`, `repair`, `decimate` (null when
/// skipped), `unwrap`, `measures` and `output`.
pub fn process(files: &dyn Files, d: &ProcessDesc) -> Result<String, Failure> {
    let mesh = load_mesh(files, &d.input, &d.import)?;
    let imported = report::imported(&mesh.format, &mesh.imported);
    let joined = mesh.joined()?;
    let (fixed, r) = shape_tools::repair(&joined, &d.repair).map_err(failed)?;
    let (low, dec) = if d.budget > 0 && fixed.triangles.len() > d.budget {
        let (low, dr) = shape_tools::decimate(
            &fixed,
            &Decimate {
                target: d.budget,
                ..Default::default()
            },
        )
        .map_err(failed)?;
        (low, report::decimate(&dr))
    } else {
        (fixed, "null".to_string())
    };
    let uvs = shape_tools::unwrap(&low, &d.unwrap).map_err(failed)?;
    let m = shape_tools::measure(
        &low,
        &Measure {
            uvs: Some(&uvs.corners),
            images: &mesh.imported.images,
            uv_resolution: d.unwrap.resolution,
            intersections: d.intersections,
        },
    );
    let output = write_solid(files, &mesh.name, &low, Some(&uvs), d.out.as_deref())?;
    Ok(object(&[
        ("import", imported),
        ("repair", report::repair(&r)),
        ("decimate", dec),
        ("unwrap", report::unwrap(&uvs)),
        ("measures", m.to_json()),
        ("output", output),
    ]))
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    let mut known = vec!["-o", "--out", "--budget", "--intersections"];
    known.extend(IMPORT_OPTIONS);
    known.extend(crate::repair::OPTIONS);
    known.extend(crate::unwrap::OPTIONS);
    a.only(&known)?;
    let mut repair = crate::repair::options(a)?;
    repair.check_intersections = false;
    process(
        &Local,
        &ProcessDesc {
            input: a.input()?,
            import: a.import()?,
            repair,
            budget: a.number("--budget")?.unwrap_or(0),
            unwrap: crate::unwrap::options(a)?,
            intersections: a.switch("--intersections"),
            out: a.out_opt()?,
        },
    )
}
