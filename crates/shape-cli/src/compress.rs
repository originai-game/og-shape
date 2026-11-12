//! `shape compress <file.glb> -o out.glb`: the GLB with its geometry
//! and animation under `EXT_meshopt_compression`
//! (`shape_tools::compress`), losslessly, as JSON: how many buffer
//! views were compressed, their bytes before and after, and the output.

use std::path::PathBuf;

use crate::args::Args;
use crate::report::object;
use crate::source::{Input, Local};
use crate::{Failure, Files, failed};

/// What to compress. The zero value has no input.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompressDesc {
    /// The GLB file.
    pub input: Input,
    /// Where to write the result.
    pub out: PathBuf,
}

/// The report: `views`, `compressed`, `bytes_before`, `bytes_after`
/// (of the views compressed), `file_before`, and `output`.
pub fn compress(files: &dyn Files, d: &CompressDesc) -> Result<String, Failure> {
    let Input::Path(path) = &d.input else {
        return Err(Failure::Usage("a GLB file to compress".into()));
    };
    let bytes = files.read(path)?;
    let (out, c) = shape_tools::compress(&bytes).map_err(failed)?;
    files.write(&d.out, &out)?;
    Ok(object(&[
        ("views", c.views.to_string()),
        ("compressed", c.compressed.to_string()),
        ("bytes_before", c.bytes_before.to_string()),
        ("bytes_after", c.bytes_after.to_string()),
        ("file_before", bytes.len().to_string()),
        ("output", crate::export::written(&d.out, out.len())),
    ]))
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    a.only(&["-o", "--out"])?;
    compress(
        &Local,
        &CompressDesc {
            input: a.input()?,
            out: a.out()?,
        },
    )
}
