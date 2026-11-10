//! `shape process <mesh> --budget N [-o out.glb]`: the mesh tools'
//! chain on one file (`guides/meshtools.md`): import → join → repair →
//! decimate to the triangle budget → unwrap → measure → export, as one
//! JSON report of every step. The output is a plain GLB with the UVs
//! (one node, one mesh), or OBJ or STL without them. Repair and unwrap
//! take their zero values unless set, as in `shape repair` and
//! `shape unwrap`.
//!
//! A mesh that carries UVs from its file keeps them, and its base-colour
//! texture (when its parts share one), through repair and decimation:
//! the unwrap step is then skipped (`"unwrap": null`) unless `--unwrap`
//! asks for fresh UVs, which drops the old UVs before decimation (their
//! seams no longer hold it back) and the texture.

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
    /// `Decimate::merge_charts`: UV islands that agree across a seam up
    /// to a rigid move within this distance are one chart for the seam
    /// rule; 0 is off.
    pub merge_charts: f64,
    /// The unwrap.
    pub unwrap: Unwrap,
    /// Unwrap even a mesh that carries UVs (its texture is then not
    /// written: it was painted for the old UVs).
    pub fresh_uvs: bool,
    /// Count self-intersecting triangle pairs in the final measures.
    pub intersections: bool,
    /// Where to write the result, if anywhere.
    pub out: Option<PathBuf>,
}

/// The chain's steps, in the order [`process_steps`] calls them.
pub const STEPS: [&str; 6] = [
    "import", "repair", "decimate", "unwrap", "measure", "export",
];

/// The chain's report: `import`, `repair`, `decimate` (null when
/// skipped), `unwrap`, `measures` and `output`.
pub fn process(files: &dyn Files, d: &ProcessDesc) -> Result<String, Failure> {
    process_steps(files, d, &mut |_| Ok(()))
}

/// As [`process`], calling `step(k)` before step `k` of [`STEPS`] (a
/// front end reports progress there). An error from `step` stops the
/// chain before that step and is returned; a step itself is never
/// interrupted.
pub fn process_steps(
    files: &dyn Files,
    d: &ProcessDesc,
    step: &mut dyn FnMut(usize) -> Result<(), Failure>,
) -> Result<String, Failure> {
    step(0)?;
    let mesh = load_mesh(files, &d.input, &d.import)?;
    let imported = report::imported(&mesh.format, &mesh.imported);
    let joined = mesh.joined()?;
    step(1)?;
    let (mut fixed, r) = shape_tools::repair(&joined, &d.repair).map_err(failed)?;
    if d.fresh_uvs {
        // The file's UVs are replaced after decimation, so their seams
        // must not hold the decimation back.
        fixed.uvs = None;
    }
    step(2)?;
    let (low, dec) = if d.budget > 0 && fixed.triangles.len() > d.budget {
        let (low, dr) = shape_tools::decimate(
            &fixed,
            &Decimate {
                target: d.budget,
                merge_charts: d.merge_charts,
                ..Default::default()
            },
        )
        .map_err(failed)?;
        (low, report::decimate(&dr))
    } else {
        (fixed, "null".to_string())
    };
    step(3)?;
    let mut low = low;
    let (unwrapped, map) = if low.uvs.is_none() || d.fresh_uvs {
        let uvs = shape_tools::unwrap(&mut low, &d.unwrap).map_err(failed)?;
        (report::unwrap(&uvs), None)
    } else {
        ("null".to_string(), mesh.map())
    };
    step(4)?;
    let m = shape_tools::measure(
        &low,
        &Measure {
            images: &mesh.imported.images,
            uv_resolution: d.unwrap.resolution,
            intersections: d.intersections,
        },
    );
    step(5)?;
    let output = write_solid(files, &mesh.name, &low, map, d.out.as_deref())?;
    Ok(object(&[
        ("import", imported),
        ("repair", report::repair(&r)),
        ("decimate", dec),
        ("unwrap", unwrapped),
        ("measures", m.to_json()),
        ("output", output),
    ]))
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    let mut known = vec![
        "-o",
        "--out",
        "--budget",
        "--merge-charts",
        "--intersections",
        "--unwrap",
    ];
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
            merge_charts: a.number("--merge-charts")?.unwrap_or(0.0),
            unwrap: crate::unwrap::options(a)?,
            fresh_uvs: a.switch("--unwrap"),
            intersections: a.switch("--intersections"),
            out: a.out_opt()?,
        },
    )
}
