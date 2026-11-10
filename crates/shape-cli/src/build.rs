//! `shape build <program.json>`: replays the program and reports what it
//! built, as JSON: the parts with their triangle counts and the joints,
//! and every boolean (`cut`, `join`, `intersect`) with what its result
//! alone does not say: the voids it left and the edges where its operands
//! touch, and the faces it left without UVs (`shape_bool::BoolReport`).

use shape_model::Model;
use shape_tools::Import;

use crate::args::Args;
use crate::report::{array, object, string};
use crate::source::{Local, Source, load_uvs};
use crate::{Failure, failed};

/// The built model's name and parts: each part's name, parent, joint
/// (or null) and triangle count; then `booleans`, in program order: `op`,
/// `target`, `tool`, `voids`, `touchingEdges`, `touching`,
/// `facesWithoutUvs`.
pub fn report(model: &Model) -> Result<String, Failure> {
    let built = model.build().map_err(failed)?;
    let scene = built.scene();
    let parts: Vec<String> = scene
        .parts
        .iter()
        .map(|p| {
            object(&[
                ("name", string(&p.name)),
                ("parent", string(&p.parent)),
                (
                    "joint",
                    p.joint.as_ref().map_or("null".into(), |j| string(&j.name)),
                ),
                ("triangles", p.solid.triangles.len().to_string()),
            ])
        })
        .collect();
    let booleans: Vec<String> = model
        .booleans()
        .iter()
        .map(|b| {
            object(&[
                ("op", string(&b.op)),
                ("target", string(&b.target)),
                ("tool", string(&b.tool)),
                ("voids", b.report.voids.to_string()),
                ("touchingEdges", b.report.touching_edges.to_string()),
                ("touching", b.report.touching.to_string()),
                ("facesWithoutUvs", b.report.faces_without_uvs.to_string()),
            ])
        })
        .collect();
    Ok(object(&[
        ("name", string(&scene.name)),
        ("parts", array(&parts)),
        ("booleans", array(&booleans)),
    ]))
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    a.only(&["--uvs"])?;
    match load_uvs(&Local, &a.input()?, &Import::default(), a.switch("--uvs"))? {
        Source::Program(m) => report(&m),
        Source::Mesh(_) => Err(Failure::Usage(
            "build takes a program; `shape import` reports a mesh file".into(),
        )),
    }
}
