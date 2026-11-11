//! The hollow of a shell, piece by piece: [`pieced`].
//!
//! A hollow can pinch off. A pocket in a case, hollowed with walls thicker
//! than half the neck beside the pocket: the pocket's own wall reaches
//! through the case's, and the hollow is two chambers. No face of the
//! copied inner solid turns over there (`collapse`); a face is cut in two,
//! and its triangles, stretched between an outline that shrank and a hole
//! that grew past it, lie over one another (`docs/research.md` §38).
//!
//! What grew past what is plain once the solid is taken apart. Where a
//! surface has a hole (a loop of its boundary it lies outside of: a
//! pocket's or a hole's mouth, or the foot of something standing on it),
//! the hole is closed twice: with a patch for the surface's side, and
//! the same triangles facing the other way, a lid, for what lies through
//! the hole. The solid's surface then falls into closed pieces: bodies,
//! facing out, and pockets, holes and voids, facing in. Each is a solid
//! of its own, usually a simple one, and its inner solid is found as any
//! shell's is (the copy; its outside; its planes; its turned faces
//! collapsed): a body's walls move in, a pocket's move out into the
//! material. The hollow is the bodies joined, less the pockets, by the
//! exact booleans, which cut a body in two where a pocket reaches
//! through it.
//!
//! Patches and lids hold nothing in place: they follow the rim they
//! close, so the walls round a hole move as if it were open, and are
//! made again on the rim as moved (a tent over it). A pocket's lid thus stays in the
//! solid's own surface, a wall above the body's inner face there, and
//! the two are never in one plane. A body's lid (the foot of a boss or a
//! handle) is carried a wall and a half on, through the wall it stands
//! on, so its hollow opens into the one beyond. A piece with no room for
//! its walls (a boss thinner than two of them) is solid through and adds
//! nothing.

use std::collections::BTreeMap;

use shape_bool::{BoolOp, Boolean, boolean_report, crossing_pairs, self_union};
use shape_core::{DVec2, DVec3};
use shape_mesh::{CornerUvs, Solid};

use crate::{collapse, inset, offset};

/// A triangle whose doubled area is under this share of its longest
/// side squared is a needle, and has no plane to speak of.
const NEEDLE: f64 = 1e-9;

/// A rim of more points than this is not filled (the ears are found one
/// by one, each against every point).
const MAX_RIM: usize = 2_000;

/// A triangle of the solid taken apart.
struct Tri {
    v: [u32; 3],
    /// The triangle of the solid it is, or beside whose hole it lies.
    src: usize,
    /// How far its plane moves along its own normal; for a lid, how far
    /// the patch it mirrors moves along the patch's.
    target: f64,
    lid: bool,
    /// A patch over a hole, on the surface's side.
    patch: bool,
    uv: Option<CornerUvs>,
}

/// The hollow of `s` with its planes moved by `targets` (per triangle,
/// along its normal: the wall `t` in for a kept face), built from the
/// pieces `s` falls into when the holes of its surfaces are closed.
/// `None` when no surface has a hole, a piece's inner solid cannot be
/// found clean, or the pieces do not combine.
pub(crate) fn pieced(s: &Solid, targets: &[f64], t: f64, diag: f64) -> Option<Solid> {
    let nt = s.triangles.len();
    let normal: Vec<DVec3> = s
        .triangles
        .iter()
        .map(|tri| {
            let [a, b, c] = tri.map(|v| s.positions[v as usize]);
            (b - a).cross(c - a).normalize_or_zero()
        })
        .collect();
    let mut dir: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for (i, tri) in s.triangles.iter().enumerate() {
        for k in 0..3 {
            if dir.insert((tri[k], tri[(k + 1) % 3]), i).is_some() {
                return None;
            }
        }
    }
    // Surfaces: triangles of one surface id, across edges.
    let mut face: Vec<usize> = (0..nt).collect();
    for (i, tri) in s.triangles.iter().enumerate() {
        for k in 0..3 {
            let j = *dir.get(&(tri[(k + 1) % 3], tri[k]))?;
            if s.surfaces[i] == s.surfaces[j] {
                let (a, b) = (find(&mut face, i), find(&mut face, j));
                face[a.max(b)] = a.min(b);
            }
        }
    }
    let mut faces: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..nt {
        faces.entry(find(&mut face, i)).or_default().push(i);
    }
    let mut tris: Vec<Tri> = (0..nt)
        .map(|i| Tri {
            v: s.triangles[i],
            src: i,
            target: targets[i],
            lid: false,
            patch: false,
            uv: s.uvs.as_ref().map(|u| u[i]),
        })
        .collect();
    // Which triangles are one piece: across every edge but a hole's rim,
    // where the surface goes on into its patch and what lies through
    // the hole into its lid.
    let mut piece: Vec<usize> = (0..nt).collect();
    let mut rims: BTreeMap<(u32, u32), ()> = BTreeMap::new();
    let mut joins: Vec<(usize, usize)> = Vec::new();
    let mut closes: Vec<Close> = Vec::new();
    for (&root, members) in &faces {
        // The surface's boundary: its edges with no twin in it, as loops.
        let mut next: BTreeMap<u32, (u32, usize)> = BTreeMap::new();
        for &i in members {
            let tri = s.triangles[i];
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                let j = *dir.get(&(b, a))?;
                if find(&mut face, j) != root && next.insert(a, (b, i)).is_some() {
                    // Pinched at a vertex: not taken apart.
                    return None;
                }
            }
        }
        while let Some((&start, _)) = next.iter().next() {
            let mut ring = vec![start];
            let mut beside = Vec::new();
            let (mut at, i) = next.remove(&start)?;
            beside.push(i);
            while at != start {
                ring.push(at);
                let (b, i) = next.remove(&at)?;
                beside.push(i);
                at = b;
            }
            // A hole: the surface lies outside the loop, so the loop,
            // with the surface on its left, turns against the surface's
            // own normal there.
            let p = |v: u32| s.positions[v as usize];
            let centre = ring.iter().map(|&v| p(v)).sum::<DVec3>() / ring.len() as f64;
            let mut turn = DVec3::ZERO;
            for k in 0..ring.len() {
                turn += (p(ring[k]) - centre).cross(p(ring[(k + 1) % ring.len()]) - centre);
            }
            let turn = turn.normalize_or_zero();
            let out = beside
                .iter()
                .map(|&i| normal[i])
                .sum::<DVec3>()
                .normalize_or_zero();
            if turn.dot(out) > -0.5 {
                continue;
            }
            let at: Vec<DVec3> = ring.iter().map(|&v| p(v)).collect();
            let filled = fill(&at)?;
            let target = beside
                .iter()
                .map(|&i| targets[i])
                .fold(f64::NEG_INFINITY, f64::max);
            let rep = beside[0];
            let rep_at = s.triangles[rep].map(&p);
            let (patch0, lid0) = (tris.len(), tris.len() + filled.len());
            closes.push(Close {
                ring: ring.clone(),
                patch0,
                lid0,
                rep,
            });
            for lid in [false, true] {
                for tri in &filled {
                    // As filled, the triangles turn with the loop: the
                    // lid, facing in through the hole. The patch is the
                    // same the other way round.
                    let mut v = tri.map(|k| ring[k]);
                    if !lid {
                        v.swap(1, 2);
                    }
                    let uv = s
                        .uvs
                        .as_ref()
                        .map(|uvs| v.map(|x| inset::along(rep_at, uvs[rep], p(x))));
                    tris.push(Tri {
                        v,
                        src: rep,
                        target,
                        lid,
                        patch: !lid,
                        uv,
                    });
                    piece.push(piece.len());
                }
            }
            for k in 0..filled.len() {
                joins.push((patch0, patch0 + k));
                joins.push((lid0, lid0 + k));
            }
            // Across the rim: the surface with the patch, what lies
            // through the hole with the lid.
            for k in 0..ring.len() {
                let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
                rims.insert((a, b), ());
                rims.insert((b, a), ());
                joins.push((beside[k], patch0));
                joins.push((*dir.get(&(b, a))?, lid0));
            }
        }
    }
    if rims.is_empty() {
        return None;
    }
    for (i, tri) in s.triangles.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            if !rims.contains_key(&(a, b)) {
                joins.push((i, *dir.get(&(b, a))?));
            }
        }
    }
    for (a, b) in joins {
        let (a, b) = (find(&mut piece, a), find(&mut piece, b));
        piece[a.max(b)] = a.min(b);
    }
    let mut pieces: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..tris.len() {
        pieces.entry(find(&mut piece, i)).or_default().push(i);
    }
    let mut bodies: Vec<Solid> = Vec::new();
    let mut pockets: Vec<Solid> = Vec::new();
    for members in pieces.values() {
        let mut c = Solid::default();
        let mut index: BTreeMap<u32, u32> = BTreeMap::new();
        let mut volume = 0.0;
        for &i in members {
            let tri = &tris[i];
            let [p, q, r] = tri.v.map(|v| s.positions[v as usize]);
            volume += p.dot(q.cross(r));
            let v = tri.v.map(|v| {
                *index
                    .entry(v)
                    .or_insert_with(|| c.push_vertex(s.positions[v as usize]))
            });
            match tri.uv {
                Some(uv) => c.push_triangle_uv(v, s.surfaces[tri.src], &["wall"], uv),
                None => c.push_triangle(v, s.surfaces[tri.src], &["wall"]),
            }
        }
        if volume == 0.0 {
            return None;
        }
        let body = volume > 0.0;
        if !body {
            // A pocket, hole or void faces in: turned out.
            for i in 0..c.triangles.len() {
                c.reverse_triangle(i);
            }
        }
        // Along each triangle's normal as it now stands. Patches and
        // lids have no plane of their own: they follow the rim, so the
        // walls round a hole move as if it were not closed.
        let moves: Vec<f64> = members
            .iter()
            .map(|&i| {
                if body {
                    tris[i].target
                } else {
                    -tris[i].target
                }
            })
            .collect();
        let free: Vec<bool> = members
            .iter()
            .map(|&i| tris[i].patch || tris[i].lid)
            .collect();
        // The holes this piece has closed, to be filled again on the
        // rim as it stands once moved.
        let root = find(&mut piece, members[0]);
        let mut refills = Vec::new();
        for close in &closes {
            for (first, lid) in [(close.patch0, false), (close.lid0, true)] {
                if find(&mut piece, first) != root {
                    continue;
                }
                let rep_at = s.triangles[close.rep].map(|v| s.positions[v as usize]);
                // The loop turns towards what lies through the hole. A
                // lid faces that way, and so out of a body it closes; a
                // pocket is turned out, and a patch faces back.
                let at: Vec<DVec3> = close
                    .ring
                    .iter()
                    .map(|&v| s.positions[v as usize])
                    .collect();
                let centre = at.iter().copied().sum::<DVec3>() / at.len() as f64;
                let mut turn = DVec3::ZERO;
                for k in 0..at.len() {
                    turn += (at[k] - centre).cross(at[(k + 1) % at.len()] - centre);
                }
                let turn = turn.normalize_or_zero();
                let out = if lid && body { turn } else { -turn };
                // A body's lid (the foot of a boss or a handle) is carried
                // a wall and a half on, through the wall it stands on,
                // so its hollow opens into the one beyond. A pocket's lid
                // in an open face is carried out past where that face's
                // own hollow ends, so nothing of it is left over the
                // mouth, in the solid's surface.
                let face = tris[first].target;
                let push = if lid && body {
                    1.5 * t
                } else if lid && face > 0.0 {
                    face + 0.5 * t
                } else {
                    0.0
                };
                let d = at
                    .iter()
                    .map(|p| out.dot(*p))
                    .fold(f64::NEG_INFINITY, f64::max)
                    + push;
                let tri = members.iter().position(|&i| i == first)?;
                refills.push(Refill {
                    cap: lid.then_some((out, d)),
                    push: out * push,
                    tent: out * (0.5 * t),
                    tri,
                    ring: close.ring.iter().map(|v| index[v]).collect(),
                    // A lid turns with its rim, a patch against it; a
                    // pocket is turned out.
                    with_ring: lid == body,
                    surface: s.surfaces[close.rep],
                    uvs: s.uvs.as_ref().map(|u| {
                        close
                            .ring
                            .iter()
                            .map(|&v| inset::along(rep_at, u[close.rep], s.positions[v as usize]))
                            .collect()
                    }),
                });
            }
        }
        match hollow_of(&c, &moves, &free, &refills, diag)? {
            Some(inner) if body => bodies.push(inner),
            Some(inner) => pockets.push(inner),
            // Solid through: it adds nothing to the hollow.
            None => {}
        }
    }
    let mut bodies = bodies.into_iter();
    let mut hollow = bodies.next()?;
    let op = |op: BoolOp| Boolean {
        op,
        target: "",
        tool: "",
    };
    for b in bodies {
        hollow = boolean_report(&hollow, &b, &op(BoolOp::Join)).ok()?.0;
    }
    for p in &pockets {
        hollow = boolean_report(&hollow, p, &op(BoolOp::Cut)).ok()?.0;
    }
    clean(&hollow).then_some(hollow)
}

/// A hole closed: its rim, with the surface on its left, and where its
/// patch's and its lid's triangles start.
struct Close {
    ring: Vec<u32>,
    patch0: usize,
    lid0: usize,
    /// A triangle of the surface beside the rim.
    rep: usize,
}

/// A free closing of one piece (a body's patch, a pocket's lid), by the
/// piece's own vertices.
struct Refill {
    ring: Vec<u32>,
    /// Whether its triangles turn with the ring.
    with_ring: bool,
    /// For a lid, the plane across the mouth, facing out of the piece,
    /// through the rim's furthest point that way: `n · x = d`.
    cap: Option<(DVec3, f64)>,
    /// How far the rim is carried on once moved (a body's lid).
    push: DVec3,
    /// Out of the piece across the mouth, half a wall long: where the
    /// tent that closes it again has its top, from the rim's middle.
    tent: DVec3,
    /// One of its triangles in the piece.
    tri: usize,
    surface: u32,
    uvs: Option<Vec<DVec2>>,
}

/// A loop filled with triangles on its own points (indices into `ring`),
/// turning with it, in the plane across it. `None` when the loop, seen
/// across, crosses itself or folds.
fn fill(ring: &[DVec3]) -> Option<Vec<[usize; 3]>> {
    let centre = ring.iter().copied().sum::<DVec3>() / ring.len() as f64;
    let mut turn = DVec3::ZERO;
    for k in 0..ring.len() {
        turn += (ring[k] - centre).cross(ring[(k + 1) % ring.len()] - centre);
    }
    let turn = turn.normalize_or_zero();
    let x = if turn.x.abs() < 0.9 {
        DVec3::X
    } else {
        DVec3::Y
    };
    let u = turn.cross(x).normalize_or_zero();
    let w = turn.cross(u);
    let pts: Vec<DVec2> = ring
        .iter()
        .map(|&p| DVec2::new((p - centre).dot(u), (p - centre).dot(w)))
        .collect();
    // Ears, cut one at a time: a corner that turns with the loop and
    // holds no other point, inside or on its sides (so points in a line
    // along the rim never make a triangle without area). Sides decided
    // exactly.
    use core::cmp::Ordering::{Greater, Less};
    use shape_core::predicates::orient2d;
    if pts.len() > MAX_RIM {
        return None;
    }
    let mut left: Vec<usize> = (0..pts.len()).collect();
    let mut out = Vec::with_capacity(pts.len().saturating_sub(2));
    while left.len() > 3 {
        let n = left.len();
        let ear = (0..n).find(|&i| {
            let (a, b, c) = (left[(i + n - 1) % n], left[i], left[(i + 1) % n]);
            let (pa, pb, pc) = (pts[a], pts[b], pts[c]);
            orient2d(pa, pb, pc) == Greater
                && left.iter().all(|&q| {
                    q == a
                        || q == b
                        || q == c
                        || orient2d(pa, pb, pts[q]) == Less
                        || orient2d(pb, pc, pts[q]) == Less
                        || orient2d(pc, pa, pts[q]) == Less
                })
        })?;
        out.push([left[(ear + n - 1) % n], left[ear], left[(ear + 1) % n]]);
        left.remove(ear);
    }
    let last = [left[0], left[1], left[2]];
    (orient2d(pts[last[0]], pts[last[1]], pts[last[2]]) == Greater).then(|| {
        out.push(last);
        out
    })
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Closed, with volume, crossing itself nowhere.
fn clean(s: &Solid) -> bool {
    let m = s.measure();
    m.closed && m.volume > 0.0 && crossing_pairs(s) == 0
}

/// One piece's inner solid, or that it has none: a convex piece with no
/// point a wall from every face (a boss thinner than two walls) is solid
/// through.
fn hollow_of(
    c: &Solid,
    targets: &[f64],
    free: &[bool],
    refills: &[Refill],
    diag: f64,
) -> Option<Option<Solid>> {
    let caps: Vec<(DVec3, f64, usize)> = refills
        .iter()
        .filter_map(|r| r.cap.map(|(n, d)| (n, d, r.tri)))
        .collect();
    if let inset::Inset::NoRoom(_) = inset::inset(c, targets, free, &caps, diag) {
        return Some(None);
    }
    inner_of(c, targets, free, refills, &caps, diag)
}

/// One piece's inner solid, the ways `shell` finds one, the first clean;
/// or that its copy came out inside out, and it has none.
fn inner_of(
    c: &Solid,
    targets: &[f64],
    free: &[bool],
    refills: &[Refill],
    caps: &[(DVec3, f64, usize)],
    diag: f64,
) -> Option<Option<Solid>> {
    let nv = c.positions.len();
    let mut planes: Vec<Vec<(DVec3, f64)>> = vec![Vec::new(); nv];
    // Triangles that hold no vertex: the closings, and needles.
    let mut loose = free.to_vec();
    let mut needles: Vec<usize> = Vec::new();
    for (i, tri) in c.triangles.iter().enumerate() {
        let [p, q, r] = tri.map(|v| c.positions[v as usize]);
        if free[i] {
            continue;
        }
        // A needle (a boolean leaves triangles 1e-15 wide on a hole's
        // wall) has a normal that is rounding, not a plane: it would
        // throw its corners anywhere.
        let area = (q - p).cross(r - p);
        let longest = (q - p)
            .length_squared()
            .max((r - q).length_squared())
            .max((p - r).length_squared());
        if area.length() <= NEEDLE * longest {
            needles.push(i);
            loose[i] = true;
            continue;
        }
        let n = area.normalize_or_zero();
        for &v in tri {
            planes[v as usize].push((n, targets[i]));
        }
    }
    // A vertex with needles only lies on the line between two others: it
    // goes where it lies between them once they have moved. The first
    // needle that holds it between two held vertices decides.
    let mut between: Vec<(u32, u32, u32, f64)> = Vec::new();
    let mut held: Vec<bool> = planes.iter().map(|p| !p.is_empty()).collect();
    for &i in &needles {
        let tri = c.triangles[i];
        for k in 0..3 {
            let (v, a, b) = (tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]);
            if held[v as usize] || planes[a as usize].is_empty() || planes[b as usize].is_empty() {
                continue;
            }
            let (pv, pa, pb) = (
                c.positions[v as usize],
                c.positions[a as usize],
                c.positions[b as usize],
            );
            let along = (pv - pa).dot(pb - pa) / (pb - pa).length_squared();
            if along > 0.0 && along < 1.0 {
                between.push((v, a, b, along));
                held[v as usize] = true;
            }
        }
    }
    if held.iter().any(|h| !h) {
        return None;
    }
    // A rim's points in a line between two of its corners are not
    // corners: the faces beyond the rim were cut there. Each has one wall
    // to move it, and where the wall's corner slides along it (a hollow
    // corner, as the wall shrinks) it would be left behind, past the
    // wall's new end. It keeps its place between the two corners instead.
    let mut spans: Vec<(u32, u32, u32, f64)> = Vec::new();
    for r in refills {
        let n = r.ring.len();
        let p = |k: usize| c.positions[r.ring[k % n] as usize];
        let corner: Vec<bool> = (0..n)
            .map(|k| {
                let (d0, d1) = (p(k) - p(k + n - 1), p(k + 1) - p(k));
                d0.cross(d1).length() > 1e-9 * d0.length() * d1.length()
            })
            .collect();
        let Some(first) = corner.iter().position(|&c| c) else {
            continue;
        };
        let mut a = first;
        for step in 1..=n {
            let k = (first + step) % n;
            if !corner[k] {
                continue;
            }
            // Corners `a` and `k`, the points between them in a line.
            let whole = (p(k) - p(a)).length();
            let mut m = (a + 1) % n;
            while m != k {
                if whole > 0.0 {
                    let along = (p(m) - p(a)).length() / whole;
                    spans.push((r.ring[m], r.ring[a], r.ring[k], along));
                }
                m = (m + 1) % n;
            }
            a = k;
        }
    }
    // The vertices as moved by their planes, then the rest placed.
    let place = |mut moved: Vec<DVec3>| -> Vec<DVec3> {
        for &(v, a, b, along) in spans.iter().chain(&between) {
            moved[v as usize] = moved[a as usize] + (moved[b as usize] - moved[a as usize]) * along;
        }
        for r in refills {
            for &v in &r.ring {
                moved[v as usize] += r.push;
            }
        }
        moved
    };
    // The piece's own triangles as they are. Its closings are made again
    // on their rims as moved: a rim's points do not move alike (those on
    // a wall's edge slide along it), so the first filling need not fit
    // them. The rim is filled again where it can be; where it folds by a
    // hair (points on a curved wall), a tent over it, its top half a wall
    // out of the piece, closes it whatever the rim does, as long as it
    // does not cross itself seen from the top.
    let build = |moved: &[DVec3]| -> Option<Solid> {
        let mut copy = Solid::default();
        for p in moved {
            copy.push_vertex(*p);
        }
        for (i, tri) in c.triangles.iter().enumerate() {
            if free[i] {
                continue;
            }
            match &c.uvs {
                Some(u) => copy.push_triangle_uv(*tri, c.surfaces[i], &["wall"], u[i]),
                None => copy.push_triangle(*tri, c.surfaces[i], &["wall"]),
            }
        }
        for r in refills {
            let n = r.ring.len();
            let at: Vec<DVec3> = r.ring.iter().map(|&v| moved[v as usize]).collect();
            if let Some(filled) = fill(&at) {
                for tri in filled {
                    let mut k = tri;
                    if !r.with_ring {
                        k.swap(1, 2);
                    }
                    let v = k.map(|x| r.ring[x]);
                    match &r.uvs {
                        Some(u) => {
                            copy.push_triangle_uv(v, r.surface, &["wall"], k.map(|x| u[x]));
                        }
                        None => copy.push_triangle(v, r.surface, &["wall"]),
                    }
                }
                continue;
            }
            let middle = at.iter().copied().sum::<DVec3>() / n as f64;
            let top = copy.push_vertex(middle + r.tent);
            let top_uv = r
                .uvs
                .as_ref()
                .map(|u| u.iter().copied().sum::<DVec2>() / n as f64);
            for k in 0..n {
                let k1 = (k + 1) % n;
                let (a, b) = if r.with_ring { (k, k1) } else { (k1, k) };
                let v = [r.ring[a], r.ring[b], top];
                match (&r.uvs, top_uv) {
                    (Some(u), Some(t)) => {
                        copy.push_triangle_uv(v, r.surface, &["wall"], [u[a], u[b], t]);
                    }
                    _ => copy.push_triangle(v, r.surface, &["wall"]),
                }
            }
        }
        copy.canonicalize().ok()?;
        Some(copy)
    };
    let moved = place(
        c.positions
            .iter()
            .enumerate()
            .map(|(v, p)| *p + offset(&planes[v]))
            .collect(),
    );
    // Inside out: most of the copy's walls face against their own. A
    // tube no wider than two walls comes out through its own middle,
    // whole and the right way round, and need not cross itself
    // (`docs/research.md` §35): it has no room, and is solid through. A
    // copy with room keeps its walls facing as they did, but for the
    // few narrower than the wall.
    let mut facing = 0.0;
    let mut whole = 0.0;
    for (i, tri) in c.triangles.iter().enumerate() {
        if loose[i] {
            continue;
        }
        let [p, q, r] = tri.map(|v| c.positions[v as usize]);
        let [p1, q1, r1] = tri.map(|v| moved[v as usize]);
        let n = (q - p).cross(r - p);
        whole += n.length();
        if (q1 - p1).cross(r1 - p1).dot(n) > 0.0 {
            facing += n.length();
        }
    }
    if facing < 0.5 * whole {
        return Some(None);
    }
    let copy = build(&moved)?;
    if clean(&copy) {
        return Some(Some(copy));
    }
    // A convex piece's planes first: exact, and no leftovers of a fold
    // for the booleans to touch on.
    if let inset::Inset::Inner(i) = inset::inset(c, targets, free, caps, diag)
        && clean(&i)
    {
        return Some(Some(*i));
    }
    if let Some(u) = self_union(&copy).ok().filter(clean) {
        return Some(Some(u));
    }
    // The walls that turned over collapsed (`collapse`), the closings
    // made again on what that leaves.
    let at = collapse::collapsed_positions(c, targets, &loose, diag)?;
    build(&place(at)).filter(clean).map(Some)
}
