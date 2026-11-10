//! Writes phase 5b's skinned figure for a check outside the kernel
//! (`crates/shape-wasm/ts/tools/check_render.ts`, research §19): the
//! skinned GLB at rest, and a JSON file with, for each of the poses
//! `bend` and `wave`, its turns (bone, axis in the bone's rest axes,
//! radians) and every vertex's rest and posed position as the kernel's
//! `pose` gives them, in solid units. A viewer that poses the GLB's
//! bones by the same turns must put each vertex where the kernel does.
//!
//! `cargo run --release -p shape-skin --example figure_poses -- <figure.glb> <poses.json>`

#[path = "../tests/figure/mod.rs"]
mod figure;

use shape_skin::{pose, skin};
use shape_tools::{Part, Scene, glb};

fn json_vec(v: [f64; 3]) -> String {
    format!("[{},{},{}]", v[0], v[1], v[2])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(glb_path), Some(json_path)) = (args.next(), args.next()) else {
        return Err("usage: figure_poses <figure.glb> <poses.json>".into());
    };
    let s = skin(&figure::body(), &figure::rig())?;
    let mut part = Part::new("figure", &s);
    part.material.color = Some([0.85, 0.62, 0.45]);
    std::fs::write(
        &glb_path,
        glb(&Scene {
            name: "figure".into(),
            parts: vec![part],
        })?,
    )?;
    let rest: Vec<String> = s.positions.iter().map(|p| json_vec(p.to_array())).collect();
    let mut poses = Vec::new();
    for (name, p) in [("bend", figure::bend()), ("wave", figure::wave())] {
        let posed = pose(&s, &p)?;
        let turns: Vec<String> = p
            .turns
            .iter()
            .map(|t| {
                format!(
                    "{{\"bone\":{:?},\"axis\":{},\"angle\":{}}}",
                    t.bone,
                    json_vec(t.axis.to_array()),
                    t.angle
                )
            })
            .collect();
        let positions: Vec<String> = posed
            .positions
            .iter()
            .map(|p| json_vec(p.to_array()))
            .collect();
        poses.push(format!(
            "{{\"name\":{name:?},\"turns\":[{}],\"positions\":[{}]}}",
            turns.join(","),
            positions.join(",")
        ));
    }
    std::fs::write(
        &json_path,
        format!(
            "{{\"rest\":[{}],\"poses\":[{}]}}\n",
            rest.join(","),
            poses.join(",")
        ),
    )?;
    Ok(())
}
