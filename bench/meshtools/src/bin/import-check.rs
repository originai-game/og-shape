//! Imports model files (glTF, GLB, OBJ, FBX) given on the command line,
//! prints what was read and its measures, and writes a four-view preview
//! next to each as `<file>.png`. For checking the importers on files not
//! written here (`docs/research.md` §9.3).
//!
//! ```text
//! cargo run --release --bin import-check -- FILE...
//! ```

use shape_tools::{Import, Measure, Part, Preview, Scene, import, measure, png, preview, sheet};
fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).unwrap();
        let dir = std::path::Path::new(&path).parent().unwrap().to_path_buf();
        let resolve = move |uri: &str| std::fs::read(dir.join(uri)).map_err(|e| e.to_string());
        let t = std::time::Instant::now();
        match import(&bytes, &Import::default(), &resolve) {
            Err(e) => println!("{path}: {e}"),
            Ok(im) => {
                for p in im.parts.iter().filter(|p| p.solid.skin.is_some()) {
                    let k = p.solid.skin.as_ref().unwrap();
                    println!("  part {:?} is skinned: skin {:?}, {} bones, {} weighted vertices", p.name, k.name, k.bones.len(), k.weighted_vertices);
                }
                // A skinned part is refused by join: place the parts for
                // looking at only, skin dropped.
                let mut s = shape_mesh::Solid::default();
                for p in &im.parts {
                    let mut q = p.clone();
                    q.solid.skin = None;
                    s.append(&shape_tools::apply_transform(&q).unwrap());
                }
                s.canonicalize().unwrap();
                let m = measure(&s, &Measure { uvs: None, images: &im.images, ..Default::default() });
                println!("{path}: {} parts, {} polys triangulated, skipped {:?}, {:.0} ms", im.parts.len(), im.polygons_triangulated, im.skipped, t.elapsed().as_secs_f64()*1000.0);
                println!("  names {:?}", im.parts.iter().map(|p| p.name.as_str()).take(8).collect::<Vec<_>>());
                println!("  uvs on parts: {}/{}", im.parts.iter().filter(|p| p.uvs.is_some()).count(), im.parts.len());
                let j = m.to_json();
                println!("  {}", &j[..j.len().min(700)]);
                let v = preview(&Scene { name: "f".into(), parts: vec![Part::new("m", &s)] }, &Preview { width: 256, views: 4, ..Default::default() }).unwrap();
                std::fs::write(format!("{path}.png"), png(&sheet(&v, 4))).unwrap();
            }
        }
    }
}
