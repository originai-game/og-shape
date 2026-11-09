//! The one stable sort, under the name og-geom's copied code calls
//! (`og_core::sort` in origin-engine).
//!
//! A stable sort's result is fully determined by the comparison, so `std`'s
//! stable sort gives the same order as origin-engine's own merge sort.

use core::cmp::Ordering;

/// Sorts `v` stably by `compare`.
pub fn sort_by<T, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], compare: F) {
    v.sort_by(compare);
}

/// Sorts `v` stably by the key `key` reads.
pub fn sort_by_key<T, K: Ord, F: FnMut(&T) -> K>(v: &mut [T], key: F) {
    v.sort_by_key(key);
}

/// Sorts `v` stably by its own order.
pub fn sort<T: Ord>(v: &mut [T]) {
    v.sort();
}
