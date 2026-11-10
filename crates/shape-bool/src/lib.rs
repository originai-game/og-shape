//! Mesh booleans and split.
//!
//! [`csg`] and [`fracture`] are copied from origin-engine's og-geom
//! (`third_party/og-geom/SOURCE`): exact-predicate CSG with winding
//! numbers per region. Phase 0 decides whether this engine or
//! manifold-rust stays (`docs/plan.md`). [`selfcheck`] finds triangles of
//! one surface that cross, for the operand check and the self-union.
//! `exact` holds the expansion arithmetic that orders points on an edge
//! in the CSG's second pass (`docs/research.md` §5.4, item 5).
#![allow(missing_docs)]

pub mod csg;
mod exact;
pub mod fracture;
pub mod named;
pub mod selfcheck;

pub use csg::{
    CsgOp, CsgOutput, OperandError, SelfUnion, csg, csg_points, csg_points_traced, csg_traced,
    self_union_points,
};
pub use named::{
    BoolError, BoolOp, BoolReport, Boolean, boolean, boolean_report, check_operands, components,
    crossing_pairs, fuse, self_union,
};
