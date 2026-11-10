//! `shape export <program.json | mesh> -o <out>`: the posed model as GLB
//! (with joints and materials), or its meshes as OBJ or STL, by the
//! output's extension; prints what it wrote. A program's GLB keeps its
//! meshes at rest and the pose in its joint nodes; its OBJ and STL are
//! written at the pose when any joint has a value other than 0, at rest
//! otherwise, and the report's `geometry` says which (`"posed"` or
//! `"rest"`). A mesh file is written with its parts placed (skins kept
//! in GLB).
//!
//! [`encode_solid`] writes the one solid the mesh tools make: GLB as
//! `shape_tools::glb_plain` (one node, one mesh, with the solid's UVs and
//! the texture given), or OBJ or STL.
//!
//! A mesh file exported as GLB keeps each part's UVs and base-colour
//! texture.

use std::path::Path;

use shape_mesh::Solid;
use shape_tools::{PlainGlb, Texture, glb, glb_plain, obj, stl};

use crate::args::{Args, IMPORT_OPTIONS};
use crate::report::{object, string};
use crate::source::{Local, Source, extension, load, scene};
use crate::{Failure, Files, failed};

fn bad_extension(out: &Path) -> Failure {
    Failure::Usage(format!(
        "{}: name the output .glb, .obj or .stl",
        out.display()
    ))
}

/// The bytes of a program or a mesh in the format `out`'s extension
/// names.
pub fn encode(source: &Source, out: &Path) -> Result<Vec<u8>, Failure> {
    let ext = extension(out);
    if !["glb", "obj", "stl"].contains(&ext.as_str()) {
        return Err(bad_extension(out));
    }
    match source {
        Source::Program(model) => match ext.as_str() {
            "glb" => model.glb().map_err(failed),
            "obj" => model.obj().map(String::into_bytes).map_err(failed),
            _ => model.stl().map_err(failed),
        },
        Source::Mesh(mesh) => {
            let placed = mesh.placed()?;
            let mut s = scene(&mesh.name, &placed);
            for (part, p) in s.parts.iter_mut().zip(&mesh.imported.parts) {
                part.material.map = p.map.clone();
            }
            match ext.as_str() {
                "glb" => glb(&s).map_err(failed),
                "obj" => obj(&s).map(String::into_bytes).map_err(failed),
                _ => stl(&s).map_err(failed),
            }
        }
    }
}

/// Writes a program or a mesh to `out`; reports the path, format and
/// size, and for a program `geometry`: `"posed"` when an OBJ or STL was
/// written at the model's pose, else `"rest"` (a GLB's meshes are always
/// at rest, posed by its joint nodes).
pub fn export(files: &dyn Files, source: &Source, out: &Path) -> Result<String, Failure> {
    let bytes = encode(source, out)?;
    files.write(out, &bytes)?;
    Ok(match source {
        Source::Program(model) => {
            let posed = extension(out) != "glb" && model.posed();
            object(&[
                ("path", string(&out.display().to_string())),
                ("format", string(&extension(out))),
                ("bytes", bytes.len().to_string()),
                ("geometry", string(if posed { "posed" } else { "rest" })),
            ])
        }
        Source::Mesh(_) => written(out, bytes.len()),
    })
}

/// `{"path", "format", "bytes"}` for a file written.
pub fn written(out: &Path, bytes: usize) -> String {
    object(&[
        ("path", string(&out.display().to_string())),
        ("format", string(&extension(out))),
        ("bytes", bytes.to_string()),
    ])
}

/// One solid in the format `out`'s extension names: a plain GLB (with
/// the solid's UVs, and `map` if given), OBJ or STL.
pub fn encode_solid(
    name: &str,
    s: &Solid,
    map: Option<&Texture>,
    out: &Path,
) -> Result<Vec<u8>, Failure> {
    let parts = [(name.to_string(), s.clone())];
    match extension(out).as_str() {
        "glb" => glb_plain(s, &PlainGlb { name, map }).map_err(failed),
        "obj" => obj(&scene(name, &parts))
            .map(String::into_bytes)
            .map_err(failed),
        "stl" => stl(&scene(name, &parts)).map_err(failed),
        _ => Err(bad_extension(out)),
    }
}

/// Writes one solid to `out` if given; reports the file or null.
pub fn write_solid(
    files: &dyn Files,
    name: &str,
    s: &Solid,
    map: Option<&Texture>,
    out: Option<&Path>,
) -> Result<String, Failure> {
    match out {
        None => Ok("null".into()),
        Some(out) => {
            let bytes = encode_solid(name, s, map, out)?;
            files.write(out, &bytes)?;
            Ok(written(out, bytes.len()))
        }
    }
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    a.only(&["-o", "--out", IMPORT_OPTIONS[0], IMPORT_OPTIONS[1]])?;
    let out = a.out()?;
    if !["glb", "obj", "stl"].contains(&extension(&out).as_str()) {
        return Err(bad_extension(&out));
    }
    let source = load(&Local, &a.input()?, &a.import()?)?;
    export(&Local, &source, &out)
}
