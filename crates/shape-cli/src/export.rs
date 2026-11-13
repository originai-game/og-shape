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
        "{}: name the output .glb, .obj, .stl, .urdf, .usda or .usdz",
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
    export_with(files, source, out, &shape_tools::UrdfDesc::default())
}

/// [`export`], a URDF with what `robot` adds: each closed link's
/// `inertial` at a density, and the joints' limits of effort and
/// velocity.
pub fn export_with(
    files: &dyn Files,
    source: &Source,
    out: &Path,
    robot: &shape_tools::UrdfDesc,
) -> Result<String, Failure> {
    if extension(out) == "urdf" {
        return export_urdf(files, source, out, robot);
    }
    if ["usda", "usdz"].contains(&extension(out).as_str()) {
        return export_usda(files, source, out);
    }
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

/// Writes a program or a mesh as URDF: the description to `out` and
/// each link's STL beside it, named `<out's stem>_<part>.stl`. Reports
/// the path, format and size, the mesh files, and what URDF asks for
/// that is not written.
fn export_urdf(
    files: &dyn Files,
    source: &Source,
    out: &Path,
    robot: &shape_tools::UrdfDesc,
) -> Result<String, Failure> {
    let stem = out
        .file_stem()
        .map_or("robot".to_string(), |s| s.to_string_lossy().into_owned());
    let urdf = match source {
        Source::Program(model) => model.urdf_with(&stem, robot).map_err(failed)?,
        Source::Mesh(mesh) => {
            let placed = mesh.placed()?;
            shape_tools::urdf_with(&scene(&mesh.name, &placed), &stem, robot).map_err(failed)?
        }
    };
    files.write(out, urdf.xml.as_bytes())?;
    let mut names = Vec::new();
    for (name, bytes) in &urdf.meshes {
        files.write(&out.with_file_name(name), bytes)?;
        names.push(string(name));
    }
    let mut lost = vec!["materials, UVs and colours".to_string()];
    if robot.density == 0.0 {
        lost.push("inertial (mass and inertia): give a density".into());
    }
    for name in &urdf.not_weighed {
        lost.push(format!("{name}: inertial (the link is not closed)"));
    }
    if robot.effort == 0.0 || robot.velocity == 0.0 {
        lost.push("limit effort and velocity (written 0 where not given)".into());
    }
    Ok(object(&[
        ("path", string(&out.display().to_string())),
        ("format", string("urdf")),
        ("bytes", urdf.xml.len().to_string()),
        ("geometry", string("rest")),
        ("meshes", crate::report::array(&names)),
        (
            "not_written",
            crate::report::array(&lost.iter().map(|s| string(s)).collect::<Vec<_>>()),
        ),
    ]))
}

/// Writes a program or a mesh as a USD stage: the `.usda` text to
/// `out` and each map beside it, or (`.usdz`) all of it in one file. A
/// mesh file's skins, morph targets and clips go with it. Reports the path, format and size, the
/// map files, and what the stage leaves out.
fn export_usda(files: &dyn Files, source: &Source, out: &Path) -> Result<String, Failure> {
    let stem = out
        .file_stem()
        .map_or("stage".to_string(), |s| s.to_string_lossy().into_owned());
    let usd = match source {
        Source::Program(model) => model.usda(&stem).map_err(failed)?,
        Source::Mesh(mesh) => {
            let placed = mesh.placed()?;
            let mut s = scene(&mesh.name, &placed);
            for (part, p) in s.parts.iter_mut().zip(&mesh.imported.parts) {
                part.material = p.material.clone();
            }
            // Each part's clips, by the name the scene gives it.
            let clips: Vec<(&str, &[shape_mesh::SkinClip])> = s
                .parts
                .iter()
                .zip(&mesh.imported.parts)
                .filter(|(_, p)| !p.clips.is_empty())
                .map(|(part, p)| (part.name.as_str(), p.clips.as_slice()))
                .collect();
            shape_tools::usda_clips(&s, &stem, &clips).map_err(failed)?
        }
    };
    let packed = extension(out) == "usdz";
    let mut names = Vec::new();
    let bytes = if packed {
        // One file: the stage and its maps inside.
        names.extend(usd.textures.iter().map(|(name, _)| string(name)));
        shape_tools::usdz(&usd, &format!("{stem}.usda"))
    } else {
        for (name, bytes) in &usd.textures {
            files.write(&out.with_file_name(name), bytes)?;
            names.push(string(name));
        }
        usd.text.clone().into_bytes()
    };
    files.write(out, &bytes)?;
    let lost: Vec<String> = usd.not_written.iter().map(|s| string(s)).collect();
    Ok(object(&[
        ("path", string(&out.display().to_string())),
        ("format", string(if packed { "usdz" } else { "usda" })),
        ("bytes", bytes.len().to_string()),
        ("geometry", string("rest")),
        ("textures", crate::report::array(&names)),
        ("not_written", crate::report::array(&lost)),
    ]))
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
    encode_solid_clips(name, s, material, &[], out)
}

/// [`encode_solid`], a skinned solid's GLB with `clips` as animations of
/// its bones (a file's own, carried through the tools).
pub fn encode_solid_clips(
    name: &str,
    s: &Solid,
    material: Option<&Material>,
    clips: &[shape_mesh::SkinClip],
    out: &Path,
) -> Result<Vec<u8>, Failure> {
    let parts = [(name.to_string(), s.clone())];
    match extension(out).as_str() {
        // A skinned solid goes out as a scene of one part: a mesh node,
        // its bones as nodes and a skin, as a program's skinned part does
        // (a plain GLB is one node and carries none).
        "glb" if s.skin.is_some() || s.morphs.is_some() => {
            let mut part = shape_tools::Part::new(name, s);
            if let Some(m) = material {
                part.material = m.clone();
            }
            shape_tools::glb_skin_clips(
                &shape_tools::Scene {
                    name: name.to_string(),
                    parts: vec![part],
                },
                &[(name, clips)],
            )
            .map_err(failed)
        }
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
    write_solid_clips(files, name, s, material, &[], out)
}

/// [`write_solid`], with a skinned solid's clips in its GLB.
pub fn write_solid_clips(
    files: &dyn Files,
    name: &str,
    s: &Solid,
    material: Option<&Material>,
    clips: &[shape_mesh::SkinClip],
    out: Option<&Path>,
) -> Result<String, Failure> {
    match out {
        None => Ok("null".into()),
        Some(out) => {
            let bytes = encode_solid_clips(name, s, material, clips, out)?;
            files.write(out, &bytes)?;
            Ok(written_lost(
                out,
                bytes.len(),
                &not_written(s, material, out),
            ))
        }
    }
}

/// [`write_solid`] as a GLB with `TANGENT` (the frame a baked normal
/// map was baked in): plain, or with its skin, morph targets and clips;
/// OBJ and STL as [`write_solid`].
pub fn write_solid_tangents(
    files: &dyn Files,
    name: &str,
    s: &Solid,
    material: &Material,
    clips: &[shape_mesh::SkinClip],
    out: Option<&Path>,
) -> Result<String, Failure> {
    let Some(out) = out else {
        return Ok("null".into());
    };
    if extension(out) != "glb" {
        return write_solid(files, name, s, Some(material), Some(out));
    }
    if s.skin.is_some() || s.morphs.is_some() {
        // With bones or morph targets: a scene of one part, as
        // [`encode_solid_clips`] writes it, with `TANGENT`.
        let mut part = shape_tools::Part::new(name, s);
        part.material = material.clone();
        part.tangents = true;
        let bytes = shape_tools::glb_skin_clips(
            &shape_tools::Scene {
                name: name.to_string(),
                parts: vec![part],
            },
            &[(name, clips)],
        )
        .map_err(failed)?;
        files.write(out, &bytes)?;
        return Ok(written_lost(
            out,
            bytes.len(),
            &not_written(s, Some(material), out),
        ));
    }
    let bytes = glb_plain(
        s,
        &PlainGlb {
            name,
            material: Some(material),
            tangents: true,
            ..Default::default()
        },
    )
    .map_err(failed)?;
    files.write(out, &bytes)?;
    Ok(written_lost(
        out,
        bytes.len(),
        &not_written(s, Some(material), out),
    ))
}

/// A program to a GLB with its parts' UVs packed into one atlas of
/// `size` texels (`Model::glb_atlas`); reports the file, `atlas` (the
/// packing and its measures), `materials` (how many the packed parts
/// share) and `own_uvs` (parts with a map, left with their UVs).
pub fn export_atlas(
    files: &dyn Files,
    source: &Source,
    out: &Path,
    size: u32,
) -> Result<String, Failure> {
    let Source::Program(model) = source else {
        return Err(Failure::Usage(
            "--atlas packs a program's parts; a mesh's UVs are its own (process --bake)".into(),
        ));
    };
    if extension(out) != "glb" {
        return Err(Failure::Usage("--atlas writes a GLB".into()));
    }
    let e = model
        .glb_atlas(&shape_tools::Atlas {
            size,
            ..Default::default()
        })
        .map_err(failed)?;
    files.write(out, &e.glb)?;
    Ok(object(&[
        ("path", string(&out.display().to_string())),
        ("format", string("glb")),
        ("bytes", e.glb.len().to_string()),
        ("geometry", string("rest")),
        ("atlas", e.atlas.to_json()),
        ("materials", e.materials.to_string()),
        (
            "own_uvs",
            crate::report::array(&e.own.iter().map(|n| string(n)).collect::<Vec<_>>()),
        ),
    ]))
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    let mut known = vec![
        "-o",
        "--out",
        "--uvs",
        "--atlas",
        "--density",
        "--effort",
        "--velocity",
    ];
    known.extend(IMPORT_OPTIONS);
    a.only(&known)?;
    let out = a.out()?;
    if !["glb", "obj", "stl", "urdf", "usda", "usdz"].contains(&extension(&out).as_str()) {
        return Err(bad_extension(&out));
    }
    let source = load_uvs(&Local, &a.input()?, &a.import()?, a.switch("--uvs"))?;
    match a.number::<u32>("--atlas")? {
        Some(size) => export_atlas(&Local, &source, &out, size),
        None => export_with(
            &Local,
            &source,
            &out,
            &shape_tools::UrdfDesc {
                density: a.number("--density")?.unwrap_or(0.0),
                effort: a.number("--effort")?.unwrap_or(0.0),
                velocity: a.number("--velocity")?.unwrap_or(0.0),
            },
        ),
    }
}
