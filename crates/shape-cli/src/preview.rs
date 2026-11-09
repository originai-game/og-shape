//! `shape preview <program.json> -o <out.png>`: a turntable of the posed
//! model from the CPU rasterizer, its views side by side in one PNG.

use shape_model::Preview;

use crate::args::{Args, Failure, failed, load, write};

pub fn run(a: Args) -> Result<String, Failure> {
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
    ])?;
    let out = a.out()?;
    let d = Preview {
        width: a.number("--width")?.unwrap_or(0),
        height: a.number("--height")?.unwrap_or(0),
        views: a.number("--views")?.unwrap_or(0),
        elevation: a.number("--elevation")?,
        azimuth: a.number("--azimuth")?.unwrap_or(0.0),
        no_edges: a.switch("--no-edges"),
        no_ground: a.switch("--no-ground"),
    };
    let model = load(&a.program()?)?;
    let views = model.preview(&d).map_err(failed)?;
    let columns = a.number("--columns")?.unwrap_or(views.len() as u32);
    write(
        &out,
        &shape_tools::png(&shape_tools::sheet(&views, columns)),
    )?;
    Ok(String::new())
}
