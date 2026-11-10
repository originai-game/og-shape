//! Why a model operation could not be honoured: one error value for the
//! whole model API.

use std::fmt;

use shape_2d::ProfileError;
use shape_bevel::BevelError;
use shape_bool::BoolError;
use shape_edit::EditError;
use shape_gen::solids::GenError;
use shape_tools::ExportError;

/// Why a model operation could not be honoured. Nothing is changed when
/// an operation returns one.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelError {
    /// No node has this id.
    UnknownId(u32),
    /// The node was used up by an earlier operation (a boolean's tool).
    UsedUp(String),
    /// The node was removed by `delete`.
    Deleted(String),
    /// A live node already has this name.
    NameTaken(String),
    /// A name must not be empty.
    EmptyName,
    /// The node is a group where a part (a solid) is needed.
    NotAPart(String),
    /// No face of the part carries the name: the names that exist follow.
    NoSuchFace {
        /// The part.
        part: String,
        /// The name asked for.
        name: String,
        /// The face names that exist, sorted.
        available: Vec<String>,
    },
    /// No edge of the part carries the name: the names that exist follow.
    NoSuchEdge {
        /// The part.
        part: String,
        /// The name asked for.
        name: String,
        /// The edge names that exist, sorted.
        available: Vec<String>,
    },
    /// The faces read as one face are not flat (the part and the name).
    NotFlat(String, String),
    /// The edges named fork: a path follows a line (the part and the name).
    Forks(String, String),
    /// No face of the part looks that way within 1° (the part and the
    /// direction).
    NotFacing(String, String),
    /// The faces or edges read have no area or no length.
    Degenerate(String),
    /// Not a side of a box: `+X`, an edge `+X-Y`, a corner `+X-Y+Z`.
    BadSides(String),
    /// A value was NaN or infinite, or out of its domain (the field named).
    BadValue(String),
    /// The unit is set before the first part, once.
    Units(String),
    /// A joint could not be declared (the joint and why).
    Joint(String, String),
    /// No joint has this name: the joints that exist follow.
    NoSuchJoint(String, Vec<String>),
    /// A pose outside the joint's range.
    OutOfRange {
        /// The joint.
        joint: String,
        /// The value asked for.
        value: f64,
        /// Its range.
        range: (f64, f64),
    },
    /// A frame was read off a part the pose moves: every frame the model
    /// gives is at rest.
    Posed {
        /// The read: `face`, `bbox`, `edge`, `snap`, `placement`, `local`
        /// or `anchor`.
        read: String,
        /// The part or group the pose moves.
        part: String,
        /// The joint whose value moves it.
        joint: String,
        /// That joint's value.
        value: f64,
    },
    /// A node cannot be used up or moved this way (the node and why).
    Refused(String, String),
    /// A profile could not be built.
    Profile(ProfileError),
    /// A generator refused.
    Gen(GenError),
    /// A boolean refused.
    Bool(BoolError),
    /// An edit refused.
    Edit(EditError),
    /// A fillet, chamfer or soften refused.
    Bevel(BevelError),
    /// The scene could not be written.
    Export(ExportError),
    /// A program could not be read (where, and what was wrong).
    Program(String),
    /// A field of an operation is missing or holds the wrong kind of value
    /// (the field, and what it should hold).
    BadField(String, String),
    /// An operation, or an object inside one, has a field it does not
    /// take.
    UnknownField {
        /// What has the field: the operation's verb, or `profile rect`,
        /// `sections[2]` and the like.
        within: String,
        /// The field given.
        field: String,
        /// The fields it takes.
        accepted: Vec<String>,
    },
    /// An operation of a program or of a live session failed: where, and
    /// why.
    At {
        /// Its index in the program: where it stands in a replayed
        /// program, or the index it would be recorded at in a live
        /// session.
        index: usize,
        /// The operation's verb (empty if it had none).
        op: String,
        /// Whether it is a reader, which is not recorded.
        reader: bool,
        /// The part or group it made or worked on, if any.
        part: Option<String>,
        /// Why it failed.
        error: Box<ModelError>,
    },
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::UnknownId(i) => write!(f, "no node has id {i}"),
            ModelError::UsedUp(n) => write!(f, "{n:?} was used up by an earlier operation"),
            ModelError::Deleted(n) => write!(f, "{n:?} was deleted by an earlier operation"),
            ModelError::NameTaken(n) => write!(f, "a node is already named {n:?}"),
            ModelError::EmptyName => f.write_str("a name must not be empty"),
            ModelError::NotAPart(n) => write!(f, "{n:?} is a group, not a part with a solid"),
            ModelError::NoSuchFace {
                part,
                name,
                available,
            } => write!(
                f,
                "{part:?} has no face named {name:?}; its face names: {}",
                available.join(", ")
            ),
            ModelError::NoSuchEdge {
                part,
                name,
                available,
            } => write!(
                f,
                "{part:?} has no edge named {name:?}; its edge names: {}",
                available.join(", ")
            ),
            ModelError::NotFlat(p, n) => write!(f, "{p:?}: the faces named {n:?} are not flat"),
            ModelError::Forks(p, n) => write!(
                f,
                "{p:?}: the edges named {n:?} fork; a path follows a line, not a junction"
            ),
            ModelError::NotFacing(p, d) => {
                write!(f, "no face of {p:?} looks {d} (within 1°)")
            }
            ModelError::Degenerate(w) => write!(f, "{w} has no area or no length"),
            ModelError::BadSides(s) => write!(
                f,
                "{s:?} names no side of a box: a side \"+X\", an edge \"+X-Y\" or a corner \"+X-Y+Z\""
            ),
            ModelError::BadValue(w) => write!(f, "{w} is not a valid value"),
            ModelError::Units(w) => write!(f, "units: {w}"),
            ModelError::Joint(j, why) => write!(f, "joint {j:?}: {why}"),
            ModelError::NoSuchJoint(j, all) => {
                write!(f, "no joint is named {j:?}; joints: {}", all.join(", "))
            }
            ModelError::OutOfRange {
                joint,
                value,
                range,
            } => write!(
                f,
                "pose {joint:?}: {value} is outside its range ({}, {})",
                range.0, range.1
            ),
            ModelError::Posed {
                read,
                part,
                joint,
                value,
            } => write!(
                f,
                "{read} gives frames at rest, and {part:?} is posed: joint {joint:?} is at \
                 {value}. Read it before posing, or pose {joint:?} back to 0 first"
            ),
            ModelError::Refused(n, why) => write!(f, "{n:?}: {why}"),
            ModelError::Profile(e) => write!(f, "profile: {e}"),
            ModelError::Gen(e) => write!(f, "{e}"),
            ModelError::Bool(e) => write!(f, "{e}"),
            ModelError::Edit(e) => write!(f, "{e}"),
            ModelError::Bevel(e) => write!(f, "{e}"),
            ModelError::Export(e) => write!(f, "{e}"),
            ModelError::Program(w) => write!(f, "program: {w}"),
            ModelError::BadField(field, want) => write!(f, "{field}: {want}"),
            ModelError::UnknownField {
                within,
                field,
                accepted,
            } => {
                write!(f, "{within} has no field {field:?}; ")?;
                if accepted.is_empty() {
                    f.write_str("it takes none")
                } else {
                    write!(f, "it takes: {}", accepted.join(", "))
                }
            }
            ModelError::At {
                index,
                op,
                reader,
                part,
                error,
            } => {
                let on = part.as_ref().map(|p| format!(" {p:?}")).unwrap_or_default();
                match (*reader, op.is_empty()) {
                    (_, true) => write!(f, "op {index}: {error}"),
                    (false, false) => write!(f, "op {index} {op}{on}: {error}"),
                    (true, false) => {
                        write!(f, "{op}{on} (a reader, before op {index}): {error}")
                    }
                }
            }
        }
    }
}

impl std::error::Error for ModelError {}

impl From<ProfileError> for ModelError {
    fn from(e: ProfileError) -> Self {
        ModelError::Profile(e)
    }
}

impl From<GenError> for ModelError {
    fn from(e: GenError) -> Self {
        ModelError::Gen(e)
    }
}

impl From<BoolError> for ModelError {
    fn from(e: BoolError) -> Self {
        ModelError::Bool(e)
    }
}

impl From<EditError> for ModelError {
    fn from(e: EditError) -> Self {
        ModelError::Edit(e)
    }
}

impl From<BevelError> for ModelError {
    fn from(e: BevelError) -> Self {
        ModelError::Bevel(e)
    }
}

impl From<ExportError> for ModelError {
    fn from(e: ExportError) -> Self {
        ModelError::Export(e)
    }
}
