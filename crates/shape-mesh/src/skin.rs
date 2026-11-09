//! A skin: bones that move a mesh's vertices (`docs/design.md` §4).
//!
//! Phase 5 keeps only a skin's description, read from an imported file, so
//! that no tool breaks it silently: a tool that would change a skinned
//! solid's vertices or write it out refuses it with an error value. Phase
//! 5b (`docs/plan.md`) adds the inverse bind matrices and the per-vertex
//! weights here and carries them through.

/// The skin an imported mesh came with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Skin {
    /// The skin's name in the file, or "".
    pub name: String,
    /// The bones' names, in the file's joint order.
    pub bones: Vec<String>,
    /// Vertices that carry bone weights.
    pub weighted_vertices: usize,
}
