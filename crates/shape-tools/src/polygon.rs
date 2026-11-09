//! Triangulation of the polygons a file holds (OBJ faces, FBX polygons):
//! ear clipping in the polygon's own plane, decided by exact predicates.

use core::cmp::Ordering;

use shape_core::predicates::orient2d;
use shape_core::{DVec2, DVec3};

/// Triangles covering the polygon `corners` (indices into `positions`),
/// wound the polygon's way. A polygon of three corners comes back as it
/// is. Ear clipping runs in the plane of the polygon's Newell normal; a
/// polygon with no ear (self-crossing, or with every corner in a line)
/// falls back to a fan from the first corner left.
pub(crate) fn triangulate(positions: &[DVec3], corners: &[u32]) -> Vec<[u32; 3]> {
    let n = corners.len();
    if n < 3 {
        return Vec::new();
    }
    if n == 3 {
        return vec![[corners[0], corners[1], corners[2]]];
    }
    let p = |i: usize| positions[corners[i] as usize];
    let mut normal = DVec3::ZERO;
    for i in 0..n {
        let (a, b) = (p(i), p((i + 1) % n));
        normal += DVec3::new(
            (a.y - b.y) * (a.z + b.z),
            (a.z - b.z) * (a.x + b.x),
            (a.x - b.x) * (a.y + b.y),
        );
    }
    let abs = normal.abs();
    // Drop the normal's largest axis; keep the projection counter-clockwise.
    let flat: Vec<DVec2> = (0..n)
        .map(|i| {
            let q = p(i);
            if abs.x >= abs.y && abs.x >= abs.z {
                if normal.x >= 0.0 {
                    DVec2::new(q.y, q.z)
                } else {
                    DVec2::new(q.z, q.y)
                }
            } else if abs.y >= abs.z {
                if normal.y >= 0.0 {
                    DVec2::new(q.z, q.x)
                } else {
                    DVec2::new(q.x, q.z)
                }
            } else if normal.z >= 0.0 {
                DVec2::new(q.x, q.y)
            } else {
                DVec2::new(q.y, q.x)
            }
        })
        .collect();
    let mut left: Vec<usize> = (0..n).collect();
    let mut out = Vec::with_capacity(n - 2);
    while left.len() > 3 {
        let m = left.len();
        let ear = (0..m).find(|&k| {
            let (a, b, c) = (left[(k + m - 1) % m], left[k], left[(k + 1) % m]);
            let (fa, fb, fc) = (flat[a], flat[b], flat[c]);
            if orient2d(fa, fb, fc) != Ordering::Greater {
                return false;
            }
            left.iter().all(|&o| {
                let fo = flat[o];
                if o == a || o == b || o == c || fo == fa || fo == fb || fo == fc {
                    return true;
                }
                // Outside unless strictly left of, or on, every edge.
                orient2d(fa, fb, fo) == Ordering::Less
                    || orient2d(fb, fc, fo) == Ordering::Less
                    || orient2d(fc, fa, fo) == Ordering::Less
            })
        });
        let Some(k) = ear else {
            // No ear: fan the rest from its first corner.
            for k in 1..left.len() - 1 {
                out.push([corners[left[0]], corners[left[k]], corners[left[k + 1]]]);
            }
            return out;
        };
        let m = left.len();
        let (a, b, c) = (left[(k + m - 1) % m], left[k], left[(k + 1) % m]);
        out.push([corners[a], corners[b], corners[c]]);
        left.remove(k);
    }
    out.push([corners[left[0]], corners[left[1]], corners[left[2]]]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_a_concave_polygon_in_any_plane() {
        // An L in the x-z plane, wound so its normal is -y.
        let pts: Vec<DVec3> = [(0, 0), (2, 0), (2, 1), (1, 1), (1, 2), (0, 2)]
            .iter()
            .map(|&(x, z)| DVec3::new(x as f64, 0.0, z as f64))
            .collect();
        let corners: Vec<u32> = (0..6).collect();
        let tris = triangulate(&pts, &corners);
        assert_eq!(tris.len(), 4);
        let area: f64 = tris
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| pts[i as usize]);
                (b - a).cross(c - a).y / 2.0
            })
            .sum();
        // Same winding as the polygon (normal −y), total area 3.
        assert_eq!(area, -3.0);
    }
}
