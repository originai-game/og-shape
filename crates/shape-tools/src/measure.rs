//! Measures of a mesh, as JSON, with no thresholds
//! (`docs/design.md` §7): counts, bounds, pivot, area, volume, topology,
//! floaters, self-intersections, UV coverage and overlap, and texture
//! memory once decoded. The guides say what values are good; the agent
//! judges.
//!
//! A skinned solid is measured as it is, in its bind pose, and its skin
//! is reported (`skinned`, bones, weighted vertices).
//!
//! Topology is measured on the solid as if exactly coincident vertices
//! were one, so an imported mesh split at UV seams is measured as the
//! surface it is.

use core::cmp::Ordering;
use std::collections::BTreeMap;

use shape_core::order::{cmp_dvec3, sorted_order};
use shape_core::predicates::orient2d;
use shape_core::{DVec2, DVec3};
use shape_mesh::{Skin, Solid, edge_key};

use crate::image::ImageInfo;
use crate::intersect::{degenerate, self_intersections, tris_overlap_2d};
use crate::json::Json;

/// What to measure besides the geometry. The zero value measures the
/// geometry, and the UVs the solid carries ([`Solid::uvs`]: imported,
/// carried through the tools, or from [`crate::unwrap()`]).
#[derive(Clone, Copy, Debug, Default)]
pub struct Measure<'a> {
    /// The textures the model uses.
    pub images: &'a [ImageInfo],
    /// Texels across the square the UV coverage is sampled at; 0 is 1024.
    pub uv_resolution: u32,
    /// Count pairs of triangles that pass through each other (exact).
    pub intersections: bool,
}

/// One connected piece.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Piece {
    /// Its triangles.
    pub triangles: usize,
    /// Its bounding box's diagonal, mm.
    pub diagonal: f64,
}

/// UV measures.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UvMeasures {
    /// Sum of the UV triangles' areas (the unit square is 1); overlaps
    /// count twice.
    pub area: f64,
    /// Fraction of texel centres in the unit square covered by a triangle.
    pub coverage: f64,
    /// Fraction of texel centres covered by two triangles or more.
    pub overlap: f64,
    /// Pairs of UV triangles whose interiors overlap (exact).
    pub overlapping_pairs: usize,
    /// UV triangles wound clockwise (mirrored against their face).
    pub flipped: usize,
    /// UV triangles of zero area.
    pub zero_area: usize,
    /// Triangles with a corner outside the unit square.
    pub outside: usize,
}

/// Texture memory.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextureMeasures {
    /// The images, with their sizes.
    pub images: Vec<ImageInfo>,
    /// Bytes once decoded to RGBA8, all images whose size is known.
    pub decoded_bytes: u64,
    /// The same with full mip chains.
    pub decoded_bytes_with_mips: u64,
    /// Images whose size could not be read.
    pub unknown: usize,
}

/// Everything [`measure`] reports.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshMeasures {
    /// Triangles.
    pub triangles: usize,
    /// Vertices as stored.
    pub vertices: usize,
    /// Distinct vertex positions.
    pub positions: usize,
    /// Bounding box, low corner, mm.
    pub bounds_min: DVec3,
    /// Bounding box, high corner, mm.
    pub bounds_max: DVec3,
    /// The model's origin less the centre of its box's base (lowest z):
    /// where the pivot sits relative to the base, mm.
    pub pivot_from_base: DVec3,
    /// Surface area, mm².
    pub area: f64,
    /// Signed volume, mm³ (positive for a closed solid facing out).
    pub volume: f64,
    /// Every edge has exactly two triangles running it opposite ways.
    pub closed: bool,
    /// Edges with one triangle.
    pub boundary_edges: usize,
    /// Rims of holes: loops of boundary edges (connected).
    pub boundary_loops: usize,
    /// Edges with more than two triangles.
    pub non_manifold_edges: usize,
    /// Edges with two triangles running it the same way (faces disagree).
    pub inconsistent_edges: usize,
    /// The shortest edge between distinct positions, mm (0 if none).
    /// Positions closer than f32 can tell apart in metres (about 10⁻⁵ mm
    /// at 0.1 m) merge on export.
    pub shortest_edge: f64,
    /// Triangles with no area (three corners in a line, exactly).
    pub degenerate_triangles: usize,
    /// Triangles that repeat another's corners.
    pub duplicate_triangles: usize,
    /// Connected pieces, largest (by diagonal) first; all but the first
    /// are floaters for the agent to judge.
    pub pieces: Vec<Piece>,
    /// Pairs of triangles sharing no vertex that meet, if asked.
    pub self_intersections: Option<usize>,
    /// UV measures, if UVs were given.
    pub uv: Option<UvMeasures>,
    /// Texture memory.
    pub texture: TextureMeasures,
    /// The solid's skin, if it is skinned (bones, weighted vertices).
    pub skin: Option<Skin>,
}

/// Measures `s` (and the UVs and images `d` gives).
pub fn measure(s: &Solid, d: &Measure) -> MeshMeasures {
    let mut m = MeshMeasures {
        triangles: s.triangles.len(),
        vertices: s.positions.len(),
        skin: s.skin.clone(),
        ..Default::default()
    };
    // Exact weld, as ids only.
    let order = sorted_order(&s.positions);
    let mut id = vec![0u32; s.positions.len()];
    let mut count = 0u32;
    for (k, &v) in order.iter().enumerate() {
        if k > 0 && cmp_dvec3(s.positions[order[k - 1] as usize], s.positions[v as usize]).is_ne() {
            count += 1;
        }
        id[v as usize] = count;
    }
    m.positions = if s.positions.is_empty() {
        0
    } else {
        count as usize + 1
    };
    let tris: Vec<[u32; 3]> = s
        .triangles
        .iter()
        .map(|t| t.map(|v| id[v as usize]))
        .collect();
    if let Some(&first) = s.positions.first() {
        let (mut lo, mut hi) = (first, first);
        for &p in &s.positions {
            lo = lo.min(p);
            hi = hi.max(p);
        }
        m.bounds_min = lo;
        m.bounds_max = hi;
        let base = DVec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, lo.z);
        m.pivot_from_base = DVec3::ZERO - base;
    }
    let mut directed: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut undirected: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut seen: BTreeMap<[u32; 3], ()> = BTreeMap::new();
    for (i, t) in tris.iter().enumerate() {
        let [a, b, c] = s.triangles[i].map(|v| s.positions[v as usize]);
        m.area += (b - a).cross(c - a).length() / 2.0;
        m.volume += a.dot(b.cross(c)) / 6.0;
        if degenerate([a, b, c]) {
            m.degenerate_triangles += 1;
        }
        for (p, q) in [(a, b), (b, c), (c, a)] {
            let l = (q - p).length();
            if l > 0.0 && (m.shortest_edge == 0.0 || l < m.shortest_edge) {
                m.shortest_edge = l;
            }
        }
        let mut key = *t;
        key.sort_unstable();
        if seen.insert(key, ()).is_some() {
            m.duplicate_triangles += 1;
        }
        for k in 0..3 {
            let (x, y) = (t[k], t[(k + 1) % 3]);
            if x == y {
                continue;
            }
            *directed.entry((x, y)).or_default() += 1;
            *undirected.entry(edge_key(x, y)).or_default() += 1;
        }
    }
    let mut parent: Vec<u32> = (0..m.positions as u32).collect();
    fn find(p: &mut [u32], mut i: u32) -> u32 {
        while p[i as usize] != i {
            p[i as usize] = p[p[i as usize] as usize];
            i = p[i as usize];
        }
        i
    }
    let mut boundary_vertices: Vec<u32> = Vec::new();
    for (&(a, b), &n) in &undirected {
        match n {
            1 => {
                m.boundary_edges += 1;
                let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                if ra != rb {
                    parent[ra.max(rb) as usize] = ra.min(rb);
                }
                boundary_vertices.extend([a, b]);
            }
            2 => {
                if directed.get(&(a, b)) != Some(&1) {
                    m.inconsistent_edges += 1;
                }
            }
            _ => m.non_manifold_edges += 1,
        }
    }
    let mut rims: Vec<u32> = boundary_vertices
        .iter()
        .map(|&v| find(&mut parent, v))
        .collect();
    rims.sort_unstable();
    rims.dedup();
    m.boundary_loops = rims.len();
    m.closed = !tris.is_empty()
        && m.boundary_edges == 0
        && m.non_manifold_edges == 0
        && m.inconsistent_edges == 0;
    // Pieces.
    let mut parent: Vec<u32> = (0..m.positions as u32).collect();
    for t in &tris {
        for k in 1..3 {
            let (a, b) = (find(&mut parent, t[0]), find(&mut parent, t[k]));
            if a != b {
                parent[a.max(b) as usize] = a.min(b);
            }
        }
    }
    let mut boxes: BTreeMap<u32, (DVec3, DVec3, usize)> = BTreeMap::new();
    for (i, t) in tris.iter().enumerate() {
        let r = find(&mut parent, t[0]);
        let e = boxes.entry(r).or_insert((
            DVec3::splat(f64::INFINITY),
            DVec3::splat(f64::NEG_INFINITY),
            0,
        ));
        e.2 += 1;
        for &v in &s.triangles[i] {
            e.0 = e.0.min(s.positions[v as usize]);
            e.1 = e.1.max(s.positions[v as usize]);
        }
    }
    m.pieces = boxes
        .values()
        .map(|(lo, hi, n)| Piece {
            triangles: *n,
            diagonal: (*hi - *lo).length(),
        })
        .collect();
    m.pieces.sort_by(|a, b| {
        b.diagonal
            .total_cmp(&a.diagonal)
            .then(b.triangles.cmp(&a.triangles))
    });
    if d.intersections {
        let (positions, triangles) = weld_exact(s);
        m.self_intersections = Some(self_intersections(&positions, &triangles).len());
    }
    if let Some(uv) = &s.uvs {
        m.uv = Some(uv_measures(
            uv,
            if d.uv_resolution == 0 {
                1024
            } else {
                d.uv_resolution
            },
        ));
    }
    let mut t = TextureMeasures {
        images: d.images.to_vec(),
        ..Default::default()
    };
    for img in d.images {
        match img.decoded_bytes() {
            Some((a, b)) => {
                t.decoded_bytes += a;
                t.decoded_bytes_with_mips += b;
            }
            None => t.unknown += 1,
        }
    }
    m.texture = t;
    m
}

/// A texel centre on a triangle's edge belongs to it when the edge runs up,
/// or flat and left: opposite edges of neighbours never both take it.
fn takes(a: DVec2, b: DVec2) -> bool {
    let d = b - a;
    d.y > 0.0 || (d.y == 0.0 && d.x < 0.0)
}

fn uv_measures(uv: &[[DVec2; 3]], res: u32) -> UvMeasures {
    let mut m = UvMeasures::default();
    let r = res as usize;
    let mut hits = vec![0u8; r * r];
    let mut live: Vec<u32> = Vec::new();
    for (i, t) in uv.iter().enumerate() {
        if t.iter()
            .any(|q| q.x < 0.0 || q.x > 1.0 || q.y < 0.0 || q.y > 1.0)
        {
            m.outside += 1;
        }
        let t = match orient2d(t[0], t[1], t[2]) {
            Ordering::Greater => *t,
            Ordering::Less => {
                m.flipped += 1;
                [t[0], t[2], t[1]]
            }
            Ordering::Equal => {
                m.zero_area += 1;
                continue;
            }
        };
        live.push(i as u32);
        m.area += (t[1] - t[0]).perp_dot(t[2] - t[0]) / 2.0;
        let lo = t[0].min(t[1]).min(t[2]);
        let hi = t[0].max(t[1]).max(t[2]);
        let span = |a: f64, b: f64| {
            let x0 = (a * res as f64 - 0.5).ceil().max(0.0) as i64;
            let x1 = (b * res as f64 - 0.5).floor().min(res as f64 - 1.0) as i64;
            (x0, x1)
        };
        let (x0, x1) = span(lo.x, hi.x);
        let (y0, y1) = span(lo.y, hi.y);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (x, y) = (x as usize, y as usize);
                let p = DVec2::new((x as f64 + 0.5) / res as f64, (y as f64 + 0.5) / res as f64);
                let inside = (0..3).all(|k| {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    match orient2d(a, b, p) {
                        Ordering::Greater => true,
                        Ordering::Equal => takes(a, b),
                        Ordering::Less => false,
                    }
                });
                if inside {
                    let h = &mut hits[y * r + x];
                    *h = h.saturating_add(1);
                }
            }
        }
    }
    let total = (r * r) as f64;
    m.coverage = hits.iter().filter(|&&h| h > 0).count() as f64 / total;
    m.overlap = hits.iter().filter(|&&h| h > 1).count() as f64 / total;
    // Exact overlapping pairs: a sweep along u.
    let boxes: Vec<(DVec2, DVec2)> = uv
        .iter()
        .map(|t| (t[0].min(t[1]).min(t[2]), t[0].max(t[1]).max(t[2])))
        .collect();
    live.sort_by(|&a, &b| {
        boxes[a as usize]
            .0
            .x
            .total_cmp(&boxes[b as usize].0.x)
            .then(a.cmp(&b))
    });
    let mut active: Vec<u32> = Vec::new();
    for &i in &live {
        let (lo, hi) = boxes[i as usize];
        active.retain(|&j| boxes[j as usize].1.x > lo.x);
        for &j in &active {
            let (lo2, hi2) = boxes[j as usize];
            if lo2.y >= hi.y || hi2.y <= lo.y {
                continue;
            }
            if tris_overlap_2d(uv[i as usize], uv[j as usize]) {
                m.overlapping_pairs += 1;
            }
        }
        active.push(i);
    }
    m
}

fn vec3(v: DVec3) -> Json {
    Json::Arr(vec![Json::Num(v.x), Json::Num(v.y), Json::Num(v.z)])
}

fn count(n: usize) -> Json {
    Json::Int(n as u64)
}

fn num(v: f64) -> Json {
    if v.is_finite() {
        Json::Num(v)
    } else {
        Json::Null
    }
}

impl MeshMeasures {
    /// The measures as one line of JSON, keys in a fixed order, lengths in
    /// mm. The same bytes on every host.
    pub fn to_json(&self) -> String {
        let pieces: Vec<Json> = self
            .pieces
            .iter()
            .map(|p| {
                Json::Obj(vec![
                    ("triangles", count(p.triangles)),
                    ("diagonal", num(p.diagonal)),
                ])
            })
            .collect();
        let floaters = self.pieces.len().saturating_sub(1);
        let floater_triangles: usize = self.pieces.iter().skip(1).map(|p| p.triangles).sum();
        let uv = match &self.uv {
            None => Json::Null,
            Some(u) => Json::Obj(vec![
                ("area", num(u.area)),
                ("coverage", num(u.coverage)),
                ("overlap", num(u.overlap)),
                ("overlapping_pairs", count(u.overlapping_pairs)),
                ("flipped", count(u.flipped)),
                ("zero_area", count(u.zero_area)),
                ("outside", count(u.outside)),
            ]),
        };
        let images: Vec<Json> = self
            .texture
            .images
            .iter()
            .map(|i| {
                let (a, b) = i
                    .decoded_bytes()
                    .map_or((Json::Null, Json::Null), |(a, b)| {
                        (Json::Int(a), Json::Int(b))
                    });
                Json::Obj(vec![
                    ("name", Json::Str(i.name.clone())),
                    ("format", Json::Str(i.format.clone())),
                    (
                        "width",
                        i.width.map_or(Json::Null, |w| Json::Int(u64::from(w))),
                    ),
                    (
                        "height",
                        i.height.map_or(Json::Null, |h| Json::Int(u64::from(h))),
                    ),
                    ("file_bytes", Json::Int(i.file_bytes)),
                    ("decoded_bytes", a),
                    ("decoded_bytes_with_mips", b),
                    (
                        "error",
                        i.error
                            .as_ref()
                            .map_or(Json::Null, |e| Json::Str(e.clone())),
                    ),
                ])
            })
            .collect();
        let size = self.bounds_max - self.bounds_min;
        let doc = Json::Obj(vec![
            ("units", Json::Str("mm".into())),
            ("triangles", count(self.triangles)),
            ("vertices", count(self.vertices)),
            ("positions", count(self.positions)),
            (
                "bounds",
                Json::Obj(vec![
                    ("min", vec3(self.bounds_min)),
                    ("max", vec3(self.bounds_max)),
                    ("size", vec3(size)),
                ]),
            ),
            ("pivot_from_base", vec3(self.pivot_from_base)),
            ("area", num(self.area)),
            ("volume", num(self.volume)),
            ("closed", Json::Bool(self.closed)),
            ("boundary_edges", count(self.boundary_edges)),
            ("boundary_loops", count(self.boundary_loops)),
            ("non_manifold_edges", count(self.non_manifold_edges)),
            ("inconsistent_edges", count(self.inconsistent_edges)),
            ("shortest_edge", num(self.shortest_edge)),
            ("degenerate_triangles", count(self.degenerate_triangles)),
            ("duplicate_triangles", count(self.duplicate_triangles)),
            ("pieces", count(self.pieces.len())),
            ("floaters", count(floaters)),
            ("floater_triangles", count(floater_triangles)),
            (
                "largest_pieces",
                Json::Arr(pieces.into_iter().take(16).collect()),
            ),
            (
                "self_intersections",
                self.self_intersections.map_or(Json::Null, count),
            ),
            ("skinned", Json::Bool(self.skin.is_some())),
            (
                "skin",
                self.skin.as_ref().map_or(Json::Null, |k| {
                    Json::Obj(vec![
                        ("bones", count(k.bones.len())),
                        (
                            "bone_names",
                            Json::Arr(k.bones.iter().map(|b| Json::Str(b.name.clone())).collect()),
                        ),
                        (
                            "weighted_vertices",
                            count(
                                k.weights
                                    .iter()
                                    .filter(|w| w.pairs().next().is_some())
                                    .count(),
                            ),
                        ),
                    ])
                }),
            ),
            ("uv", uv),
            (
                "texture",
                Json::Obj(vec![
                    ("images", Json::Arr(images)),
                    ("decoded_bytes", Json::Int(self.texture.decoded_bytes)),
                    (
                        "decoded_bytes_with_mips",
                        Json::Int(self.texture.decoded_bytes_with_mips),
                    ),
                    ("unknown", count(self.texture.unknown)),
                ]),
            ),
        ]);
        let mut out = String::new();
        doc.write(&mut out);
        out
    }
}

/// The solid's triangles over vertices welded where their positions are
/// equal. An imported mesh splits a vertex wherever its UVs or normals
/// differ, and triangles that share a position but not an id are not
/// neighbours to the self-intersection count: every seam would count.
pub(crate) fn weld_exact(s: &Solid) -> (Vec<DVec3>, Vec<[u32; 3]>) {
    // +0.0 makes -0.0 and 0.0 one key.
    let key = |p: DVec3| [p.x + 0.0, p.y + 0.0, p.z + 0.0];
    let same = |a: DVec3, b: DVec3| a.x == b.x && a.y == b.y && a.z == b.z;
    let mut order: Vec<u32> = (0..s.positions.len() as u32).collect();
    order.sort_by(|&a, &b| {
        let (ka, kb) = (key(s.positions[a as usize]), key(s.positions[b as usize]));
        ka[0]
            .total_cmp(&kb[0])
            .then(ka[1].total_cmp(&kb[1]))
            .then(ka[2].total_cmp(&kb[2]))
            .then(a.cmp(&b))
    });
    let mut to = vec![0u32; s.positions.len()];
    let mut positions: Vec<DVec3> = Vec::new();
    for &v in &order {
        let p = s.positions[v as usize];
        if positions.last().is_none_or(|&q| !same(p, q)) {
            positions.push(p);
        }
        to[v as usize] = positions.len() as u32 - 1;
    }
    let triangles = s
        .triangles
        .iter()
        .map(|t| t.map(|v| to[v as usize]))
        .collect();
    (positions, triangles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Unwrap, unwrap};
    use shape_gen::solids::sphere;

    #[test]
    fn split_vertices_are_not_self_intersections() {
        // A sphere with every triangle on its own three vertices, as a
        // mesh comes in from a file with seams.
        let whole = sphere(10.0).unwrap();
        let mut split = Solid::default();
        for t in &whole.triangles {
            let ids = t.map(|v| split.push_vertex(whole.positions[v as usize]));
            split.push_triangle(ids, 0, &[]);
        }
        let d = Measure {
            intersections: true,
            ..Default::default()
        };
        assert_eq!(measure(&split, &d).self_intersections, Some(0));
    }

    #[test]
    fn measures_a_sphere_and_its_uvs() {
        let mut s = sphere(10.0).unwrap();
        unwrap(&mut s, &Unwrap::default()).unwrap();
        let m = measure(
            &s,
            &Measure {
                intersections: true,
                uv_resolution: 256,
                ..Default::default()
            },
        );
        assert!(m.closed && m.pieces.len() == 1);
        assert_eq!(m.self_intersections, Some(0));
        let u = m.uv.unwrap();
        assert_eq!((u.overlapping_pairs, u.flipped, u.outside), (0, 0, 0));
        assert_eq!(u.overlap, 0.0);
        assert!(u.coverage > 0.3 && u.coverage <= u.area + 0.05, "{u:?}");
        let json = m.to_json();
        assert!(json.starts_with(r#"{"units":"mm","triangles":"#));
        assert!(json.contains(r#""closed":true"#));
        // Everything on one texel: full overlap.
        let same = vec![[DVec2::ZERO, DVec2::X, DVec2::Y]; 2];
        let mut two = Solid::default();
        for _ in 0..2 {
            let a = two.push_vertex(DVec3::ZERO);
            let b = two.push_vertex(DVec3::X);
            let c = two.push_vertex(DVec3::Y);
            two.push_triangle([a, b, c], 0, &[]);
        }
        two.uvs = Some(same);
        let m = measure(
            &two,
            &Measure {
                uv_resolution: 64,
                ..Default::default()
            },
        );
        let u = m.uv.unwrap();
        assert_eq!(u.overlapping_pairs, 1);
        assert_eq!(u.coverage, u.overlap);
        assert_eq!(m.duplicate_triangles, 1);
    }
}
