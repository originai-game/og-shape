//! Before and after `shape place` (`docs/research.md` §31), for a person
//! to look at: per file, one PNG of two rows of three views, the file as
//! read (top) and as placed (bottom), beside a post one decade of metres
//! tall (0.1, 1 or 10 m, the tallest not above the object) for its size.
//!
//! ```text
//! cargo run --release --bin place_previews -- DIR OUT PNGDIR NAME...
//! ```
//!
//! DIR as `fetch.ts`'s, OUT as `place.ts`'s (`OUT/place/NAME.glb`).
//! OBJ files are read as `place.ts` read them before placing: plainly,
//! Y up.

use shape_core::DVec3;
use shape_gen::solids::{BoxDesc, cuboid};
use shape_mesh::Solid;
use shape_tools::{Import, Part, Preview, Scene, apply_transform, import, png, preview, sheet};

fn read(path: &std::path::Path) -> Vec<Solid> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let dir = path.parent().unwrap().to_path_buf();
    let resolve = move |u: &str| std::fs::read(dir.join(u)).map_err(|e| e.to_string());
    let m = import(&bytes, &Import::default(), &resolve)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    m.parts
        .iter()
        .map(|p| {
            if p.solid.skin.is_some() {
                p.solid.clone()
            } else {
                apply_transform(p).unwrap()
            }
        })
        .collect()
}

fn mesh_in(dir: &std::path::Path) -> std::path::PathBuf {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("glb" | "gltf" | "obj" | "fbx")
            )
        })
        .collect();
    files.sort();
    files.remove(0)
}

fn views(parts: &[Solid]) -> Vec<shape_tools::Image> {
    let names: Vec<String> = (0..parts.len()).map(|k| format!("m{k}")).collect();
    let scene = Scene {
        name: "m".into(),
        parts: parts.iter().zip(&names).map(|(s, n)| Part::new(n, s)).collect(),
    };
    preview(
        &scene,
        &Preview {
            width: 200,
            height: 200,
            views: 3,
            ..Default::default()
        },
    )
    .unwrap()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir, out, png_dir, names @ ..] = &args[..] else {
        eprintln!("usage: place_previews DIR OUT PNGDIR NAME...");
        std::process::exit(2);
    };
    for name in names {
        let before = read(&mesh_in(&std::path::Path::new(dir).join(name)));
        let mut after = read(&std::path::Path::new(out).join("place").join(format!("{name}.glb")));
        let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
        for p in after.iter().flat_map(|s| &s.positions) {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        let height = hi.z - lo.z;
        let post = [10_000.0, 1000.0, 100.0]
            .into_iter()
            .find(|&h| h <= height)
            .unwrap_or(100.0);
        let mut bar = cuboid(&BoxDesc {
            width: post / 20.0,
            depth: post / 20.0,
            height: post,
        })
        .unwrap();
        let x = hi.x + post / 10.0 + post / 40.0;
        bar.map_positions(|p| p + DVec3::new(x, 0.0, 0.0), false);
        after.push(bar);
        let mut images = views(&before);
        images.extend(views(&after));
        let file = std::path::Path::new(png_dir).join(format!("place-{name}.png"));
        std::fs::write(&file, png(&sheet(&images, 3))).unwrap();
        println!("{} (post {} m)", file.display(), post / 1000.0);
    }
}
