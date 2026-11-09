//! sin, cos and atan2 from basic arithmetic only (the camera systems'
//! series, as og-anim's rig2d has them in f64): IEEE `+ - * /` and `sqrt`
//! round the same on every host, libm's transcendental functions do not,
//! so every generator reads its angles here.

pub const PI: f64 = std::f64::consts::PI;
pub const TAU: f64 = std::f64::consts::TAU;

/// sin and cos of `x`: reduced to `[-pi/4, pi/4]` and a quadrant, then
/// Taylor series to x^15 / x^14 (below 1e-15 there). The quadrant is
/// rounded as JavaScript's `Math.round` rounds (a tie up, towards +∞), so
/// the TypeScript twin (`sincos` of the camera systems, which og.Shape's
/// arcs read) reduces every angle the same way, -π/4 included.
pub fn sincos(x: f64) -> (f64, f64) {
    let k = x / (PI / 2.0);
    let floor = k.floor();
    let q = if k - floor >= 0.5 { floor + 1.0 } else { floor };
    let r = x - q * (PI / 2.0);
    let r2 = r * r;
    let mut s = 1.0 / 1_307_674_368_000.0;
    for d in [6_227_020_800.0, 39_916_800.0, 362_880.0, 5040.0, 120.0, 6.0] {
        s = s * -r2 + 1.0 / d;
    }
    let s = r - r * r2 * s;
    let mut c = 1.0 / 87_178_291_200.0;
    for d in [479_001_600.0, 3_628_800.0, 40_320.0, 720.0, 24.0, 2.0] {
        c = c * -r2 + 1.0 / d;
    }
    let c = 1.0 - r2 * c;
    match (q as i64).rem_euclid(4) {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

fn atan_small(x: f64) -> f64 {
    let x2 = x * x;
    let mut s = 0.0;
    for n in (0..=22).rev() {
        s = s * -x2 + 1.0 / (2 * n + 1) as f64;
    }
    x * s
}

fn atan_unit(x: f64) -> f64 {
    if x > 0.414_213_562_373_095_03 {
        PI / 4.0 + atan_small((x - 1.0) / (x + 1.0))
    } else {
        atan_small(x)
    }
}

/// atan2(y, x), with JavaScript's (IEEE's) answers for signed zeros, so a
/// three.js formula that reads `Math.atan2` reads the same branch here.
pub fn atan2(y: f64, x: f64) -> f64 {
    let negative_y = y < 0.0 || (y == 0.0 && y.is_sign_negative());
    let negative_x = x < 0.0 || (x == 0.0 && x.is_sign_negative());
    if y == 0.0 {
        let r = if negative_x { PI } else { 0.0 };
        return if negative_y { -r } else { r };
    }
    let (ax, ay) = (x.abs(), y.abs());
    let a = if x == 0.0 {
        PI / 2.0
    } else if ay <= ax {
        atan_unit(ay / ax)
    } else {
        PI / 2.0 - atan_unit(ax / ay)
    };
    let r = if negative_x { PI - a } else { a };
    if negative_y { -r } else { r }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_series_match_libm_closely() {
        for k in -200..=200 {
            let x = k as f64 * 0.0731;
            let (s, c) = sincos(x);
            assert!((s - shape_core::trig::sin(x)).abs() < 1e-14, "sin {x}");
            assert!((c - shape_core::trig::cos(x)).abs() < 1e-14, "cos {x}");
            for j in -7..=7 {
                let (y, xx) = (x, j as f64 * 0.37);
                assert!(
                    (atan2(y, xx) - shape_core::trig::atan2(y, xx)).abs() < 1e-14,
                    "atan2 {y} {xx}"
                );
            }
        }
        assert_eq!(atan2(0.0, -1.0), PI);
        assert_eq!(atan2(-0.0, -1.0), -PI);
        assert_eq!(atan2(-0.0, 1.0).to_bits(), (-0.0f64).to_bits());
        assert_eq!(atan2(1.0, 0.0), PI / 2.0);
    }
}
