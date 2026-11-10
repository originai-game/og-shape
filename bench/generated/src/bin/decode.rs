//! Decodes image files with `shape_tools::decode` and writes each as raw
//! RGB beside it (`<file>.rgb`), with the time taken, for comparing the
//! kernel's PNG and JPEG decoder with a reference one
//! (`docs/research.md` §29.5).
//!
//! ```text
//! cargo run --release --bin decode -- IMAGE…
//! ```

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).expect("read");
        let t0 = std::time::Instant::now();
        match shape_tools::decode(&bytes) {
            Ok(img) => {
                let ms = t0.elapsed().as_secs_f64() * 1000.0;
                let raw: Vec<u8> = img.rgba.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
                std::fs::write(format!("{path}.rgb"), raw).expect("write");
                println!("{path}\t{}\t{}\t{ms:.1}", img.width, img.height);
            }
            Err(e) => println!("{path}\terror\t{e}"),
        }
    }
}
