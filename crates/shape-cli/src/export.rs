//! `shape export <program.json> -o <out>`: the posed model as GLB (with
//! joints and materials), or its meshes as OBJ or STL, by the output's
//! extension.

use crate::args::{Args, Failure, failed, load, write};

pub fn run(a: Args) -> Result<String, Failure> {
    a.only(&["-o", "--out"])?;
    let out = a.out()?;
    let model = load(&a.program()?)?;
    let ext = out
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let bytes = match ext.as_str() {
        "glb" => model.glb().map_err(failed)?,
        "obj" => model.obj().map_err(failed)?.into_bytes(),
        "stl" => model.stl().map_err(failed)?,
        _ => {
            return Err(Failure::Usage(format!(
                "{}: name the output .glb, .obj or .stl",
                out.display()
            )));
        }
    };
    write(&out, &bytes)?;
    Ok(String::new())
}
