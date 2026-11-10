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
use shape_tools::{Material, PlainGlb, glb, glb_plain, obj, stl};

use crate::args::{Args, IMPORT_OPTIONS};
use crate::report::{object, string};
use crate::source::{Local, Source, extension, load_uvs, scene};
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
                part.material = p.material.clone();
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
        Source::Mesh(mesh) => {
            let mut lost = Vec::new();
            if extension(out) != "glb" {
                for p in &mesh.imported.parts {
                    lost.extend(not_written(&p.solid, Some(&p.material), out));
                }
                lost.sort();
                lost.dedup();
            }
            written_lost(out, bytes.len(), &lost)
        }
    })
}

/// What a solid carries that writing it to `out` leaves out: OBJ and STL
/// hold positions only.
pub fn not_written(s: &Solid, m: Option<&Material>, out: &Path) -> Vec<String> {
    let mut lost = Vec::new();
    if extension(out) == "glb" {
        return lost;
    }
    if s.uvs.is_some() {
        lost.push("uvs".to_string());
    }
    if s.colors.is_some() {
        lost.push("colors".to_string());
    }
    if m.is_some_and(|m| *m != Material::default()) {
        lost.push("material".to_string());
    }
    lost
}

/// [`written`], with `not_written` when `lost` names anything.
fn written_lost(out: &Path, bytes: usize, lost: &[String]) -> String {
    let mut fields = vec![
        ("path", string(&out.display().to_string())),
        ("format", string(&extension(out))),
        ("bytes", bytes.to_string()),
    ];
    if !lost.is_empty() {
        fields.push((
            "not_written",
            crate::report::array(&lost.iter().map(|x| string(x)).collect::<Vec<_>>()),
        ));
    }
    object(&fields)
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
/// the solid's UVs and colours, and `material` if given), OBJ or STL.
pub fn encode_solid(
    name: &str,
    s: &Solid,
    material: Option<&Material>,
    out: &Path,
) -> Result<Vec<u8>, Failure> {
    let parts = [(name.to_string(), s.clone())];
    match extension(out).as_str() {
        "glb" => glb_plain(
            s,
            &PlainGlb {
                name,
                material,
                ..Default::default()
            },
        )
        .map_err(failed),
        "obj" => obj(&scene(name, &parts))
            .map(String::into_bytes)
            .map_err(failed),
        "stl" => stl(&scene(name, &parts)).map_err(failed),
        _ => Err(bad_extension(out)),
    }
}

/// Writes one solid to `out` if given; reports the file (with
/// `not_written` for what an OBJ or STL leaves out) or null.
pub fn write_solid(
    files: &dyn Files,
    name: &str,
    s: &Solid,
    material: Option<&Material>,
    out: Option<&Path>,
) -> Result<String, Failure> {
    match out {
        None => Ok("null".into()),
        Some(out) => {
            let bytes = encode_solid(name, s, material, out)?;
            files.write(out, &bytes)?;
            Ok(written_lost(
                out,
                bytes.len(),
                &not_written(s, material, out),
            ))
        }
    }
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    let mut known = vec!["-o", "--out", "--uvs"];
    known.extend(IMPORT_OPTIONS);
    a.only(&known)?;
    let out = a.out()?;
    if !["glb", "obj", "stl"].contains(&extension(&out).as_str()) {
        return Err(bad_extension(&out));
    }
    let source = load_uvs(&Local, &a.input()?, &a.import()?, a.switch("--uvs"))?;
    export(&Local, &source, &out)
}
