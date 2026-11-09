//! Writes phase 5b's figure (`docs/plan.md`): its skinned GLB and the
//! previews of its rest pose, the acceptance bend (left elbow and right
//! knee at 90°) and a livelier pose, into the directory given, and prints
//! the pose measures. With a second path, also writes the skinned GLB
//! there (about 1 MB, so not committed).
//!
//! `cargo run --release -p shape-skin --example phase5b -- docs/previews/phase5b [figure.glb]`

#[path = "../tests/figure/mod.rs"]
mod figure;

use shape_skin::{Pose, measure_pose, pose, skin};
use shape_tools::{Part, Preview, Scene, glb, png, preview, sheet};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .ok_or("usage: phase5b <directory>")?;
    std::fs::create_dir_all(&dir)?;
    let body = figure::body();
    let s = skin(&body, &figure::rig())?;
    let sk = s.skin.as_ref().ok_or("no skin")?;
    println!(
        "figure: {} triangles before skinning, {} after refining the bands; {} vertices, {} bones, blend {} mm",
        body.triangles.len(),
        s.triangles.len(),
        s.positions.len(),
        sk.bones.len(),
        figure::BLEND
    );
    let color = Some([0.85, 0.62, 0.45]);
    let mut part = Part::new("figure", &s);
    part.material.color = color;
    let bytes = glb(&Scene {
        name: "figure".into(),
        parts: vec![part],
    })?;
    println!("GLB: {} bytes", bytes.len());
    if let Some(path) = std::env::args().nth(2) {
        std::fs::write(path, &bytes)?;
    }
    for (name, p) in [
        ("rest", Pose::default()),
        ("bend", figure::bend()),
        ("wave", figure::wave()),
    ] {
        let m = measure_pose(&s, &p)?;
        println!("{name}: {m:?}");
        let posed = pose(&s, &p)?;
        let mut part = Part::new("figure", &posed);
        part.material.color = color;
        let views = preview(
            &Scene {
                name: name.into(),
                parts: vec![part],
            },
            &Preview {
                width: 320,
                views: 4,
                azimuth: -0.6,
                ..Default::default()
            },
        )?;
        std::fs::write(format!("{dir}/{name}.png"), png(&sheet(&views, 2)))?;
    }
    Ok(())
}
