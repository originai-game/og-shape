//! The output hash the gate compares across targets (`docs/plan.md`
//! §Gate).
//!
//! It is SHA-256 over a byte stream with a fixed encoding: little-endian
//! integers and the IEEE-754 bits of each f64. A NaN is refused, because
//! its bits are not part of the determinism contract.

use core::fmt;

use glam::DVec3;
use sha2::{Digest, Sha256};

/// Why a value could not be hashed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashError {
    /// A NaN reached the output.
    NaN,
}

impl fmt::Display for HashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HashError::NaN => f.write_str("NaN in hashed output"),
        }
    }
}

impl std::error::Error for HashError {}

/// An incremental output hash.
#[derive(Clone, Default)]
pub struct OutputHash {
    sha: Sha256,
}

impl OutputHash {
    /// An empty hash.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds raw bytes, prefixed by their length.
    pub fn bytes(&mut self, bytes: &[u8]) {
        self.u64(bytes.len() as u64);
        self.sha.update(bytes);
    }

    /// Adds a label, e.g. the name of a section of the output.
    pub fn str(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }

    /// Adds an integer.
    pub fn u64(&mut self, v: u64) {
        self.sha.update(v.to_le_bytes());
    }

    /// Adds an f64's bits. `-0.0` and `0.0` hash differently: the gate
    /// compares bytes.
    pub fn f64(&mut self, v: f64) -> Result<(), HashError> {
        if v.is_nan() {
            return Err(HashError::NaN);
        }
        self.sha.update(v.to_bits().to_le_bytes());
        Ok(())
    }

    /// Adds a sequence of f64 values, prefixed by their count.
    pub fn f64s(&mut self, values: &[f64]) -> Result<(), HashError> {
        self.u64(values.len() as u64);
        values.iter().try_for_each(|&v| self.f64(v))
    }

    /// Adds a sequence of points, prefixed by their count.
    pub fn dvec3s(&mut self, points: &[DVec3]) -> Result<(), HashError> {
        self.u64(points.len() as u64);
        points.iter().try_for_each(|p| {
            self.f64(p.x)?;
            self.f64(p.y)?;
            self.f64(p.z)
        })
    }

    /// The digest as 64 lowercase hex digits.
    pub fn finish_hex(self) -> String {
        self.sha
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_is_refused() {
        let mut h = OutputHash::new();
        assert_eq!(h.f64(f64::NAN), Err(HashError::NaN));
        assert_eq!(h.f64s(&[1.0, f64::NAN]), Err(HashError::NaN));
    }

    #[test]
    fn length_prefix_separates_fields() {
        let mut a = OutputHash::new();
        a.str("ab");
        a.str("c");
        let mut b = OutputHash::new();
        b.str("a");
        b.str("bc");
        assert_ne!(a.finish_hex(), b.finish_hex());
    }
}
