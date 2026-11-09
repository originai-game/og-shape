//! Prints a program's interference report and what it cost: at rest, then
//! at each pose given as `joint=value,joint=value`.
//!
//! `cargo run --release -p shape-model --example report --
//! golden/programs/washing_machine.json door_hinge=2.4958,drawer_slide=200`

use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("usage: report <program.json> [joint=value,...]...")?;
    let m = shape_model::program::run(&std::fs::read_to_string(path)?)?;
    let mut poses: Vec<Vec<(String, f64)>> = vec![Vec::new()];
    for a in args {
        let mut pose = Vec::new();
        for kv in a.split(',') {
            let (k, v) = kv.split_once('=').ok_or("joint=value")?;
            pose.push((k.to_string(), v.parse()?));
        }
        poses.push(pose);
    }
    let triangles: usize = m.measure()?.iter().map(|p| p.measures.triangles).sum();
    println!("{} parts, {triangles} triangles", m.measure()?.len());
    for pose in &poses {
        let t = Instant::now();
        let r = m.interference(pose)?;
        let ms = t.elapsed().as_secs_f64() * 1e3;
        println!("pose {pose:?}: {} pairs, {ms:.1} ms", r.len());
        for x in &r {
            println!(
                "  {} / {}: crossings {}, touching {}, inside {:?}",
                x.a, x.b, x.crossings, x.touching, x.inside
            );
        }
    }
    Ok(())
}
