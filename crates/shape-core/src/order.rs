//! Total orderings for canonical output (`docs/design.md` §6).
//!
//! Canonical ordering sorts vertices by position, then edges and faces by
//! vertex index. These comparisons are total, treat `-0.0` and `0.0` as
//! equal, and are meant for finite values: a NaN never reaches canonical
//! ordering, because every operation rejects it as an error value first.

use core::cmp::Ordering;

use glam::{DVec2, DVec3};

/// Compares two finite f64 values; `-0.0` equals `0.0`.
pub fn cmp_f64(a: f64, b: f64) -> Ordering {
    // Adding +0.0 turns -0.0 into +0.0 and leaves every other value alone.
    (a + 0.0).total_cmp(&(b + 0.0))
}

/// Compares two points lexicographically by x, then y.
pub fn cmp_dvec2(a: DVec2, b: DVec2) -> Ordering {
    cmp_f64(a.x, b.x).then_with(|| cmp_f64(a.y, b.y))
}

/// Compares two points lexicographically by x, then y, then z.
pub fn cmp_dvec3(a: DVec3, b: DVec3) -> Ordering {
    cmp_f64(a.x, b.x)
        .then_with(|| cmp_f64(a.y, b.y))
        .then_with(|| cmp_f64(a.z, b.z))
}

/// The permutation that sorts `points` by [`cmp_dvec3`], ties kept in
/// input order. `result[i]` is the old index of the point that goes to
/// position `i`.
pub fn sorted_order(points: &[DVec3]) -> Vec<u32> {
    let mut order: Vec<u32> = (0..points.len() as u32).collect();
    // `sort_by` is stable, so equal points keep their input order.
    order.sort_by(|&i, &j| cmp_dvec3(points[i as usize], points[j as usize]));
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_zeros_compare_equal() {
        assert_eq!(cmp_f64(-0.0, 0.0), Ordering::Equal);
        assert_eq!(cmp_f64(-1.0, 0.0), Ordering::Less);
    }

    #[test]
    fn sorted_order_is_stable() {
        let p = [
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(0.0, 2.0, 0.0),
            DVec3::new(-0.0, 2.0, 0.0),
            DVec3::new(0.0, 1.0, 0.0),
        ];
        assert_eq!(sorted_order(&p), vec![3, 1, 2, 0]);
    }
}
