//! Text outlines from a font the caller supplies (TrueType or OpenType,
//! read with ttf-parser). No font ships with the kernel.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use shape_core::DVec2;

use crate::curve::{bezier_segments, cubic, quadratic};
use crate::{Contour, Profile, ProfileError};

/// A line of text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Text {
    /// The characters; a character the font lacks is skipped.
    pub text: String,
    /// The font file's bytes.
    pub font: Vec<u8>,
    /// Cap-to-baseline scale: the em size in millimetres.
    pub size: f64,
    /// Extra space between characters, millimetres.
    pub spacing: f64,
}

/// Outline commands, as a font gives them, already scaled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outline {
    /// Starts a contour.
    Move(DVec2),
    /// A straight segment.
    Line(DVec2),
    /// A quadratic Bézier (control, end).
    Quad(DVec2, DVec2),
    /// A cubic Bézier (control, control, end).
    Cubic(DVec2, DVec2, DVec2),
    /// Closes the contour.
    Close,
}

/// A point where two segments meet is a corner when they turn by more than
/// about 11° (cos < 0.98); points inside a curve are smooth.
const SMOOTH_COS: f64 = 0.98;

/// Flattens outline commands into a profile, filled by the non-zero rule
/// (so glyph holes and overlapping strokes come out right).
pub fn outline_profile(commands: &[Outline]) -> Result<Profile, ProfileError> {
    let mut contours: Vec<(Vec<DVec2>, Vec<bool>)> = Vec::new();
    let mut pts: Vec<DVec2> = Vec::new();
    // Per point, whether it ends a segment (a candidate corner).
    let mut joint: Vec<bool> = Vec::new();
    let mut flush = |pts: &mut Vec<DVec2>, joint: &mut Vec<bool>| {
        if pts.len() > 1 && pts.first() == pts.last() {
            pts.pop();
            joint.pop();
            if let Some(j) = joint.first_mut() {
                *j = true;
            }
        }
        if pts.len() >= 3 {
            let n = pts.len();
            let sharp = (0..n)
                .map(|i| {
                    if !joint[i] {
                        return false;
                    }
                    let a = (pts[i] - pts[(i + n - 1) % n]).normalize_or_zero();
                    let b = (pts[(i + 1) % n] - pts[i]).normalize_or_zero();
                    a.dot(b) < SMOOTH_COS
                })
                .collect();
            contours.push((std::mem::take(pts), sharp));
        }
        pts.clear();
        joint.clear();
    };
    for c in commands {
        match *c {
            Outline::Move(p) => {
                flush(&mut pts, &mut joint);
                pts.push(p);
                joint.push(true);
            }
            Outline::Line(p) => {
                pts.push(p);
                joint.push(true);
            }
            Outline::Quad(c1, p) => {
                let a = *pts.last().unwrap_or(&p);
                let q = quadratic(a, c1, p, bezier_segments(&[a, c1, p]));
                joint.extend(std::iter::repeat_n(false, q.len() - 1));
                joint.push(true);
                pts.extend(q);
            }
            Outline::Cubic(c1, c2, p) => {
                let a = *pts.last().unwrap_or(&p);
                let q = cubic(a, c1, c2, p, bezier_segments(&[a, c1, c2, p]));
                joint.extend(std::iter::repeat_n(false, q.len() - 1));
                joint.push(true);
                pts.extend(q);
            }
            Outline::Close => flush(&mut pts, &mut joint),
        }
    }
    flush(&mut pts, &mut joint);
    if contours.is_empty() {
        return Err(ProfileError::Empty);
    }
    // Resolve the winding with a self-union, then restore the corner flags.
    let input = Profile {
        regions: contours
            .into_iter()
            .map(|(points, sharp)| crate::Region {
                outer: Contour { points, sharp },
                holes: Vec::new(),
            })
            .collect(),
    };
    let subject: Vec<Vec<[f64; 2]>> = input
        .contours()
        .map(|c| c.points.iter().map(|q| [q.x, q.y]).collect())
        .collect();
    let empty: Vec<Vec<[f64; 2]>> = Vec::new();
    let out = subject.overlay(&empty, OverlayRule::Subject, FillRule::NonZero);
    let p = crate::ops::from_overlay(out, &[&input]);
    if p.regions.is_empty() {
        Err(ProfileError::Empty)
    } else {
        Ok(p)
    }
}

struct Builder {
    scale: f64,
    offset: f64,
    out: Vec<Outline>,
}

impl Builder {
    fn p(&self, x: f32, y: f32) -> DVec2 {
        DVec2::new(
            f64::from(x) * self.scale + self.offset,
            f64::from(y) * self.scale,
        )
    }
}

impl ttf_parser::OutlineBuilder for Builder {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.p(x, y);
        self.out.push(Outline::Move(p));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.p(x, y);
        self.out.push(Outline::Line(p));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (c, p) = (self.p(x1, y1), self.p(x, y));
        self.out.push(Outline::Quad(c, p));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (c1, c2, p) = (self.p(x1, y1), self.p(x2, y2), self.p(x, y));
        self.out.push(Outline::Cubic(c1, c2, p));
    }
    fn close(&mut self) {
        self.out.push(Outline::Close);
    }
}

/// The text's outline, baseline on y = 0, starting at x = 0, advancing by
/// the font's horizontal advances (no kerning or shaping).
pub fn text(d: &Text) -> Result<Profile, ProfileError> {
    if !(d.size.is_finite() && d.spacing.is_finite()) {
        return Err(ProfileError::NotFinite("size"));
    }
    if d.size <= 0.0 {
        return Err(ProfileError::NotPositive("size"));
    }
    let face = ttf_parser::Face::parse(&d.font, 0).map_err(|_| ProfileError::Font)?;
    let scale = d.size / f64::from(face.units_per_em());
    let mut b = Builder {
        scale,
        offset: 0.0,
        out: Vec::new(),
    };
    for ch in d.text.chars() {
        let Some(id) = face.glyph_index(ch) else {
            continue;
        };
        face.outline_glyph(id, &mut b);
        let advance = face.glyph_hor_advance(id).map_or(0.0, f64::from);
        b.offset += advance * scale + d.spacing;
    }
    outline_profile(&b.out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An "O": an outer square and an inner one wound the same way, as
    /// some fonts draw it; non-zero filling must still make a hole.
    #[test]
    fn a_glyph_with_a_hole_and_a_curve() {
        let v = DVec2::new;
        let cmds = [
            Outline::Move(v(0.0, 0.0)),
            Outline::Line(v(10.0, 0.0)),
            Outline::Line(v(10.0, 10.0)),
            Outline::Line(v(0.0, 10.0)),
            Outline::Close,
            Outline::Move(v(3.0, 3.0)),
            Outline::Line(v(3.0, 7.0)),
            Outline::Line(v(7.0, 7.0)),
            Outline::Line(v(7.0, 3.0)),
            Outline::Close,
            // A quarter disc beside it, from a quadratic.
            Outline::Move(v(20.0, 0.0)),
            Outline::Line(v(25.0, 0.0)),
            Outline::Quad(v(25.0, 5.0), v(20.0, 5.0)),
            Outline::Close,
        ];
        let p = outline_profile(&cmds).unwrap();
        assert_eq!(p.regions.len(), 2);
        let o = p.regions.iter().find(|r| !r.holes.is_empty()).unwrap();
        assert_eq!(o.holes.len(), 1);
        // The O, plus the triangle under the chord and the parabolic segment
        // over it (2/3 of its control triangle).
        let want = (100.0 - 16.0) + 12.5 + 12.5 * 2.0 / 3.0;
        assert!((p.area() - want).abs() < 0.2, "{}", p.area());
        // The square's corners are corners; the curve's inside is smooth.
        let q = p.regions.iter().find(|r| r.holes.is_empty()).unwrap();
        assert_eq!(q.outer.sharp.iter().filter(|s| **s).count(), 3);
    }
}
