//! `shape compress <file.glb> -o out.glb`: the GLB with its geometry
//! and animation under `EXT_meshopt_compression`
//! (`shape_tools::compress`), losslessly, as JSON: how many buffer
//! views were compressed, their bytes before and after, and the output.
//! With `--draco`, its triangle meshes under
//! `KHR_draco_mesh_compression` instead (`shape_tools::compress_draco`,
//! lossy: `--position-bits`, `--normal-bits`, `--uv-bits`,
//! `--color-bits`).

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
    /// Compress with Draco (lossy: values kept to a number of bits)
    /// in place of meshopt (lossless).
    pub draco: Option<shape_tools::Draco>,
    /// Before meshopt: write the vertex attributes in fewer bytes
    /// (`shape_tools::quantize`, lossy), positions to this many bits
    /// (0 is 14).
    pub quantize: Option<u8>,
}

/// The report: `views`, `compressed`, `bytes_before`, `bytes_after`
/// (of the views compressed), `file_before`, and `output`.
pub fn compress(files: &dyn Files, d: &CompressDesc) -> Result<String, Failure> {
    let Input::Path(path) = &d.input else {
        return Err(Failure::Usage("a GLB file to compress".into()));
    };
    let bytes = files.read(path)?;
    if let Some(draco) = &d.draco {
        let (out, c) = shape_tools::compress_draco(&bytes, draco).map_err(failed)?;
        files.write(&d.out, &out)?;
        return Ok(object(&[
            ("primitives", c.primitives.to_string()),
            ("compressed", c.compressed.to_string()),
            ("bytes_before", c.bytes_before.to_string()),
            ("bytes_after", c.bytes_after.to_string()),
            ("file_before", bytes.len().to_string()),
            ("output", crate::export::written(&d.out, out.len())),
        ]));
    }
    let (smaller, q) = match d.quantize {
        Some(bits) => shape_tools::quantize(&bytes, bits).map_err(failed)?,
        None => (bytes.clone(), shape_tools::Quantized::default()),
    };
    let (out, c) = shape_tools::compress(&smaller).map_err(failed)?;
    files.write(&d.out, &out)?;
    Ok(object(&[
        (
            "quantized",
            match d.quantize {
                None => "null".to_string(),
                Some(_) => object(&[
                    ("accessors", q.accessors.to_string()),
                    ("meshes", q.meshes.to_string()),
                    ("bytes_before", q.bytes_before.to_string()),
                    ("bytes_after", q.bytes_after.to_string()),
                ]),
            },
        ),
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
    a.only(&[
        "-o",
        "--out",
        "--draco",
        "--quantize",
        "--position-bits",
        "--normal-bits",
        "--uv-bits",
        "--color-bits",
    ])?;
    let bits = |name: &str| a.number::<u8>(name).map(|v| v.unwrap_or(0));
    let draco = shape_tools::Draco {
        position_bits: bits("--position-bits")?,
        normal_bits: bits("--normal-bits")?,
        uv_bits: bits("--uv-bits")?,
        color_bits: bits("--color-bits")?,
    };
    compress(
        &Local,
        &CompressDesc {
            input: a.input()?,
            out: a.out()?,
            draco: a.switch("--draco").then_some(draco),
            quantize: a.switch("--quantize").then_some(draco.position_bits),
        },
    )
}
