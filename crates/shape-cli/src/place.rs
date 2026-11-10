//! `shape place <mesh> [-o out]`: the file's parts turned, sized and
//! moved as one (`shape_tools::place`), so the model stands where a game
//! wants it: up along +Z and facing −Y, at a real size, its origin at
//! its base. Each step is optional and the report gives the numbers
//! before and after (`shape_tools::PlaceReport`). With `-o`, writes the
//! parts placed: GLB with their skins, UVs and base-colour textures, or
//! OBJ or STL.
//!
//! `--up` and `--front` name the file's own axes (as every mesh command
//! reads them; `--z-up` is `--up +z --front -y`); the report's `up` and
//! `front` are the kernel axes of the file read plainly (Y up for OBJ
//! and glTF) that became up and front. Lengths take a unit: `1.2m`,
//! `90cm`, `900mm`.

use std::path::{Path, PathBuf};

use shape_mesh::Solid;
use shape_tools::{Import, Pivot, Place, Size, glb, obj, place as place_parts, stl};

use crate::args::{Args, IMPORT_OPTIONS};
use crate::export::written;
use crate::report::{self, object};
use crate::source::{Input, Local, Mesh, extension, load_mesh, scene};
use crate::{Failure, Files, failed};

/// What to place. The zero value has no input; with an input, it
/// changes nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlaceDesc {
    /// The mesh file.
    pub input: Input,
    /// How to read it: its unit, and the axes that are its up and front.
    pub import: Import,
    /// The placement. Its `up` and `front` are kernel axes, for a mesh
    /// read plainly; give them here or in `import`, not both.
    pub place: Place,
    /// Where to write the result, if anywhere.
    pub out: Option<PathBuf>,
}

/// The import options as a plain read and a [`Place`] that turns as they
/// would, merged into `place`.
pub fn plain_and_place(
    format: &str,
    import: &Import,
    place: &Place,
) -> Result<(Import, Place), Failure> {
    if format == "fbx" && import.turns() {
        return Err(Failure::Usage(
            "up, front and z_up are for OBJ and glTF; an FBX file says its axes".into(),
        ));
    }
    let (plain, up, front) = import.as_turn().map_err(failed)?;
    if up.is_some() && (place.up.is_some() || place.front.is_some()) {
        return Err(Failure::Usage(
            "give up and front once: as the file's axes, or as the placement's".into(),
        ));
    }
    let mut p = *place;
    if up.is_some() {
        p.up = up;
        p.front = front;
    }
    Ok((plain, p))
}

/// The place report: `import`, `place` (what was done, and the axis
/// measures before and after) and `output`.
pub fn place(files: &dyn Files, d: &PlaceDesc) -> Result<String, Failure> {
    let format = match &d.input {
        Input::Path(p) => extension(p),
        _ => String::new(),
    };
    let (plain, p) = plain_and_place(&format, &d.import, &d.place)?;
    let mesh = load_mesh(files, &d.input, &plain)?;
    let imported = report::imported(&mesh.format, &mesh.imported);
    let placed = mesh.placed()?;
    let solids: Vec<Solid> = placed.iter().map(|(_, s)| s.clone()).collect();
    let (out, r) = place_parts(&solids, &p).map_err(failed)?;
    let parts: Vec<(String, Solid)> = placed.into_iter().map(|(n, _)| n).zip(out).collect();
    let output = match &d.out {
        None => "null".to_string(),
        Some(path) => {
            let bytes = encode_parts(&mesh, &parts, path)?;
            files.write(path, &bytes)?;
            written(path, bytes.len())
        }
    };
    Ok(object(&[
        ("import", imported),
        ("place", r.to_json()),
        ("output", output),
    ]))
}

/// A mesh file's parts, as placed here, in the format `out` names: GLB
/// keeps skins, UVs and each part's base-colour texture.
fn encode_parts(mesh: &Mesh, parts: &[(String, Solid)], out: &Path) -> Result<Vec<u8>, Failure> {
    let mut s = scene(&mesh.name, parts);
    for (part, p) in s.parts.iter_mut().zip(&mesh.imported.parts) {
        part.material.map = p.map.clone();
    }
    match extension(out).as_str() {
        "glb" => glb(&s).map_err(failed),
        "obj" => obj(&s).map(String::into_bytes).map_err(failed),
        "stl" => stl(&s).map_err(failed),
        _ => Err(Failure::Usage(format!(
            "{}: name the output .glb, .obj or .stl",
            out.display()
        ))),
    }
}

/// A length with its unit (`1.2m`, `90cm`, `900mm`), in mm.
pub fn length(what: &str, v: &str) -> Result<f64, Failure> {
    let v = v.trim();
    let (number, unit) = [("mm", 1.0), ("cm", 10.0), ("m", 1000.0)]
        .iter()
        .find_map(|(u, f)| v.strip_suffix(u).map(|n| (n, *f)))
        .ok_or_else(|| {
            Failure::Usage(format!(
                "{what}: {v:?} needs a unit: m, cm or mm (1.2m, 900mm)"
            ))
        })?;
    number
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|x| x.is_finite())
        .map(|x| x * unit)
        .ok_or_else(|| Failure::Usage(format!("{what}: {v:?} is not a length")))
}

/// A pivot: `base`, `center`, or a point `x,y,z` with a unit
/// (`0,0,-480mm`), in the input's coordinates.
pub fn pivot(v: &str) -> Result<Pivot, Failure> {
    match v.trim() {
        "base" => Ok(Pivot::Base),
        "center" | "centre" => Ok(Pivot::Center),
        p => {
            let bad = || {
                Failure::Usage(format!(
                    "pivot: {p:?} is not base, center or a point x,y,z with a unit (0,0,-480mm)"
                ))
            };
            let unit = p.trim_start_matches(|c: char| !c.is_ascii_alphabetic());
            let numbers: Vec<&str> = p[..p.len() - unit.len()].split(',').collect();
            if numbers.len() != 3 || unit.is_empty() {
                return Err(bad());
            }
            let mut xyz = [0.0; 3];
            for (k, n) in numbers.iter().enumerate() {
                xyz[k] = length("pivot", &format!("{n}{unit}")).map_err(|_| bad())?;
            }
            Ok(Pivot::Point(xyz.into()))
        }
    }
}

/// The placement options of the command line.
pub const OPTIONS: [&str; 7] = [
    "--turn",
    "--pivot",
    "--longest",
    "--height",
    "--width",
    "--depth",
    "--scale",
];

/// [`OPTIONS`] read into a placement (its axes are the import's).
pub fn options(a: &Args) -> Result<Place, Failure> {
    let mut sizes = Vec::new();
    for (name, size) in [
        ("--longest", Size::Longest as fn(f64) -> Size),
        ("--height", Size::Height),
        ("--width", Size::Width),
        ("--depth", Size::Depth),
    ] {
        if let Some(v) = a.value(&[name])? {
            sizes.push(size(length(name, v)?));
        }
    }
    if let Some(f) = a.number("--scale")? {
        sizes.push(Size::Scale(f));
    }
    if sizes.len() > 1 {
        return Err(Failure::Usage(
            "one size: --longest, --height, --width, --depth or --scale".into(),
        ));
    }
    Ok(Place {
        turn: a.number("--turn")?.unwrap_or(0.0),
        size: sizes.pop().unwrap_or_default(),
        pivot: a
            .value(&["--pivot"])?
            .map(pivot)
            .transpose()?
            .unwrap_or_default(),
        ..Default::default()
    })
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    let mut known = vec!["-o", "--out"];
    known.extend(IMPORT_OPTIONS);
    known.extend(OPTIONS);
    a.only(&known)?;
    place(
        &Local,
        &PlaceDesc {
            input: a.input()?,
            import: a.import()?,
            place: options(a)?,
            out: a.out_opt()?,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_need_a_unit() {
        assert_eq!(length("h", "1.2m").unwrap(), 1200.0);
        assert_eq!(length("h", "90cm").unwrap(), 900.0);
        assert_eq!(length("h", "900mm").unwrap(), 900.0);
        assert!(length("h", "900").is_err());
        assert_eq!(
            pivot("1,2,-3cm").unwrap(),
            Pivot::Point([10.0, 20.0, -30.0].into())
        );
        assert!(pivot("1,2cm").is_err());
    }
}
