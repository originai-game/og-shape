//! `shape preview <program.json | mesh> -o <out.png>`: a turntable of the
//! posed model (or of a mesh file's parts, placed) from the CPU
//! rasterizer, its views side by side in one PNG.

use shape_tools::Preview;

use crate::args::{Args, IMPORT_OPTIONS};
use crate::source::{Local, Source, load_uvs, scene};
use crate::{Failure, Files, failed};

/// The views and how to lay them out. The zero value is the kernel's
/// default turntable (`shape_tools::Preview`) in one row.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreviewDesc {
    /// The views.
    pub preview: Preview,
    /// Views a row; 0 puts them all in one row.
    pub columns: u32,
}

/// The turntable as PNG bytes.
pub fn render(source: &Source, d: &PreviewDesc) -> Result<Vec<u8>, Failure> {
    let views = match source {
        Source::Program(m) => m.preview(&d.preview).map_err(failed)?,
        Source::Mesh(mesh) => {
            let placed = mesh.placed()?;
            shape_tools::preview(&scene(&mesh.name, &placed), &d.preview).map_err(failed)?
        }
    };
    let columns = if d.columns == 0 {
        views.len() as u32
    } else {
        d.columns
    };
    Ok(shape_tools::png(&shape_tools::sheet(&views, columns)))
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    a.only(&[
        "-o",
        "--out",
        "--views",
        "--width",
        "--height",
        "--columns",
        "--elevation",
        "--azimuth",
        "--no-edges",
        "--no-ground",
        "--uvs",
        IMPORT_OPTIONS[0],
        IMPORT_OPTIONS[1],
    ])?;
    let out = a.out()?;
    let d = PreviewDesc {
        preview: Preview {
            width: a.number("--width")?.unwrap_or(0),
            height: a.number("--height")?.unwrap_or(0),
            views: a.number("--views")?.unwrap_or(0),
            elevation: a.number("--elevation")?,
            azimuth: a.number("--azimuth")?.unwrap_or(0.0),
            no_edges: a.switch("--no-edges"),
            no_ground: a.switch("--no-ground"),
        },
        columns: a.number("--columns")?.unwrap_or(0),
    };
    let source = load_uvs(&Local, &a.input()?, &a.import()?, a.switch("--uvs"))?;
    Local.write(&out, &render(&source, &d)?)?;
    Ok(String::new())
}
