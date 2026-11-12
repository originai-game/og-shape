//! Skinning from the program (`docs/design.md` §4 "Skin", `docs/plan.md`
//! phase 5b): bones from named face regions, weights that blend across
//! the seams between regions, and posing by linear blend skinning.
//!
//! Entry: [`skin`], which takes a solid and a zero-value [`Rig`] and
//! returns the solid with a [`shape_mesh::Skin`]. Then:
//! - [`pose`]: the posed mesh for a [`Pose`] (bone turns), for the
//!   preview and for measures;
//! - [`skin_matrices`]: each bone's skinning matrix for a pose;
//! - [`measure_pose`]: how a pose deforms the triangles, with no
//!   thresholds (`docs/design.md` §7).
//!
//! The weights (`docs/research.md` §11):
//! - A face belongs to the bone whose region names it carries: a region
//!   `forearm` matches the tag `forearm` and the qualified tags
//!   `forearm.*` a boolean writes.
//! - A seam is where faces of two bones meet. A vertex is fully weighted
//!   to its own bone, except within `blend / 2` of a seam, measured along
//!   the surface (Dijkstra over the mesh's edges: an upper bound on the
//!   geodesic distance), where the other bone's weight rises smoothly to
//!   ½ on the seam: the raw weight of a bone at surface distance `d` is
//!   `1 − smoothstep(2d / blend)`, normalized over the bones. The blend
//!   is C¹ across the seam.
//! - Edges longer than `max_edge` that reach into a blend band are split
//!   first, so the band has vertices to bend. The surface keeps its shape:
//!   only flat triangles are split.
//!
//! Names (`docs/design.md` §4): a split triangle's pieces keep its face
//! names and surface; a split named edge's halves keep its names.
//!
//! Posing is linear blend skinning. Dual quaternions are not used: they
//! fix the collapse of twisted joints, and the figures here bend about
//! hinges, where with a blend of about four limb radii `measure_pose`
//! finds no inverted triangle but seam slivers at 90°
//! (`docs/research.md` §11).

mod mat;
mod refine;
mod rig;

pub use mat::{apply, apply_vector, mul, turn};
pub use rig::{
    Pose, PoseMeasures, Rig, RigBone, RigError, Turn, measure_pose, pose, pose_by, skin,
    skin_matrices,
};

#[cfg(test)]
mod tests;
