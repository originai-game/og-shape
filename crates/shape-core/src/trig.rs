//! Transcendental functions, from `libm` (pure Rust, correctly specified,
//! the same result on every target).
//!
//! The kernel takes every function beyond `+ - * /` and `sqrt` from here.
//! `std`'s versions call the platform's maths library or LLVM intrinsics,
//! whose last bits differ between hosts.

/// The circle constant π.
pub const PI: f64 = core::f64::consts::PI;
/// 2π, one full turn in radians.
pub const TAU: f64 = core::f64::consts::TAU;

/// Sine of `x` radians.
pub fn sin(x: f64) -> f64 {
    libm::sin(x)
}

/// Cosine of `x` radians.
pub fn cos(x: f64) -> f64 {
    libm::cos(x)
}

/// Sine and cosine of `x` radians, as `(sin, cos)`.
pub fn sin_cos(x: f64) -> (f64, f64) {
    libm::sincos(x)
}

/// Tangent of `x` radians.
pub fn tan(x: f64) -> f64 {
    libm::tan(x)
}

/// Arcsine, in radians.
pub fn asin(x: f64) -> f64 {
    libm::asin(x)
}

/// Arccosine, in radians.
pub fn acos(x: f64) -> f64 {
    libm::acos(x)
}

/// Arctangent, in radians.
pub fn atan(x: f64) -> f64 {
    libm::atan(x)
}

/// Four-quadrant arctangent of `y / x`, in radians.
pub fn atan2(y: f64, x: f64) -> f64 {
    libm::atan2(y, x)
}

/// e raised to `x`.
pub fn exp(x: f64) -> f64 {
    libm::exp(x)
}

/// Natural logarithm.
pub fn ln(x: f64) -> f64 {
    libm::log(x)
}

/// `x` raised to the real power `y`.
pub fn pow(x: f64, y: f64) -> f64 {
    libm::pow(x, y)
}

/// Cube root.
pub fn cbrt(x: f64) -> f64 {
    libm::cbrt(x)
}

/// `sqrt(x * x + y * y)` without undue overflow.
pub fn hypot(x: f64, y: f64) -> f64 {
    libm::hypot(x, y)
}

/// Degrees to radians.
pub fn to_radians(degrees: f64) -> f64 {
    degrees * (PI / 180.0)
}

/// Radians to degrees.
pub fn to_degrees(radians: f64) -> f64 {
    radians * (180.0 / PI)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libm_bits_are_pinned() {
        // A libm change that moves a last bit shows up here first, with
        // the function named, before it moves a golden hash.
        assert_eq!(sin(1.0).to_bits(), 0x3FEA_ED54_8F09_0CEE);
        assert_eq!(cos(1.0).to_bits(), 0x3FE1_4A28_0FB5_068C);
        assert_eq!(atan2(1.0, 2.0).to_bits(), 0x3FDD_AC67_0561_BB4F);
        // One ulp above core::f64::consts::E: libm is not correctly
        // rounded, only the same everywhere.
        assert_eq!(exp(1.0).to_bits(), 0x4005_BF0A_8B14_576A);
        assert_eq!(pow(2.0, 0.5).to_bits(), core::f64::consts::SQRT_2.to_bits());
    }
}
