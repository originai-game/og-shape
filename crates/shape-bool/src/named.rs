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

use std::collections::BTreeMap;

use shape_core::DVec3;
use shape_mesh::{EdgeKey, Solid, edge_key};

use crate::csg::{CsgOp, OperandError, csg_points};

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
            let mut c = Solid {
                positions: s.positions.clone(),
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
/// interpenetrating parts) into one by joining them in turn. A solid of
/// one piece comes back as it is.
pub fn fuse(s: &Solid) -> Result<Solid, BoolError> {
    let mut parts = components(s).into_iter();
    let Some(mut acc) = parts.next() else {
        return Err(BoolError::Empty);
    };
    for p in parts {
        acc = boolean_once(&acc, &p, &Boolean::default())?;
    }
    Ok(acc)
}

/// `target op tool`, with names carried through. An operand of several
/// shells is fused first ([`fuse`]).
pub fn boolean(target: &Solid, tool: &Solid, d: &Boolean) -> Result<Solid, BoolError> {
    let fused = |s: &Solid| {
        if components(s).len() > 1 {
            fuse(s)
        } else {
            Ok(s.clone())
        }
    };
    boolean_once(&fused(target)?, &fused(tool)?, d)
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
    let operands = [(target, d.target), (tool, d.tool)];
    let surface_offset = target.surfaces.iter().max().map_or(0, |s| s + 1);
    let mut out = Solid {
        positions: points,
        ..Default::default()
    };
    for (t, &(m, src)) in tris.iter().zip(&origins) {
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
        if ms.contains(&0) && ms.contains(&1) {
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
