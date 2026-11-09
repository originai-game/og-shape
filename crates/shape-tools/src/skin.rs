//! A part's skin in glTF: bone nodes, inverse bind matrices, JOINTS_0 and
//! WEIGHTS_0 (`docs/research.md` §11).
//!
//! Kernel space (millimetres, Z up) maps to glTF space by `C(p) =
//! y_up(p) · METRES`. A bone's node carries its rest frame relative to its
//! parent bone, in glTF space; its inverse bind matrix is `C · IBM · C⁻¹`
//! (for a rigid IBM: the rotation conjugated by the axis swap, the
//! translation in metres).

use shape_core::DVec3;
use shape_mesh::skin::Mat4;
use shape_mesh::{Skin, Solid};

use crate::export::{ExportError, Part, Rigid};
use crate::json::{Json, nums};
use crate::surface::{METRES, y_up};

/// The skin of `part`, checked for export: valid, at most 65 536 bones
/// (JOINTS_0 is u16), and on a part without a joint (a skinned mesh's
/// node must stay at the world's origin, where its bones are bound).
pub(crate) fn checked<'a>(part: &Part<'a>) -> Result<Option<&'a Skin>, ExportError> {
    let solid: &'a Solid = part.solid;
    let Some(skin) = &solid.skin else {
        return Ok(None);
    };
    let bad = |why: String| ExportError::BadSkin {
        part: part.name.clone(),
        why,
    };
    skin.check(solid.positions.len())
        .map_err(|e| bad(e.to_string()))?;
    if skin.bones.len() > usize::from(u16::MAX) + 1 {
        return Err(bad("more than 65536 bones".into()));
    }
    if part.joint.is_some() {
        return Err(bad("a skinned part cannot hang on a joint".into()));
    }
    Ok(Some(skin))
}

/// JOINTS_0 (u16 × 4) and WEIGHTS_0 (f32 × 4) bytes for the split
/// vertices, each from its source vertex: copies of one vertex get the
/// same weights, so the mesh cannot tear where surfaces split. The f32
/// weights are made to sum to 1 in f32 by taking the largest as one minus
/// the others.
pub(crate) fn vertex_data(skin: &Skin, sources: &[u32]) -> (Vec<u8>, Vec<u8>) {
    let mut joints = Vec::with_capacity(sources.len() * 8);
    let mut weights = Vec::with_capacity(sources.len() * 16);
    for &v in sources {
        let w = &skin.weights[v as usize];
        for (k, &b) in w.bones.iter().enumerate() {
            let b = if w.weights[k] > 0.0 { b as u16 } else { 0 };
            joints.extend_from_slice(&b.to_le_bytes());
        }
        let mut f = w.weights.map(|x| x as f32);
        f[0] = 1.0 - (f[1] + f[2] + f[3]);
        for x in f {
            weights.extend_from_slice(&x.to_le_bytes());
        }
    }
    (joints, weights)
}

/// A bone's node fields and its parent bone.
type BoneNode = (Vec<(&'static str, Json)>, Option<u32>);

/// Each bone's node fields (name, translation, rotation) and its parent
/// bone, parents first.
pub(crate) fn bone_nodes(skin: &Skin) -> Vec<BoneNode> {
    skin.bones
        .iter()
        .map(|b| {
            let rest = Rigid::of(&b.rest);
            let local = match b.parent {
                Some(p) => Rigid::of(&skin.bones[p as usize].rest)
                    .inverse()
                    .then(&rest),
                None => rest,
            };
            let (t, r) = local.gltf();
            (
                vec![
                    ("name", Json::Str(b.name.clone())),
                    ("translation", nums(&[t.x, t.y, t.z])),
                    ("rotation", nums(&r)),
                ],
                b.parent,
            )
        })
        .collect()
}

/// The inverse bind matrices in glTF space, f32, column-major.
pub(crate) fn inverse_binds(skin: &Skin) -> Vec<u8> {
    let mut out = Vec::with_capacity(skin.bones.len() * 64);
    for b in &skin.bones {
        for x in gltf_matrix(&b.inverse_bind) {
            out.extend_from_slice(&(x as f32).to_le_bytes());
        }
    }
    out
}

/// `C · m · C⁻¹` for an affine `m` in kernel space.
fn gltf_matrix(m: &Mat4) -> Mat4 {
    let col = |c: usize| DVec3::new(m[4 * c], m[4 * c + 1], m[4 * c + 2]);
    // C⁻¹ sends glTF x, y, z to kernel x, z, −y (over METRES, which
    // cancels for the linear part).
    let linear = |v: DVec3| y_up(col(0) * v.x + col(1) * v.y + col(2) * v.z);
    let x = linear(DVec3::X);
    let y = linear(DVec3::Z);
    let z = linear(-DVec3::Y);
    let t = y_up(col(3)) * METRES;
    [
        x.x, x.y, x.z, 0.0, //
        y.x, y.y, y.z, 0.0, //
        z.x, z.y, z.z, 0.0, //
        t.x, t.y, t.z, 1.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use shape_core::Frame;
    use shape_mesh::skin::{frame_inverse_matrix, frame_matrix};

    fn apply(m: &Mat4, p: DVec3) -> DVec3 {
        DVec3::new(
            m[0] * p.x + m[4] * p.y + m[8] * p.z + m[12],
            m[1] * p.x + m[5] * p.y + m[9] * p.z + m[13],
            m[2] * p.x + m[6] * p.y + m[10] * p.z + m[14],
        )
    }

    #[test]
    fn gltf_matrix_conjugates_by_the_axis_change() {
        let f = Frame::from_normal(
            DVec3::new(3.0, -2.0, 7.0),
            DVec3::new(1.0, 1.0, 0.5),
            DVec3::Z,
        )
        .unwrap();
        for m in [frame_matrix(&f), frame_inverse_matrix(&f)] {
            let g = gltf_matrix(&m);
            let p = DVec3::new(10.0, -20.0, 5.0);
            let lhs = apply(&g, y_up(p) * METRES);
            let rhs = y_up(apply(&m, p)) * METRES;
            assert!((lhs - rhs).length() < 1e-15, "{lhs} {rhs}");
        }
    }
}
