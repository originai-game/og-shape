//! How far a tool moved a mesh's UVs: for every corner of the output,
//! the UV the input surface has at the nearest point to it (interpolated
//! in the input triangle there) against the corner's own. Where several
//! input triangles are nearest (an edge, a seam), the closest UV of
//! theirs counts, so a corner on a seam matches its own side.

use std::collections::BTreeMap;

use shape_core::{DVec2, DVec3};
use shape_mesh::Solid;

/// The UV error of `output` against `input`, in UV units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UvError {
    /// Largest over the output's corners.
    pub max: f64,
    /// Mean over the output's corners.
    pub mean: f64,
    /// The 99th percentile over the output's corners.
    pub p99: f64,
    /// The largest distance, mm, from an output corner to the input
    /// surface (the geometric error the UV error rides on).
    pub max_distance: f64,
}

/// The closest point of triangle `abc` to `p` and its barycentric
/// coordinates (Ericson, Real-Time Collision Detection, 5.1.5).
fn closest(p: DVec3, a: DVec3, b: DVec3, c: DVec3) -> (DVec3, [f64; 3]) {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return (a, [1.0, 0.0, 0.0]);
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return (b, [0.0, 1.0, 0.0]);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return (a + ab * v, [1.0 - v, v, 0.0]);
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return (c, [0.0, 0.0, 1.0]);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return (a + ac * w, [1.0 - w, 0.0, w]);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (b + (c - b) * w, [0.0, 1.0 - w, w]);
    }
    let denom = va + vb + vc;
    if denom == 0.0 {
        return (a, [1.0, 0.0, 0.0]);
    }
    let (v, w) = (vb / denom, vc / denom);
    (a + ab * v + ac * w, [1.0 - v - w, v, w])
}

/// [`UvError`] of `output` against `input`; both must have UVs.
pub fn uv_error(input: &Solid, output: &Solid) -> UvError {
    let (iu, ou) = (
        input.uvs.as_ref().expect("input UVs"),
        output.uvs.as_ref().expect("output UVs"),
    );
    // A grid over the input's triangles, by their boxes.
    let (lo, hi) = input.positions.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(l, h), p| (l.min(*p), h.max(*p)),
    );
    let cells = (input.triangles.len() as f64).sqrt().ceil().max(1.0);
    let size = ((hi - lo).max_element() / cells).max(1e-9);
    let key = |p: DVec3| {
        let c = ((p - lo) / size).floor();
        (c.x as i64, c.y as i64, c.z as i64)
    };
    let mut grid: BTreeMap<(i64, i64, i64), Vec<u32>> = BTreeMap::new();
    for (i, t) in input.triangles.iter().enumerate() {
        let ps = t.map(|v| input.positions[v as usize]);
        let (a, b) = (
            key(ps[0].min(ps[1]).min(ps[2])),
            key(ps[0].max(ps[1]).max(ps[2])),
        );
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                for z in a.2..=b.2 {
                    grid.entry((x, y, z)).or_default().push(i as u32);
                }
            }
        }
    }
    let reach = ((hi - lo) / size).max_element() as i64 + 2;
    let mut out = UvError::default();
    let mut all: Vec<f64> = Vec::new();
    for (i, t) in output.triangles.iter().enumerate() {
        for k in 0..3 {
            let p = output.positions[t[k] as usize];
            let uv = ou[i][k];
            let c = key(p);
            let mut found: Vec<(f64, DVec2)> = Vec::new();
            let mut best = f64::INFINITY;
            let mut seen: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
            for r in 0..=reach {
                // Every input point nearer than `best` lies within r rings.
                if best.is_finite() && (r - 1) as f64 * size > best {
                    break;
                }
                for x in c.0 - r..=c.0 + r {
                    for y in c.1 - r..=c.1 + r {
                        for z in c.2 - r..=c.2 + r {
                            let shell = (x - c.0).abs().max((y - c.1).abs()).max((z - c.2).abs());
                            if shell != r {
                                continue;
                            }
                            for &j in grid.get(&(x, y, z)).into_iter().flatten() {
                                if !seen.insert(j) {
                                    continue;
                                }
                                let tri = input.triangles[j as usize];
                                let [a, b, cc] = tri.map(|v| input.positions[v as usize]);
                                let (q, w) = closest(p, a, b, cc);
                                let d = (q - p).length();
                                let u = iu[j as usize];
                                let at = u[0] * w[0] + u[1] * w[1] + u[2] * w[2];
                                found.push((d, at));
                                best = best.min(d);
                            }
                        }
                    }
                }
            }
            let tol = 1e-6 * (1.0 + best);
            let e = found
                .iter()
                .filter(|f| f.0 <= best + tol)
                .map(|f| (f.1 - uv).length())
                .fold(f64::INFINITY, f64::min);
            out.max = out.max.max(e);
            out.mean += e;
            out.max_distance = out.max_distance.max(best);
            all.push(e);
        }
    }
    if !all.is_empty() {
        out.mean /= all.len() as f64;
        all.sort_by(f64::total_cmp);
        out.p99 = all[(all.len() * 99 / 100).min(all.len() - 1)];
    }
    out
}
