//! Generators whose output is checked here, where `shape-bool`'s exact
//! tests are in reach: [`loft`].
//!
//! A loft on a curve (`steps` > 1) can overshoot between sections and pass
//! its surface through itself; `shape_gen::paths::loft` builds whatever
//! the curve gives. [`loft`] tests the result with
//! [`shape_bool::selfcheck::self_crossings`] and refuses a folded one,
//! naming the sections it folds between.

use shape_bool::selfcheck::self_crossings;
use shape_gen::paths::Loft;
use shape_gen::solids::GenError;
use shape_mesh::Solid;

use crate::error::ModelError;

/// [`shape_gen::paths::loft`], refused with [`GenError::Invalid`] when the
/// surface passes through itself: the message names the lowest pair of
/// sections a crossing lies between, and how many triangle pairs cross.
pub fn loft(d: &Loft) -> Result<Solid, ModelError> {
    loft_with(d, false)
}

/// [`loft`], with UVs when `uvs` (`shape_gen::solids::textured`).
pub fn loft_with(d: &Loft, uvs: bool) -> Result<Solid, ModelError> {
    let s = if uvs {
        shape_gen::solids::textured(&shape_gen::solids::Generator::Loft(d))?
    } else {
        shape_gen::paths::loft(d)?
    };
    let pairs = self_crossings(&s.positions, &s.triangles);
    let Some(&(i, j)) = pairs.first() else {
        return Ok(s);
    };
    let heights: Vec<f64> = d.sections.iter().map(|x| x.1).collect();
    // The span a triangle lies in: the last section at or below its lowest
    // point (the top section's own plane belongs to the span below it).
    let span = |t: u32| {
        let lo = s.triangles[t as usize]
            .iter()
            .map(|&v| s.positions[v as usize].z)
            .fold(f64::INFINITY, f64::min);
        let k = heights.iter().rposition(|&h| h <= lo).unwrap_or(0);
        k.min(heights.len() - 2)
    };
    let low = pairs
        .iter()
        .map(|&(a, b)| span(a).min(span(b)))
        .min()
        .unwrap_or(span(i).min(span(j)));
    Err(ModelError::Gen(GenError::Invalid(format!(
        "loft: the surface passes through itself between sections {low} and {} \
         (z {} to {}; {} crossing triangle pairs): the curve overshoots there. \
         Add a section between them, bring the sizes closer, or set steps to 1 \
         (straight)",
        low + 1,
        heights[low],
        heights[low + 1],
        pairs.len()
    ))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shape_2d::{Circle, circle};

    fn disc(radius: f64) -> shape_2d::Profile {
        circle(&Circle {
            radius,
            segments: 24,
        })
        .unwrap()
    }

    #[test]
    fn a_curved_loft_through_a_narrow_neck_is_refused() {
        // Catmull-Rom from 20 to 1 heads for 60 next: the radius runs
        // below 0 before the neck, so the wall passes through the axis.
        let d = Loft {
            sections: vec![(disc(20.0), 0.0), (disc(1.0), 10.0), (disc(60.0), 20.0)],
            steps: 6,
            ..Default::default()
        };
        let e = loft(&d).unwrap_err().to_string();
        assert!(e.contains("between sections 0 and 1"), "{e}");
        // Straight, the same sections make a solid.
        let ok = loft(&Loft { steps: 1, ..d }).unwrap();
        assert!(ok.measure().closed);
    }
}
