//! [`interference`]: which parts of a scene pass through each other, touch,
//! or hold one another wholly inside, as posed by its joints' values.
//!
//! Every side is decided exactly (`shape_bool::selfcheck`'s triangle tests,
//! `orient3d` for the rest); pairs of triangles come from one sweep of
//! boxes over every part. Nothing is judged: the counts are reported and
//! the caller decides what is too much (a pin seated in its hole touches
//! it; a door through its frame crosses it).

use std::collections::BTreeMap;

use shape_bool::selfcheck::{box_pairs, tris_meet};
use shape_core::DVec3;
use shape_core::predicates::orient3d;

use crate::export::{ExportError, Scene, check};

/// One pair of parts that meet, or one wholly inside the other.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Interfering {
    /// The part declared first.
    pub a: String,
    /// The part declared later.
    pub b: String,
    /// Triangle pairs passing through each other: each triangle has
    /// corners strictly on both sides of the other's plane.
    pub crossings: usize,
    /// Triangle pairs that meet without crossing: faces lying on each
    /// other, an edge or a corner on a face.
    pub touching: usize,
    /// When no triangles cross: the part wholly inside the other (`a` or
    /// `b`'s name; touching it from inside counts), by the parity of a ray
    /// from one of its corners off the other's surface through the other,
    /// which must be closed. Empty otherwise.
    pub inside: String,
    /// The box of the triangles that meet, or of the part inside, posed,
    /// in the scene's units.
    pub bounds_min: DVec3,
    /// Its high corner.
    pub bounds_max: DVec3,
}

/// Rays tried from a corner, in order, until one misses every edge and
/// corner of the surface it is cast through.
const RAYS: [[f64; 3]; 8] = [
    [1.0, 0.377_964, 0.218_218],
    [-0.267_261, 1.0, 0.534_522],
    [0.408_248, -0.316_228, 1.0],
    [-1.0, -0.447_214, 0.288_675],
    [0.235_702, 0.894_427, -1.0],
    [0.612_372, -1.0, -0.377_964],
    [-0.555_556, 0.169_031, -1.0],
    [1.0, -0.632_456, 0.832_050],
];

/// Several parts' triangles in one mesh, each moved by a map, and the
/// part each triangle belongs to.
#[derive(Default)]
struct Mesh {
    positions: Vec<DVec3>,
    triangles: Vec<[u32; 3]>,
    owner: Vec<usize>,
    /// Per part (by scene index), its triangles' range.
    ranges: BTreeMap<usize, (usize, usize)>,
}

impl Mesh {
    fn new(scene: &Scene, parts: &[usize], map: impl Fn(usize, DVec3) -> DVec3) -> Mesh {
        let mut m = Mesh::default();
        for &k in parts {
            let s = scene.parts[k].solid;
            let base = m.positions.len() as u32;
            let first = m.triangles.len();
            m.positions.extend(s.positions.iter().map(|&v| map(k, v)));
            m.triangles
                .extend(s.triangles.iter().map(|t| t.map(|v| v + base)));
            m.owner.extend(std::iter::repeat_n(k, s.triangles.len()));
            m.ranges.insert(k, (first, m.triangles.len()));
        }
        m
    }

    fn tri(&self, i: u32) -> [DVec3; 3] {
        self.triangles[i as usize].map(|v| self.positions[v as usize])
    }

    /// The corners of part `k`'s triangles, in order.
    fn corners(&self, k: usize) -> impl Iterator<Item = DVec3> + '_ {
        let (lo, hi) = self.ranges[&k];
        self.triangles[lo..hi]
            .iter()
            .flatten()
            .map(|&v| self.positions[v as usize])
    }

    /// Part `k`'s box, if it has triangles.
    fn bounds(&self, k: usize) -> Option<(DVec3, DVec3)> {
        self.corners(k).fold(None, |b: Option<(DVec3, DVec3)>, p| {
            Some(b.map_or((p, p), |(l, h)| (l.min(p), h.max(p))))
        })
    }

    /// Part `inner` is inside the closed part `outer` (see [`held_by`]).
    fn holds(&self, outer: usize, inner: usize) -> bool {
        let (lo, hi) = self.ranges[&outer];
        let Some(b) = self.bounds(outer) else {
            return false;
        };
        held_by(
            self.corners(inner),
            &self.positions,
            &self.triangles[lo..hi],
            b,
        )
    }
}

/// Per pair of parts: crossings, touching, and the box of what met.
type Met = BTreeMap<(usize, usize), (usize, usize, DVec3, DVec3)>;

/// Counts into `met` the triangle pairs of `mesh` that meet, between
/// parts `keep` accepts; boxes taken through `show` (into the posed
/// world).
fn meet(
    mesh: &Mesh,
    keep: impl Fn(usize, usize) -> bool,
    show: impl Fn(usize, DVec3) -> DVec3,
    met: &mut Met,
) {
    for (i, j) in box_pairs(&mesh.positions, &mesh.triangles) {
        let (pi, pj) = (mesh.owner[i as usize], mesh.owner[j as usize]);
        if pi == pj || !keep(pi, pj) {
            continue;
        }
        let (ta, tb) = (mesh.tri(i), mesh.tri(j));
        if !tris_meet(ta, tb) {
            continue;
        }
        let e = met.entry((pi.min(pj), pi.max(pj))).or_insert((
            0,
            0,
            DVec3::INFINITY,
            DVec3::NEG_INFINITY,
        ));
        if crosses(ta, tb) {
            e.0 += 1;
        } else {
            e.1 += 1;
        }
        for (k, t) in [(pi, ta), (pj, tb)] {
            for p in t {
                let p = show(k, p);
                e.2 = e.2.min(p);
                e.3 = e.3.max(p);
            }
        }
    }
}

/// Every pair of parts that meets, or nests, at the scene's pose (each
/// joint at its `value`; followers from their leaders), in the order of
/// the first part's then the second's declaration. Parts with no
/// triangles are left out.
///
/// Parts that the pose moves by the very same motion (a door's glass and
/// its ring) are compared where they were built: their placement relative
/// to each other is unchanged, and a rotation's rounding would make faces
/// that lie exactly on each other cross. Other pairs are compared posed.
///
/// Errors as [`crate::glb`] does on an invalid scene.
pub fn interference(scene: &Scene) -> Result<Vec<Interfering>, ExportError> {
    let placed = check(scene)?;
    let n = scene.parts.len();
    // Parts moved alike share a group: the first part with that motion.
    let key = |k: usize| {
        let r = &placed[k];
        [r.x, r.y, r.z, r.t].map(|v| [v.x, v.y, v.z].map(f64::to_bits))
    };
    let mut first: BTreeMap<[[u64; 3]; 4], usize> = BTreeMap::new();
    let group: Vec<usize> = (0..n).map(|k| *first.entry(key(k)).or_insert(k)).collect();
    let all: Vec<usize> = (0..n).collect();
    let posed = Mesh::new(scene, &all, |k, v| placed[k].apply(v));
    let mut met = Met::new();
    meet(&posed, |a, b| group[a] != group[b], |_, p| p, &mut met);
    // Each group of two or more parts, as built.
    let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (k, &g) in group.iter().enumerate() {
        members.entry(g).or_default().push(k);
    }
    let mut rest: BTreeMap<usize, Mesh> = BTreeMap::new();
    for (g, parts) in members.into_iter().filter(|(_, p)| p.len() > 1) {
        let mesh = Mesh::new(scene, &parts, |_, v| v);
        meet(&mesh, |_, _| true, |k, p| placed[k].apply(p), &mut met);
        rest.insert(g, mesh);
    }
    let closed: Vec<bool> = scene
        .parts
        .iter()
        .map(|p| p.solid.measure().closed)
        .collect();
    let mut out = Vec::new();
    for a in 0..n {
        for b in a + 1..n {
            let (Some(ba), Some(bb)) = (posed.bounds(a), posed.bounds(b)) else {
                continue;
            };
            let name = |k: usize| scene.parts[k].name.clone();
            let (crossings, touching, lo, hi) =
                met.get(&(a, b))
                    .copied()
                    .unwrap_or((0, 0, DVec3::INFINITY, DVec3::NEG_INFINITY));
            // Nested parts: when no triangles cross, one may hold the other,
            // tested where the pair is compared.
            let mesh = if group[a] == group[b] {
                &rest[&group[a]]
            } else {
                &posed
            };
            let within =
                |x: (DVec3, DVec3), y: (DVec3, DVec3)| y.0.cmple(x.0).all() && x.1.cmple(y.1).all();
            let nested = if crossings > 0 {
                None
            } else if within(ba, bb) && closed[b] && mesh.holds(b, a) {
                Some((a, ba))
            } else if within(bb, ba) && closed[a] && mesh.holds(a, b) {
                Some((b, bb))
            } else {
                None
            };
            if touching == 0 && crossings == 0 && nested.is_none() {
                continue;
            }
            let (inside, lo, hi) = match nested {
                Some((k, (l, h))) => (name(k), lo.min(l), hi.max(h)),
                None => (String::new(), lo, hi),
            };
            out.push(Interfering {
                a: name(a),
                b: name(b),
                crossings,
                touching,
                inside,
                bounds_min: lo,
                bounds_max: hi,
            });
        }
    }
    Ok(out)
}

/// Two triangles that meet pass through each other: each has corners
/// strictly on both sides of the other's plane.
fn crosses(a: [DVec3; 3], b: [DVec3; 3]) -> bool {
    use core::cmp::Ordering::{Greater, Less};
    let both = |t: [DVec3; 3], u: [DVec3; 3]| {
        let s = u.map(|p| orient3d(t[0], t[1], t[2], p));
        s.contains(&Greater) && s.contains(&Less)
    };
    both(a, b) && both(b, a)
}

/// Corners of a part tried, in order, for one off the other's surface.
const TRIES: usize = 16;

/// The first of the points (at most [`TRIES`]) not on the closed surface
/// `tris` is inside it.
fn held_by(
    points: impl Iterator<Item = DVec3>,
    positions: &[DVec3],
    tris: &[[u32; 3]],
    bounds: (DVec3, DVec3),
) -> bool {
    points
        .take(TRIES)
        .find_map(|p| inside(p, positions, tris, bounds))
        .unwrap_or(false)
}

/// Whether `p` is inside the closed surface `tris`: an odd number of them
/// cross a ray from `p`. `None` when `p` lies on the surface, or every one
/// of [`RAYS`] passes through an edge or a corner or ends in a triangle's
/// plane.
fn inside(
    p: DVec3,
    positions: &[DVec3],
    tris: &[[u32; 3]],
    bounds: (DVec3, DVec3),
) -> Option<bool> {
    use core::cmp::Ordering::{Equal, Greater, Less};
    // Far enough that the ray's end lies outside the box.
    let reach = 2.0 * ((bounds.1 - bounds.0).length() + (p - bounds.0).length() + 1.0);
    'ray: for r in RAYS {
        let q = p + DVec3::from_array(r) * reach;
        let mut odd = false;
        for t in tris {
            let [a, b, c] = t.map(|v| positions[v as usize]);
            let sp = orient3d(a, b, c, p);
            if sp == Equal && on_triangle(p, [a, b, c]) {
                return None;
            }
            let sq = orient3d(a, b, c, q);
            if sq == Equal {
                continue 'ray;
            }
            if sp == sq || sp == Equal {
                continue;
            }
            let e = [
                orient3d(p, q, a, b),
                orient3d(p, q, b, c),
                orient3d(p, q, c, a),
            ];
            if e.contains(&Greater) && e.contains(&Less) {
                continue;
            }
            if e.contains(&Equal) {
                continue 'ray;
            }
            odd = !odd;
        }
        return Some(odd);
    }
    None
}

/// `p`, in the plane of the triangle `t`, lies in it (edges included).
fn on_triangle(p: DVec3, t: [DVec3; 3]) -> bool {
    use shape_core::DVec2;
    use shape_core::predicates::orient2d;
    // Drop the axis the normal leans along most; for a triangle with any
    // area that projection keeps area.
    let n = (t[1] - t[0]).cross(t[2] - t[0]).abs();
    let f = |v: DVec3| {
        if n.x >= n.y && n.x >= n.z {
            DVec2::new(v.y, v.z)
        } else if n.y >= n.z {
            DVec2::new(v.z, v.x)
        } else {
            DVec2::new(v.x, v.y)
        }
    };
    let [a, b, c] = t.map(f);
    let q = f(p);
    let o = [orient2d(a, b, q), orient2d(b, c, q), orient2d(c, a, q)];
    let turn = orient2d(a, b, c);
    turn != core::cmp::Ordering::Equal
        && o.iter()
            .all(|&x| x == turn || x == core::cmp::Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{Joint, JointKind, Part};
    use shape_core::Frame;
    use shape_mesh::Solid;

    fn cube(lo: DVec3, size: f64) -> Solid {
        let mut s = Solid::default();
        let c = |x: f64, y: f64, z: f64| lo + DVec3::new(x, y, z) * size;
        let v: Vec<u32> = [
            c(0.0, 0.0, 0.0),
            c(1.0, 0.0, 0.0),
            c(1.0, 1.0, 0.0),
            c(0.0, 1.0, 0.0),
            c(0.0, 0.0, 1.0),
            c(1.0, 0.0, 1.0),
            c(1.0, 1.0, 1.0),
            c(0.0, 1.0, 1.0),
        ]
        .into_iter()
        .map(|p| s.push_vertex(p))
        .collect();
        for q in [
            [0, 3, 2, 1],
            [4, 5, 6, 7],
            [0, 1, 5, 4],
            [1, 2, 6, 5],
            [2, 3, 7, 6],
            [3, 0, 4, 7],
        ] {
            s.push_triangle([v[q[0]], v[q[1]], v[q[2]]], 0, &[]);
            s.push_triangle([v[q[0]], v[q[2]], v[q[3]]], 0, &[]);
        }
        s
    }

    #[test]
    fn crossing_touching_and_inside_are_told_apart() {
        let frame = cube(DVec3::ZERO, 10.0);
        // Overlaps the frame by 2 along x: crosses.
        let through = cube(DVec3::new(8.0, 2.0, 2.0), 4.0);
        // Sits on the frame's top: touches.
        let on = cube(DVec3::new(2.0, 2.0, 10.0), 4.0);
        // Wholly inside, standing on the frame's floor.
        let core = cube(DVec3::new(3.0, 3.0, 0.0), 2.0);
        // Far away.
        let far = cube(DVec3::new(50.0, 0.0, 0.0), 1.0);
        let scene = Scene {
            name: "t".into(),
            parts: vec![
                Part::new("frame", &frame),
                Part::new("through", &through),
                Part::new("on", &on),
                Part::new("core", &core),
                Part::new("far", &far),
            ],
        };
        let r = interference(&scene).unwrap();
        let find = |a: &str, b: &str| r.iter().find(|x| x.a == a && x.b == b).cloned();
        let t = find("frame", "through").unwrap();
        assert!(t.crossings > 0 && t.inside.is_empty(), "{t:?}");
        let o = find("frame", "on").unwrap();
        assert_eq!(o.crossings, 0);
        assert!(o.touching > 0);
        let c = find("frame", "core").unwrap();
        assert_eq!((c.crossings, c.inside.as_str()), (0, "core"));
        assert!(c.touching > 0);
        assert!(find("frame", "far").is_none());
        // The core is inside the frame only: `through` and `core` are apart.
        assert!(find("through", "core").is_none());
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn a_door_clears_its_post_at_rest_and_crosses_it_turned() {
        // A door hinged on the z axis beside a post; a quarter turn swings
        // it into the post.
        let post = cube(DVec3::new(-2.0, 0.0, 0.0), 2.0);
        let door = cube(DVec3::new(0.5, 0.5, 0.5), 3.0);
        // Welded to the door, on its top face: they touch in every pose.
        let pane = cube(DVec3::new(1.0, 1.0, 3.5), 1.0);
        let mut scene = Scene {
            name: "t".into(),
            parts: vec![
                Part::new("post", &post),
                Part {
                    parent: "post".into(),
                    joint: Some(Joint {
                        name: "swing".into(),
                        kind: JointKind::Revolute,
                        pivot: Frame::IDENTITY,
                        axis: DVec3::Z,
                        value: 0.0,
                        range: Some((0.0, 2.0)),
                        mimic: None,
                    }),
                    ..Part::new("door", &door)
                },
                Part {
                    parent: "door".into(),
                    ..Part::new("pane", &pane)
                },
            ],
        };
        let rest = interference(&scene).unwrap();
        assert_eq!(rest.len(), 1);
        assert_eq!((rest[0].b.as_str(), rest[0].crossings), ("pane", 0));
        if let Some(j) = scene.parts[1].joint.as_mut() {
            j.value = shape_core::trig::PI / 2.0;
        }
        let open = interference(&scene).unwrap();
        assert!(open[0].a == "post" && open[0].crossings > 0, "{open:?}");
        // Turned about x by an angle whose rotation rounds, the pane still
        // only touches the door: the two move alike and are compared as
        // built.
        if let Some(j) = scene.parts[1].joint.as_mut() {
            j.axis = DVec3::X;
            j.value = 1.0;
        }
        let tilted = interference(&scene).unwrap();
        assert_eq!(tilted.len(), 1, "{tilted:?}");
        assert_eq!(
            (tilted[0].crossings, tilted[0].touching),
            (0, rest[0].touching)
        );
    }
}
