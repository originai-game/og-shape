//! `shape build <program.json>`: replays the program and reports what it
//! built, as JSON: the parts with their triangle counts and the joints.

use shape_model::Model;
use shape_tools::Import;

use crate::args::Args;
use crate::report::{array, object, string};
use crate::source::{Local, Source, load};
use crate::{Failure, failed};

/// The built model's name and parts: each part's name, parent, joint
/// (or null) and triangle count.
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
    Ok(object(&[
        ("name", string(&scene.name)),
        ("parts", array(&parts)),
    ]))
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    a.only(&[])?;
    match load(&Local, &a.input()?, &Import::default())? {
        Source::Program(m) => report(&m),
        Source::Mesh(_) => Err(Failure::Usage(
            "build takes a program; `shape import` reports a mesh file".into(),
        )),
    }
}
