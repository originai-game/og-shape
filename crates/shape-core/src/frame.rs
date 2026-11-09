//! A right-handed orthonormal frame: an origin and three axes.

use glam::DVec3;

/// A placement in space: an origin and right-handed orthonormal axes.
///
/// The zero value is not a valid frame; start from [`Frame::IDENTITY`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// Where the frame's local origin sits.
    pub origin: DVec3,
    /// Local +X.
    pub x: DVec3,
    /// Local +Y.
    pub y: DVec3,
    /// Local +Z.
    pub z: DVec3,
}

impl Default for Frame {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Frame {
    /// The world frame.
    pub const IDENTITY: Frame = Frame {
        origin: DVec3::ZERO,
        x: DVec3::X,
        y: DVec3::Y,
        z: DVec3::Z,
    };

    /// A frame at `origin` whose +Z is `normal` and whose +X is `x_hint`
    /// made orthogonal to it. Returns `None` if `normal` is zero or
    /// `x_hint` is parallel to it.
    pub fn from_normal(origin: DVec3, normal: DVec3, x_hint: DVec3) -> Option<Frame> {
        let z = normal.try_normalize()?;
        let x = (x_hint - z * x_hint.dot(z)).try_normalize()?;
        let y = z.cross(x);
        Some(Frame { origin, x, y, z })
    }

    /// A local point in world coordinates.
    pub fn point_to_world(&self, p: DVec3) -> DVec3 {
        self.origin + self.vector_to_world(p)
    }

    /// A local direction in world coordinates.
    pub fn vector_to_world(&self, v: DVec3) -> DVec3 {
        self.x * v.x + self.y * v.y + self.z * v.z
    }

    /// A world point in local coordinates.
    pub fn point_to_local(&self, p: DVec3) -> DVec3 {
        self.vector_to_local(p - self.origin)
    }

    /// A world direction in local coordinates.
    pub fn vector_to_local(&self, v: DVec3) -> DVec3 {
        DVec3::new(v.dot(self.x), v.dot(self.y), v.dot(self.z))
    }

    /// The frame `local`, given in this frame's coordinates, in world
    /// coordinates.
    pub fn compose(&self, local: &Frame) -> Frame {
        Frame {
            origin: self.point_to_world(local.origin),
            x: self.vector_to_world(local.x),
            y: self.vector_to_world(local.y),
            z: self.vector_to_world(local.z),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_normal_is_right_handed_and_round_trips() {
        let f = Frame::from_normal(
            DVec3::new(1.0, 2.0, 3.0),
            DVec3::new(0.0, 0.0, 2.0),
            DVec3::new(1.0, 1.0, 5.0),
        )
        .unwrap();
        assert!((f.x.cross(f.y) - f.z).length() < 1e-15);
        let p = DVec3::new(-4.0, 0.5, 7.0);
        assert!((f.point_to_local(f.point_to_world(p)) - p).length() < 1e-12);
    }

    #[test]
    fn from_normal_rejects_degenerate_input() {
        assert!(Frame::from_normal(DVec3::ZERO, DVec3::ZERO, DVec3::X).is_none());
        assert!(Frame::from_normal(DVec3::ZERO, DVec3::Z, DVec3::Z * 3.0).is_none());
    }
}
