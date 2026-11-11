//! Colliders: [`colliders`] covers a mesh with a few convex hulls, the
//! shape a physics engine tests against instead of the mesh itself.
//!
//! One hull is the mesh's convex hull (`shape_gen::hull`, exact). A mesh
//! with hollows (a cup, an L, a chair) is cut into pieces, each with its
//! own hull, until every piece's *concavity* is within a tolerance or the
//! count of hulls is reached. The concavity of a piece is how deep its
//! surface lies under its hull: the largest distance from a face of the
//! piece (its corners and centre), along the face's outward normal, to
//! the hull's surface. A face on the hull has none, so a convex piece
//! has none; the wall of a hollow has the hollow's width. The mesh's
//! faces must point outward for this to mean anything (`repair` turns
//! flipped ones).
//!
//! The cut: the piece with the deepest concavity is split by a plane
//! square to one of the three axes, through its deepest point or at a
//! quarter of its box; of those, the one that leaves the shallowest
//! hollow, then the least hull volume. Its triangles are clipped at the
//! plane, so both pieces reach it.
//!
//! Cost: each cut tries a dozen planes and builds two hulls for each, so
//! it grows with the mesh's points. Colliders are for a game's mesh:
//! decimate a dense one first.
//!
//! [`Collide::max_vertices`] then thins each hull to the points that
//! reach furthest along a fixed set of directions, and takes their hull:
//! that hull lies inside the first, so the mesh can poke out of it by
//! what the report gives as `outside`. No tolerance is hidden: the report
//! gives each hull's volume, vertices, concavity and `outside`, and the
//! agent judges.
//!
//! Everything is decided by the triangles alone, so an open or tangled
//! mesh gets colliders too.

use std::collections::BTreeSet;

use shape_core::DVec3;
use shape_gen::hull::convex_hull;
use shape_mesh::Solid;

use crate::json::Json;

/// How to cover a mesh. The zero value is one hull with every corner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Collide {
    /// The most hulls to make; 0 and 1 make one.
    pub max_hulls: u32,
    /// Stop cutting when every piece's concavity is at most this (the
    /// mesh's units); 0 cuts until `max_hulls` is reached or every piece
    /// is convex.
    pub concavity: f64,
    /// The most vertices a hull may have; 0 keeps every corner. At least 4.
    pub max_vertices: u32,
}

/// Why a mesh has no colliders.
#[derive(Clone, Debug, PartialEq)]
pub enum CollideError {
    /// The mesh has no triangles.
    Empty,
    /// Its points all lie in one plane: no hull has a volume.
    Flat,
    /// A position is not finite.
    NotFinite,
    /// `concavity` is negative or not finite, or `max_vertices` is 1 to 3.
    BadValue(&'static str),
}

impl std::fmt::Display for CollideError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CollideError::Empty => f.write_str("the mesh has no triangles"),
            CollideError::Flat => f.write_str(
                "the mesh's points all lie in one plane: a collider needs a volume; give the sheet a thickness",
            ),
            CollideError::NotFinite => f.write_str("a position is not finite"),
            CollideError::BadValue(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for CollideError {}

/// One hull of a [`ColliderReport`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HullReport {
    /// Its vertices.
    pub vertices: usize,
    /// Its triangles.
    pub triangles: usize,
    /// Its volume.
    pub volume: f64,
    /// How deep the piece's surface lies under the hull: the largest
    /// distance from a face of the piece, along its outward normal, to
    /// the hull's surface.
    pub concavity: f64,
    /// How far the piece pokes out of the hull, when `max_vertices`
    /// thinned it: the largest distance from a vertex of the piece
    /// outside the hull to it; 0 for a hull with every corner.
    pub outside: f64,
}

/// What [`colliders`] made.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColliderReport {
    /// Per hull, in the order of the solids returned.
    pub hulls: Vec<HullReport>,
    /// The hulls' volumes together (they may overlap at the cuts).
    pub volume: f64,
    /// The mesh's own volume, if it is closed.
    pub mesh_volume: Option<f64>,
    /// The deepest concavity left.
    pub concavity: f64,
    /// The furthest any piece pokes out of its hull.
    pub outside: f64,
    /// Planes cut.
    pub cuts: usize,
}

impl ColliderReport {
    /// The report as one line of JSON: `hulls` (per hull `vertices`,
    /// `triangles`, `volume`, `concavity`, `outside`), `volume`,
    /// `mesh_volume` (null for an open mesh), `concavity`, `outside`,
    /// `cuts`.
    pub fn to_json(&self) -> String {
        let hulls = self
            .hulls
            .iter()
            .map(|h| {
                Json::Obj(vec![
                    ("vertices", Json::Int(h.vertices as u64)),
                    ("triangles", Json::Int(h.triangles as u64)),
                    ("volume", Json::Num(h.volume)),
                    ("concavity", Json::Num(h.concavity)),
                    ("outside", Json::Num(h.outside)),
                ])
            })
            .collect();
        let doc = Json::Obj(vec![
            ("hulls", Json::Arr(hulls)),
            ("volume", Json::Num(self.volume)),
            (
                "mesh_volume",
                self.mesh_volume.map_or(Json::Null, Json::Num),
            ),
            ("concavity", Json::Num(self.concavity)),
            ("outside", Json::Num(self.outside)),
            ("cuts", Json::Int(self.cuts as u64)),
        ]);
        let mut out = String::new();
        doc.write(&mut out);
        out
    }
}

type Tri = [DVec3; 3];

/// A piece of the mesh with its hull.
struct Piece {
    tris: Vec<Tri>,
    hull: Solid,
    /// The deepest point under the hull, and how deep.
    deepest: (DVec3, f64),
}

/// Covers `s` with convex hulls, as [`Collide`] says: the hulls, each a
/// closed solid whose faces and edges are named `hull`, and the report.
pub fn colliders(s: &Solid, d: &Collide) -> Result<(Vec<Solid>, ColliderReport), CollideError> {
    if !(d.concavity.is_finite() && d.concavity >= 0.0) {
        return Err(CollideError::BadValue("concavity is a distance: 0 or more"));
    }
    if (1..4).contains(&d.max_vertices) {
        return Err(CollideError::BadValue(
            "max_vertices is 0 (every corner) or at least 4",
        ));
    }
    if s.triangles.is_empty() {
        return Err(CollideError::Empty);
    }
    if s.positions.iter().any(|p| !p.is_finite()) {
        return Err(CollideError::NotFinite);
    }
    let tris: Vec<Tri> = s
        .triangles
        .iter()
        .map(|t| t.map(|v| s.positions[v as usize]))
        .collect();
    let first = piece(tris).ok_or(CollideError::Flat)?;
    let mut pieces = vec![first];
    let mut cuts = 0;
    let most = d.max_hulls.max(1) as usize;
    while pieces.len() < most {
        // The deepest piece still over the tolerance; the first of equals.
        let Some(at) = (0..pieces.len())
            .filter(|&i| pieces[i].deepest.1 > d.concavity && pieces[i].deepest.1 > 0.0)
            .reduce(|a, b| {
                if pieces[b].deepest.1 > pieces[a].deepest.1 {
                    b
                } else {
                    a
                }
            })
        else {
            break;
        };
        let Some((lo, hi)) = split(&pieces[at]) else {
            // No plane parts it into two pieces with a volume: leave it,
            // and never try it again.
            pieces[at].deepest.1 = 0.0;
            continue;
        };
        pieces[at] = lo;
        pieces.insert(at + 1, hi);
        cuts += 1;
    }

    let mut hulls = Vec::new();
    let mut report = ColliderReport {
        mesh_volume: {
            let m = s.measure();
            m.closed.then_some(m.volume)
        },
        cuts,
        ..Default::default()
    };
    for p in &pieces {
        let (hull, outside) = if d.max_vertices > 0 && hull_len(&p.hull) > d.max_vertices as usize {
            thin(p, d.max_vertices as usize)
        } else {
            (p.hull.clone(), 0.0)
        };
        let h = HullReport {
            vertices: hull.positions.len(),
            triangles: hull.triangles.len(),
            volume: hull.measure().volume,
            concavity: depth(&p.tris, &hull).1,
            outside,
        };
        report.volume += h.volume;
        report.concavity = report.concavity.max(h.concavity);
        report.outside = report.outside.max(h.outside);
        report.hulls.push(h);
        hulls.push(hull);
    }
    Ok((hulls, report))
}

fn hull_len(h: &Solid) -> usize {
    h.positions.len()
}

fn points(tris: &[Tri]) -> Vec<DVec3> {
    tris.iter().flatten().copied().collect()
}

/// The piece of these triangles, if their points have a hull.
fn piece(tris: Vec<Tri>) -> Option<Piece> {
    if tris.is_empty() {
        return None;
    }
    let hull = convex_hull(&points(&tris)).ok()?;
    let deepest = depth(&tris, &hull);
    Some(Piece {
        tris,
        hull,
        deepest,
    })
}

/// The hull's planes: a point and the outward unit normal of each
/// triangle (a measurement: lengths, not sides).
fn planes(hull: &Solid) -> Vec<(DVec3, DVec3)> {
    hull.triangles
        .iter()
        .filter_map(|t| {
            let [a, b, c] = t.map(|v| hull.positions[v as usize]);
            let n = (b - a).cross(c - a);
            let len = n.length();
            (len > 0.0).then(|| (a, n / len))
        })
        .collect()
}

/// The point of `tris` deepest under `hull`, and its depth: over every
/// triangle's corners and centre, the distance from the point along the
/// triangle's outward normal to the hull's surface. A face on the hull
/// has none; the wall of a hollow has the hollow's width. The first of
/// equals in the order given.
fn depth(tris: &[Tri], hull: &Solid) -> (DVec3, f64) {
    let planes = planes(hull);
    let mut best = (tris[0][0], 0.0);
    for t in tris {
        let n = (t[1] - t[0]).cross(t[2] - t[0]);
        let len = n.length();
        if len <= 0.0 {
            continue;
        }
        let n = n / len;
        let centre = (t[0] + t[1] + t[2]) / 3.0;
        for p in [t[0], t[1], t[2], centre] {
            // Where the ray from `p` along `n` leaves the hull: the
            // nearest plane it runs toward.
            let d = planes
                .iter()
                .filter_map(|(a, m)| {
                    let toward = n.dot(*m);
                    (toward > 1e-12).then(|| ((*a - p).dot(*m) / toward).max(0.0))
                })
                .fold(f64::INFINITY, f64::min);
            if d.is_finite() && d > best.1 {
                best = (p, d);
            }
        }
    }
    // A face on the hull is a rounding error under it, not a hollow.
    let (mut lo, mut hi) = (hull.positions[0], hull.positions[0]);
    for &c in &hull.positions {
        lo = lo.min(c);
        hi = hi.max(c);
    }
    if best.1 <= (hi - lo).length() * 1e-9 {
        best.1 = 0.0;
    }
    best
}

/// The piece cut in two by the best of a few axis planes: square to each
/// axis, through its deepest point and at the quarters of its box; of
/// those that leave two pieces with a volume, the one that leaves the
/// shallowest hollow in the deeper of the two, then the one whose two
/// hulls have the least volume together (the first of equals, in that
/// order).
/// `None` if none does.
fn split(p: &Piece) -> Option<(Piece, Piece)> {
    let at = p.deepest.0;
    let (mut lo, mut hi) = (p.hull.positions[0], p.hull.positions[0]);
    for &c in &p.hull.positions {
        lo = lo.min(c);
        hi = hi.max(c);
    }
    let mut best: Option<((f64, f64), Piece, Piece)> = None;
    for axis in 0..3 {
        let (a, b) = (lo[axis], hi[axis]);
        let mut places = vec![at[axis]];
        for quarter in [0.5, 0.25, 0.75] {
            let c = a + (b - a) * quarter;
            if !places.contains(&c) {
                places.push(c);
            }
        }
        for c in places {
            if !(c > a && c < b) {
                continue;
            }
            let (below, above) = clip(&p.tris, axis, c);
            let (Some(below), Some(above)) = (piece(below), piece(above)) else {
                continue;
            };
            // The deeper of the two hollows left, then the hulls' volume.
            let cost = (
                below.deepest.1.max(above.deepest.1),
                below.hull.measure().volume + above.hull.measure().volume,
            );
            if best
                .as_ref()
                .is_none_or(|(c, _, _)| cost.0 < c.0 || (cost.0 == c.0 && cost.1 < c.1))
            {
                best = Some((cost, below, above));
            }
        }
    }
    best.map(|(_, below, above)| (below, above))
}

/// The triangles clipped at the plane `axis = c`: those below (with the
/// plane) and those above (with it). A triangle in the plane goes below.
fn clip(tris: &[Tri], axis: usize, c: f64) -> (Vec<Tri>, Vec<Tri>) {
    let mut lo = Vec::new();
    let mut hi = Vec::new();
    for t in tris {
        let side = t.map(|p| p[axis] - c);
        if side.iter().all(|&s| s <= 0.0) {
            lo.push(*t);
            continue;
        }
        if side.iter().all(|&s| s >= 0.0) {
            hi.push(*t);
            continue;
        }
        // The polygon on each side of the plane, fanned.
        let mut below: Vec<DVec3> = Vec::new();
        let mut above: Vec<DVec3> = Vec::new();
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let (sa, sb) = (side[k], side[(k + 1) % 3]);
            if sa <= 0.0 {
                below.push(a);
            }
            if sa >= 0.0 {
                above.push(a);
            }
            if (sa < 0.0 && sb > 0.0) || (sa > 0.0 && sb < 0.0) {
                // The crossing, from the lower end so both sides get the
                // same point whichever way the edge runs.
                let (l, h, sl, sh) = if sa < 0.0 {
                    (a, b, sa, sb)
                } else {
                    (b, a, sb, sa)
                };
                let mut x = l + (h - l) * (sl / (sl - sh));
                x[axis] = c;
                below.push(x);
                above.push(x);
            }
        }
        for (poly, out) in [(&below, &mut lo), (&above, &mut hi)] {
            for k in 1..poly.len().saturating_sub(1) {
                out.push([poly[0], poly[k], poly[k + 1]]);
            }
        }
    }
    (lo, hi)
}

/// The fixed directions a thinned hull starts from: every vector with
/// whole components from −1 to 1 but zero, nearest the axes first (by
/// length squared, then by components): the six axes, the twelve face
/// diagonals, the eight body diagonals.
fn directions() -> Vec<[i32; 3]> {
    let mut d: Vec<[i32; 3]> = Vec::new();
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                if (x, y, z) != (0, 0, 0) {
                    d.push([x, y, z]);
                }
            }
        }
    }
    d.sort_by_key(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2], *v));
    d
}

/// How far `v` lies outside the solid these planes bound (0 inside).
fn outside_of(planes: &[(DVec3, DVec3)], v: DVec3) -> f64 {
    planes
        .iter()
        .map(|(a, n)| (v - *a).dot(*n))
        .fold(0.0, f64::max)
}

/// The piece's hull thinned to at most `most` vertices, and how far the
/// piece pokes out of it. It starts from the corners furthest along the
/// axes (and along [`directions`] in order, until four have a volume),
/// then adds, one at a time, the corner furthest outside the hull so far
/// (the first of equals: corners are in order of position), so each
/// vertex spent removes the largest error left. The first hull, when no
/// four such corners have a volume.
fn thin(p: &Piece, most: usize) -> (Solid, f64) {
    let corners = &p.hull.positions;
    let furthest = |d: [i32; 3]| {
        let d = DVec3::new(f64::from(d[0]), f64::from(d[1]), f64::from(d[2]));
        let mut best = 0;
        for (i, c) in corners.iter().enumerate() {
            if c.dot(d) > corners[best].dot(d) {
                best = i;
            }
        }
        best
    };
    let hull_of = |keep: &BTreeSet<usize>| {
        let chosen: Vec<DVec3> = keep.iter().map(|&i| corners[i]).collect();
        convex_hull(&chosen).ok()
    };
    let mut keep: BTreeSet<usize> = BTreeSet::new();
    let mut hull = None;
    for (k, d) in directions().into_iter().enumerate() {
        if keep.len() >= most {
            break;
        }
        keep.insert(furthest(d));
        if k >= 5 {
            hull = hull_of(&keep);
            if hull.is_some() {
                break;
            }
        }
    }
    let Some(mut hull) = hull.or_else(|| hull_of(&keep)) else {
        return (p.hull.clone(), 0.0);
    };
    while keep.len() < most {
        let planes = planes(&hull);
        let mut best = (0, 0.0);
        for (i, &c) in corners.iter().enumerate() {
            let d = outside_of(&planes, c);
            if d > best.1 {
                best = (i, d);
            }
        }
        // Nothing left outside, or only a kept corner by a rounding error.
        if best.1 <= 0.0 || !keep.insert(best.0) {
            break;
        }
        match hull_of(&keep) {
            Some(h) => hull = h,
            None => break,
        }
    }
    let planes = planes(&hull);
    let outside = p
        .tris
        .iter()
        .flatten()
        .map(|&v| outside_of(&planes, v))
        .fold(0.0, f64::max);
    (hull, outside)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shape_gen::solids::{BoxDesc, SphereDesc, cuboid, sphere_desc};

    fn block(w: f64, d: f64, h: f64, at: DVec3) -> Solid {
        let mut s = cuboid(&BoxDesc {
            width: w,
            depth: d,
            height: h,
        })
        .unwrap();
        s.map_positions(|p| p + at, false);
        s
    }

    /// An L, 10 thick: a bar along x and a leg along y at its left end.
    fn ell() -> Solid {
        let pts = [
            (-15.0, -5.0),
            (15.0, -5.0),
            (15.0, 5.0),
            (-5.0, 5.0),
            (-5.0, 25.0),
            (-15.0, 25.0),
        ];
        let profile = shape_2d::poly(&shape_2d::Poly {
            points: pts
                .iter()
                .map(|&(x, y)| shape_2d::PolyPoint {
                    x,
                    y,
                    ..Default::default()
                })
                .collect(),
        })
        .unwrap();
        shape_gen::solids::extrude(
            &profile,
            &shape_gen::solids::Extrude {
                height: 10.0,
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn a_convex_mesh_is_one_hull_and_an_l_is_cut_at_its_inner_corner() {
        let cube = block(10.0, 10.0, 10.0, DVec3::ZERO);
        let d = Collide {
            max_hulls: 8,
            ..Default::default()
        };
        let (hulls, r) = colliders(&cube, &d).unwrap();
        assert_eq!((hulls.len(), r.cuts), (1, 0));
        assert_eq!(r.concavity, 0.0);
        assert!((r.mesh_volume.unwrap() - 1000.0).abs() < 1e-9);

        // One hull bridges the L's hollow: its inner corner lies under it.
        let (one, r) = colliders(&ell(), &Collide::default()).unwrap();
        assert_eq!(one.len(), 1);
        assert!(r.concavity > 5.0 && r.volume > 5000.0, "{r:?}");
        // Two fit it exactly: the bar and the leg.
        let (two, r) = colliders(&ell(), &d).unwrap();
        assert_eq!((two.len(), r.cuts), (2, 1));
        assert!(r.concavity < 1e-9, "{r:?}");
        assert!((r.volume - 5000.0).abs() < 1e-9, "{r:?}");
        assert!(
            two.iter()
                .all(|h| h.measure().closed && h.positions.len() == 8)
        );
        // A tolerance over the hollow's depth leaves the one hull.
        let loose = Collide {
            max_hulls: 8,
            concavity: 50.0,
            ..Default::default()
        };
        assert_eq!(colliders(&ell(), &loose).unwrap().0.len(), 1);
    }

    #[test]
    fn a_thinned_hull_has_few_vertices_and_says_how_far_the_mesh_pokes_out() {
        let ball = sphere_desc(&SphereDesc {
            radius: 10.0,
            ..Default::default()
        })
        .unwrap();
        let (full, r) = colliders(&ball, &Collide::default()).unwrap();
        assert_eq!(full[0].positions.len(), ball.positions.len());
        assert_eq!(r.outside, 0.0);
        let thin = Collide {
            max_vertices: 14,
            ..Default::default()
        };
        let (hulls, r) = colliders(&ball, &thin).unwrap();
        assert!(hulls[0].positions.len() <= 14 && hulls[0].measure().closed);
        assert!(r.outside > 0.5 && r.outside < 3.0, "{r:?}");
        // A vertex spent where the error is largest: more vertices, less outside.
        let more = Collide {
            max_vertices: 32,
            ..Default::default()
        };
        assert!(colliders(&ball, &more).unwrap().1.outside < r.outside);
        assert!(r.volume < full[0].measure().volume);
    }

    #[test]
    fn a_flat_or_empty_mesh_and_bad_values_are_errors() {
        let mut sheet = Solid::default();
        let v = [DVec3::ZERO, DVec3::X, DVec3::Y].map(|p| sheet.push_vertex(p));
        sheet.push_triangle(v, 0, &[]);
        assert_eq!(
            colliders(&sheet, &Collide::default()),
            Err(CollideError::Flat)
        );
        assert_eq!(
            colliders(&Solid::default(), &Collide::default()),
            Err(CollideError::Empty)
        );
        let bad = Collide {
            max_vertices: 3,
            ..Default::default()
        };
        assert!(matches!(
            colliders(&ell(), &bad),
            Err(CollideError::BadValue(_))
        ));
    }
}
