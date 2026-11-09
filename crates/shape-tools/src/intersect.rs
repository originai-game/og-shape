//! Exact intersection tests between triangles, in 3D (self-intersections
//! of a surface) and in 2D (overlapping UV triangles). Every side is
//! decided by Shewchuk's predicates; no tolerance is used.
//!
//! The 3D tests are `shape_bool::selfcheck`'s, shared with the booleans'
//! operand check and self-union: one implementation.

use core::cmp::Ordering;

use shape_core::DVec2;
use shape_core::predicates::orient2d;

pub(crate) use shape_bool::selfcheck::degenerate;

/// Pairs of triangles `(i, j)`, `i < j`, that share no vertex index and
/// still meet: a surface passing through (or touching) itself. Degenerate
/// triangles are left out (they are counted on their own).
pub(crate) fn self_intersections(
    positions: &[shape_core::DVec3],
    triangles: &[[u32; 3]],
) -> Vec<(u32, u32)> {
    shape_bool::selfcheck::touching_pairs(positions, triangles)
}

/// Two 2D triangles overlap in a region of positive area (touching along
/// an edge or at a point does not count). Zero-area triangles overlap
/// nothing.
pub(crate) fn tris_overlap_2d(a: [DVec2; 3], b: [DVec2; 3]) -> bool {
    let ccw = |t: [DVec2; 3]| match orient2d(t[0], t[1], t[2]) {
        Ordering::Greater => Some(t),
        Ordering::Less => Some([t[0], t[2], t[1]]),
        Ordering::Equal => None,
    };
    let (Some(a), Some(b)) = (ccw(a), ccw(b)) else {
        return false;
    };
    // Separating axis: an edge of either with the other wholly on its
    // outer side (or on it).
    let separates = |t: [DVec2; 3], o: [DVec2; 3]| {
        (0..3).any(|k| {
            o.iter()
                .all(|&p| orient2d(t[k], t[(k + 1) % 3], p) != Ordering::Greater)
        })
    };
    !(separates(a, b) || separates(b, a))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_triangles_overlap_only_with_area() {
        let w = DVec2::new;
        let s = [w(0.0, 0.0), w(1.0, 0.0), w(0.0, 1.0)];
        // Shares an edge: no overlap.
        assert!(!tris_overlap_2d(s, [w(1.0, 0.0), w(1.0, 1.0), w(0.0, 1.0)]));
        assert!(tris_overlap_2d(s, [w(0.2, 0.2), w(1.0, 1.0), w(0.0, 1.0)]));
    }
}
