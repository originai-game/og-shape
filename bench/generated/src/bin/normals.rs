//! What the mesh tools do to a tangent-space normal map
//! (`docs/research.md` §29.4): the normal a viewer shades with, on the
//! input and on an output of the chain, at the same points.
//!
//! A glTF without `TANGENT` (none of the generated files has one) is
//! shaded with a tangent frame the viewer derives per pixel from the
//! triangle's positions and UVs (three.js `perturbNormal2Arb`: the
//! derivatives of position against u and v), around the interpolated
//! vertex normal. So at a surface point:
//!
//! - `T`, `B`: the triangle's dP/du and dP/dv, made orthogonal to `N`
//!   and unit (their handedness is the UV triangle's winding);
//! - `N`: the vertex normals interpolated;
//! - the shading normal: `normalize(T·x + B·y + N·z)` for the map's texel
//!   `(x, y, z)` at the point's UV.
//!
//! For each output triangle's centre, the nearest point of the input
//! surface is found, and the shading normals there are compared: the
//! angle between them, the angle between the two `N`s alone, and whether
//! the UV winding (handedness) differs. Nothing is judged.
//!
//! ```text
//! cargo run --release --bin normals -- INPUT.glb OUTPUT.glb [OUTPUT.glb …]
//! ```

use std::collections::BTreeMap;

use shape_core::{DVec2, DVec3};
use shape_tools::decode;

struct Mesh {
    pos: Vec<DVec3>,
    nrm: Vec<DVec3>,
    uv: Vec<DVec2>,
    tris: Vec<[usize; 3]>,
    /// The normal map, RGBA8, and its size.
    map: Option<(Vec<u8>, usize, usize)>,
}

fn read(path: &str) -> Mesh {
    let b = std::fs::read(path).expect("read");
    let len = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
    let json: serde_json::Value = serde_json::from_slice(&b[20..20 + len]).expect("json");
    let bin = &b[20 + len + 8..];
    let view = |i: usize| -> &[u8] {
        let v = &json["bufferViews"][i];
        let off = v["byteOffset"].as_u64().unwrap_or(0) as usize;
        &bin[off..off + v["byteLength"].as_u64().unwrap() as usize]
    };
    let acc = |i: usize| -> (Vec<f64>, usize) {
        let a = &json["accessors"][i];
        let w = match a["type"].as_str().unwrap() {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            _ => 4,
        };
        let ct = a["componentType"].as_u64().unwrap();
        let size = match ct {
            5121 => 1,
            5123 => 2,
            _ => 4,
        };
        let n = a["count"].as_u64().unwrap() as usize;
        let v = &json["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
        let stride = v["byteStride"].as_u64().map_or(w * size, |s| s as usize);
        let data = view(a["bufferView"].as_u64().unwrap() as usize);
        let off = a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let mut out = Vec::with_capacity(n * w);
        for e in 0..n {
            for c in 0..w {
                let at = off + e * stride + c * size;
                out.push(match ct {
                    5121 => f64::from(data[at]),
                    5123 => f64::from(u16::from_le_bytes([data[at], data[at + 1]])),
                    5125 => f64::from(u32::from_le_bytes(data[at..at + 4].try_into().unwrap())),
                    _ => f64::from(f32::from_le_bytes(data[at..at + 4].try_into().unwrap())),
                });
            }
        }
        (out, w)
    };
    let prim = &json["meshes"][0]["primitives"][0];
    let at = &prim["attributes"];
    let (p, _) = acc(at["POSITION"].as_u64().unwrap() as usize);
    let (n, _) = acc(at["NORMAL"].as_u64().expect("NORMAL") as usize);
    let (t, _) = acc(at["TEXCOORD_0"].as_u64().expect("TEXCOORD_0") as usize);
    let (i, _) = acc(prim["indices"].as_u64().unwrap() as usize);
    let v3 = |x: &[f64]| x.chunks(3).map(|c| DVec3::new(c[0], c[1], c[2])).collect::<Vec<_>>();
    let m = &json["materials"][prim["material"].as_u64().unwrap_or(0) as usize];
    let map = m["normalTexture"]["index"].as_u64().map(|ti| {
        let src = json["textures"][ti as usize]["source"].as_u64().unwrap() as usize;
        let img = decode(view(json["images"][src]["bufferView"].as_u64().unwrap() as usize))
            .expect("decode");
        (img.rgba, img.width as usize, img.height as usize)
    });
    Mesh {
        pos: v3(&p),
        nrm: v3(&n),
        uv: t.chunks(2).map(|c| DVec2::new(c[0], c[1])).collect(),
        tris: i
            .chunks(3)
            .map(|c| [c[0] as usize, c[1] as usize, c[2] as usize])
            .collect(),
        map,
    }
}

/// The closest point of triangle `abc` to `p`, as barycentric weights.
fn closest(p: DVec3, a: DVec3, b: DVec3, c: DVec3) -> [f64; 3] {
    // Ericson, Real-Time Collision Detection, 5.1.5.
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return [1.0, 0.0, 0.0];
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return [0.0, 1.0, 0.0];
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return [1.0 - v, v, 0.0];
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return [0.0, 0.0, 1.0];
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return [1.0 - w, 0.0, w];
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return [0.0, 1.0 - w, w];
    }
    let denom = 1.0 / (va + vb + vc);
    let (v, w) = (vb * denom, vc * denom);
    [1.0 - v - w, v, w]
}

/// The shading normal at barycentric `w` of triangle `t`, the vertex
/// normal there, and the UV winding's sign.
fn shade(m: &Mesh, t: usize, w: [f64; 3], map: &(Vec<u8>, usize, usize)) -> Option<(DVec3, DVec3, f64)> {
    let [a, b, c] = m.tris[t];
    let (p0, p1, p2) = (m.pos[a], m.pos[b], m.pos[c]);
    let (t0, t1, t2) = (m.uv[a], m.uv[b], m.uv[c]);
    let n = (m.nrm[a] * w[0] + m.nrm[b] * w[1] + m.nrm[c] * w[2]).normalize_or_zero();
    let uv = t0 * w[0] + t1 * w[1] + t2 * w[2];
    let (e1, e2) = (p1 - p0, p2 - p0);
    let (s1, s2) = (t1 - t0, t2 - t0);
    let det = s1.x * s2.y - s2.x * s1.y;
    if det == 0.0 || n == DVec3::ZERO {
        return None;
    }
    let tan = (e1 * s2.y - e2 * s1.y) / det;
    let bit = (e2 * s1.x - e1 * s2.x) / det;
    let tan = (tan - n * n.dot(tan)).normalize_or_zero();
    let bit = (bit - n * n.dot(bit)).normalize_or_zero();
    let (rgba, w_, h_) = map;
    // glTF UVs: (0, 0) at the top left; repeat.
    let x = ((uv.x.rem_euclid(1.0) * *w_ as f64) as usize).min(w_ - 1);
    let y = ((uv.y.rem_euclid(1.0) * *h_ as f64) as usize).min(h_ - 1);
    let o = (y * w_ + x) * 4;
    let ts = |k: usize| f64::from(rgba[o + k]) / 255.0 * 2.0 - 1.0;
    // glTF's normal map: +y up the texture (v down), so B is −dP/dv here.
    let s = (tan * ts(0) - bit * ts(1) + n * ts(2)).normalize_or_zero();
    Some((s, n, det.signum()))
}

fn angle(a: DVec3, b: DVec3) -> f64 {
    a.dot(b).clamp(-1.0, 1.0).acos().to_degrees()
}

fn pct(xs: &mut [f64], q: f64) -> f64 {
    xs.sort_by(f64::total_cmp);
    if xs.is_empty() {
        return f64::NAN;
    }
    xs[((xs.len() - 1) as f64 * q).round() as usize]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let input = read(&args[0]);
    let map = input.map.clone().expect("the input has no normal map");
    // A grid over the input triangles' boxes.
    let (lo, hi) = input.pos.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(l, h), p| (l.min(*p), h.max(*p)),
    );
    let cell = (hi - lo).max_element() / 96.0;
    let key = |p: DVec3| {
        let q = ((p - lo) / cell).floor();
        (q.x as i64, q.y as i64, q.z as i64)
    };
    let mut grid: BTreeMap<(i64, i64, i64), Vec<usize>> = BTreeMap::new();
    for (t, tri) in input.tris.iter().enumerate() {
        let ps = tri.map(|v| input.pos[v]);
        let (a, b) = (key(ps[0].min(ps[1]).min(ps[2])), key(ps[0].max(ps[1]).max(ps[2])));
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                for z in a.2..=b.2 {
                    grid.entry((x, y, z)).or_default().push(t);
                }
            }
        }
    }
    println!("output\tsamples\tshading median°\tp95°\tmax°\tvertex normal median°\tp95°\thandedness flipped %\tmoved median mm");
    for path in &args[1..] {
        let out = read(path);
        let (mut shading, mut normal, mut moved) = (Vec::new(), Vec::new(), Vec::new());
        let (mut flipped, mut samples) = (0usize, 0usize);
        for t in 0..out.tris.len() {
            let w = [1.0 / 3.0; 3];
            let p = out.tris[t].iter().map(|&v| out.pos[v]).sum::<DVec3>() / 3.0;
            let Some((so, no, ho)) = shade(&out, t, w, &map) else {
                continue;
            };
            // The nearest input triangle: rings of cells until one holds
            // a point nearer than the ring's inner radius.
            let c = key(p);
            let mut best: Option<(f64, usize, [f64; 3])> = None;
            for r in 0i64..96 {
                for x in c.0 - r..=c.0 + r {
                    for y in c.1 - r..=c.1 + r {
                        for z in c.2 - r..=c.2 + r {
                            if (x - c.0).abs().max((y - c.1).abs()).max((z - c.2).abs()) != r {
                                continue;
                            }
                            for &it in grid.get(&(x, y, z)).map_or(&[][..], |v| v.as_slice()) {
                                let [a, b, cc] = input.tris[it].map(|v| input.pos[v]);
                                let bw = closest(p, a, b, cc);
                                let q = a * bw[0] + b * bw[1] + cc * bw[2];
                                let d = (q - p).length();
                                if best.is_none_or(|b| d < b.0) {
                                    best = Some((d, it, bw));
                                }
                            }
                        }
                    }
                }
                if best.is_some_and(|b| b.0 <= r as f64 * cell) {
                    break;
                }
            }
            let Some((d, it, bw)) = best else { continue };
            let Some((si, ni, hi_)) = shade(&input, it, bw, &map) else {
                continue;
            };
            samples += 1;
            shading.push(angle(so, si));
            normal.push(angle(no, ni));
            moved.push(d * 1000.0);
            if ho != hi_ {
                flipped += 1;
            }
        }
        println!(
            "{}\t{samples}\t{:.2}\t{:.2}\t{:.1}\t{:.2}\t{:.2}\t{:.2}\t{:.3}",
            path.rsplit('/').next().unwrap_or(path),
            pct(&mut shading.clone(), 0.5),
            pct(&mut shading.clone(), 0.95),
            pct(&mut shading, 1.0),
            pct(&mut normal.clone(), 0.5),
            pct(&mut normal, 0.95),
            100.0 * flipped as f64 / samples.max(1) as f64,
            pct(&mut moved, 0.5),
        );
    }
}
