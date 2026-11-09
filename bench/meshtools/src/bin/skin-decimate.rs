//! Phase 5b part 2's acceptance on other tools' skinned GLB: each skinned
//! part decimated to half, then checked (`docs/research.md` §11.5):
//! - the same bones;
//! - every vertex's weights sum to 1 (`Skin::check`);
//! - no vertex weighted to a bone that weighted no vertex of the full
//!   mesh within one decimated edge of it;
//! - posed by the same bone the same way, the two surfaces' Hausdorff
//!   distance, and inverted triangles against the full mesh's.
//!
//! ```text
//! cargo run --release --bin skin-decimate -- [--previews DIR] FILE.glb...
//! ```

#[allow(dead_code)]
#[path = "../../../../crates/shape-tools/tests/support/hausdorff.rs"]
mod hausdorff;

use std::path::PathBuf;

use glam::DVec3;
use shape_mesh::Solid;
use shape_skin::{Pose, Turn, measure_pose, pose};
use shape_tools::{
    Decimate, Import, Part, Preview, Scene, decimate, import, no_files, png, preview, sheet,
};

fn turned_bone(s: &Solid) -> String {
    let skin = s.skin.as_ref().unwrap();
    let below = |b: usize| {
        skin.bones
            .iter()
            .filter(|x| {
                let mut at = x.parent;
                while let Some(p) = at {
                    if p as usize == b {
                        return true;
                    }
                    at = skin.bones[p as usize].parent;
                }
                false
            })
            .count()
    };
    let b = (1..skin.bones.len())
        .max_by_key(|&b| (below(b), usize::MAX - b))
        .unwrap_or(0);
    skin.bones[b].name.clone()
}

fn stray_weights(full: &Solid, half: &Solid) -> usize {
    let (kf, kh) = (full.skin.as_ref().unwrap(), half.skin.as_ref().unwrap());
    let mut reach: f64 = 0.0;
    for t in &half.triangles {
        for i in 0..3 {
            reach = reach.max(
                (half.positions[t[i] as usize] - half.positions[t[(i + 1) % 3] as usize]).length(),
            );
        }
    }
    let mut stray = 0;
    for (p, w) in half.positions.iter().zip(&kh.weights) {
        for (b, _) in w.pairs() {
            let near = full
                .positions
                .iter()
                .zip(&kf.weights)
                .any(|(q, wq)| (*q - *p).length() <= reach && wq.pairs().any(|(x, _)| x == b));
            if !near {
                stray += 1;
            }
        }
    }
    stray
}

fn main() {
    let mut previews: Option<PathBuf> = None;
    let mut files = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        if a == "--previews" {
            previews = it.next().map(PathBuf::from);
        } else {
            files.push(a);
        }
    }
    for path in files {
        let bytes = std::fs::read(&path).expect("read");
        let im = import(&bytes, &Import::default(), &no_files).expect("import");
        let file = std::path::Path::new(&path)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();
        for part in im.parts.iter().filter(|p| p.solid.skin.is_some()) {
            let mut full = part.solid.clone();
            full.canonicalize().unwrap();
            let bone = turned_bone(&full);
            let bend = Pose {
                turns: vec![Turn {
                    bone: bone.clone(),
                    axis: DVec3::X,
                    angle: 0.5,
                }],
            };
            let posed_full = pose(&full, &bend).unwrap();
            let mf = measure_pose(&full, &bend).unwrap();
            for cross in [false, true] {
                let t = std::time::Instant::now();
                let (half, r) = decimate(
                    &full,
                    &Decimate {
                        ratio: 0.5,
                        cross_bone_borders: cross,
                        ..Default::default()
                    },
                )
                .unwrap();
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                let kh = half.skin.as_ref().unwrap();
                let same_bones = kh.bones == full.skin.as_ref().unwrap().bones;
                let check = kh.check(half.positions.len());
                let stray = stray_weights(&full, &half);
                let posed_half = pose(&half, &bend).unwrap();
                let d = hausdorff::hausdorff(&posed_full, &posed_half);
                let rest = hausdorff::hausdorff(&full, &half);
                let mh = measure_pose(&half, &bend).unwrap();
                // Skin drift: kept vertices (at a position the full mesh
                // has), posed with their new weights against their old.
                let mut drift: f64 = 0.0;
                for (i, p) in half.positions.iter().enumerate() {
                    if let Some(j) = full.positions.iter().position(|q| q == p) {
                        drift =
                            drift.max((posed_half.positions[i] - posed_full.positions[j]).length());
                    }
                }
                let (lo, hi) = full.positions.iter().fold(
                    (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
                    |(l, h), p| (l.min(*p), h.max(*p)),
                );
                println!(
                    "{file} {:?} [{}]: {} → {} triangles (target {}), {ms:.0} ms; same bones {same_bones}; weights {}; stray weights {stray}; rest Hausdorff {rest:.3} mm; {bone:?} turned 0.5 rad: posed Hausdorff {d:.3} mm, skin drift {drift:.3} mm, over a {:.0} mm model; inverted {} (full mesh {})",
                    part.name,
                    if cross {
                        "borders crossed"
                    } else {
                        "borders kept"
                    },
                    full.triangles.len(),
                    half.triangles.len(),
                    r.target,
                    if check.is_ok() {
                        "sum to 1".to_string()
                    } else {
                        format!("{check:?}")
                    },
                    (hi - lo).length(),
                    mh.inverted,
                    mf.inverted
                );
                if let (Some(dir), false) = (&previews, cross) {
                    let _ = std::fs::create_dir_all(dir);
                    let d = Preview {
                        width: 320,
                        views: 1,
                        azimuth: 0.6,
                        ..Default::default()
                    };
                    for (label, s) in [
                        ("full.rest", &full),
                        ("full.posed", &posed_full),
                        ("half.rest", &half),
                        ("half.posed", &posed_half),
                    ] {
                        let mut s = s.clone();
                        s.skin = None;
                        let v = preview(
                            &Scene {
                                name: label.into(),
                                parts: vec![Part::new(label, &s)],
                            },
                            &d,
                        )
                        .unwrap();
                        let _ = std::fs::write(
                            dir.join(format!("{file}.{}.{label}.png", part.name)),
                            png(&sheet(&v, 1)),
                        );
                    }
                }
            }
        }
    }
}
