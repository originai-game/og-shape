//! Booleans on named solids, carrying names through
//! (`docs/design.md` §4, `docs/research.md` §1.3).
//!
//! - Every output face keeps its operand's face names, and with an operand
//!   name also `<name>.<tag>`. Faces a cutting tool leaves get `cut`.
//! - Edges where the operands meet get `cut`, `<target>.cut`, the tool's
//!   name, and the cap names (`top`, `bottom`, `start`, `end`) of the
//!   faces they bound.
//! - An input edge that survives, whole or in pieces, keeps its names.
//! - A named edge left between coplanar faces (within 0.5°) loses them.
//!
//! An operand whose surface passes through itself is refused
//! ([`BoolError::SelfIntersecting`]): by [`check_operands`] before any
//! work, or by [`boolean`] when its result comes out open. [`self_union`]
//! untangles one.

use std::collections::BTreeMap;

use shape_core::DVec3;
use shape_mesh::{EdgeKey, Solid, edge_key};

use crate::csg::{CsgOp, OperandError, csg_points, self_union_points};
use crate::selfcheck;

/// The operation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoolOp {
    /// A ∪ B.
    #[default]
    Join,
    /// A − B.
    Cut,
    /// A ∩ B.
    Intersect,
}

/// A boolean between a target and a tool.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Boolean<'a> {
    /// The operation.
    pub op: BoolOp,
    /// The target's part name, or "" for none.
    pub target: &'a str,
    /// The tool's name, or "" for none.
    pub tool: &'a str,
}

/// Why a boolean could not be honoured.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoolError {
    /// An operand is open, not oriented or not manifold ("target"/"tool").
    NotClosed(&'static str),
    /// An operand has a position that is not finite.
    NotFinite(&'static str),
    /// The result has no triangles (e.g. an intersection of disjoint solids).
    Empty,
    /// An operand is skinned ("target"/"tool"): booleans do not carry
    /// skins yet, and never drop one silently.
    Skinned(&'static str),
    /// The result came out open: `boundary` edges are not matched the
    /// other way. Both operands were closed and oriented, so one of them
    /// most likely passes through itself (`docs/research.md` §5.4). The
    /// open result is never returned.
    OpenResult {
        /// Directed edges with no twin.
        boundary: usize,
    },
    /// An operand ("target"/"tool") passes through itself: `pairs` pairs
    /// of its triangles, within one shell, cross where its surface does
    /// not join them (decided exactly, [`crossing_pairs`]). Found by
    /// [`check_operands`] before any work, or by [`boolean`] when the
    /// result came out open; [`self_union`] untangles such a shell.
    SelfIntersecting {
        /// Which operand.
        operand: &'static str,
        /// Crossing pairs of triangles.
        pairs: usize,
    },
}

impl std::fmt::Display for BoolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoolError::NotClosed(w) => write!(
                f,
                "the {w} is not a closed, oriented solid; repair it first"
            ),
            BoolError::NotFinite(w) => write!(f, "the {w} has a position that is not finite"),
            BoolError::Empty => f.write_str("the result is empty"),
            BoolError::OpenResult { boundary } => write!(
                f,
                "the result is open ({boundary} unmatched edges): an operand most likely passes through itself; repair or fuse its parts first"
            ),
            BoolError::SelfIntersecting { operand, pairs } => write!(
                f,
                "the {operand} passes through itself ({pairs} crossing triangle pairs); untangle it with self_union first"
            ),
            BoolError::Skinned(w) => write!(
                f,
                "the {w} is skinned; booleans do not carry skins yet: build the solid first, then skin it"
            ),
        }
    }
}

impl std::error::Error for BoolError {}

const CAPS: [&str; 4] = ["top", "bottom", "start", "end"];
/// cos 0.5°: faces closer than this are coplanar for the naming rule.
const COPLANAR_COS: f64 = 0.999_961_923_064_171_3;

/// The solid's connected pieces (sharing a vertex after exact welding),
/// each a solid with its names, in canonical order.
pub fn components(s: &Solid) -> Vec<Solid> {
    let mut parent: Vec<u32> = (0..s.positions.len() as u32).collect();
    fn find(p: &mut [u32], mut i: u32) -> u32 {
        while p[i as usize] != i {
            p[i as usize] = p[p[i as usize] as usize];
            i = p[i as usize];
        }
        i
    }
    for t in &s.triangles {
        for k in 1..3 {
            let (a, b) = (find(&mut parent, t[0]), find(&mut parent, t[k]));
            if a != b {
                parent[a.max(b) as usize] = a.min(b);
            }
        }
    }
    let mut groups: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, t) in s.triangles.iter().enumerate() {
        groups.entry(find(&mut parent, t[0])).or_default().push(i);
    }
    groups
        .into_values()
        .map(|tris| {
            // The skin rides along whole; canonicalize keeps the weights
            // of the vertices this piece uses.
            let mut c = Solid {
                positions: s.positions.clone(),
                skin: s.skin.clone(),
                ..Default::default()
            };
            for i in tris {
                let tags: Vec<&str> = s.face_tags[i].iter().map(|&x| s.names.name(x)).collect();
                c.push_triangle(s.triangles[i], s.surfaces[i], &tags);
            }
            for (&(a, b), tags) in &s.edge_tags {
                let tags: Vec<&str> = tags.iter().map(|&x| s.names.name(x)).collect();
                c.tag_edge(a, b, &tags);
            }
            // Canonicalize drops the other pieces' vertices and edges.
            let _ = c.canonicalize();
            c
        })
        .collect()
}

/// Fuses a solid made of several closed shells (`merge`, or a provider's
/// interpenetrating parts) into one: each shell that passes through itself
/// is untangled first ([`self_union`]), then the shells are joined in
/// turn. A solid of one shell that does not pass through itself comes back
/// as it is.
pub fn fuse(s: &Solid) -> Result<Solid, BoolError> {
    if s.skin.is_some() {
        return Err(BoolError::Skinned("target"));
    }
    // A shell that crosses itself only by rounding (an earlier boolean's
    // output) may not untangle closed; it is then joined as it is, and the
    // join's own check on its result still holds.
    let untangled = |p: Solid| -> Result<Solid, BoolError> {
        if crossing_pairs(&p) > 0 {
            Ok(self_union(&p).unwrap_or(p))
        } else {
            Ok(p)
        }
    };
    let mut parts = components(s).into_iter();
    let Some(first) = parts.next() else {
        return Err(BoolError::Empty);
    };
    let mut acc = untangled(first)?;
    for p in parts {
        acc = boolean_once(&acc, &untangled(p)?, &Boolean::default())?;
    }
    Ok(acc)
}

/// Pairs of triangles, within one shell ([`components`]), that cross where
/// the surface does not join them, after welding by exact position: they
/// share no vertex and meet, or share a vertex or an edge and overlap
/// beyond it ([`selfcheck::crossing`], exact predicates). Zero for a shell
/// that does not pass through itself; separate shells passing through
/// each other are not counted ([`fuse`] joins those).
pub fn crossing_pairs(s: &Solid) -> usize {
    // Each triangle's shell, as `components` finds them.
    let mut parent: Vec<u32> = (0..s.positions.len() as u32).collect();
    fn find(p: &mut [u32], mut i: u32) -> u32 {
        while p[i as usize] != i {
            p[i as usize] = p[p[i as usize] as usize];
            i = p[i as usize];
        }
        i
    }
    for t in &s.triangles {
        for k in 1..3 {
            let (a, b) = (find(&mut parent, t[0]), find(&mut parent, t[k]));
            if a != b {
                parent[a.max(b) as usize] = a.min(b);
            }
        }
    }
    let shell: Vec<u32> = s
        .triangles
        .iter()
        .map(|t| find(&mut parent, t[0]))
        .collect();
    let (w, _) = weld(&s.positions);
    let tris: Vec<[u32; 3]> = s
        .triangles
        .iter()
        .map(|t| t.map(|v| w[v as usize]))
        .collect();
    let mut points = vec![DVec3::ZERO; s.positions.len()];
    for (i, &p) in s.positions.iter().enumerate() {
        points[w[i] as usize] = p;
    }
    // Triangles welded down to an edge or a point are left out, as the
    // booleans drop them.
    let live: Vec<u32> = (0..tris.len() as u32)
        .filter(|&i| {
            let t = tris[i as usize];
            t[0] != t[1] && t[1] != t[2] && t[0] != t[2]
        })
        .collect();
    let live_tris: Vec<[u32; 3]> = live.iter().map(|&i| tris[i as usize]).collect();
    selfcheck::self_crossings(&points, &live_tris)
        .into_iter()
        .filter(|&(i, j)| shell[live[i as usize] as usize] == shell[live[j as usize] as usize])
        .count()
}

/// Untangles a solid whose surface passes through itself: its outside
/// surface, around the space its surface winds round at least once
/// ([`self_union_points`]). Separate shells passing through each other are
/// joined too. Names are carried as [`fuse`] carries them: every face
/// keeps its face names and surface; edges along which the surface
/// crossed itself are named `cut`, plus the cap names of the faces they
/// bound; pieces of named input edges keep their names.
///
/// Refused: a skinned solid, one that is not closed and oriented after
/// welding, an empty result, or an open one.
pub fn self_union(s: &Solid) -> Result<Solid, BoolError> {
    if s.skin.is_some() {
        return Err(BoolError::Skinned("target"));
    }
    let u = self_union_points((&s.positions, &s.triangles)).map_err(|e| match e {
        OperandError::NotClosed(_) => BoolError::NotClosed("target"),
        OperandError::NotFinite(_) => BoolError::NotFinite("target"),
    })?;
    if u.tris.is_empty() {
        return Err(BoolError::Empty);
    }
    let boundary = open_edges(&u.points, &u.tris);
    if boundary > 0 {
        return Err(BoolError::OpenResult { boundary });
    }
    let origins: Vec<(u8, u32)> = u.source.iter().map(|&t| (0, t)).collect();
    let seams: std::collections::BTreeSet<EdgeKey> =
        u.seams.iter().map(|&[a, b]| edge_key(a, b)).collect();
    named_result(
        u.points,
        &u.tris,
        &origins,
        [(s, ""), (s, "")],
        &Boolean::default(),
        |e, _| seams.contains(&e),
    )
}

/// `target op tool`, with names carried through. An operand of several
/// shells is fused first ([`fuse`]).
///
/// Refused: a skinned operand ([`BoolError::Skinned`]); an open result
/// ([`BoolError::OpenResult`]), never returned. When the result is open
/// and an operand passes through itself, the refusal says so instead
/// ([`BoolError::SelfIntersecting`]). [`check_operands`] refuses such an
/// operand before any work.
pub fn boolean(target: &Solid, tool: &Solid, d: &Boolean) -> Result<Solid, BoolError> {
    if target.skin.is_some() {
        return Err(BoolError::Skinned("target"));
    }
    if tool.skin.is_some() {
        return Err(BoolError::Skinned("tool"));
    }
    let fused = |s: &Solid| {
        if components(s).len() > 1 {
            fuse(s)
        } else {
            Ok(s.clone())
        }
    };
    match boolean_once(&fused(target)?, &fused(tool)?, d) {
        Err(BoolError::OpenResult { boundary }) => {
            check_operands(target, tool)?;
            Err(BoolError::OpenResult { boundary })
        }
        r => r,
    }
}

/// Refuses, before any work, an operand with a shell that passes through
/// itself ([`BoolError::SelfIntersecting`], counted by
/// [`crossing_pairs`]); the target is checked first. [`boolean`] does not
/// call it up front: the kernel's own boolean outputs can cross themselves
/// by a rounding error where crossing points meet (an ulp, along a
/// countersink's rim), and booleans on them come out closed
/// (`docs/research.md` §5.4). Call it on imported or generated meshes;
/// it costs a sweep over each operand's triangles.
pub fn check_operands(target: &Solid, tool: &Solid) -> Result<(), BoolError> {
    for (s, operand) in [(target, "target"), (tool, "tool")] {
        let pairs = crossing_pairs(s);
        if pairs > 0 {
            return Err(BoolError::SelfIntersecting { operand, pairs });
        }
    }
    Ok(())
}

fn boolean_once(target: &Solid, tool: &Solid, d: &Boolean) -> Result<Solid, BoolError> {
    let op = match d.op {
        BoolOp::Join => CsgOp::Union,
        BoolOp::Cut => CsgOp::Subtract,
        BoolOp::Intersect => CsgOp::Intersect,
    };
    let (points, tris, origins) = csg_points(
        (&target.positions, &target.triangles),
        (&tool.positions, &tool.triangles),
        op,
    )
    .map_err(|e| match e {
        OperandError::NotClosed(0) => BoolError::NotClosed("target"),
        OperandError::NotClosed(_) => BoolError::NotClosed("tool"),
        OperandError::NotFinite(0) => BoolError::NotFinite("target"),
        OperandError::NotFinite(_) => BoolError::NotFinite("tool"),
    })?;
    if tris.is_empty() {
        return Err(BoolError::Empty);
    }
    // Never return open output (`docs/design.md` §6): every directed edge,
    // after welding by exact position, needs a twin the other way.
    let boundary = open_edges(&points, &tris);
    if boundary > 0 {
        return Err(BoolError::OpenResult { boundary });
    }
    named_result(
        points,
        &tris,
        &origins,
        [(target, d.target), (tool, d.tool)],
        d,
        |_, ms| ms.contains(&0) && ms.contains(&1),
    )
}

/// The output of a boolean (or self-union) as a solid with names:
/// `origins` gives each triangle's `(operand, source triangle)`, and
/// `seam` whether an output edge, with its triangles' operands, is where
/// the operands met.
fn named_result(
    points: Vec<DVec3>,
    tris: &[[u32; 3]],
    origins: &[(u8, u32)],
    operands: [(&Solid, &str); 2],
    d: &Boolean,
    seam: impl Fn(EdgeKey, &[u8]) -> bool,
) -> Result<Solid, BoolError> {
    let (target, tool) = (operands[0].0, operands[1].0);
    let surface_offset = target.surfaces.iter().max().map_or(0, |s| s + 1);
    let mut out = Solid {
        positions: points,
        ..Default::default()
    };
    for (t, &(m, src)) in tris.iter().zip(origins) {
        let (solid, name) = operands[m as usize];
        let mut tags: Vec<String> = solid.face_tags[src as usize]
            .iter()
            .map(|&x| solid.names.name(x).to_string())
            .collect();
        if !name.is_empty() {
            let qualified: Vec<String> = tags
                .iter()
                .filter(|t| !t.contains('.'))
                .map(|t| format!("{name}.{t}"))
                .collect();
            tags.extend(qualified);
        }
        if m == 1 && d.op == BoolOp::Cut {
            tags.push("cut".into());
        }
        let refs: Vec<&str> = tags.iter().map(String::as_str).collect();
        let surface = solid.surfaces[src as usize] + if m == 1 { surface_offset } else { 0 };
        out.push_triangle(*t, surface, &refs);
    }
    // Edges: each output edge's two triangles.
    let mut by_edge: BTreeMap<EdgeKey, Vec<usize>> = BTreeMap::new();
    for (i, t) in out.triangles.iter().enumerate() {
        for k in 0..3 {
            by_edge
                .entry(edge_key(t[k], t[(k + 1) % 3]))
                .or_default()
                .push(i);
        }
    }
    let normal = |s: &Solid, t: [u32; 3]| {
        let [a, b, c] = t.map(|v| s.positions[v as usize]);
        (b - a).cross(c - a).normalize_or_zero()
    };
    let input_edges: [Vec<(DVec3, DVec3, Vec<String>)>; 2] = [target, tool].map(|s| {
        s.edge_tags
            .iter()
            .map(|(&(a, b), tags)| {
                (
                    s.positions[a as usize],
                    s.positions[b as usize],
                    tags.iter().map(|&x| s.names.name(x).to_string()).collect(),
                )
            })
            .collect()
    });
    let mut edge_names: Vec<(EdgeKey, Vec<String>)> = Vec::new();
    for (&(u, v), ts) in &by_edge {
        let (pu, pv) = (out.positions[u as usize], out.positions[v as usize]);
        let mut names: Vec<String> = Vec::new();
        let ms: Vec<u8> = ts.iter().map(|&t| origins[t].0).collect();
        if seam((u, v), &ms) {
            names.push("cut".into());
            if !d.target.is_empty() {
                names.push(format!("{}.cut", d.target));
            }
            if !d.tool.is_empty() {
                names.push(d.tool.to_string());
            }
            for &t in ts {
                let (solid, _) = operands[origins[t].0 as usize];
                for &x in &solid.face_tags[origins[t].1 as usize] {
                    let n = solid.names.name(x);
                    if CAPS.contains(&n) {
                        names.push(n.to_string());
                    }
                }
            }
        }
        // Pieces of named input edges keep their names.
        for &m in &ms {
            for (p, q, tags) in &input_edges[m as usize] {
                if on_segment(*p, *q, pu) && on_segment(*p, *q, pv) {
                    names.extend(tags.iter().cloned());
                }
            }
        }
        if names.is_empty() {
            continue;
        }
        // Coplanar faces on both sides: not a real edge any more.
        if ts.len() == 2
            && normal(&out, out.triangles[ts[0]]).dot(normal(&out, out.triangles[ts[1]]))
                > COPLANAR_COS
        {
            continue;
        }
        names.sort();
        names.dedup();
        edge_names.push(((u, v), names));
    }
    for ((u, v), names) in edge_names {
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        out.tag_edge(u, v, &refs);
    }
    out.canonicalize()
        .map_err(|_| BoolError::NotFinite("result"))?;
    Ok(out)
}

/// Directed edges, after welding by exact position, used more often one
/// way than the other (zero-area triangles skipped).
fn open_edges(points: &[DVec3], tris: &[[u32; 3]]) -> usize {
    let (w, _) = weld(points);
    let mut directed: BTreeMap<(u32, u32), i64> = BTreeMap::new();
    for t in tris {
        let t = t.map(|v| w[v as usize]);
        if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
            continue;
        }
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *directed.entry((a.min(b), a.max(b))).or_default() += if a < b { 1 } else { -1 };
        }
    }
    directed.values().map(|n| n.unsigned_abs() as usize).sum()
}

/// Each point's id after welding by exact position (ids in first-seen
/// order), and how many there are.
fn weld(points: &[DVec3]) -> (Vec<u32>, usize) {
    let mut id: BTreeMap<[u64; 3], u32> = BTreeMap::new();
    let w = points
        .iter()
        .map(|p| {
            let n = id.len() as u32;
            *id.entry([
                (p.x + 0.0).to_bits(),
                (p.y + 0.0).to_bits(),
                (p.z + 0.0).to_bits(),
            ])
            .or_insert(n)
        })
        .collect();
    (w, id.len())
}

/// Is `x` on the segment `p`–`q` (to 1e-9 of its length)?
fn on_segment(p: DVec3, q: DVec3, x: DVec3) -> bool {
    let d = q - p;
    let l2 = d.length_squared();
    if l2 == 0.0 {
        return false;
    }
    let w = x - p;
    let t = w.dot(d);
    if t < -1e-9 * l2 || t > l2 * (1.0 + 1e-9) {
        return false;
    }
    d.cross(w).length_squared() <= 1e-18 * l2 * l2
}

#[cfg(test)]
mod tests {
    use super::*;
    use shape_core::trig;
    use shape_gen::solids::{BoxDesc, CylinderDesc, cuboid, cylinder};

    fn plate() -> Solid {
        cuboid(&BoxDesc {
            width: 80.0,
            depth: 40.0,
            height: 6.0,
        })
        .unwrap()
    }

    fn pin(z0: f64, h: f64) -> Solid {
        let mut c = cylinder(&CylinderDesc {
            radius: 3.3,
            radius_top: 0.0,
            height: h,
        })
        .unwrap();
        c.map_positions(|p| p + DVec3::new(0.0, 0.0, z0), false);
        c
    }

    #[test]
    fn a_through_hole_names_its_rims_and_keeps_the_plate_s_edges() {
        let s = boolean(
            &plate(),
            &pin(-1.0, 8.0),
            &Boolean {
                op: BoolOp::Cut,
                target: "plate",
                tool: "hole",
            },
        )
        .unwrap();
        let m = s.measure();
        assert!(m.closed, "{m:?}");
        // The two rims: on the top and on the bottom.
        let cut = s.edges(&["cut"]).unwrap();
        let top_rim = s
            .edges(&["top"])
            .unwrap()
            .into_iter()
            .filter(|e| cut.contains(e))
            .count();
        assert!(top_rim > 20, "{top_rim}");
        assert!(s.edges(&["plate.cut"]).is_ok() && s.edges(&["hole"]).is_ok());
        // The bore walls are tool faces left by the cut.
        assert!(!s.faces(&["cut"]).unwrap().is_empty());
        assert!(!s.faces(&["plate.top"]).unwrap().is_empty());
        // The plate's outer corner edges survive.
        assert_eq!(
            s.edges(&["side"])
                .unwrap()
                .iter()
                .filter(|e| !cut.contains(e))
                .count(),
            4
        );
        let want = 80.0 * 40.0 * 6.0 - trig::PI * 3.3 * 3.3 * 6.0;
        assert!((m.volume - want).abs() / want < 1e-3);
    }

    #[test]
    fn a_boss_joined_flush_loses_the_seam_it_buried() {
        let s = boolean(
            &plate(),
            &pin(6.0, 10.0),
            &Boolean {
                op: BoolOp::Join,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(s.measure().closed);
        // The pin's bottom rim lies on the plate's top: a real edge (cut).
        assert!(!s.edges(&["cut"]).unwrap().is_empty());
    }

    #[test]
    fn overlapping_shells_are_fused_before_a_boolean() {
        let mut two = plate();
        let mut other = plate();
        other.map_positions(|p| p + DVec3::new(20.0, 10.0, 3.0), false);
        two.append(&other);
        two.canonicalize().unwrap();
        assert_eq!(components(&two).len(), 2);
        let f = fuse(&two).unwrap();
        let m = f.measure();
        assert!(m.closed && m.components == 1);
        let want = 2.0 * 80.0 * 40.0 * 6.0 - 60.0 * 30.0 * 3.0;
        assert!((m.volume - want).abs() < 1e-6, "{}", m.volume);
    }

    /// A sphere with one vertex pulled through the far side: closed,
    /// oriented, one piece, and passing through itself.
    fn folded_ball() -> Solid {
        let mut ball = shape_gen::solids::sphere(10.0).unwrap();
        let dir = DVec3::new(-0.51, -0.38, 0.77);
        let v = (0..ball.positions.len())
            .max_by(|&a, &b| {
                ball.positions[a]
                    .dot(dir)
                    .total_cmp(&ball.positions[b].dot(dir))
            })
            .unwrap();
        ball.positions[v] = -ball.positions[v] * 1.5;
        ball
    }

    fn bar() -> Solid {
        let mut bar = cuboid(&BoxDesc {
            width: 10.0,
            depth: 10.0,
            height: 60.0,
        })
        .unwrap();
        bar.map_positions(|p| p + DVec3::new(0.0, 0.0, -30.0), false);
        bar
    }

    /// The volume where `s` winds round at least once, sampled at the
    /// centres of an `n`³ grid over its box.
    fn sampled_volume(s: &Solid, n: usize) -> f64 {
        let m = s.measure();
        let (lo, size) = (m.bounds_min, m.bounds_max - m.bounds_min);
        let cell = size / n as f64;
        let mut inside = 0;
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    let p = lo + cell * DVec3::new(i as f64 + 0.5, j as f64 + 0.5, k as f64 + 0.5);
                    if crate::csg::winding(&s.positions, &s.triangles, p) > 0.5 {
                        inside += 1;
                    }
                }
            }
        }
        inside as f64 * cell.x * cell.y * cell.z
    }

    #[test]
    fn a_surface_folded_through_itself_is_refused_then_untangled() {
        let ball = folded_ball();
        let pairs = crossing_pairs(&ball);
        assert!(pairs > 0);
        assert_eq!(
            check_operands(&ball, &bar()),
            Err(BoolError::SelfIntersecting {
                operand: "target",
                pairs
            })
        );
        assert_eq!(
            check_operands(&bar(), &ball),
            Err(BoolError::SelfIntersecting {
                operand: "tool",
                pairs
            })
        );
        // Without the check first, every operation is still refused, never
        // open (before the result check, all three came back open with Ok:
        // `docs/research.md` §9.6).
        for op in [BoolOp::Join, BoolOp::Cut, BoolOp::Intersect] {
            let d = Boolean {
                op,
                ..Default::default()
            };
            assert_eq!(
                boolean(&ball, &bar(), &d),
                Err(BoolError::SelfIntersecting {
                    operand: "target",
                    pairs
                }),
                "{op:?}"
            );
        }
        let u = self_union(&ball).unwrap();
        let m = u.measure();
        assert!(m.closed && m.components == 1, "{m:?}");
        assert_eq!(crossing_pairs(&u), 0);
        // The space the folded surface winds round at least once: less
        // than the ball's, and the sampled volume to the grid's error.
        let want = sampled_volume(&ball, 24);
        assert!((m.volume - want).abs() / want < 0.02, "{} {want}", m.volume);
        // Every face keeps its names.
        assert!(u.face_tags.iter().all(|t| !t.is_empty()));
        assert!(!u.edges(&["cut"]).unwrap().is_empty());
        // The untangled ball takes booleans. (With the bar centred they
        // come out open where its faces meet the fold's slivers: a limit of
        // the two-operand CSG, `docs/research.md` §5.4.)
        let mut moved = bar();
        moved.map_positions(|p| p + DVec3::new(0.31, 0.17, 0.05), false);
        for op in [BoolOp::Join, BoolOp::Cut, BoolOp::Intersect] {
            let d = Boolean {
                op,
                ..Default::default()
            };
            assert!(boolean(&u, &moved, &d).unwrap().measure().closed, "{op:?}");
        }
        // fuse untangles a single shell too.
        assert_eq!(fuse(&ball).unwrap().measure().volume, m.volume);
    }

    #[test]
    fn two_overlapping_copies_untangle_into_one_shell() {
        let mut two = plate();
        let mut other = plate();
        other.map_positions(|p| p + DVec3::new(20.0, 10.0, 3.0), false);
        two.append(&other);
        two.canonicalize().unwrap();
        // Separate shells crossing each other are not a self-intersection.
        assert_eq!(crossing_pairs(&two), 0);
        let u = self_union(&two).unwrap();
        let m = u.measure();
        assert!(m.closed && m.components == 1, "{m:?}");
        let want = 2.0 * 80.0 * 40.0 * 6.0 - 60.0 * 30.0 * 3.0;
        assert!((m.volume - want).abs() < 1e-6, "{}", m.volume);
        // A clean solid comes back with its volume and names.
        let p = self_union(&plate()).unwrap();
        assert_eq!(p.triangles.len(), plate().triangles.len());
        assert_eq!(p.measure().volume, plate().measure().volume);
        assert!(p.faces(&["top"]).is_ok());
        // Two copies lying exactly on each other are one plate.
        let mut same = plate();
        same.append(&plate());
        same.canonicalize().unwrap();
        let u = self_union(&same).unwrap();
        let m = u.measure();
        assert!(m.closed && m.components == 1, "{m:?}");
        assert!((m.volume - 80.0 * 40.0 * 6.0).abs() < 1e-6, "{}", m.volume);
    }

    #[test]
    fn open_operands_are_refused() {
        let mut open = plate();
        open.triangles.pop();
        open.surfaces.pop();
        open.face_tags.pop();
        assert_eq!(
            boolean(&open, &pin(-1.0, 8.0), &Boolean::default()),
            Err(BoolError::NotClosed("target"))
        );
        let mut flipped = pin(-1.0, 8.0);
        flipped.triangles[0].swap(1, 2);
        assert_eq!(
            boolean(&plate(), &flipped, &Boolean::default()),
            Err(BoolError::NotClosed("tool"))
        );
    }
}
