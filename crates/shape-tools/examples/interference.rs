//! Times `interference` on phase 1's ten sand3d rebuilds (`docs/plan.md`),
//! as each program poses them, and prints what it found.
//!
//! `cargo run --release -p shape-tools --example interference`

#[path = "../tests/phase1/mod.rs"]
mod phase1;

use std::time::Instant;

use shape_tools::interference;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for m in phase1::programs::all() {
        let scene = m.scene();
        let triangles: usize = scene.parts.iter().map(|p| p.solid.triangles.len()).sum();
        let t = Instant::now();
        let r = interference(&scene)?;
        let ms = t.elapsed().as_secs_f64() * 1e3;
        let crossing = r.iter().filter(|x| x.crossings > 0).count();
        let inside = r.iter().filter(|x| !x.inside.is_empty()).count();
        println!(
            "{}: {} parts, {triangles} triangles, {ms:.1} ms; {} pairs meet or nest, {crossing} crossing, {inside} inside",
            m.name,
            scene.parts.len(),
            r.len()
        );
    }
    Ok(())
}
