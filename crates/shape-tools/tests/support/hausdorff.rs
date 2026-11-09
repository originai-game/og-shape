//! Test support shared with `bench/meshtools`: distances between meshes.

use shape_core::DVec3;
use shape_mesh::Solid;

/// The closest point to `p` on triangle `a b c` (Ericson, *Real-Time
/// Collision Detection*, 5.1.5).
pub fn closest_on_triangle(p: DVec3, a: DVec3, b: DVec3, c: DVec3) -> DVec3 {
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
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// The largest distance from a vertex of `a` to the surface of `b`.
pub fn one_sided(a: &Solid, b: &Solid) -> f64 {
    let tris: Vec<([DVec3; 3], DVec3, DVec3)> = b
        .triangles
        .iter()
        .map(|t| {
            let p = t.map(|v| b.positions[v as usize]);
            (p, p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]))
        })
        .collect();
    let mut worst: f64 = 0.0;
    for &p in &a.positions {
        let mut best = f64::INFINITY;
        for (t, lo, hi) in &tris {
            // A box farther than the best so far cannot hold a nearer point.
            let gap = (*lo - p).max(p - *hi).max(DVec3::ZERO);
            if gap.length_squared() >= best * best {
                continue;
            }
            let d = (closest_on_triangle(p, t[0], t[1], t[2]) - p).length();
            best = best.min(d);
        }
        if best.is_finite() {
            worst = worst.max(best);
        }
    }
    worst
}

/// The two-sided Hausdorff distance between the meshes' surfaces, by their
/// vertices.
pub fn hausdorff(a: &Solid, b: &Solid) -> f64 {
    one_sided(a, b).max(one_sided(b, a))
}
