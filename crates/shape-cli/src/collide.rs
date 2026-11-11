//! `shape collide <input> [-o out]`: colliders for a program's parts or
//! a mesh file's (`shape_tools::colliders`): each part covered by convex
//! hulls, the shape a physics engine tests against. The report gives,
//! per part, each hull's vertices, triangles, volume, concavity (how
//! deep the part's surface lies under the hull) and `outside` (how far
//! the part pokes out of a hull thinned by `--max-vertices`). With `-o`,
//! writes the hulls alone as parts named `<part>.collider.<k>`: GLB, OBJ
//! or STL.
//!
//! A program's parts are taken at rest, where they stand in the world.

use std::path::PathBuf;

use shape_mesh::Solid;
use shape_tools::{Collide, Import, colliders, glb, obj, stl};

use crate::args::{Args, IMPORT_OPTIONS};
use crate::export::written;
use crate::report::{array, object, string};
use crate::source::{Input, Local, Source, extension, load, scene};
use crate::{Failure, Files, failed};

/// What to cover. The zero value has no input; with an input, one hull
/// a part.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CollideDesc {
    /// A program or a mesh file.
    pub input: Input,
    /// How to read a mesh file.
    pub import: Import,
    /// How to cover each part.
    pub collide: Collide,
    /// Where to write the hulls, if anywhere.
    pub out: Option<PathBuf>,
}

/// The options [`options`] reads.
pub const OPTIONS: [&str; 3] = ["--hulls", "--concavity", "--max-vertices"];

/// The `Collide` the command line gives: `--hulls N`, `--concavity MM`,
/// `--max-vertices N`.
pub fn options(a: &Args) -> Result<Collide, Failure> {
    Ok(Collide {
        max_hulls: a.number("--hulls")?.unwrap_or(0),
        concavity: a.number("--concavity")?.unwrap_or(0.0),
        max_vertices: a.number("--max-vertices")?.unwrap_or(0),
    })
}

/// The collide report: `parts` (per part its `name` and `colliders`, the
/// library's report, lengths in mm) and `output`.
pub fn collide(files: &dyn Files, d: &CollideDesc) -> Result<String, Failure> {
    let source = load(files, &d.input, &d.import)?;
    let (name, parts): (String, Vec<(String, Solid)>) = match &source {
        Source::Program(model) => {
            let built = model.build().map_err(failed)?;
            let s = built.scene();
            (
                s.name.clone(),
                s.parts
                    .iter()
                    .map(|p| (p.name.clone(), p.solid.clone()))
                    .collect(),
            )
        }
        Source::Mesh(mesh) => (mesh.name.clone(), mesh.placed()?),
    };
    let mut reports = Vec::new();
    let mut hulls: Vec<(String, Solid)> = Vec::new();
    for (part, solid) in &parts {
        // A group has no mesh of its own.
        if solid.triangles.is_empty() {
            continue;
        }
        let (made, r) =
            colliders(solid, &d.collide).map_err(|e| Failure::Failed(format!("{part}: {e}")))?;
        reports.push(object(&[
            ("name", string(part)),
            ("colliders", r.to_json()),
        ]));
        for (k, h) in made.into_iter().enumerate() {
            hulls.push((format!("{part}.collider.{k}"), h));
        }
    }
    let output = match &d.out {
        None => "null".to_string(),
        Some(path) => {
            let s = scene(&name, &hulls);
            let bytes = match extension(path).as_str() {
                "glb" => glb(&s).map_err(failed)?,
                "obj" => obj(&s).map(String::into_bytes).map_err(failed)?,
                "stl" => stl(&s).map_err(failed)?,
                _ => {
                    return Err(Failure::Usage(format!(
                        "{}: name the output .glb, .obj or .stl",
                        path.display()
                    )));
                }
            };
            files.write(path, &bytes)?;
            written(path, bytes.len())
        }
    };
    Ok(object(&[
        ("units", string("mm")),
        ("parts", array(&reports)),
        ("output", output),
    ]))
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    let mut known = vec!["-o", "--out"];
    known.extend(IMPORT_OPTIONS);
    known.extend(OPTIONS);
    a.only(&known)?;
    collide(
        &Local,
        &CollideDesc {
            input: a.input()?,
            import: a.import()?,
            collide: options(a)?,
            out: a.out_opt()?,
        },
    )
}
