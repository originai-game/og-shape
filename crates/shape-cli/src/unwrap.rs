//! `shape unwrap <mesh> [-o out]`: the file's parts joined and UV-unwrapped
//! (`shape_tools::unwrap`), as JSON: the charts, the UV scale and the
//! measures with the new UVs. With `-o`, writes a plain GLB with the UVs
//! (OBJ and STL carry none). A skinned file of one part is unwrapped as
//! it is; the plain GLB refuses its skin.

use std::path::PathBuf;

use shape_tools::{Import, Measure, Unwrap};

use crate::args::{Args, IMPORT_OPTIONS};
use crate::export::write_solid;
use crate::report::{self, object};
use crate::source::{Input, Local, load_mesh};
use crate::{Failure, Files, failed};

/// What to unwrap. The zero value has no input.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnwrapDesc {
    /// The mesh file.
    pub input: Input,
    /// How to read the mesh file: its unit (OBJ) and axes.
    pub import: Import,
    /// The unwrap.
    pub unwrap: Unwrap,
    /// Where to write the result, if anywhere.
    pub out: Option<PathBuf>,
}

/// The unwrap report: `unwrap` (charts, scale), `measures`, `output`.
pub fn unwrap(files: &dyn Files, d: &UnwrapDesc) -> Result<String, Failure> {
    let mesh = load_mesh(files, &d.input, &d.import)?;
    let s = match &mesh.imported.parts[..] {
        [one] if one.solid.skin.is_some() => one.solid.clone(),
        _ => mesh.joined()?,
    };
    let mut s = s;
    let uvs = shape_tools::unwrap(&mut s, &d.unwrap).map_err(failed)?;
    let m = shape_tools::measure(
        &s,
        &Measure {
            uv_resolution: d.unwrap.resolution,
            ..Default::default()
        },
    );
    let output = write_solid(files, &mesh.name, &s, None, d.out.as_deref())?;
    Ok(object(&[
        ("unwrap", report::unwrap(&uvs)),
        ("measures", m.to_json()),
        ("output", output),
    ]))
}

/// The unwrap options of the command line.
pub const OPTIONS: [&str; 3] = ["--angle", "--resolution", "--padding"];

/// [`OPTIONS`] read into an unwrap desc.
pub fn options(a: &Args) -> Result<Unwrap, Failure> {
    Ok(Unwrap {
        angle: a.number("--angle")?.unwrap_or(0.0),
        resolution: a.number("--resolution")?.unwrap_or(0),
        padding: a.number("--padding")?.unwrap_or(0.0),
    })
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    let mut known = vec!["-o", "--out"];
    known.extend(IMPORT_OPTIONS);
    known.extend(OPTIONS);
    a.only(&known)?;
    unwrap(
        &Local,
        &UnwrapDesc {
            input: a.input()?,
            import: a.import()?,
            unwrap: options(a)?,
            out: a.out_opt()?,
        },
    )
}
