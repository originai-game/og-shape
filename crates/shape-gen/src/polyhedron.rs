//! three.js `PolyhedronGeometry(vertices, indices, radius, detail)` and
//! its four solids (`IcosahedronGeometry`, `DodecahedronGeometry`,
//! `OctahedronGeometry`, `TetrahedronGeometry`): each face subdivided
//! `detail` times, every vertex pushed onto the sphere of `radius`,
//! unindexed as three.js (flat normals at detail 0, the sphere's normals
//! above it) with three.js's spherical uvs and seam corrections; azimuth
//! and inclination read [`crate::trig::atan2`].

use crate::{Mesh, finish, normalize, trig, vertex_normals};
use glam::DVec3;

#[derive(Clone, Debug, PartialEq)]
pub struct Polyhedron {
    pub vertices: Vec<f64>,
    pub indices: Vec<u32>,
    pub radius: f64,
    pub detail: u32,
}

fn azimuth(v: DVec3) -> f64 {
    trig::atan2(v.z, -v.x)
}

fn inclination(v: DVec3) -> f64 {
    trig::atan2(-v.y, (v.x * v.x + v.z * v.z).sqrt())
}

/// three.js's Vector3.lerp: `a + (b - a) * t`, component by component.
fn lerp(a: DVec3, b: DVec3, t: f64) -> DVec3 {
    DVec3::new(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.z + (b.z - a.z) * t,
    )
}

/// Refused for indices past the vertices (or not triangles).
pub fn polyhedron(p: &Polyhedron) -> Result<Mesh, String> {
    if !p.vertices.len().is_multiple_of(3) {
        return Err(format!(
            "vertices holds {} numbers: xyz triples",
            p.vertices.len()
        ));
    }
    if !p.indices.len().is_multiple_of(3) {
        return Err(format!(
            "indices holds {} numbers: triangles are triples",
            p.indices.len()
        ));
    }
    let n = p.vertices.len() / 3;
    if let Some(bad) = p.indices.iter().find(|&&i| i as usize >= n) {
        return Err(format!("indices holds {bad}, past the {n} vertices"));
    }
    let vert = |i: u32| {
        let s = i as usize * 3;
        DVec3::new(p.vertices[s], p.vertices[s + 1], p.vertices[s + 2])
    };
    let cols = p.detail as usize + 1;
    let mut buf: Vec<DVec3> = Vec::with_capacity(p.indices.len() * cols * cols);
    for f in p.indices.as_chunks::<3>().0.iter() {
        let (a, b, c) = (vert(f[0]), vert(f[1]), vert(f[2]));
        let mut v: Vec<Vec<DVec3>> = Vec::with_capacity(cols + 1);
        for i in 0..=cols {
            let aj = lerp(a, c, i as f64 / cols as f64);
            let bj = lerp(b, c, i as f64 / cols as f64);
            let rows = cols - i;
            let row = (0..=rows)
                .map(|j| {
                    if j == 0 && i == cols {
                        aj
                    } else {
                        lerp(aj, bj, j as f64 / rows as f64)
                    }
                })
                .collect();
            v.push(row);
        }
        for i in 0..cols {
            for j in 0..(2 * (cols - i) - 1) {
                let k = j / 2;
                if j % 2 == 0 {
                    buf.extend([v[i][k + 1], v[i + 1][k], v[i][k]]);
                } else {
                    buf.extend([v[i][k + 1], v[i + 1][k + 1], v[i + 1][k]]);
                }
            }
        }
    }
    for x in buf.iter_mut() {
        *x = normalize(*x) * p.radius;
    }
    let mut uvs: Vec<[f64; 2]> = buf
        .iter()
        // three.js stores 1 - v; the engine's v runs down, so v itself.
        .map(|&x| {
            [
                azimuth(x) / 2.0 / trig::PI + 0.5,
                inclination(x) / trig::PI + 0.5,
            ]
        })
        .collect();
    // correctUVs: a pole takes its face's azimuth, and a u of 1 on a face
    // west of the seam wraps to 0.
    for (t, tri) in buf.as_chunks::<3>().0.iter().enumerate() {
        let centroid = (tri[0] + tri[1] + tri[2]) / 3.0;
        let azi = azimuth(centroid);
        for (k, x) in tri.iter().enumerate() {
            let uv = &mut uvs[t * 3 + k];
            let u0 = uv[0];
            if azi < 0.0 && u0 == 1.0 {
                uv[0] = u0 - 1.0;
            }
            if x.x == 0.0 && x.z == 0.0 {
                uv[0] = azi / 2.0 / trig::PI + 0.5;
            }
        }
    }
    // correctSeam: a face that straddles u = 0 moves its small us past 1.
    for t in uvs.as_chunks_mut::<3>().0.iter_mut() {
        let (x0, x1, x2) = (t[0][0], t[1][0], t[2][0]);
        let max = x0.max(x1).max(x2);
        let min = x0.min(x1).min(x2);
        if max > 0.9 && min < 0.1 {
            for uv in t.iter_mut() {
                if uv[0] < 0.2 {
                    uv[0] += 1.0;
                }
            }
        }
    }
    let indices: Vec<u32> = (0..buf.len() as u32).collect();
    let normals = if p.detail == 0 {
        vertex_normals(&buf, &indices)
    } else {
        buf.iter().map(|&x| normalize(x)).collect()
    };
    Ok(finish(&buf, &normals, &uvs, indices))
}

fn solid(vertices: Vec<f64>, indices: Vec<u32>, radius: f64, detail: u32) -> Mesh {
    polyhedron(&Polyhedron {
        vertices,
        indices,
        radius,
        detail,
    })
    .expect("a solid's tables are valid")
}

/// three.js `IcosahedronGeometry(radius, detail)`.
pub fn icosahedron(radius: f64, detail: u32) -> Mesh {
    let t = (1.0 + 5f64.sqrt()) / 2.0;
    #[rustfmt::skip]
    let v = vec![
        -1.0, t, 0.0, 1.0, t, 0.0, -1.0, -t, 0.0, 1.0, -t, 0.0,
        0.0, -1.0, t, 0.0, 1.0, t, 0.0, -1.0, -t, 0.0, 1.0, -t,
        t, 0.0, -1.0, t, 0.0, 1.0, -t, 0.0, -1.0, -t, 0.0, 1.0,
    ];
    #[rustfmt::skip]
    let i = vec![
        0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11,
        1, 5, 9, 5, 11, 4, 11, 10, 2, 10, 7, 6, 7, 1, 8,
        3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9,
        4, 9, 5, 2, 4, 11, 6, 2, 10, 8, 6, 7, 9, 8, 1,
    ];
    solid(v, i, radius, detail)
}

/// three.js `DodecahedronGeometry(radius, detail)`.
pub fn dodecahedron(radius: f64, detail: u32) -> Mesh {
    let t = (1.0 + 5f64.sqrt()) / 2.0;
    let r = 1.0 / t;
    #[rustfmt::skip]
    let v = vec![
        -1.0, -1.0, -1.0, -1.0, -1.0, 1.0,
        -1.0, 1.0, -1.0, -1.0, 1.0, 1.0,
        1.0, -1.0, -1.0, 1.0, -1.0, 1.0,
        1.0, 1.0, -1.0, 1.0, 1.0, 1.0,
        0.0, -r, -t, 0.0, -r, t,
        0.0, r, -t, 0.0, r, t,
        -r, -t, 0.0, -r, t, 0.0,
        r, -t, 0.0, r, t, 0.0,
        -t, 0.0, -r, t, 0.0, -r,
        -t, 0.0, r, t, 0.0, r,
    ];
    #[rustfmt::skip]
    let i = vec![
        3, 11, 7, 3, 7, 15, 3, 15, 13,
        7, 19, 17, 7, 17, 6, 7, 6, 15,
        17, 4, 8, 17, 8, 10, 17, 10, 6,
        8, 0, 16, 8, 16, 2, 8, 2, 10,
        0, 12, 1, 0, 1, 18, 0, 18, 16,
        6, 10, 2, 6, 2, 13, 6, 13, 15,
        2, 16, 18, 2, 18, 3, 2, 3, 13,
        18, 1, 9, 18, 9, 11, 18, 11, 3,
        4, 14, 12, 4, 12, 0, 4, 0, 8,
        11, 9, 5, 11, 5, 19, 11, 19, 7,
        19, 5, 14, 19, 14, 4, 19, 4, 17,
        1, 12, 14, 1, 14, 5, 1, 5, 9,
    ];
    solid(v, i, radius, detail)
}

/// three.js `OctahedronGeometry(radius, detail)`.
pub fn octahedron(radius: f64, detail: u32) -> Mesh {
    #[rustfmt::skip]
    let v = vec![
        1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0, 0.0,
        0.0, -1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0,
    ];
    let i = vec![
        0, 2, 4, 0, 4, 3, 0, 3, 5, 0, 5, 2, 1, 2, 5, 1, 5, 3, 1, 3, 4, 1, 4, 2,
    ];
    solid(v, i, radius, detail)
}

/// three.js `TetrahedronGeometry(radius, detail)`.
pub fn tetrahedron(radius: f64, detail: u32) -> Mesh {
    let v = vec![
        1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0,
    ];
    let i = vec![2, 1, 0, 0, 3, 2, 1, 3, 0, 2, 3, 1];
    solid(v, i, radius, detail)
}
