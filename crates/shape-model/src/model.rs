//! The model: a tree of named parts and groups in the world, the joints
//! between them, their materials and pose, built into an export scene.

use std::collections::{BTreeMap, BTreeSet};

use shape_bevel::{Chamfer, Fillet, Soften};
use shape_bool::{BoolOp, BoolReport, Boolean};
use shape_core::{DVec3, Frame};
use shape_edit::{Dent, Hole, Keep, Pocket, Shell};
use shape_mesh::{Measures, Solid};
use shape_tools::{
    Image, Interfering, JointKind, Material, Mimic, Part, Placement, Preview, Scene,
};

use crate::error::ModelError;
use crate::place::{self, Path};

/// A node of a [`Model`]: a part or a group. Ids are given in order of
/// creation and never reused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Id(pub u32);

/// The length unit of a program. Tolerances stay in millimetres whatever
/// the unit; export writes metres.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Unit {
    /// Millimetres (the default).
    #[default]
    Millimetre,
    /// Centimetres.
    Centimetre,
    /// Metres.
    Metre,
}

impl Unit {
    /// Millimetres per unit.
    pub fn millimetres(self) -> f64 {
        match self {
            Unit::Millimetre => 1.0,
            Unit::Centimetre => 10.0,
            Unit::Metre => 1000.0,
        }
    }
}

/// How a child moves on its parent: a zero-value desc.
///
/// The zero value is a revolute joint about the world origin's +Z with no
/// name and no range, which [`Model::joint`] refuses until a name and a
/// range are given.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JointDesc {
    /// Unique among the model's joints.
    pub name: String,
    /// Revolute, continuous, prismatic, fixed or floating.
    pub kind: JointKind,
    /// The pivot, in the world at rest: revolute and continuous joints turn
    /// about `axis` through its origin; the joint's node sits here.
    pub on: Frame,
    /// The axis, a world direction; zero is `on`'s +Z (its normal).
    /// Positive values turn by the right hand about it, or slide along it.
    /// Fixed and floating joints take none.
    pub axis: DVec3,
    /// `(lower, upper)` containing 0: radians for turns, the model's unit
    /// for slides. Revolute and prismatic joints need one; the others,
    /// and followers, take none.
    pub range: Option<(f64, f64)>,
    /// This joint's value is `multiplier` × another's (radians or units
    /// per radian or unit, across kinds).
    pub mimic: Option<Mimic>,
}

#[derive(Clone, Debug)]
struct Node {
    name: String,
    /// The solid in the part's own space; `None` for a group.
    solid: Option<Solid>,
    /// Own space to world, at rest.
    placement: Frame,
    /// The group it is in.
    parent: Option<Id>,
    /// Its own material, if one was set on it, with the size (in the
    /// model's unit) its map spans.
    material: Option<(Material, f64)>,
    /// Named frames an assembly publishes, in the world.
    anchors: BTreeMap<String, Frame>,
    used_up: bool,
    /// Removed by [`Model::delete`] (also `used_up`).
    deleted: bool,
}

#[derive(Clone, Debug)]
struct JointRec {
    child: Id,
    parent: Id,
    desc: JointDesc,
    /// Unit world axis (none for fixed and floating).
    axis: Option<DVec3>,
    value: f64,
    /// The parent's own space seen from the pivot when the joint was
    /// declared (`on⁻¹ ∘ parent placement`): [`Model::seating`] measures
    /// how far the parent has moved off it since.
    seat: Frame,
}

/// How a joint holds, measured at rest in the model's unit
/// ([`Model::seating`]). Measurements only: no threshold.
#[derive(Clone, Debug, PartialEq)]
pub struct Seating {
    /// The joint.
    pub joint: String,
    /// The node it moves.
    pub child: String,
    /// The node it hangs on.
    pub parent: String,
    /// Its value now (a follower's from its leader).
    pub value: f64,
    /// From the pivot's origin to the nearest point of the parent's
    /// parts (those under it but not under the child); `None` when the
    /// parent has no triangles.
    pub pivot_to_parent: Option<f64>,
    /// From the pivot's origin to the nearest point of the child's parts.
    pub pivot_to_child: Option<f64>,
    /// How far the parent's own space has moved from the pivot since the
    /// joint was declared: a parent moved (`at`, `transform`) without its
    /// child leaves the pivot, and the child, where they were.
    pub parent_moved: f64,
    /// How far it has turned since, in radians.
    pub parent_turned: f64,
}

/// One part's measures in the world, in the model's unit.
#[derive(Clone, Debug, PartialEq)]
pub struct PartMeasures {
    /// The part's name.
    pub name: String,
    /// Its measures at rest.
    pub measures: Measures,
}

/// A model built for export: every part in the world, in millimetres,
/// posed by the joints' values at export. [`Built::scene`] lends it to the
/// tools.
#[derive(Clone, Debug, Default)]
pub struct Built {
    name: String,
    parts: Vec<(String, Solid, Material, String, Option<shape_tools::Joint>)>,
}

impl Built {
    /// The scene the tools read: parents before children.
    pub fn scene(&self) -> Scene<'_> {
        Scene {
            name: self.name.clone(),
            parts: self
                .parts
                .iter()
                .map(|(name, solid, material, parent, joint)| Part {
                    material: material.clone(),
                    parent: parent.clone(),
                    joint: joint.clone(),
                    ..Part::new(name, solid)
                })
                .collect(),
        }
    }
}

/// A model: named parts (solids with a placement and a material), groups
/// and assemblies above them, and joints between them (`docs/design.md`
/// §4).
///
/// - Every placement, frame, pivot and axis the API takes or gives is in
///   the world at rest (every joint at 0), in the model's unit.
/// - A part's solid stays in its own space; [`Model::at`] and
///   [`Model::transform`] move the part (and everything under it, with
///   the pivots of joints it carries) without touching the solid.
/// - Edits take their frames in the world and run in the part's space.
///   A boolean uses its tool up: the tool's id answers
///   [`ModelError::UsedUp`] after.
/// - Joints hang a child (a part or a group) on a parent part. Several on
///   one child compose in declaration order, the first nearest the parent.
///   [`Model::pose`] sets a value. Frames are read at rest only: a read
///   of a part a non-zero joint value moves ([`Model::bbox`],
///   [`Model::face`], [`Model::edge`], [`Model::snap`],
///   [`Model::placement`], [`Model::local`], [`Model::anchor`]) is
///   refused with [`ModelError::Posed`]. Export, preview and
///   [`Model::measure_posed`] show the pose.
#[derive(Clone, Debug, Default)]
pub struct Model {
    name: String,
    unit: Unit,
    nodes: Vec<Node>,
    joints: Vec<JointRec>,
    booleans: Vec<BoolRecord>,
    uvs: bool,
}

/// How [`Model::combine`] runs a boolean: a zero-value desc (a join, not
/// checked).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Combine {
    /// Join, cut or intersect.
    pub op: BoolOp,
    /// Refuse, before any work, an operand that passes through itself
    /// ([`shape_bool::check_operands`]): for operands from outside the
    /// kernel. It costs a sweep over each operand's triangles.
    pub check: bool,
}

/// One boolean of a model and what it did ([`Model::booleans`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoolRecord {
    /// `"cut"`, `"join"` or `"intersect"`.
    pub op: String,
    /// The target's name.
    pub target: String,
    /// The tool's name.
    pub tool: String,
    /// What the result alone does not say.
    pub report: BoolReport,
}

fn finite_frame(f: &Frame, what: &str) -> Result<(), ModelError> {
    if f.is_rigid(1e-9) {
        Ok(())
    } else {
        Err(ModelError::BadValue(format!(
            "{what} (a frame must be finite, orthonormal and right-handed)"
        )))
    }
}

/// The rigid motion taking `from` onto `to`: `to ∘ from⁻¹`.
fn motion(from: &Frame, to: &Frame) -> Frame {
    to.compose(&from.inverse())
}

/// A solid mapped by a placement (rigid: no mirror).
fn placed(mut s: Solid, f: &Frame) -> Solid {
    s.map_positions(|p| f.point_to_world(p), false);
    rounded(s)
}

/// A solid whose positions were just rounded by a motion or a scale, made
/// canonical and kept from crossing itself (`docs/research.md` §26).
///
/// - Rounding can put a needle a rounding error wide onto one line
///   (§15.4): such slivers are flipped away, exactly.
/// - It can turn a needle between two points a rounding error apart over
///   its neighbours (§25.3, §26): where the solid has an edge shorter
///   than the boolean's snap distance or a corner that close to the
///   opposite edge, and only then, its crossing pairs are counted, and
///   any are repaired by the boolean's repair (`uncross_output`, §23.2),
///   every point free to move. A solid it cannot improve is returned as
///   it is.
fn rounded(mut s: Solid) -> Solid {
    let _ = s.canonicalize();
    if shape_bool::drop_slivers(&mut s) {
        let _ = s.canonicalize();
    }
    if thin(&s) && shape_bool::crossing_pairs(&s) > 0 {
        s = uncrossed(s);
    }
    s
}

/// The boolean's snap distance on `s`: 2⁻⁴⁰ of its box's diagonal
/// (`docs/research.md` §18.1, §23.2), with the box's corners.
fn snap_distance(s: &Solid) -> (f64, [DVec3; 2]) {
    let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
    for &p in &s.positions {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    ((hi - lo).length() * (1.0 / (1u64 << 40) as f64), [lo, hi])
}

/// Whether `s` has a triangle that rounding can turn over: an edge shorter
/// than the snap distance, or a corner that close to the line of the
/// opposite (longest) edge. Only these decide whether to look; what is
/// changed is decided by exact predicates.
fn thin(s: &Solid) -> bool {
    if s.triangles.is_empty() {
        return false;
    }
    let (tol, _) = snap_distance(s);
    let tol2 = tol * tol;
    s.triangles.iter().any(|t| {
        let [a, b, c] = t.map(|v| s.positions[v as usize]);
        let longest = (b - a)
            .length_squared()
            .max((c - b).length_squared())
            .max((a - c).length_squared());
        let shortest = (b - a)
            .length_squared()
            .min((c - b).length_squared())
            .min((a - c).length_squared());
        // Twice the area is the height on the longest edge times its length.
        shortest < tol2 || (b - a).cross(c - a).length_squared() < tol2 * longest
    })
}

/// `s` (canonical, closed) with its crossing pairs repaired by
/// [`shape_bool::uncross_output`]: points closer than the snap distance
/// merged, needles split away, each change kept only if it leaves the
/// solid closed and crossing itself strictly less. Each kept triangle
/// keeps its source's surface, names and UVs (interpolated at a moved
/// corner); a named edge keeps its names on what it became; a skinned
/// vertex keeps its weights. Unchanged if nothing improves.
fn uncrossed(s: Solid) -> Solid {
    let (_, corners) = snap_distance(&s);
    let out = shape_bool::CsgOutput {
        points: s.positions.clone(),
        tris: s.triangles.clone(),
        origins: (0..s.triangles.len() as u32).map(|i| (0, i)).collect(),
        coplanar: Vec::new(),
        touching: Vec::new(),
        second_pass: false,
        unmatched: 0,
        touching_edges: 0,
        non_manifold: 0,
        snapped: false,
    };
    // The box's corners stand in for the operands: they set the snap
    // distance and pin nothing a rounding error matters to.
    let (out, report) = shape_bool::uncross_output(&corners, &[], out);
    if report.after >= report.before || out.unmatched > 0 || out.non_manifold > 0 {
        return s;
    }
    let key = |p: DVec3| p.to_array().map(|x| (x + 0.0).to_bits());
    let at: BTreeMap<[u64; 3], u32> = out
        .points
        .iter()
        .enumerate()
        .map(|(i, &p)| (key(p), i as u32))
        .collect();
    // Each old vertex's new one: itself, or for a merged point the
    // nearest point left (it was merged onto a point within the snap
    // distance); ties to the lower index.
    let new_of: Vec<u32> = s
        .positions
        .iter()
        .map(|&p| {
            at.get(&key(p)).copied().unwrap_or_else(|| {
                let mut best = (f64::INFINITY, 0u32);
                for (i, &q) in out.points.iter().enumerate() {
                    let d = (q - p).length_squared();
                    if d < best.0 {
                        best = (d, i as u32);
                    }
                }
                best.1
            })
        })
        .collect();
    let mut r = Solid {
        positions: out.points,
        names: s.names.clone(),
        ..Default::default()
    };
    let mut uvs = s.uvs.as_ref().map(|_| Vec::with_capacity(out.tris.len()));
    for (t, &(_, src)) in out.tris.iter().zip(&out.origins) {
        let src = src as usize;
        r.triangles.push(*t);
        r.surfaces.push(s.surfaces[src]);
        r.face_tags.push(s.face_tags[src].clone());
        if let (Some(u), Some(old)) = (&mut uvs, &s.uvs) {
            u.push(t.map(|v| s.uv_at(src, r.positions[v as usize]).unwrap_or(old[src][0])));
        }
    }
    r.uvs = uvs;
    // Named edges: on the edge their ends became, or, split at a point,
    // on both its pieces.
    let mut edges: BTreeSet<(u32, u32)> = BTreeSet::new();
    for t in &r.triangles {
        for k in 0..3 {
            edges.insert(shape_mesh::edge_key(t[k], t[(k + 1) % 3]));
        }
    }
    let mut tagged: BTreeMap<(u32, u32), Vec<shape_mesh::Tag>> = BTreeMap::new();
    for (&(a, b), tags) in &s.edge_tags {
        let (a, b) = (new_of[a as usize], new_of[b as usize]);
        if a == b {
            continue;
        }
        let pieces: Vec<(u32, u32)> = if edges.contains(&shape_mesh::edge_key(a, b)) {
            vec![shape_mesh::edge_key(a, b)]
        } else {
            (0..r.positions.len() as u32)
                .find(|&m| {
                    edges.contains(&shape_mesh::edge_key(a, m))
                        && edges.contains(&shape_mesh::edge_key(m, b))
                })
                .map(|m| vec![shape_mesh::edge_key(a, m), shape_mesh::edge_key(m, b)])
                .unwrap_or_default()
        };
        for e in pieces {
            let entry = tagged.entry(e).or_default();
            entry.extend(tags);
            entry.sort_unstable();
            entry.dedup();
        }
    }
    r.edge_tags = tagged;
    if let Some(skin) = &s.skin {
        let mut weights = vec![Default::default(); r.positions.len()];
        for (old, &new) in new_of.iter().enumerate() {
            if r.positions[new as usize] == s.positions[old] {
                weights[new as usize] = skin.weights.get(old).copied().unwrap_or_default();
            }
        }
        r.skin = Some(shape_mesh::Skin {
            bones: skin.bones.clone(),
            weights,
        });
    }
    let _ = r.canonicalize();
    r
}

impl Model {
    /// An empty model named `name` (the scene's name on export), in
    /// millimetres.
    pub fn new(name: &str) -> Model {
        Model {
            name: name.to_string(),
            ..Default::default()
        }
    }

    /// The model's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The program's length unit, set before the first part.
    pub fn units(&mut self, unit: Unit) -> Result<(), ModelError> {
        if !self.nodes.is_empty() && unit != self.unit {
            return Err(ModelError::Units(
                "set the unit before the first part".into(),
            ));
        }
        self.unit = unit;
        Ok(())
    }

    /// The unit in force.
    pub fn unit(&self) -> Unit {
        self.unit
    }

    // ── nodes ────────────────────────────────────────────────────────────────

    fn node(&self, id: Id) -> Result<&Node, ModelError> {
        let n = self
            .nodes
            .get(id.0 as usize)
            .ok_or(ModelError::UnknownId(id.0))?;
        if n.deleted {
            return Err(ModelError::Deleted(n.name.clone()));
        }
        if n.used_up {
            return Err(ModelError::UsedUp(n.name.clone()));
        }
        Ok(n)
    }

    fn solid_of(&self, id: Id) -> Result<&Solid, ModelError> {
        let n = self.node(id)?;
        n.solid
            .as_ref()
            .ok_or_else(|| ModelError::NotAPart(n.name.clone()))
    }

    /// The node's name.
    pub fn name_of(&self, id: Id) -> Result<&str, ModelError> {
        Ok(&self.node(id)?.name)
    }

    /// The name of node `id`, used up or not; `None` for no such node.
    pub(crate) fn label(&self, id: Id) -> Option<&str> {
        self.nodes.get(id.0 as usize).map(|n| n.name.as_str())
    }

    /// The live node named `name`.
    pub fn find(&self, name: &str) -> Option<Id> {
        self.nodes
            .iter()
            .position(|n| !n.used_up && n.name == name)
            .map(|i| Id(i as u32))
    }

    fn check_name(&self, name: &str) -> Result<(), ModelError> {
        if name.is_empty() {
            return Err(ModelError::EmptyName);
        }
        if self.find(name).is_some() {
            return Err(ModelError::NameTaken(name.into()));
        }
        Ok(())
    }

    /// `name` if free, else `name.001`, `name.002`… (Blender's suffix, as a
    /// copy gets in sand3d).
    fn unique(&self, name: &str) -> String {
        let mut out = name.to_string();
        let mut k = 0;
        while self.find(&out).is_some() {
            k += 1;
            out = format!("{name}.{k:03}");
        }
        out
    }

    fn push(&mut self, node: Node) -> Id {
        self.nodes.push(node);
        Id(self.nodes.len() as u32 - 1)
    }

    /// A part named `name` holding `solid`, its own space on the world's.
    pub fn add(&mut self, name: &str, solid: Solid) -> Result<Id, ModelError> {
        self.check_name(name)?;
        if solid.positions.iter().any(|p| !p.is_finite()) {
            return Err(ModelError::BadValue(format!("{name:?}'s positions")));
        }
        Ok(self.push(Node {
            name: name.into(),
            solid: Some(solid),
            placement: Frame::IDENTITY,
            parent: None,
            material: None,
            anchors: BTreeMap::new(),
            used_up: false,
            deleted: false,
        }))
    }

    /// Whether `inner` is `outer` or somewhere under it.
    fn under(&self, inner: Id, outer: Id) -> bool {
        let mut at = Some(inner);
        while let Some(i) = at {
            if i == outer {
                return true;
            }
            at = self.nodes[i.0 as usize].parent;
        }
        false
    }

    /// The live nodes under `id`, itself first, in id order below it.
    fn subtree(&self, id: Id) -> Vec<Id> {
        let mut out = vec![id];
        for i in 0..self.nodes.len() {
            let i = Id(i as u32);
            if i != id && !self.nodes[i.0 as usize].used_up && self.under(i, id) {
                out.push(i);
            }
        }
        out
    }

    /// A group named `name` holding `children`, which keep their places.
    /// A child already in a group moves to this one.
    pub fn group(&mut self, name: &str, children: &[Id]) -> Result<Id, ModelError> {
        self.check_name(name)?;
        for &c in children {
            self.node(c)?;
        }
        for (k, &a) in children.iter().enumerate() {
            for &b in &children[k + 1..] {
                if self.under(a, b) || self.under(b, a) {
                    return Err(ModelError::Refused(
                        self.nodes[a.0 as usize].name.clone(),
                        "a group holds a node and one of its own parts".into(),
                    ));
                }
            }
        }
        let id = self.push(Node {
            name: name.into(),
            solid: None,
            placement: Frame::IDENTITY,
            parent: None,
            material: None,
            anchors: BTreeMap::new(),
            used_up: false,
            deleted: false,
        });
        for &c in children {
            self.nodes[c.0 as usize].parent = Some(id);
        }
        Ok(id)
    }

    /// An assembly: a group that publishes named frames (`anchors`, in
    /// the world) for details and joints to use.
    pub fn assembly(
        &mut self,
        name: &str,
        children: &[Id],
        anchors: &[(String, Frame)],
    ) -> Result<Id, ModelError> {
        for (n, f) in anchors {
            finite_frame(f, &format!("anchor {n:?}"))?;
        }
        let id = self.group(name, children)?;
        self.nodes[id.0 as usize].anchors = anchors.iter().cloned().collect();
        Ok(id)
    }

    /// The anchor `name` of an assembly, where the assembly is now.
    pub fn anchor(&self, assembly: Id, name: &str) -> Result<Frame, ModelError> {
        let n = self.node(assembly)?;
        self.at_rest("anchor", &[assembly])?;
        let local = n.anchors.get(name).ok_or_else(|| {
            ModelError::Refused(
                n.name.clone(),
                format!(
                    "no anchor named {name:?}; anchors: {}",
                    n.anchors.keys().cloned().collect::<Vec<_>>().join(", ")
                ),
            )
        })?;
        Ok(n.placement.compose(local))
    }

    /// A detail: `children` grouped under `name`, the group put under
    /// `assembly`.
    pub fn attach(&mut self, assembly: Id, name: &str, children: &[Id]) -> Result<Id, ModelError> {
        self.node(assembly)?;
        for &c in children {
            if self.under(assembly, c) {
                return Err(ModelError::Refused(
                    self.nodes[c.0 as usize].name.clone(),
                    "a detail cannot hold the assembly it is attached to".into(),
                ));
            }
        }
        let id = self.group(name, children)?;
        self.nodes[id.0 as usize].parent = Some(assembly);
        Ok(id)
    }

    // ── placement ────────────────────────────────────────────────────────────

    /// Moves `id` and everything under it by the rigid motion `m` (world),
    /// with the pivots and axes of joints whose child is among them.
    fn move_by(&mut self, id: Id, m: &Frame) {
        let moved = self.subtree(id);
        for &i in &moved {
            let n = &mut self.nodes[i.0 as usize];
            n.placement = m.compose(&n.placement);
        }
        for j in &mut self.joints {
            if moved.contains(&j.child) {
                j.desc.on = m.compose(&j.desc.on);
                j.axis = j.axis.map(|a| m.vector_to_world(a));
                if j.desc.axis != DVec3::ZERO {
                    j.desc.axis = m.vector_to_world(j.desc.axis);
                }
            }
        }
    }

    /// Puts `id`'s own space on `frame` (world); what is under it moves
    /// with it. Use [`Frame::translate`], [`Frame::turn`] and
    /// [`Frame::offset`] on the frame for offsets.
    pub fn at(&mut self, id: Id, frame: &Frame) -> Result<(), ModelError> {
        finite_frame(frame, "at")?;
        let m = motion(&self.node(id)?.placement, frame);
        self.move_by(id, &m);
        Ok(())
    }

    /// Moves `id` (and what is under it) by `motion`, a rigid motion in
    /// the world: turned about the world origin, then moved.
    pub fn transform(&mut self, id: Id, motion: &Frame) -> Result<(), ModelError> {
        finite_frame(motion, "transform")?;
        self.node(id)?;
        self.move_by(id, motion);
        Ok(())
    }

    /// `id`'s placement: its own space in the world.
    pub fn placement(&self, id: Id) -> Result<Frame, ModelError> {
        let p = self.node(id)?.placement;
        self.at_rest("placement", &[id])?;
        Ok(p)
    }

    /// A frame given in `id`'s own space (where it was built), in the
    /// world: a source's joint origin, as written.
    pub fn local(&self, id: Id, frame: &Frame) -> Result<Frame, ModelError> {
        let p = self.node(id)?.placement;
        self.at_rest("local", &[id])?;
        Ok(p.compose(frame))
    }

    /// A part's solid in the world, at rest (whatever the pose).
    pub fn world_solid(&self, id: Id) -> Result<Solid, ModelError> {
        let n = self.node(id)?;
        let s = self.solid_of(id)?.clone();
        Ok(placed(s, &n.placement))
    }

    /// Every world position of the parts under `ids`.
    fn points(&self, ids: &[Id]) -> Result<Vec<DVec3>, ModelError> {
        let mut out = Vec::new();
        for &id in ids {
            self.node(id)?;
            for i in self.subtree(id) {
                let n = &self.nodes[i.0 as usize];
                if let Some(s) = &n.solid {
                    out.extend(s.positions.iter().map(|&p| n.placement.point_to_world(p)));
                }
            }
        }
        Ok(out)
    }

    /// The frame on the box bounding `ids` (and everything under them):
    /// `"+X"` a side's centre, `"+X-Y"` an edge's middle, `"+X-Y+Z"` a
    /// corner; its normal looks the first way named.
    pub fn bbox(&self, ids: &[Id], which: &str) -> Result<Frame, ModelError> {
        let points = self.points(ids)?;
        self.at_rest("bbox", ids)?;
        place::bbox(&points, which)
    }

    /// The frame on part `id`'s face named `which` (or furthest along a
    /// direction, `"+X"`): origin at the face's centroid, +Z along its
    /// normal ([`place::face`]).
    pub fn face(&self, id: Id, which: &str) -> Result<Frame, ModelError> {
        let s = self.world_solid(id)?;
        self.at_rest("face", &[id])?;
        place::face(&s, &self.nodes[id.0 as usize].name, which)
    }

    /// The edges of part `id` named `which`, as a path in the world
    /// ([`place::edge`]).
    pub fn edge(&self, id: Id, which: &str) -> Result<Path, ModelError> {
        let s = self.world_solid(id)?;
        self.at_rest("edge", &[id])?;
        place::edge(&s, &self.nodes[id.0 as usize].name, which)
    }

    /// Frames on the surface of the parts `targets` ([`place::snap`]).
    pub fn snap(
        &self,
        targets: &[Id],
        points: &[DVec3],
        direction: Option<DVec3>,
    ) -> Result<Vec<Frame>, ModelError> {
        let solids: Vec<Solid> = targets
            .iter()
            .map(|&t| self.world_solid(t))
            .collect::<Result<_, _>>()?;
        self.at_rest("snap", targets)?;
        let refs: Vec<&Solid> = solids.iter().collect();
        Ok(place::snap(&refs, points, direction))
    }

    /// The face and edge names part `id` carries, sorted.
    pub fn names(&self, id: Id) -> Result<(Vec<String>, Vec<String>), ModelError> {
        let s = self.solid_of(id)?;
        let faces: Vec<String> = s
            .face_tags
            .iter()
            .flatten()
            .map(|&t| s.names.name(t).to_string())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let edges: Vec<String> = s
            .edge_tags
            .values()
            .flatten()
            .map(|&t| s.names.name(t).to_string())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Ok((faces, edges))
    }

    // ── copies ───────────────────────────────────────────────────────────────

    /// Copies `id` and everything under it with the geometry each node
    /// gets from `geometry`. The copy's root takes `name`, the nodes under
    /// it their own names with a free suffix. Answers the copy's root and
    /// each copied node's id by the original's.
    fn duplicate(
        &mut self,
        id: Id,
        name: &str,
        geometry: impl Fn(&Node) -> (Option<Solid>, Frame),
    ) -> Result<(Id, BTreeMap<Id, Id>), ModelError> {
        self.node(id)?;
        self.check_name(name)?;
        let tree = self.subtree(id);
        let mut map: BTreeMap<Id, Id> = BTreeMap::new();
        for &old in &tree {
            let src = self.nodes[old.0 as usize].clone();
            let (solid, placement) = geometry(&src);
            let new_name = if old == id {
                name.to_string()
            } else {
                self.unique(&src.name)
            };
            let parent = if old == id {
                src.parent
            } else {
                src.parent.and_then(|p| map.get(&p).copied())
            };
            let new = self.push(Node {
                name: new_name,
                solid,
                placement,
                parent,
                material: src.material.clone(),
                anchors: src.anchors.clone(),
                used_up: false,
                deleted: false,
            });
            map.insert(old, new);
        }
        Ok((map[&id], map))
    }

    /// `name` if no joint has it, else `name.001`, `name.002`…
    fn unique_joint(&self, name: &str) -> String {
        let taken = |n: &str| self.joints.iter().any(|j| j.desc.name == n);
        let mut out = name.to_string();
        let mut k = 0;
        while taken(&out) {
            k += 1;
            out = format!("{name}.{k:03}");
        }
        out
    }

    /// Copies every joint whose child was copied (`map`: original id to
    /// copy), in declaration order, onto the copies: each named with a
    /// free suffix (`hinge.001`), hanging on the parent's copy when the
    /// parent was copied too and on the same parent otherwise, following
    /// its leader's copy when that was copied. `image` maps the pivot, the
    /// axis (world) and the explicit axis of a joint of kind `kind`.
    fn copy_joints(
        &mut self,
        map: &BTreeMap<Id, Id>,
        image: impl Fn(JointKind, &Frame, Option<DVec3>, DVec3) -> (Frame, Option<DVec3>, DVec3),
    ) {
        let mut names: BTreeMap<String, String> = BTreeMap::new();
        let copied: Vec<JointRec> = self
            .joints
            .iter()
            .filter(|j| map.contains_key(&j.child))
            .cloned()
            .collect();
        for j in copied {
            let name = self.unique_joint(&j.desc.name);
            names.insert(j.desc.name.clone(), name.clone());
            let parent = map.get(&j.parent).copied().unwrap_or(j.parent);
            let (on, axis, explicit) = image(j.desc.kind, &j.desc.on, j.axis, j.desc.axis);
            let mimic = j.desc.mimic.as_ref().map(|m| Mimic {
                joint: names.get(&m.joint).cloned().unwrap_or(m.joint.clone()),
                multiplier: m.multiplier,
            });
            let seat = on
                .inverse()
                .compose(&self.nodes[parent.0 as usize].placement);
            self.joints.push(JointRec {
                child: map[&j.child],
                parent,
                desc: JointDesc {
                    name,
                    on,
                    axis: explicit,
                    mimic,
                    ..j.desc
                },
                axis,
                value: j.value,
                seat,
            });
        }
    }

    /// A copy of `id` and everything under it, in the same place, named
    /// `name` (the nodes under it keep their names with a suffix:
    /// `.001`…). The joints of the parts and groups copied come with them
    /// at the same pose, named with a free suffix (`hinge.001`): a copied
    /// part hangs on its parent's copy when the parent was copied too, on
    /// the same parent otherwise; a follower follows its leader's copy
    /// when that was copied.
    pub fn copy(&mut self, id: Id, name: &str) -> Result<Id, ModelError> {
        let (root, map) = self.duplicate(id, name, |n| (n.solid.clone(), n.placement))?;
        self.copy_joints(&map, |_, on, axis, explicit| (*on, axis, explicit));
        Ok(root)
    }

    /// The mirror image of `id` and everything under it across `plane`'s
    /// XY plane (its normal is +Z), named `name`. Each part's image is
    /// built into its solid (a mirror is no placement), so the copy's
    /// parts sit with their own space on the world's.
    ///
    /// Joints come as [`Model::copy`] brings them, each moving as the
    /// image of the original's motion (sand3d: "the image of the
    /// motion"): the pivot's origin mirrored, its x and y mirrored and its
    /// z their cross product. A slide's axis is the axis's mirror image; a
    /// turn's is the image negated (a mirror reverses the sense of a
    /// turn). Ranges, values and mimics are kept, so the same value poses
    /// the copy as the image of the original at that value.
    pub fn mirror(&mut self, id: Id, plane: &Frame, name: &str) -> Result<Id, ModelError> {
        finite_frame(plane, "mirror")?;
        let (o, n) = (plane.origin, plane.z);
        let image = move |p: DVec3| p - n * (2.0 * (p - o).dot(n));
        let turn = move |v: DVec3| v - n * (2.0 * v.dot(n));
        let (root, map) = self.duplicate(id, name, |node| {
            let solid = node.solid.clone().map(|mut s| {
                let f = node.placement;
                s.map_positions(|p| image(f.point_to_world(p)), true);
                rounded(s)
            });
            (solid, Frame::IDENTITY)
        })?;
        self.copy_joints(&map, |kind, on, axis, explicit| {
            let x = turn(on.x);
            let y = turn(on.y);
            let pivot = Frame {
                origin: image(on.origin),
                x,
                y,
                z: x.cross(y),
            };
            // A turn's axis is a pseudovector: its image is negated. A
            // slide's is a plain direction; its default (the pivot's z)
            // is written out, since the new pivot's z is the negated one.
            let slide = kind == JointKind::Prismatic;
            let image_of = |v: DVec3| if slide { turn(v) } else { -turn(v) };
            let explicit = if explicit != DVec3::ZERO {
                image_of(explicit)
            } else if slide {
                turn(on.z)
            } else {
                DVec3::ZERO
            };
            (pivot, axis.map(image_of), explicit)
        });
        Ok(root)
    }

    // ── materials ────────────────────────────────────────────────────────────

    /// `material` on each node: a group's reaches every part under it
    /// that names none of its own (the nearest wins).
    pub fn material(&mut self, ids: &[Id], material: &Material) -> Result<(), ModelError> {
        self.material_mapped(ids, material, 1.0)
    }

    /// [`Model::material`] with its map (`material.map`, a base-colour
    /// texture) spanning `map_size` units of the parts' UVs, which are in
    /// the model's unit: export and measure divide the UVs of the parts
    /// it reaches by `map_size`, so the image covers that many units each
    /// way and repeats beyond. Refused: a size that is not positive, or a
    /// map on a part with no UVs.
    pub fn material_mapped(
        &mut self,
        ids: &[Id],
        material: &Material,
        map_size: f64,
    ) -> Result<(), ModelError> {
        if !(map_size.is_finite() && map_size > 0.0) {
            return Err(ModelError::BadValue(
                "mapSize (the units the map spans; positive)".into(),
            ));
        }
        let m = material;
        for v in m
            .color
            .iter()
            .flatten()
            .chain(&m.emissive)
            .chain(m.roughness.iter())
            .chain(m.opacity.iter())
            .chain([&m.metalness])
        {
            if !v.is_finite() {
                return Err(ModelError::BadValue("a material value".into()));
            }
        }
        for &id in ids {
            self.node(id)?;
        }
        if m.map.is_some() {
            for &id in ids {
                for i in self.subtree(id) {
                    let n = &self.nodes[i.0 as usize];
                    if n.solid.as_ref().is_some_and(|s| s.uvs.is_none()) {
                        return Err(ModelError::Refused(
                            n.name.clone(),
                            "a material with a map needs UVs on the parts it reaches: give the \
                             part `uvs: true` (or the model `uvs`) when it is made"
                                .into(),
                        ));
                    }
                }
            }
        }
        for &id in ids {
            self.nodes[id.0 as usize].material = Some((material.clone(), map_size));
        }
        Ok(())
    }

    fn material_of(&self, id: Id) -> Material {
        self.mapped_of(id).0
    }

    /// The nearest material and the size its map spans.
    fn mapped_of(&self, id: Id) -> (Material, f64) {
        let mut at = Some(id);
        while let Some(i) = at {
            let n = &self.nodes[i.0 as usize];
            if let Some(m) = &n.material {
                return m.clone();
            }
            at = n.parent;
        }
        (Material::default(), 1.0)
    }

    /// Asks for UVs on every part made from now on (`docs/design.md` §4):
    /// generators give them in the model's unit on the surface. Refused
    /// while a part without UVs exists: give it `uvs` itself when it is
    /// made, or ask first.
    pub fn ask_uvs(&mut self) -> Result<(), ModelError> {
        if let Some(n) = self
            .nodes
            .iter()
            .find(|n| !n.used_up && n.solid.as_ref().is_some_and(|s| s.uvs.is_none()))
        {
            return Err(ModelError::Refused(
                n.name.clone(),
                "it was made without UVs: ask for UVs before the first part, or give each part \
                 `uvs: true`"
                    .into(),
            ));
        }
        self.uvs = true;
        Ok(())
    }

    /// Whether parts get UVs unless they say otherwise ([`Model::ask_uvs`]).
    pub fn wants_uvs(&self) -> bool {
        self.uvs
    }

    /// UV measures of part `name` as exported: its UVs divided by its
    /// material's map size (`shape_tools::measure`). `None` for a part
    /// without UVs or no such part.
    pub fn uv_measures(&self, name: &str) -> Option<shape_tools::UvMeasures> {
        let id = self.find(name)?;
        let mut s = self.solid_of(id).ok()?.clone();
        let (material, size) = self.mapped_of(id);
        if material.map.is_some() && size != 1.0 {
            for q in s.uvs.as_mut()?.iter_mut().flatten() {
                *q /= size;
            }
        }
        shape_tools::measure(&s, &shape_tools::Measure::default()).uv
    }

    // ── edits ────────────────────────────────────────────────────────────────

    /// Replaces part `id`'s solid with `f` of it (in its own space).
    fn edit(
        &mut self,
        id: Id,
        f: impl FnOnce(&Solid, &Frame) -> Result<Solid, ModelError>,
    ) -> Result<(), ModelError> {
        let placement = self.node(id)?.placement;
        let s = f(self.solid_of(id)?, &placement)?;
        self.nodes[id.0 as usize].solid = Some(s);
        Ok(())
    }

    /// Why `tool` may not be used up.
    fn usable(&self, tool: Id, target: Id) -> Result<(), ModelError> {
        let n = self.node(tool)?;
        if tool == target {
            return Err(ModelError::Refused(
                n.name.clone(),
                "a part cannot be its own tool".into(),
            ));
        }
        if self
            .nodes
            .iter()
            .any(|m| !m.used_up && m.parent == Some(tool))
        {
            return Err(ModelError::Refused(
                n.name.clone(),
                "it holds other nodes; a tool is a single part".into(),
            ));
        }
        if let Some(j) = self
            .joints
            .iter()
            .find(|j| j.child == tool || j.parent == tool)
        {
            return Err(ModelError::Refused(
                n.name.clone(),
                format!(
                    "joint {:?} needs it; seat cuts a cavity and leaves the part",
                    j.desc.name
                ),
            ));
        }
        Ok(())
    }

    /// `tool`'s solid in `target`'s own space.
    fn tool_in(&self, tool: Id, target: Id) -> Result<Solid, ModelError> {
        let w = self.world_solid(tool)?;
        Ok(placed(w, &self.node(target)?.placement.inverse()))
    }

    /// `target` combined with `tool` by `d.op` ([`shape_bool::boolean_report`]);
    /// the tool is used up. Answers what the result alone does not say
    /// (voids left, edges where the operands touch), which the model also
    /// keeps ([`Model::booleans`]).
    pub fn combine(&mut self, target: Id, tool: Id, d: &Combine) -> Result<BoolReport, ModelError> {
        self.solid_of(target)?;
        self.solid_of(tool)?;
        self.usable(tool, target)?;
        let mut t = self.tool_in(tool, target)?;
        if d.op == BoolOp::Cut {
            // The cut turns the tool's faces inside out; its UVs would
            // read mirrored from the cavity (`shape_edit::cut_tool_uvs`).
            t = shape_edit::cut_tool_uvs(&t);
        }
        let (tn, un) = (
            self.nodes[target.0 as usize].name.clone(),
            self.nodes[tool.0 as usize].name.clone(),
        );
        let mut report = BoolReport::default();
        self.edit(target, |s, _| {
            if d.check {
                shape_bool::check_operands(s, &t)?;
            }
            let (out, r) = shape_bool::boolean_report(
                s,
                &t,
                &Boolean {
                    op: d.op,
                    target: &tn,
                    tool: &un,
                },
            )?;
            report = r;
            Ok(out)
        })?;
        self.nodes[tool.0 as usize].used_up = true;
        let op = match d.op {
            BoolOp::Cut => "cut",
            BoolOp::Join => "join",
            BoolOp::Intersect => "intersect",
        };
        self.booleans.push(BoolRecord {
            op: op.into(),
            target: tn,
            tool: un,
            report: report.clone(),
        });
        Ok(report)
    }

    /// Every boolean [`Model::combine`] (and so `cut`, `join`,
    /// `intersect`) ran, in order, with its report.
    pub fn booleans(&self) -> &[BoolRecord] {
        &self.booleans
    }

    /// `target` minus `tool`; the tool is used up. Names as
    /// [`shape_bool::boolean`] gives them (`cut`, `<target>.cut`, the
    /// tool's name).
    pub fn cut(&mut self, target: Id, tool: Id) -> Result<BoolReport, ModelError> {
        self.combine(
            target,
            tool,
            &Combine {
                op: BoolOp::Cut,
                ..Default::default()
            },
        )
    }

    /// `target` joined with `tool`; the tool is used up.
    pub fn join(&mut self, target: Id, tool: Id) -> Result<BoolReport, ModelError> {
        self.combine(target, tool, &Combine::default())
    }

    /// `target` intersected with `tool`; the tool is used up.
    pub fn intersect(&mut self, target: Id, tool: Id) -> Result<BoolReport, ModelError> {
        self.combine(
            target,
            tool,
            &Combine {
                op: BoolOp::Intersect,
                ..Default::default()
            },
        )
    }

    /// `tool`'s solid added to `target`'s as it is: no boolean, so shells
    /// that touch or cross stay as they are ([`shape_edit::merge`]); faces
    /// and edges keep their names. The tool is used up.
    pub fn merge(&mut self, target: Id, tool: Id) -> Result<(), ModelError> {
        self.solid_of(target)?;
        self.solid_of(tool)?;
        self.usable(tool, target)?;
        let t = self.tool_in(tool, target)?;
        if t.uvs.is_some() != self.solid_of(target)?.uvs.is_some() {
            let without = if t.uvs.is_none() { tool } else { target };
            return Err(ModelError::Refused(
                self.nodes[without.0 as usize].name.clone(),
                "it has no UVs and the part it would merge with has: give both UVs, or neither"
                    .into(),
            ));
        }
        self.edit(target, |s, _| Ok(shape_edit::merge(&[s, &t])))?;
        self.nodes[tool.0 as usize].used_up = true;
        Ok(())
    }

    /// Removes `id` and everything under it. Joints whose child is
    /// removed go with it. Refused, changing nothing: a joint that hangs a
    /// node that stays on a node removed, or one that follows (`mimic`) a
    /// joint removed.
    pub fn delete(&mut self, id: Id) -> Result<(), ModelError> {
        let name = self.node(id)?.name.clone();
        let gone: BTreeSet<Id> = self.subtree(id).into_iter().collect();
        let dropped: BTreeSet<String> = self
            .joints
            .iter()
            .filter(|j| gone.contains(&j.child))
            .map(|j| j.desc.name.clone())
            .collect();
        for j in self.joints.iter().filter(|j| !gone.contains(&j.child)) {
            if gone.contains(&j.parent) {
                return Err(ModelError::Refused(
                    name,
                    format!(
                        "joint {:?} hangs {:?} on {:?}; delete {:?} too",
                        j.desc.name,
                        self.nodes[j.child.0 as usize].name,
                        self.nodes[j.parent.0 as usize].name,
                        self.nodes[j.child.0 as usize].name,
                    ),
                ));
            }
            if let Some(m) = j.desc.mimic.as_ref().filter(|m| dropped.contains(&m.joint)) {
                return Err(ModelError::Refused(
                    name,
                    format!(
                        "joint {:?} follows joint {:?}, which would go",
                        j.desc.name, m.joint
                    ),
                ));
            }
        }
        self.joints.retain(|j| !gone.contains(&j.child));
        for i in gone {
            let n = &mut self.nodes[i.0 as usize];
            n.used_up = true;
            n.deleted = true;
        }
        Ok(())
    }

    /// The volume `part` fills, taken out of `host`; `part` is untouched
    /// ([`shape_edit::seat`]).
    pub fn seat(&mut self, host: Id, part: Id) -> Result<(), ModelError> {
        if host == part {
            return Err(ModelError::Refused(
                self.node(host)?.name.clone(),
                "a part cannot seat itself".into(),
            ));
        }
        let t = self.tool_in(part, host)?;
        let name = self.node(part)?.name.clone();
        self.edit(host, |s, _| Ok(shape_edit::seat(s, &t, &name)?))
    }

    /// Scales part `id`'s solid along its own axes, about its own origin,
    /// by `factors` (each positive; a mirror is [`Model::mirror`]). Its
    /// placement, names and joints stay; a group is refused.
    pub fn scale(&mut self, id: Id, factors: DVec3) -> Result<(), ModelError> {
        if !(factors.is_finite() && factors.min_element() > 0.0) {
            return Err(ModelError::BadValue(
                "a scale factor (each must be positive)".into(),
            ));
        }
        self.edit(id, |s, _| {
            let mut s = s.clone();
            s.map_positions(|p| p * factors, false);
            s.canonicalize()
                .map_err(|_| ModelError::BadValue("the scaled positions".into()))?;
            Ok(s)
        })
    }

    /// Holes ([`shape_edit::hole`]); `d.at` frames are in the world.
    pub fn hole(&mut self, id: Id, d: &Hole) -> Result<(), ModelError> {
        self.edit(id, |s, p| {
            let inv = p.inverse();
            let local = Hole {
                at: d.at.iter().map(|f| inv.compose(f)).collect(),
                ..d.clone()
            };
            Ok(shape_edit::hole(s, &local)?)
        })
    }

    /// A pocket ([`shape_edit::pocket`]); `d.at` frames are in the world.
    pub fn pocket(&mut self, id: Id, d: &Pocket) -> Result<(), ModelError> {
        self.edit(id, |s, p| {
            let inv = p.inverse();
            let at = if d.at.is_empty() {
                vec![inv]
            } else {
                d.at.iter().map(|f| inv.compose(f)).collect()
            };
            let local = Pocket { at, ..d.clone() };
            Ok(shape_edit::pocket(s, &local)?)
        })
    }

    /// The part cut by the plane of `frame` (world), keeping one side.
    pub fn split(&mut self, id: Id, frame: &Frame, keep: Keep) -> Result<(), ModelError> {
        finite_frame(frame, "split")?;
        self.edit(id, |s, p| {
            Ok(shape_edit::split(s, &p.inverse().compose(frame), keep)?)
        })
    }

    /// Hollows the part ([`shape_edit::shell`]); directions in `d.open`
    /// are the part's own axes.
    pub fn shell(&mut self, id: Id, d: &Shell) -> Result<(), ModelError> {
        self.edit(id, |s, _| Ok(shape_edit::shell(s, d)?))
    }

    /// Dents ([`shape_edit::dent`]); `d.at` points are in the world.
    pub fn dent(&mut self, id: Id, d: &Dent) -> Result<(), ModelError> {
        self.edit(id, |s, p| {
            let inv = p.inverse();
            let local = Dent {
                at: d.at.iter().map(|&q| inv.point_to_world(q)).collect(),
                ..d.clone()
            };
            Ok(shape_edit::dent(s, &local)?)
        })
    }

    /// Fillets on named edges ([`shape_bevel::fillet`]).
    pub fn fillet(&mut self, id: Id, d: &Fillet) -> Result<(), ModelError> {
        self.edit(id, |s, _| Ok(shape_bevel::fillet(s, d)?))
    }

    /// Chamfers on named edges ([`shape_bevel::chamfer`]).
    pub fn chamfer(&mut self, id: Id, d: &Chamfer) -> Result<(), ModelError> {
        self.edit(id, |s, _| Ok(shape_bevel::chamfer(s, d)?))
    }

    /// Softened edges ([`shape_bevel::soften`]).
    pub fn soften(&mut self, id: Id, d: &Soften) -> Result<(), ModelError> {
        self.edit(id, |s, _| Ok(shape_bevel::soften(s, d)?))
    }

    // ── joints ───────────────────────────────────────────────────────────────

    /// The jointed node `id` moves with: its nearest jointed
    /// ancestor-or-self, or `None` for the root body.
    fn body(&self, id: Id) -> Option<Id> {
        let mut at = Some(id);
        while let Some(i) = at {
            if self.joints.iter().any(|j| j.child == i) {
                return Some(i);
            }
            at = self.nodes[i.0 as usize].parent;
        }
        None
    }

    fn joint_index(&self, name: &str) -> Result<usize, ModelError> {
        self.joints
            .iter()
            .position(|j| j.desc.name == name)
            .ok_or_else(|| {
                ModelError::NoSuchJoint(
                    name.into(),
                    self.joints.iter().map(|j| j.desc.name.clone()).collect(),
                )
            })
    }

    /// Whether the joint at `k` turns without end (continuous, or a turn
    /// following one).
    fn endless(&self, k: usize) -> bool {
        let j = &self.joints[k];
        match &j.desc.mimic {
            None => j.desc.kind == JointKind::Continuous,
            Some(m) => {
                j.desc.kind != JointKind::Prismatic
                    && self.joint_index(&m.joint).is_ok_and(|l| self.endless(l))
            }
        }
    }

    /// Declares how `child` (a part or a group) moves on `parent` (a part,
    /// or a group). It does not move the child: place it first, at rest.
    /// A second joint on one child must hang on the same parent; it rides
    /// on the first one's motion.
    pub fn joint(&mut self, child: Id, parent: Id, d: &JointDesc) -> Result<(), ModelError> {
        let name = &d.name;
        let err = |why: &str| Err(ModelError::Joint(name.clone(), why.into()));
        if name.is_empty() {
            return Err(ModelError::EmptyName);
        }
        let cn = self.node(child)?.name.clone();
        let pn = self.node(parent)?.name.clone();
        if self.joints.iter().any(|j| &j.desc.name == name) {
            return err("a joint already has this name");
        }
        finite_frame(&d.on, "the joint's frame `on`")?;
        if !d.axis.is_finite() {
            return Err(ModelError::BadValue(format!("joint {name:?}'s axis")));
        }
        if self.under(parent, child) {
            return err(&format!(
                "its parent {pn:?} is {cn:?} or inside it; hang it on the part it moves on"
            ));
        }
        if let Some(j) = self.joints.iter().find(|j| j.child == child)
            && j.parent != parent
        {
            return err(&format!(
                "{cn:?} already hangs on {:?}; every joint of one child hangs on the same parent",
                self.nodes[j.parent.0 as usize].name
            ));
        }
        // No cycle: walk up from the parent's body through the joints.
        let mut b = self.body(parent);
        let mut seen = BTreeSet::new();
        while let Some(x) = b {
            let above = self.joints.iter().find(|j| j.child == x).map(|j| j.parent);
            if self.under(x, child) || above.is_some_and(|p| self.under(p, child)) {
                return err(&format!("{pn:?} already hangs on {cn:?}: a cycle"));
            }
            if !seen.insert(x) {
                break;
            }
            b = above.and_then(|p| self.body(p));
        }
        let kind = d.kind;
        let free = matches!(kind, JointKind::Fixed | JointKind::Floating);
        if free && (d.axis != DVec3::ZERO || d.range.is_some() || d.mimic.is_some()) {
            return err("fixed and floating joints take no axis, range or mimic");
        }
        let axis = if free {
            None
        } else if d.axis == DVec3::ZERO {
            Some(d.on.z)
        } else {
            Some(
                d.axis
                    .try_normalize()
                    .ok_or_else(|| ModelError::Joint(name.clone(), "its axis is zero".into()))?,
            )
        };
        if let Some(m) = &d.mimic {
            if d.range.is_some() {
                return err("a follower takes no range: its range is its leader's × multiplier");
            }
            if !m.multiplier.is_finite() || m.multiplier == 0.0 {
                return err("the mimic's multiplier must be finite and not 0");
            }
            let l = self.joint_index(&m.joint)?;
            if matches!(
                self.joints[l].desc.kind,
                JointKind::Fixed | JointKind::Floating
            ) {
                return err("it follows a fixed or floating joint, which has no value");
            }
            if kind == JointKind::Prismatic && self.endless(l) {
                return err("a slide that follows an endless turn would slide without end");
            }
        } else {
            match (kind, d.range) {
                (JointKind::Revolute | JointKind::Prismatic, None) => {
                    return err(
                        "a revolute or prismatic joint needs a range (continuous turns without end)",
                    );
                }
                (JointKind::Continuous, Some(_)) => {
                    return err("a continuous joint turns without end: it takes no range");
                }
                (_, Some((lo, hi))) => {
                    if !(lo.is_finite() && hi.is_finite()) {
                        return Err(ModelError::BadValue(format!("joint {name:?}'s range")));
                    }
                    if !(lo < hi && lo <= 0.0 && 0.0 <= hi) {
                        return err("its range must run from low to high and contain 0, the rest");
                    }
                }
                _ => {}
            }
        }
        let seat =
            d.on.inverse()
                .compose(&self.nodes[parent.0 as usize].placement);
        self.joints.push(JointRec {
            child,
            parent,
            desc: d.clone(),
            axis,
            value: 0.0,
            seat,
        });
        Ok(())
    }

    /// Poses joint `name` at `value` (radians for turns, the unit for
    /// slides), within its range. Followers move with it; 0 is the rest.
    /// Export, preview and [`Model::measure_posed`] show the pose; frames
    /// are not read off it ([`ModelError::Posed`]).
    pub fn pose(&mut self, name: &str, value: f64) -> Result<(), ModelError> {
        let k = self.joint_index(name)?;
        if !value.is_finite() {
            return Err(ModelError::BadValue(format!("pose {name:?}")));
        }
        let j = &self.joints[k];
        if let Some(m) = &j.desc.mimic {
            return Err(ModelError::Joint(
                name.into(),
                format!("it follows {:?}: pose that one", m.joint),
            ));
        }
        if matches!(j.desc.kind, JointKind::Fixed | JointKind::Floating) {
            return Err(ModelError::Joint(
                name.into(),
                "a fixed or floating joint has no value to pose".into(),
            ));
        }
        if let Some((lo, hi)) = j.desc.range {
            let slack = 1e-9 * lo.abs().max(hi.abs()).max(1.0);
            if !(lo - slack <= value && value <= hi + slack) {
                return Err(ModelError::OutOfRange {
                    joint: name.into(),
                    value,
                    range: (lo, hi),
                });
            }
        }
        self.joints[k].value = value;
        Ok(())
    }

    /// The joints' names, in declaration order.
    pub fn joint_names(&self) -> Vec<String> {
        self.joints.iter().map(|j| j.desc.name.clone()).collect()
    }

    /// The value of the joint at `k`: its own, or a follower's leader's ×
    /// its multiplier.
    fn value_of(&self, k: usize) -> f64 {
        let j = &self.joints[k];
        match &j.desc.mimic {
            None => j.value,
            Some(m) => self
                .joint_index(&m.joint)
                .map_or(0.0, |l| self.value_of(l) * m.multiplier),
        }
    }

    /// The pose: every joint that can be posed (not a follower, fixed or
    /// floating) with its value, in declaration order.
    pub fn pose_values(&self) -> Vec<(String, f64)> {
        self.joints
            .iter()
            .filter(|j| {
                j.desc.mimic.is_none()
                    && !matches!(j.desc.kind, JointKind::Fixed | JointKind::Floating)
            })
            .map(|j| (j.desc.name.clone(), j.value))
            .collect()
    }

    /// Whether any joint has a value other than 0: export writes OBJ and
    /// STL posed then ([`Model::obj`]).
    pub fn posed(&self) -> bool {
        self.joints.iter().any(|j| j.value != 0.0)
    }

    /// The first joint (nearest the node, then up through its parents)
    /// with a non-zero value that moves node `id`.
    fn moved_by(&self, id: Id) -> Option<usize> {
        let mut seen = BTreeSet::new();
        let mut at = self.body(id);
        while let Some(b) = at {
            if !seen.insert(b) {
                break;
            }
            let chain: Vec<usize> = (0..self.joints.len())
                .filter(|&k| self.joints[k].child == b)
                .collect();
            if let Some(&k) = chain.iter().rev().find(|&&k| self.value_of(k) != 0.0) {
                return Some(k);
            }
            at = chain
                .first()
                .and_then(|&k| self.body(self.joints[k].parent));
        }
        None
    }

    /// Refuses a read (`what`) of nodes the pose moves: every frame the
    /// model gives is at rest.
    fn at_rest(&self, what: &str, ids: &[Id]) -> Result<(), ModelError> {
        for &id in ids {
            for i in self.subtree(id) {
                if let Some(k) = self.moved_by(i) {
                    return Err(ModelError::Posed {
                        read: what.into(),
                        part: self.nodes[i.0 as usize].name.clone(),
                        joint: self.joints[k].desc.name.clone(),
                        value: self.value_of(k),
                    });
                }
            }
        }
        Ok(())
    }

    /// How each joint holds, at rest, in declaration order ([`Seating`]):
    /// the pivot's distance to the parent's and the child's surfaces, and
    /// how far the parent has moved off the pivot since the joint was
    /// declared. A joint frame read on another part than the parent, or a
    /// parent moved after, shows here; nothing is refused.
    pub fn seating(&self) -> Result<Vec<Seating>, ModelError> {
        let nearest = |ids: &[Id], p: DVec3| -> Result<Option<f64>, ModelError> {
            let solids: Vec<Solid> = ids
                .iter()
                .map(|&i| self.world_solid(i))
                .collect::<Result<_, _>>()?;
            let refs: Vec<&Solid> = solids.iter().collect();
            Ok(place::snap(&refs, &[p], None)
                .first()
                .map(|f| (f.origin - p).length()))
        };
        let live = |root: Id, not: Option<Id>| -> Vec<Id> {
            self.subtree(root)
                .into_iter()
                .filter(|&i| {
                    let n = &self.nodes[i.0 as usize];
                    !n.used_up && n.solid.is_some() && not.is_none_or(|c| !self.under(i, c))
                })
                .collect()
        };
        let mut out = Vec::with_capacity(self.joints.len());
        for (k, j) in self.joints.iter().enumerate() {
            let now = j
                .desc
                .on
                .inverse()
                .compose(&self.nodes[j.parent.0 as usize].placement);
            // The turn between the two: ‖R − I‖ = 2√2 sin(θ/2), exact
            // enough near 0 where acos is not.
            let d = [now.x - j.seat.x, now.y - j.seat.y, now.z - j.seat.z];
            let norm = d.iter().map(|v| v.length_squared()).sum::<f64>().sqrt();
            let half = (norm / (2.0 * 2.0f64.sqrt())).min(1.0);
            out.push(Seating {
                joint: j.desc.name.clone(),
                child: self.nodes[j.child.0 as usize].name.clone(),
                parent: self.nodes[j.parent.0 as usize].name.clone(),
                value: self.value_of(k),
                pivot_to_parent: nearest(&live(j.parent, Some(j.child)), j.desc.on.origin)?,
                pivot_to_child: nearest(&live(j.child, None), j.desc.on.origin)?,
                parent_moved: (now.origin - j.seat.origin).length(),
                parent_turned: 2.0 * shape_core::trig::asin(half),
            });
        }
        Ok(out)
    }

    // ── building ─────────────────────────────────────────────────────────────

    /// Every live part in the world at rest, in id order.
    fn parts(&self) -> Vec<Id> {
        (0..self.nodes.len() as u32)
            .map(Id)
            .filter(|&i| {
                let n = &self.nodes[i.0 as usize];
                !n.used_up && n.solid.is_some()
            })
            .collect()
    }

    /// Measures of every part in the world at rest, in id order.
    pub fn measure(&self) -> Result<Vec<PartMeasures>, ModelError> {
        self.parts()
            .into_iter()
            .map(|i| {
                Ok(PartMeasures {
                    name: self.nodes[i.0 as usize].name.clone(),
                    measures: self.world_solid(i)?.measure(),
                })
            })
            .collect()
    }

    /// Measures of every part in the world at the pose (each joint at its
    /// value), in id order, in the model's unit. A part no non-zero joint
    /// value moves measures as at rest, bit for bit; a moved part as moved
    /// (`shape_tools::posed`): counts, area and closure are the rest's,
    /// bounds the pose's, and the volume may differ in its last bits.
    pub fn measure_posed(&self) -> Result<Vec<PartMeasures>, ModelError> {
        if !self.posed() {
            return self.measure();
        }
        let built = self.build()?;
        let solids = shape_tools::posed(&built.scene())?;
        let k = self.unit.millimetres();
        let mut out = Vec::new();
        for i in self.parts() {
            let n = &self.nodes[i.0 as usize];
            let measures = if self.moved_by(i).is_none() {
                self.world_solid(i)?.measure()
            } else {
                let at = built
                    .parts
                    .iter()
                    .position(|p| p.0 == n.name)
                    .ok_or_else(|| ModelError::Refused(n.name.clone(), "not built".into()))?;
                let mut s = solids[at].clone();
                if k != 1.0 {
                    s.map_positions(|p| p / k, false);
                }
                s.measure()
            };
            out.push(PartMeasures {
                name: n.name.clone(),
                measures,
            });
        }
        Ok(out)
    }

    /// Pairs of parts that pass through each other, touch, or hold one
    /// wholly inside the other (`shape_tools::interference`), with the
    /// joints in `pose` at their values and every other joint at rest
    /// (an empty pose is the rest). Bounds are in the model's unit.
    ///
    /// The model is not changed. Errors as [`Model::pose`] on a joint that
    /// does not exist, follows another, or is posed out of its range.
    pub fn interference(&self, pose: &[(String, f64)]) -> Result<Vec<Interfering>, ModelError> {
        let mut m = self.clone();
        for j in &mut m.joints {
            j.value = 0.0;
        }
        for (name, value) in pose {
            m.pose(name, *value)?;
        }
        let k = self.unit.millimetres();
        let mut out = shape_tools::interference(&m.build()?.scene())?;
        if k != 1.0 {
            for x in &mut out {
                x.bounds_min /= k;
                x.bounds_max /= k;
            }
        }
        Ok(out)
    }

    /// The scene part a joint hanging on node `p` hangs on: a part's own,
    /// a jointed group's link, or the body it moves with ("" for the
    /// root).
    fn representative(&self, p: Id) -> String {
        let n = &self.nodes[p.0 as usize];
        if n.solid.is_some() || self.joints.iter().any(|j| j.child == p) {
            return n.name.clone();
        }
        match self.body(p) {
            Some(b) => self.nodes[b.0 as usize].name.clone(),
            None => String::new(),
        }
    }

    /// The model as an export scene: every live part in the world at rest
    /// (millimetres), each hanging on the body it moves with.
    ///
    /// - A jointed part carries its joint; a jointed group becomes a
    ///   part with no mesh, named after it, which its parts hang on.
    /// - A child with several joints gets one link per joint before the
    ///   last, named `<child>:<joint>` and with no mesh, chained from the
    ///   parent, the first nearest it.
    /// - Joint values are the pose; a follower's comes from its leader.
    pub fn build(&self) -> Result<Built, ModelError> {
        let k = self.unit.millimetres();
        let mm = |f: &Frame| Frame {
            origin: f.origin * k,
            ..*f
        };
        // (name, solid, material, the name it hangs on, joint).
        type Item = (String, Solid, Material, String, Option<shape_tools::Joint>);
        let to_scene = |j: &JointRec| -> shape_tools::Joint {
            let pivot = mm(&j.desc.on);
            let si = |kind: JointKind| if kind == JointKind::Prismatic { k } else { 1.0 };
            let s = si(j.desc.kind);
            shape_tools::Joint {
                name: j.desc.name.clone(),
                kind: j.desc.kind,
                pivot,
                axis: j.axis.map_or(DVec3::ZERO, |a| pivot.vector_to_local(a)),
                value: j.value * s,
                range: j.desc.range.map(|(lo, hi)| (lo * s, hi * s)),
                mimic: j.desc.mimic.as_ref().map(|m| {
                    let lead = self
                        .joints
                        .iter()
                        .find(|l| l.desc.name == m.joint)
                        .map_or(JointKind::Revolute, |l| l.desc.kind);
                    Mimic {
                        joint: m.joint.clone(),
                        multiplier: m.multiplier * s / si(lead),
                    }
                }),
            }
        };
        // Every scene entry with the name of the entry it hangs on, then
        // ordered parents first by passes in a fixed order.
        let mut entries: Vec<Item> = Vec::new();
        let mut bodies: Vec<Id> = self.joints.iter().map(|j| j.child).collect();
        bodies.sort();
        bodies.dedup();
        for &b in &bodies {
            let chain: Vec<&JointRec> = self.joints.iter().filter(|j| j.child == b).collect();
            let bn = self.nodes[b.0 as usize].name.clone();
            let mut hang = self.representative(chain[0].parent);
            for (i, j) in chain.iter().enumerate() {
                let last = i + 1 == chain.len();
                let (name, solid, material) = if last {
                    let solid = if self.nodes[b.0 as usize].solid.is_some() {
                        self.scaled(b, k)?
                    } else {
                        Solid::default()
                    };
                    (bn.clone(), solid, self.material_of(b))
                } else {
                    (
                        format!("{bn}:{}", j.desc.name),
                        Solid::default(),
                        Material::default(),
                    )
                };
                entries.push((name.clone(), solid, material, hang, Some(to_scene(j))));
                hang = name;
            }
        }
        for p in self.parts() {
            if bodies.contains(&p) {
                continue;
            }
            let above = self
                .body(p)
                .map(|x| self.nodes[x.0 as usize].name.clone())
                .unwrap_or_default();
            entries.push((
                self.nodes[p.0 as usize].name.clone(),
                self.scaled(p, k)?,
                self.material_of(p),
                above,
                None,
            ));
        }
        let mut out: Vec<Item> = Vec::with_capacity(entries.len());
        let mut emitted: BTreeSet<String> = BTreeSet::new();
        let mut left: Vec<Option<Item>> = entries.into_iter().map(Some).collect();
        while out.len() < left.len() {
            let before = out.len();
            for slot in &mut left {
                let ready = slot
                    .as_ref()
                    .is_some_and(|e| e.3.is_empty() || emitted.contains(&e.3));
                if ready && let Some(e) = slot.take() {
                    emitted.insert(e.0.clone());
                    out.push(e);
                }
            }
            if out.len() == before {
                return Err(ModelError::Joint(
                    String::new(),
                    "the joints hang parts on each other in a cycle".into(),
                ));
            }
        }
        Ok(Built {
            name: self.name.clone(),
            parts: out,
        })
    }

    fn scaled(&self, id: Id, k: f64) -> Result<Solid, ModelError> {
        let mut s = self.world_solid(id)?;
        if k != 1.0 {
            s.map_positions(|p| p * k, false);
            s = rounded(s);
        }
        // A map spanning `size` units: UVs in images, not units.
        let (material, size) = self.mapped_of(id);
        if material.map.is_some()
            && size != 1.0
            && let Some(uvs) = &mut s.uvs
        {
            for q in uvs.iter_mut().flatten() {
                *q /= size;
            }
        }
        Ok(s)
    }

    /// The model as GLB, posed (`shape_tools::glb`): metres, Y up, joints
    /// as `s3d_joint` extras.
    pub fn glb(&self) -> Result<Vec<u8>, ModelError> {
        Ok(shape_tools::glb(&self.build()?.scene())?)
    }

    /// Where [`Model::obj`] and [`Model::stl`] put the parts: at the pose
    /// when any joint has a value other than 0, else at rest.
    pub fn mesh_placement(&self) -> Placement {
        if self.posed() {
            Placement::Pose
        } else {
            Placement::Rest
        }
    }

    /// The model's meshes as OBJ text, at its pose when one is set (the
    /// header then ends `posed`; [`Model::mesh_placement`]), else at rest.
    /// Parts no joint moves keep their rest positions bit for bit.
    pub fn obj(&self) -> Result<String, ModelError> {
        Ok(shape_tools::obj_at(
            &self.build()?.scene(),
            self.mesh_placement(),
        )?)
    }

    /// The model's meshes as binary STL, at its pose when one is set (the
    /// header then ends `posed`), else at rest.
    pub fn stl(&self) -> Result<Vec<u8>, ModelError> {
        Ok(shape_tools::stl_at(
            &self.build()?.scene(),
            self.mesh_placement(),
        )?)
    }

    /// A turntable of the posed model (`shape_tools::preview`).
    pub fn preview(&self, d: &Preview) -> Result<Vec<Image>, ModelError> {
        Ok(shape_tools::preview(&self.build()?.scene(), d)?)
    }
}
