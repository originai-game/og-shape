//! Exact arithmetic on f64 inputs: Shewchuk's expansions (a sum of f64
//! components that do not overlap, increasing in magnitude), with `+ - *`
//! only, no fused multiply-add, so every host gets the same bits. Used to
//! order points made on one edge that lie a rounding error apart
//! (`docs/research.md` §5.4, item 5); the common case never needs it.

use std::cmp::Ordering;

/// An exact value: the sum of its components.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Exp(Vec<f64>);

fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let x = a + b;
    let bv = x - a;
    let av = x - bv;
    (x, (a - av) + (b - bv))
}

/// Dekker's split: `a = hi + lo`, each with at most 26 significant bits.
fn split(a: f64) -> (f64, f64) {
    const SPLITTER: f64 = 134_217_729.0; // 2^27 + 1
    let c = SPLITTER * a;
    let big = c - a;
    let hi = c - big;
    (hi, a - hi)
}

fn two_product(a: f64, b: f64) -> (f64, f64) {
    let x = a * b;
    let (ahi, alo) = split(a);
    let (bhi, blo) = split(b);
    let err = x - ahi * bhi - alo * bhi - ahi * blo;
    (x, alo * blo - err)
}

impl Exp {
    #[cfg(test)]
    pub(crate) fn from(a: f64) -> Exp {
        Exp(if a == 0.0 { Vec::new() } else { vec![a] })
    }

    /// `a - b`, exactly.
    pub(crate) fn diff(a: f64, b: f64) -> Exp {
        let (x, y) = two_sum(a, -b);
        let mut v = Vec::with_capacity(2);
        if y != 0.0 {
            v.push(y);
        }
        if x != 0.0 {
            v.push(x);
        }
        Exp(v)
    }

    /// Grow-Expansion with zero elimination.
    fn grow(&self, b: f64) -> Exp {
        let mut out = Vec::with_capacity(self.0.len() + 1);
        let mut q = b;
        for &e in &self.0 {
            let (s, h) = two_sum(q, e);
            if h != 0.0 {
                out.push(h);
            }
            q = s;
        }
        if q != 0.0 {
            out.push(q);
        }
        Exp(out)
    }

    pub(crate) fn add(&self, o: &Exp) -> Exp {
        let mut acc = self.clone();
        for &c in &o.0 {
            acc = acc.grow(c);
        }
        acc
    }

    pub(crate) fn neg(&self) -> Exp {
        Exp(self.0.iter().map(|&c| -c).collect())
    }

    pub(crate) fn sub(&self, o: &Exp) -> Exp {
        self.add(&o.neg())
    }

    fn scale(&self, b: f64) -> Exp {
        let mut acc = Exp::default();
        for &c in &self.0 {
            let (x, y) = two_product(c, b);
            acc = acc.grow(y).grow(x);
        }
        acc
    }

    pub(crate) fn mul(&self, o: &Exp) -> Exp {
        let mut acc = Exp::default();
        for &c in &o.0 {
            acc = acc.add(&self.scale(c));
        }
        acc
    }

    /// -1, 0 or 1: the sign of the largest component.
    pub(crate) fn sign(&self) -> i32 {
        match self.0.iter().rev().find(|&&c| c != 0.0) {
            Some(&c) if c > 0.0 => 1,
            Some(_) => -1,
            None => 0,
        }
    }
}

/// `orient2d` as an exact value: (b − a) × (c − a).
pub(crate) fn orient2d(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> Exp {
    let (bax, bay) = (Exp::diff(b[0], a[0]), Exp::diff(b[1], a[1]));
    let (cax, cay) = (Exp::diff(c[0], a[0]), Exp::diff(c[1], a[1]));
    bax.mul(&cay).sub(&bay.mul(&cax))
}

/// `orient3d` as an exact value: (a − d) · ((b − d) × (c − d)).
pub(crate) fn orient3d(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> Exp {
    let v = |p: [f64; 3]| [0, 1, 2].map(|k| Exp::diff(p[k], d[k]));
    let (a, b, c) = (v(a), v(b), v(c));
    let cross = [
        b[1].mul(&c[2]).sub(&b[2].mul(&c[1])),
        b[2].mul(&c[0]).sub(&b[0].mul(&c[2])),
        b[0].mul(&c[1]).sub(&b[1].mul(&c[0])),
    ];
    a[0].mul(&cross[0])
        .add(&a[1].mul(&cross[1]))
        .add(&a[2].mul(&cross[2]))
}

/// A parameter along an edge as an exact fraction `num / den` (`den` not 0).
#[derive(Clone, Debug)]
pub(crate) struct Fraction {
    pub(crate) num: Exp,
    pub(crate) den: Exp,
}

impl Fraction {
    /// None when the denominator is 0.
    pub(crate) fn new(num: Exp, den: Exp) -> Option<Fraction> {
        (den.sign() != 0).then_some(Fraction { num, den })
    }

    /// The exact order of two fractions.
    pub(crate) fn cmp(&self, o: &Fraction) -> Ordering {
        let s =
            self.num.mul(&o.den).sub(&o.num.mul(&self.den)).sign() * self.den.sign() * o.den.sign();
        s.cmp(&0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_signs_where_f64_cancels() {
        // (1 + 2⁻⁵²)² − 1 − 2⁻⁵¹ = 2⁻¹⁰⁴, lost in f64.
        let e = f64::EPSILON;
        let a = Exp::from(1.0 + e);
        let v = a.mul(&a).sub(&Exp::from(1.0)).sub(&Exp::from(2.0 * e));
        assert_eq!(v.sign(), 1);
        assert_eq!(v.sub(&Exp::from(e * e)).sign(), 0);
        // Collinear in exact terms, and an ulp off.
        assert_eq!(orient2d([0.0, 0.0], [1.0, 1.0], [3.0, 3.0]).sign(), 0);
        assert_eq!(
            orient2d([0.0, 0.0], [1.0, 1.0], [3.0, 3.0 + 4.0 * e]).sign(),
            1
        );
        let o = orient3d(
            [0.0; 3],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.1, 0.1, 1e-300],
        );
        assert_eq!(
            o.sign(),
            robust::orient3d(
                robust::Coord3D {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0
                },
                robust::Coord3D {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0
                },
                robust::Coord3D {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0
                },
                robust::Coord3D {
                    x: 0.1,
                    y: 0.1,
                    z: 1e-300
                },
            )
            .signum() as i32
        );
    }
}
