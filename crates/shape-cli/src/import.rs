//! `shape import <mesh> [-o out]`: what a mesh file holds, as JSON: its
//! parts (name, material, triangles, UVs, bones), images, polygons
//! triangulated, and what the importer skipped. With `-o`, also writes
//! the parts placed as GLB (skins kept), OBJ or STL. Imported UVs and
//! textures are measured, not carried (`docs/research.md` §9.8).

use std::path::PathBuf;

use shape_tools::Import;

use crate::args::{Args, IMPORT_OPTIONS};
use crate::export::export;
use crate::report::{imported, object};
use crate::source::{Input, Local, Source, load_mesh};
use crate::{Failure, Files};

/// What to import. The zero value has no input.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportDesc {
    /// The mesh file.
    pub input: Input,
    /// How to read an OBJ file.
    pub import: Import,
    /// Where to write the parts, if anywhere.
    pub out: Option<PathBuf>,
}

/// The import report; `"output"` is the file written, or null.
pub fn import(files: &dyn Files, d: &ImportDesc) -> Result<String, Failure> {
    let mesh = load_mesh(files, &d.input, &d.import)?;
    let report = imported(&mesh.format, &mesh.imported);
    let output = match &d.out {
        None => "null".to_string(),
        Some(out) => export(files, &Source::Mesh(mesh), out)?,
    };
    Ok(object(&[("import", report), ("output", output)]))
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    a.only(&["-o", "--out", IMPORT_OPTIONS[0], IMPORT_OPTIONS[1]])?;
    import(
        &Local,
        &ImportDesc {
            input: a.input()?,
            import: a.import()?,
            out: a.out_opt()?,
        },
    )
}
