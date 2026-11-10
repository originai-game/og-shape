//! How far an output's vertex colours are from the input's
//! (`docs/research.md` §29.6): for each output corner, the input
//! surface's colour at the nearest point (interpolated in that input
//! triangle), against the corner's own; the largest channel difference,
//! in 0..=1 linear. And how far the corner is from the input, mm.
//!
//! ```text
//! cargo run --release --bin colors -- INPUT OUTPUT.glb [OUTPUT.glb …]
//! ```

use std::collections::BTreeMap;

use shape_core::DVec3;
use shape_mesh::Solid;
use shape_tools::{Import, import, join};

fn load(path: &str) -> Solid {
    let bytes = std::fs::read(path).expect("read");
    let dir = std::path::Path::new(path).parent().unwrap().to_path_buf();
    let files = move |u: &str| std::fs::read(dir.join(u)).map_err(|e| e.to_string());
    let im = import(&bytes, &Import::default(), &files).expect("import");
    join(&im.parts).expect("join")
}

fn closest(p: DVec3, a: DVec3, b: DVec3, c: DVec3) -> DVec3 {
    // Ericson 5.1.5.
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let d = 1.0 / (va + vb + vc);
    a + ab * (vb * d) + ac * (vc * d)
}

fn pct(xs: &mut [f64], q: f64) -> f64 {
    xs.sort_by(f64::total_cmp);
    xs.get(((xs.len().max(1) - 1) as f64 * q).round() as usize).copied().unwrap_or(f64::NAN)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let input = load(&args[0]);
    assert!(input.colors.is_some(), "the input has no colours");
    let (lo, hi) = input.positions.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(l, h), p| (l.min(*p), h.max(*p)),
    );
    let cell = (hi - lo).max_element() / 96.0;
    let key = |p: DVec3| {
        let q = ((p - lo) / cell).floor();
        (q.x as i64, q.y as i64, q.z as i64)
    };
    let mut grid: BTreeMap<(i64, i64, i64), Vec<usize>> = BTreeMap::new();
    for (t, tri) in input.triangles.iter().enumerate() {
        let ps = tri.map(|v| input.positions[v as usize]);
        let (a, b) = (key(ps[0].min(ps[1]).min(ps[2])), key(ps[0].max(ps[1]).max(ps[2])));
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                for z in a.2..=b.2 {
                    grid.entry((x, y, z)).or_default().push(t);
                }
            }
        }
    }
    println!("output\tcorners\tcolour median\tp95\tp99\tmax\tdistance median mm");
    for path in &args[1..] {
        let out = load(path);
        let oc = out.colors.as_ref().expect("the output has no colours");
        let (mut err, mut dist) = (Vec::new(), Vec::new());
        // One sample per (vertex, colour): each corner, deduplicated.
        let mut seen = BTreeMap::new();
        for (i, t) in out.triangles.iter().enumerate() {
            for k in 0..3 {
                let c = oc[i][k];
                if seen.insert((t[k], c.map(f64::to_bits)), ()).is_some() {
                    continue;
                }
                let p = out.positions[t[k] as usize];
                let ck = key(p);
                let mut best: Option<(f64, usize, DVec3)> = None;
                for r in 0i64..96 {
                    for x in ck.0 - r..=ck.0 + r {
                        for y in ck.1 - r..=ck.1 + r {
                            for z in ck.2 - r..=ck.2 + r {
                                if (x - ck.0).abs().max((y - ck.1).abs()).max((z - ck.2).abs()) != r {
                                    continue;
                                }
                                for &it in grid.get(&(x, y, z)).map_or(&[][..], |v| v.as_slice()) {
                                    let [a, b, cc] = input.triangles[it].map(|v| input.positions[v as usize]);
                                    let q = closest(p, a, b, cc);
                                    let d = (q - p).length();
                                    if best.is_none_or(|b| d < b.0) {
                                        best = Some((d, it, q));
                                    }
                                }
                            }
                        }
                    }
                    if best.is_some_and(|b| b.0 <= r as f64 * cell) {
                        break;
                    }
                }
                let Some((d, it, q)) = best else { continue };
                let Some(want) = input.color_at(it, q) else { continue };
                err.push((0..4).map(|ch| (c[ch] - want[ch]).abs()).fold(0.0, f64::max));
                dist.push(d);
            }
        }
        println!(
            "{}\t{}\t{:.4}\t{:.4}\t{:.4}\t{:.3}\t{:.3}",
            path.rsplit('/').next().unwrap_or(path),
            err.len(),
            pct(&mut err.clone(), 0.5),
            pct(&mut err.clone(), 0.95),
            pct(&mut err.clone(), 0.99),
            pct(&mut err, 1.0),
            pct(&mut dist, 0.5),
        );
    }
}
