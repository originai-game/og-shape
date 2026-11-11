//! The golden cases. A case feeds fixed inputs to one operation and hashes
//! its whole output.

use core::cmp::Ordering;

use shape_core::{DVec2, DVec3, Frame, HashError, OutputHash, order, predicates, trig};

/// One golden case.
pub struct Case {
    /// Unique name; the hash file is keyed by it.
    pub name: &'static str,
    /// Writes the operation's output into the hash.
    pub run: fn(&mut OutputHash) -> Result<(), HashError>,
}

/// Every case, in any order; the caller sorts them.
pub fn all() -> Vec<Case> {
    let mut cases = vec![
        Case {
            name: "core.trig",
            run: core_trig,
        },
        Case {
            name: "core.predicates",
            run: core_predicates,
        },
        Case {
            name: "core.frame",
            run: core_frame,
        },
        Case {
            name: "core.order",
            run: core_order,
        },
    ];
    cases.extend(crate::geom::cases());
    cases.extend(crate::solids::cases());
    cases.extend(crate::tools::cases());
    cases.extend(crate::look::cases());
    cases.extend(crate::painted::cases());
    cases.extend(crate::bevel::cases());
    cases.extend(crate::gen2::cases());
    cases.extend(crate::mcp::cases());
    cases.extend(crate::model::cases());
    cases.extend(crate::meshtools::cases());
    cases.extend(crate::meshrepair::cases());
    cases.extend(crate::skin::cases());
    cases.extend(crate::selfunion::cases());
    cases.extend(crate::touching::cases());
    cases.extend(crate::interference::cases());
    cases.extend(crate::surface::cases());
    cases.extend(crate::rest::cases());
    cases.extend(crate::smallgaps::cases());
    cases.extend(crate::uvs::cases());
    cases.extend(crate::place::cases());
    cases.extend(crate::bake::cases());
    cases
}

/// SplitMix64: a fixed, portable stream of test inputs.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[lo, hi)`, built from 53 random bits.
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        let unit = (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64);
        lo + (hi - lo) * unit
    }

    fn dvec2(&mut self, r: f64) -> DVec2 {
        DVec2::new(self.range(-r, r), self.range(-r, r))
    }

    fn dvec3(&mut self, r: f64) -> DVec3 {
        DVec3::new(self.range(-r, r), self.range(-r, r), self.range(-r, r))
    }
}

fn ordering(h: &mut OutputHash, o: Ordering) {
    h.u64(match o {
        Ordering::Less => 0,
        Ordering::Equal => 1,
        Ordering::Greater => 2,
    });
}

fn core_trig(h: &mut OutputHash) -> Result<(), HashError> {
    let mut rng = Rng(1);
    for _ in 0..4096 {
        let x = rng.range(-1000.0, 1000.0);
        let u = rng.range(-1.0, 1.0);
        let p = rng.range(0.0, 50.0);
        h.f64s(&[
            trig::sin(x),
            trig::cos(x),
            trig::tan(x),
            trig::asin(u),
            trig::acos(u),
            trig::atan(x),
            trig::atan2(x, u),
            trig::exp(u * 700.0),
            trig::ln(p),
            trig::pow(p, u * 10.0),
            trig::cbrt(x),
            trig::hypot(x, u),
        ])?;
        let (s, c) = trig::sin_cos(x);
        h.f64s(&[s, c])?;
    }
    Ok(())
}

fn core_predicates(h: &mut OutputHash) -> Result<(), HashError> {
    let mut rng = Rng(2);
    for _ in 0..2048 {
        let [a, b, c, d] = [0; 4].map(|_| rng.dvec2(10.0));
        ordering(h, predicates::orient2d(a, b, c));
        ordering(h, predicates::incircle(a, b, c, d));
        // Nearly degenerate: c on the segment a-b, up to rounding.
        let t = rng.range(0.0, 1.0);
        ordering(h, predicates::orient2d(a, b, a + (b - a) * t));
        let [a, b, c, d, e] = [0; 5].map(|_| rng.dvec3(10.0));
        ordering(h, predicates::orient3d(a, b, c, d));
        ordering(h, predicates::insphere(a, b, c, d, e));
        let s = rng.range(0.0, 1.0);
        ordering(
            h,
            predicates::orient3d(a, b, c, a + (b - a) * t + (c - a) * s),
        );
    }
    Ok(())
}

fn core_frame(h: &mut OutputHash) -> Result<(), HashError> {
    let mut rng = Rng(3);
    let mut acc = Frame::IDENTITY;
    for _ in 0..1024 {
        let Some(f) = Frame::from_normal(rng.dvec3(100.0), rng.dvec3(1.0), rng.dvec3(1.0)) else {
            h.str("degenerate");
            continue;
        };
        acc = acc.compose(&f);
        let p = rng.dvec3(50.0);
        h.dvec3s(&[
            acc.origin,
            acc.x,
            acc.y,
            acc.z,
            f.point_to_world(p),
            f.point_to_local(p),
        ])?;
    }
    Ok(())
}

fn core_order(h: &mut OutputHash) -> Result<(), HashError> {
    let mut rng = Rng(4);
    // A coarse grid, so many points tie and stability matters.
    let points: Vec<DVec3> = (0..4096)
        .map(|_| {
            let p = rng.dvec3(4.0);
            DVec3::new(p.x.floor(), p.y.floor(), p.z.floor() * -0.0)
        })
        .collect();
    let order = order::sorted_order(&points);
    order.iter().for_each(|&i| h.u64(u64::from(i)));
    Ok(())
}
