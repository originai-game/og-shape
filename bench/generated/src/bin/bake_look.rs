//! The look's error of a decimated mesh against its source
//! (`docs/research.md` §34): both rendered by the unlit preview from the
//! same fixed views, and the colour difference over the pixels both
//! cover. Numbers, no verdict.
//!
//! ```text
//! cargo run --release --bin bake_look -- SOURCE RESULT... [--size PX] [--sheet OUT.png]
//! ```
//!
//! SOURCE and each RESULT are mesh files (the repaired source, written by
//! `shape repair`, and `shape process` outputs). Every scene is framed by
//! the box of all of them together (a zero-area triangle across it is
//! added to each scene; it draws nothing), so a pixel sees the same place
//! in each. Four orthographic views, 256² (`--size`), `unlit`, no edges, no ground.
//! A pixel is covered where a black, unlit pass of the same scene draws
//! black. Per RESULT one JSON line: `mean` and `p95` of the colour
//! difference over pixels both cover (√(ΔR² + ΔG² + ΔB²) / √3 in sRGB
//! bytes, 0..=255), `both` (pixels both cover) and `one` (pixels only one
//! covers: the silhouette's change). `--sheet` writes rows of three
//! 160² lit views, the source first, then each result, for a person.

use shape_core::DVec3;
use shape_mesh::Solid;
use shape_tools::{
    Image, Import, Material, Part, Preview, Scene, apply_transform, import, png, preview, sheet,
};

fn read(path: &str) -> Vec<(Solid, Material)> {
    let p = std::path::Path::new(path);
    let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("{path}: {e}"));
    let dir = p.parent().unwrap().to_path_buf();
    let resolve = move |u: &str| std::fs::read(dir.join(u)).map_err(|e| e.to_string());
    let m = import(&bytes, &Import::default(), &resolve).unwrap_or_else(|e| panic!("{path}: {e}"));
    m.parts
        .iter()
        .map(|p| {
            let mut s = apply_transform(p).unwrap();
            // A file splits vertices at seams: weld by exact position, as
            // `shape preview` does.
            s.canonicalize().unwrap();
            (s, p.material.clone())
        })
        .collect()
}

fn bounds(parts: &[(Solid, Material)]) -> (DVec3, DVec3) {
    let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
    for (s, _) in parts {
        for p in &s.positions {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
    }
    (lo, hi)
}

fn render(
    parts: &[(Solid, Material)],
    frame: (DVec3, DVec3),
    d: &Preview,
    black: bool,
) -> Vec<Image> {
    let mut pin = Solid::default();
    pin.positions = vec![frame.0, frame.1, (frame.0 + frame.1) * 0.5];
    pin.push_triangle([0, 1, 2], 0, &[]);
    let names: Vec<String> = (0..=parts.len()).map(|k| format!("m{k}")).collect();
    let mut scene = Scene {
        name: "m".into(),
        parts: Vec::new(),
    };
    for ((s, m), n) in parts.iter().zip(&names) {
        let material = if black {
            Material {
                color: Some([0.0; 3]),
                ..Default::default()
            }
        } else {
            m.clone()
        };
        scene.parts.push(Part {
            material,
            ..Part::new(n, s)
        });
    }
    scene.parts.push(Part::new(&names[parts.len()], &pin));
    preview(&scene, d).unwrap()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut files: Vec<String> = Vec::new();
    let mut sheet_out: Option<String> = None;
    let mut px: u32 = 256;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--sheet" {
            sheet_out = it.next();
        } else if a == "--size" {
            px = it.next().and_then(|v| v.parse().ok()).unwrap_or(256);
        } else {
            files.push(a);
        }
    }
    assert!(
        files.len() >= 2,
        "usage: bake_look SOURCE RESULT... [--sheet OUT.png]"
    );
    let meshes: Vec<Vec<(Solid, Material)>> = files.iter().map(|f| read(f)).collect();
    let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
    for m in &meshes {
        let (a, b) = bounds(m);
        lo = lo.min(a);
        hi = hi.max(b);
    }
    let d = Preview {
        width: px,
        height: px,
        views: 4,
        no_edges: true,
        no_ground: true,
        unlit: true,
        ..Default::default()
    };
    let colour: Vec<Vec<Image>> = meshes
        .iter()
        .map(|m| render(m, (lo, hi), &d, false))
        .collect();
    let mask: Vec<Vec<Image>> = meshes
        .iter()
        .map(|m| render(m, (lo, hi), &d, true))
        .collect();
    let covered = |img: &Image, k: usize| img.rgba[k * 4..k * 4 + 3] == [0, 0, 0];
    for r in 1..meshes.len() {
        let mut diffs: Vec<f64> = Vec::new();
        let mut one = 0usize;
        for v in 0..colour[0].len() {
            let n = (colour[0][v].width * colour[0][v].height) as usize;
            for k in 0..n {
                let (a, b) = (covered(&mask[0][v], k), covered(&mask[r][v], k));
                if a != b {
                    one += 1;
                }
                if !(a && b) {
                    continue;
                }
                let (p, q) = (
                    &colour[0][v].rgba[k * 4..k * 4 + 3],
                    &colour[r][v].rgba[k * 4..k * 4 + 3],
                );
                let s: f64 = (0..3)
                    .map(|c| (f64::from(p[c]) - f64::from(q[c])).powi(2))
                    .sum();
                diffs.push((s / 3.0).sqrt());
            }
        }
        diffs.sort_by(f64::total_cmp);
        let mean = diffs.iter().sum::<f64>() / diffs.len().max(1) as f64;
        let p95 = diffs
            .get((diffs.len() * 95).div_ceil(100).saturating_sub(1))
            .copied()
            .unwrap_or(0.0);
        println!(
            "{{\"result\": {:?}, \"mean\": {mean:.3}, \"p95\": {p95:.3}, \"both\": {}, \"one\": {one}}}",
            files[r],
            diffs.len()
        );
    }
    if let Some(out) = sheet_out {
        let lit = Preview {
            width: 160,
            height: 160,
            views: 3,
            no_edges: true,
            ..Default::default()
        };
        let mut all = Vec::new();
        for m in &meshes {
            all.extend(render(m, (lo, hi), &lit, false));
        }
        std::fs::write(&out, png(&sheet(&all, 3))).unwrap();
    }
}
