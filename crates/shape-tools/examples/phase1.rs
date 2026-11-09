//! Writes the previews of phase 1's ten sand3d rebuilds (`docs/plan.md`):
//! one PNG per program, four views around it, into the directory given.
//!
//! `cargo run --release -p shape-tools --example phase1 -- docs/previews/phase1`

#[path = "../tests/phase1/mod.rs"]
mod phase1;

use shape_tools::{Preview, glb, png, preview, sheet};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args().nth(1).ok_or("usage: phase1 <directory>")?;
    std::fs::create_dir_all(&dir)?;
    for m in phase1::programs::all() {
        let views = preview(
            &m.scene(),
            &Preview {
                width: 320,
                views: 4,
                azimuth: -0.6,
                ..Default::default()
            },
        )?;
        let bytes = png(&sheet(&views, 2));
        let path = format!("{dir}/{}.png", m.name);
        std::fs::write(&path, &bytes)?;
        let scene = m.scene();
        let triangles: usize = scene.parts.iter().map(|p| p.solid.triangles.len()).sum();
        let joints = scene.parts.iter().filter(|p| p.joint.is_some()).count();
        println!(
            "{}: {} parts, {joints} joints, {triangles} triangles, GLB {} bytes, PNG {} bytes",
            m.name,
            scene.parts.len(),
            glb(&scene)?.len(),
            bytes.len()
        );
    }
    Ok(())
}
