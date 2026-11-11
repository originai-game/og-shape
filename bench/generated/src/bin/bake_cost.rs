//! What the bake itself costs (`docs/research.md` §34): per repaired
//! source (`bake.ts`'s OUT/rep/NAME.glb), the time and memory of each
//! step of `process --bake`, the bake alone apart.
//!
//! ```text
//! cargo run --release --bin bake_cost -- BUDGET SIZE REPAIRED.glb...
//! ```
//!
//! One JSON line per file: source triangles; milliseconds to read,
//! decimate (without UVs or colours), pack and bake; the resident memory
//! (VmRSS) before the bake, the high-water mark (VmHWM) after each step,
//! in MiB; and the bake's own counts.

use std::time::Instant;

use shape_tools::{
    Atlas, Bake, BakeSource, Decimate, Import, Unwrap, bake, decimate, import, join, unwrap_atlas,
};

fn status(key: &str) -> f64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    s.lines()
        .find(|l| l.starts_with(key))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<f64>().ok())
        .map_or(0.0, |kib| kib / 1024.0)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let budget: usize = args[0].parse().unwrap();
    let size: u32 = args[1].parse().unwrap();
    for path in &args[2..] {
        let t = Instant::now();
        let bytes = std::fs::read(path).unwrap();
        let m = import(&bytes, &Import::default(), &|_| Err("no files".into())).unwrap();
        let src = join(&m.parts).unwrap();
        let read_ms = t.elapsed().as_millis();
        let hwm_read = status("VmHWM:");
        let t = Instant::now();
        let mut low = src.clone();
        low.uvs = None;
        low.colors = None;
        let (mut low, _) = if low.triangles.len() > budget {
            decimate(
                &low,
                &Decimate {
                    target: budget,
                    ..Default::default()
                },
            )
            .unwrap()
        } else {
            (low, Default::default())
        };
        let dec_ms = t.elapsed().as_millis();
        let t = Instant::now();
        unwrap_atlas(
            &mut low,
            &Unwrap::default(),
            &Atlas {
                size,
                padding: 4,
                no_turns: false,
            },
        )
        .unwrap();
        let pack_ms = t.elapsed().as_millis();
        let hwm_before = status("VmHWM:");
        let rss_before = status("VmRSS:");
        let materials: Vec<_> = m.parts.iter().take(1).map(|p| p.material.clone()).collect();
        let t = Instant::now();
        let b = bake(
            &BakeSource {
                solid: &src,
                materials: &materials,
                material_of: &[],
            },
            &low,
            &Bake {
                size,
                ..Default::default()
            },
        )
        .unwrap();
        let bake_ms = t.elapsed().as_millis();
        let hwm_after = status("VmHWM:");
        let r = &b.report;
        println!(
            "{{\"file\": {path:?}, \"source_triangles\": {}, \"read_ms\": {read_ms}, \"decimate_ms\": {dec_ms}, \"pack_ms\": {pack_ms}, \"bake_ms\": {bake_ms}, \"hwm_read_mib\": {hwm_read:.0}, \"rss_before_bake_mib\": {rss_before:.0}, \"hwm_before_bake_mib\": {hwm_before:.0}, \"hwm_after_bake_mib\": {hwm_after:.0}, \"covered\": {}, \"hits\": {}, \"nearest\": {}, \"no_source\": {}}}",
            src.triangles.len(),
            r.covered,
            r.hits,
            r.nearest,
            r.no_source
        );
    }
}
