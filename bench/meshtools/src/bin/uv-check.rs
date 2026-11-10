//! Textured files through the chain with their UVs kept
//! (`docs/research.md` §17): import → join → repair → decimate to half →
//! plain GLB with the file's base-colour texture, read back by the
//! independent `gltf` crate and by our importer. Prints the UV error of
//! each step against the imported UVs (UV units, and texels of the
//! texture), flipped UV triangles, UV coverage and overlap before and
//! after, and writes `<file>.half.glb` and a preview `<file>.half.png`.
//!
//! ```text
//! cargo run --release --bin uv-check -- FILE...
//! ```

#[path = "../../../../crates/shape-tools/tests/support/uverror.rs"]
mod uverror;

use std::time::Instant;

use shape_core::predicates::orient2d;
use shape_tools::{
    Decimate, Floaters, Import, Measure, Part, PlainGlb, Preview, Repair, Scene, decimate,
    glb_plain, import, join, measure, png, preview, repair, sheet,
};
use uverror::uv_error;

fn flipped(s: &shape_mesh::Solid) -> (usize, usize) {
    let u = s.uvs.as_ref().unwrap();
    let mut f = (0, 0);
    for (i, t) in s.triangles.iter().enumerate() {
        let [a, b, c] = t.map(|v| s.positions[v as usize]);
        if (b - a).cross(c - a).length() == 0.0 {
            continue;
        }
        match orient2d(u[i][0], u[i][1], u[i][2]) {
            core::cmp::Ordering::Less => f.0 += 1,
            core::cmp::Ordering::Equal => f.1 += 1,
            _ => {}
        }
    }
    f
}

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).unwrap();
        let dir = std::path::Path::new(&path).parent().unwrap().to_path_buf();
        let resolve = move |uri: &str| std::fs::read(dir.join(uri)).map_err(|e| e.to_string());
        let im = import(&bytes, &Import::default(), &resolve).unwrap();
        let map = im.parts.iter().find_map(|p| p.map.clone());
        let texels = im
            .images
            .iter()
            .find(|i| map.as_ref().is_some_and(|m| m.name == i.name))
            .and_then(|i| i.width)
            .unwrap_or(0);
        println!(
            "{path}: {} parts, uvs on {}, map {:?} ({} texels across), skipped {:?}",
            im.parts.len(),
            im.parts.iter().filter(|p| p.solid.uvs.is_some()).count(),
            map.as_ref().map(|m| (&m.name, &m.mime, m.bytes.len())),
            texels,
            im.skipped
        );
        let t = Instant::now();
        let joined = join(&im.parts).unwrap();
        let (fixed, r) = repair(
            &joined,
            &Repair {
                weld: 0.001,
                floaters: Floaters {
                    fraction: 0.05,
                    ..Default::default()
                },
                max_hole_edges: 64,
                ..Default::default()
            },
        )
        .unwrap();
        let repair_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let (half, dr) = decimate(
            &fixed,
            &Decimate {
                ratio: 0.5,
                ..Default::default()
            },
        )
        .unwrap();
        let decimate_ms = t.elapsed().as_secs_f64() * 1e3;
        let out = glb_plain(
            &half,
            &PlainGlb {
                name: "half",
                map: map.as_ref(),
            },
        )
        .unwrap();
        std::fs::write(format!("{path}.half.glb"), &out).unwrap();
        // Read back: the gltf crate, then ours.
        let g = gltf::Gltf::from_slice(&out).unwrap();
        let prim = g.meshes().next().unwrap().primitives().next().unwrap();
        let has_uv = prim.get(&gltf::Semantic::TexCoords(0)).is_some();
        let has_map = prim
            .material()
            .pbr_metallic_roughness()
            .base_color_texture()
            .is_some();
        let back = import(&out, &Import::default(), &shape_tools::no_files).unwrap();
        let same_map = back.parts[0].map.as_ref().map(|m| &m.bytes) == map.as_ref().map(|m| &m.bytes);
        let texel = |e: f64| e * f64::from(texels.max(1));
        let (er, ed) = (uv_error(&joined, &fixed), uv_error(&fixed, &half));
        // The plain GLB keeps the triangles in order: corner by corner.
        let (hu, bu) = (half.uvs.as_ref().unwrap(), back.parts[0].solid.uvs.as_ref().unwrap());
        assert_eq!(hu.len(), bu.len());
        let round_trip = hu
            .iter()
            .zip(bu)
            .flat_map(|(a, b)| (0..3).map(move |k| (a[k] - b[k]).length()))
            .fold(0.0, f64::max);
        let uvm = |s: &shape_mesh::Solid| {
            measure(s, &Measure { uv_resolution: 1024, ..Default::default() }).uv.unwrap()
        };
        let (mj, mf, mh) = (uvm(&joined), uvm(&fixed), uvm(&half));
        let (lo, hi) = hu.iter().flatten().fold(
            (shape_core::DVec2::splat(f64::INFINITY), shape_core::DVec2::splat(f64::NEG_INFINITY)),
            |(l, h), q| (l.min(*q), h.max(*q)),
        );
        println!("  uv bounds {lo:.3} .. {hi:.3}");
        println!(
            "  triangles {} → repair {} ({:.0} ms; welded {}, holes {} filled / {} left, {} across a UV seam) → half {} ({:.0} ms, error {:.3} mm)",
            joined.triangles.len(),
            fixed.triangles.len(),
            repair_ms,
            r.welded,
            r.holes_filled,
            r.holes_left,
            r.uv_seam_fills,
            half.triangles.len(),
            decimate_ms,
            dr.error
        );
        for (step, e) in [("repair", er), ("decimate", ed)] {
            println!(
                "  uv error {step:>8}: max {:.2e} ({:.2} texels), p99 {:.2e} ({:.3} texels), mean {:.2e} ({:.3} texels); surface {:.3} mm",
                e.max,
                texel(e.max),
                e.p99,
                texel(e.p99),
                e.mean,
                texel(e.mean),
                e.max_distance
            );
        }
        println!("  uv error, glb round trip corner by corner: {round_trip:.2e} ({:.4} texels)", texel(round_trip));
        println!(
            "  flipped / zero-area uv triangles: joined {:?}, repaired {:?}, half {:?}",
            flipped(&joined),
            flipped(&fixed),
            flipped(&half)
        );
        for (step, m) in [("joined", mj), ("repaired", mf), ("half", mh)] {
            println!(
                "  {step:>8}: coverage {:.4}, overlap {:.4}, overlapping pairs {}, outside {}",
                m.coverage, m.overlap, m.overlapping_pairs, m.outside
            );
        }
        println!("  glb: TEXCOORD_0 {has_uv}, baseColorTexture {has_map}, same image bytes back {same_map}, {} bytes", out.len());
        let v = preview(
            &Scene {
                name: "half".into(),
                parts: vec![Part::new("half", &half)],
            },
            &Preview {
                width: 256,
                views: 4,
                ..Default::default()
            },
        )
        .unwrap();
        std::fs::write(format!("{path}.half.png"), png(&sheet(&v, 4))).unwrap();
    }
}
