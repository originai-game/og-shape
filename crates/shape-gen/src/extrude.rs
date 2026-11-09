//! three.js `ExtrudeGeometry`: shapes pushed along +Z by `depth` in
//! `steps`, or along a path (`extrudePath`, its frames three.js's Frenet
//! frames), with a rounded bevel of `bevelSegments` rings on each face.
//! Ported from three.js's source step for step, so the vertex count and
//! order are three.js's: unindexed (every triangle its own three
//! vertices), the lids first (back, then front), then the side walls, flat
//! normals (three.js `computeVertexNormals` of an unindexed geometry) and
//! the `WorldUVGenerator`'s uvs with v flipped to run down the image.

use crate::frames::{Frame, frenet_frames};
use crate::shape::{ShapePoints, is_clockwise, triangulate_shape};
use crate::{Mesh, finish, trig, vertex_normals};
use glam::{DVec2, DVec3};

/// three.js's extrude options (`curveSegments` is the TypeScript side's:
/// it flattens the shapes before they cross).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Extrude {
    pub depth: f64,
    pub steps: u32,
    pub bevel_enabled: bool,
    pub bevel_thickness: f64,
    pub bevel_size: f64,
    pub bevel_offset: f64,
    pub bevel_segments: u32,
}

impl Default for Extrude {
    fn default() -> Self {
        Extrude {
            depth: 1.0,
            steps: 1,
            bevel_enabled: true,
            bevel_thickness: 0.2,
            bevel_size: 0.1,
            bevel_offset: 0.0,
            bevel_segments: 3,
        }
    }
}

/// A path to extrude along: its point and unit tangent at each fraction of
/// its length, and whether it closes (three.js's `extrudePath`).
pub struct ExtrudePath<'a> {
    pub sample: &'a dyn Fn(f64) -> (DVec3, DVec3),
    pub closed: bool,
}

/// three.js's `mergeOverlappingPoints`: points closer than 1e-10 of the
/// coordinates' scale to the previous one go, the first included.
fn merge_overlapping(points: &mut Vec<DVec2>) {
    if points.is_empty() {
        return;
    }
    const T2: f64 = 1e-10 * 1e-10;
    let mut prev = points[0];
    let mut i = 1usize;
    while i <= points.len() && !points.is_empty() {
        let ci = i % points.len();
        let cur = points[ci];
        let d = cur - prev;
        let s = cur
            .x
            .abs()
            .max(cur.y.abs())
            .max(prev.x.abs())
            .max(prev.y.abs());
        if d.x * d.x + d.y * d.y <= T2 * s * s {
            points.remove(ci);
            continue;
        }
        prev = cur;
        i += 1;
    }
}

/// three.js's `getBevelVec`: where a point moves for a unit outward shift
/// of the outline, without normalising (corners stay sharp), but at most
/// sqrt(2).
fn bevel_vec(pt: DVec2, prev: DVec2, next: DVec2) -> DVec2 {
    let vp = pt - prev;
    let vn = next - pt;
    let vp_lensq = vp.x * vp.x + vp.y * vp.y;
    let collinear0 = vp.x * vn.y - vp.y * vn.x;
    let (tx, ty, shrink);
    if collinear0.abs() > f64::EPSILON {
        let vp_len = vp_lensq.sqrt();
        let vn_len = (vn.x * vn.x + vn.y * vn.y).sqrt();
        let pps = DVec2::new(prev.x - vp.y / vp_len, prev.y + vp.x / vp_len);
        let pns = DVec2::new(next.x - vn.y / vn_len, next.y + vn.x / vn_len);
        let sf = ((pns.x - pps.x) * vn.y - (pns.y - pps.y) * vn.x) / (vp.x * vn.y - vp.y * vn.x);
        tx = pps.x + vp.x * sf - pt.x;
        ty = pps.y + vp.y * sf - pt.y;
        let lsq = tx * tx + ty * ty;
        if lsq <= 2.0 {
            return DVec2::new(tx, ty);
        }
        shrink = (lsq / 2.0).sqrt();
    } else {
        let eps = f64::EPSILON;
        let direction_eq = if vp.x > eps {
            vn.x > eps
        } else if vp.x < -eps {
            vn.x < -eps
        } else {
            // Math.sign, with sign(0) == sign(-0) as JavaScript compares them.
            let sgn = |v: f64| {
                if v > 0.0 {
                    1
                } else if v < 0.0 {
                    -1
                } else {
                    0
                }
            };
            sgn(vp.y) == sgn(vn.y)
        };
        if direction_eq {
            tx = -vp.y;
            ty = vp.x;
            shrink = vp_lensq.sqrt();
        } else {
            tx = vp.x;
            ty = vp.y;
            shrink = (vp_lensq / 2.0).sqrt();
        }
    }
    DVec2::new(tx / shrink, ty / shrink)
}

fn movements(ring: &[DVec2]) -> Vec<DVec2> {
    let il = ring.len();
    (0..il)
        .map(|i| bevel_vec(ring[i], ring[(i + il - 1) % il], ring[(i + 1) % il]))
        .collect()
}

/// three.js `new ExtrudeGeometry(shapes, options)`.
pub fn extrude(shapes: &[ShapePoints], o: &Extrude, path: Option<&ExtrudePath>) -> Mesh {
    let mut out_pos: Vec<DVec3> = Vec::new();
    let mut out_uv: Vec<[f64; 2]> = Vec::new();
    for shape in shapes {
        add_shape(shape, o, path, &mut out_pos, &mut out_uv);
    }
    let indices: Vec<u32> = (0..out_pos.len() as u32).collect();
    let normals = vertex_normals(&out_pos, &indices);
    finish(&out_pos, &normals, &out_uv, indices)
}

fn add_shape(
    shape: &ShapePoints,
    o: &Extrude,
    path: Option<&ExtrudePath>,
    out: &mut Vec<DVec3>,
    uvs: &mut Vec<[f64; 2]>,
) {
    let steps = o.steps.max(1) as usize;
    let depth = o.depth;
    let mut bevel_enabled = o.bevel_enabled;
    let (mut bevel_thickness, mut bevel_size, mut bevel_offset) =
        (o.bevel_thickness, o.bevel_size, o.bevel_offset);
    let mut bevel_segments = o.bevel_segments as usize;

    let mut extrude_pts = Vec::new();
    let mut tube: Vec<Frame> = Vec::new();
    if let Some(p) = path {
        // getSpacedPoints(steps) and computeFrenetFrames(steps, closed).
        let samples: Vec<(DVec3, DVec3)> = (0..=steps)
            .map(|i| (p.sample)(i as f64 / steps as f64))
            .collect();
        extrude_pts = samples.iter().map(|s| s.0).collect();
        let tangents: Vec<DVec3> = samples.iter().map(|s| s.1).collect();
        tube = frenet_frames(&tangents, p.closed);
        bevel_enabled = false;
    }
    let by_path = path.is_some();
    if !bevel_enabled {
        bevel_segments = 0;
        bevel_thickness = 0.0;
        bevel_size = 0.0;
        bevel_offset = 0.0;
    }

    let mut vertices = shape.points.clone();
    let mut holes = shape.holes.clone();
    if !is_clockwise(&vertices) {
        vertices.reverse();
        for h in holes.iter_mut() {
            if is_clockwise(h) {
                h.reverse();
            }
        }
    }
    merge_overlapping(&mut vertices);
    for h in holes.iter_mut() {
        merge_overlapping(h);
    }
    let contour = vertices.clone();
    for h in &holes {
        vertices.extend_from_slice(h);
    }
    let vlen = vertices.len();

    let contour_mov = movements(&contour);
    let holes_mov: Vec<Vec<DVec2>> = holes.iter().map(|h| movements(h)).collect();
    let mut vertices_mov = contour_mov.clone();
    for m in &holes_mov {
        vertices_mov.extend_from_slice(m);
    }

    let mut placeholder: Vec<DVec3> = Vec::new();
    let bevel = |t: f64| {
        let (s, c) = trig::sincos(t * trig::PI / 2.0);
        (bevel_thickness * c, bevel_size * s + bevel_offset)
    };
    let faces = if bevel_segments == 0 {
        let mut c = contour.clone();
        let mut h = holes.clone();
        triangulate_shape(&mut c, &mut h)
    } else {
        let mut contracted = Vec::new();
        let mut expanded: Vec<Vec<DVec2>> = Vec::new();
        for b in 0..bevel_segments {
            let t = b as f64 / bevel_segments as f64;
            let (z, bs) = bevel(t);
            for (i, p) in contour.iter().enumerate() {
                let vert = *p + contour_mov[i] * bs;
                placeholder.push(DVec3::new(vert.x, vert.y, -z));
                if t == 0.0 {
                    contracted.push(vert);
                }
            }
            for (h, ahole) in holes.iter().enumerate() {
                let mut one = Vec::new();
                for (i, p) in ahole.iter().enumerate() {
                    let vert = *p + holes_mov[h][i] * bs;
                    placeholder.push(DVec3::new(vert.x, vert.y, -z));
                    if t == 0.0 {
                        one.push(vert);
                    }
                }
                if t == 0.0 {
                    expanded.push(one);
                }
            }
        }
        triangulate_shape(&mut contracted, &mut expanded)
    };
    let bs = bevel_size + bevel_offset;
    let place = |vert: DVec2, s: usize| {
        let f = &tube[s];
        extrude_pts[s] + f.normal * vert.x + f.binormal * vert.y
    };
    for i in 0..vlen {
        let vert = if bevel_enabled {
            vertices[i] + vertices_mov[i] * bs
        } else {
            vertices[i]
        };
        placeholder.push(if by_path {
            place(vert, 0)
        } else {
            DVec3::new(vert.x, vert.y, 0.0)
        });
    }
    for s in 1..=steps {
        for i in 0..vlen {
            let vert = if bevel_enabled {
                vertices[i] + vertices_mov[i] * bs
            } else {
                vertices[i]
            };
            placeholder.push(if by_path {
                place(vert, s)
            } else {
                DVec3::new(vert.x, vert.y, depth / steps as f64 * s as f64)
            });
        }
    }
    for b in (0..bevel_segments).rev() {
        let t = b as f64 / bevel_segments as f64;
        let (z, bs) = bevel(t);
        for (i, p) in contour.iter().enumerate() {
            let vert = *p + contour_mov[i] * bs;
            placeholder.push(DVec3::new(vert.x, vert.y, depth + z));
        }
        for (h, ahole) in holes.iter().enumerate() {
            for (i, p) in ahole.iter().enumerate() {
                let vert = *p + holes_mov[h][i] * bs;
                placeholder.push(DVec3::new(vert.x, vert.y, depth + z));
            }
        }
    }

    // Lids: the back reversed, the front as earcut made it.
    let top_uv = |out: &mut Vec<DVec3>, uvs: &mut Vec<[f64; 2]>, abc: [usize; 3]| {
        for i in abc {
            let p = placeholder[i];
            out.push(p);
            uvs.push([p.x, 1.0 - p.y]);
        }
    };
    let top_layer = if bevel_enabled {
        steps + bevel_segments * 2
    } else {
        steps
    };
    for f in &faces {
        top_uv(out, uvs, [f[2], f[1], f[0]]);
    }
    for f in &faces {
        top_uv(out, uvs, f.map(|i| i + vlen * top_layer));
    }

    // Side walls, ring by ring, each edge walked backwards as three.js.
    let mut side = |ring_len: usize, layer_offset: usize| {
        let mut i = ring_len;
        while i > 0 {
            i -= 1;
            let j = i;
            let k = if i == 0 { ring_len - 1 } else { i - 1 };
            for s in 0..(steps + bevel_segments * 2) {
                let (s1, s2) = (vlen * s, vlen * (s + 1));
                let a = layer_offset + j + s1;
                let b = layer_offset + k + s1;
                let c = layer_offset + k + s2;
                let d = layer_offset + j + s2;
                let [pa, pb, pc, pd] = [a, b, c, d].map(|x| placeholder[x]);
                out.extend([pa, pb, pd, pb, pc, pd]);
                let wall = |p: DVec3| {
                    if (pa.y - pb.y).abs() < (pa.x - pb.x).abs() {
                        [p.x, p.z]
                    } else {
                        [p.y, p.z]
                    }
                };
                uvs.extend([wall(pa), wall(pb), wall(pd), wall(pb), wall(pc), wall(pd)]);
            }
        }
    };
    side(contour.len(), 0);
    let mut layer_offset = contour.len();
    for h in &holes {
        side(h.len(), layer_offset);
        layer_offset += h.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_points_go_including_the_closing_one() {
        let mut p = vec![DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::ZERO];
        merge_overlapping(&mut p);
        assert_eq!(p, vec![DVec2::X, DVec2::ONE, DVec2::ZERO]);
    }
}
