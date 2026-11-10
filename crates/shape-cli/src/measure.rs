//! `shape measure <program.json | mesh>`: measures as JSON
//! (`docs/design.md` §7), with no thresholds. A program gives every part
//! and the whole model at rest (`shape_model::program::measures_json`);
//! with `--posed`, at its pose, with how each joint holds and the
//! interference at the pose (`shape_model::program::posed_json`); a mesh
//! file gives every part placed, and the parts joined
//! (`shape_tools::MeshMeasures`, null for a skinned file, which cannot be
//! joined).

use shape_tools::Measure;

use crate::args::{Args, IMPORT_OPTIONS};
use crate::report::{array, object, string};
use crate::source::{Local, Mesh, Source, load_uvs};
use crate::{Failure, failed};

/// What to measure besides the defaults. The zero value skips the exact
/// self-intersection count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeasureDesc {
    /// Meshes only: count self-intersecting triangle pairs (exact; costs a
    /// sweep over the triangles).
    pub intersections: bool,
}

/// The measures of a program or a mesh.
pub fn measure(source: &Source, d: &MeasureDesc) -> Result<String, Failure> {
    match source {
        Source::Program(m) => shape_model::program::measures_json(m).map_err(failed),
        Source::Mesh(mesh) => mesh_measures(mesh, d),
    }
}

/// A program's measures at its pose (`shape_model::program::posed_json`).
/// A mesh file has no joints: refused.
pub fn measure_posed(source: &Source) -> Result<String, Failure> {
    match source {
        Source::Program(m) => shape_model::program::posed_json(m).map_err(failed),
        Source::Mesh(_) => Err(Failure::Usage(
            "--posed measures a program at its pose; a mesh file has no joints".into(),
        )),
    }
}

fn mesh_measures(mesh: &Mesh, d: &MeasureDesc) -> Result<String, Failure> {
    let placed = mesh.placed()?;
    let parts: Vec<String> = placed
        .iter()
        .map(|(name, s)| {
            let m = shape_tools::measure(
                s,
                &Measure {
                    intersections: d.intersections,
                    ..Default::default()
                },
            );
            object(&[("name", string(name)), ("measures", m.to_json())])
        })
        .collect();
    let whole = if mesh.skinned() {
        "null".to_string()
    } else {
        shape_tools::measure(
            &mesh.joined()?,
            &Measure {
                images: &mesh.imported.images,
                intersections: d.intersections,
                ..Default::default()
            },
        )
        .to_json()
    };
    Ok(object(&[
        ("name", string(&mesh.name)),
        ("format", string(&mesh.format)),
        ("parts", array(&parts)),
        ("whole", whole),
    ]))
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    a.only(&[
        IMPORT_OPTIONS[0],
        IMPORT_OPTIONS[1],
        "--intersections",
        "--posed",
        "--uvs",
    ])?;
    let source = load_uvs(&Local, &a.input()?, &a.import()?, a.switch("--uvs"))?;
    if a.switch("--posed") {
        return measure_posed(&source);
    }
    measure(
        &source,
        &MeasureDesc {
            intersections: a.switch("--intersections"),
        },
    )
}
