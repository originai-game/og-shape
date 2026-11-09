//! Mesh booleans and split.
//!
//! [`csg`] and [`fracture`] are copied from origin-engine's og-geom
//! (`third_party/og-geom/SOURCE`): exact-predicate CSG with winding
//! numbers per region. Phase 0 decides whether this engine or
//! manifold-rust stays (`docs/plan.md`).
#![allow(missing_docs)]

pub mod csg;
pub mod fracture;

pub use csg::{CsgOp, csg, csg_traced};
