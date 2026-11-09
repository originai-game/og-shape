//! Golden cases for the generators and booleans copied from og-geom. The
//! inputs are og-geom's own determinism fixtures, so the engine and this
//! repository can be compared directly.

use shape_bool::{CsgOp, csg};
use shape_core::{DVec2, DVec3, HashError, OutputHash};
use shape_gen::extrude::ExtrudePath;
use shape_gen::{
    Extrude, Key, Lathe, Mesh, Placement, ShapePoints, Sweep, dodecahedron, extrude, icosahedron,
    lathe, sweep, trig,
};

use crate::cases::Case;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "gen.extrude.gear",
            run: |h| mesh(h, &gear_extrude()),
        },
        Case {
            name: "gen.lathe.vase",
            run: |h| mesh(h, &vase()),
        },
        Case {
            name: "gen.sweep.horn",
            run: |h| mesh(h, &horn()),
        },
        Case {
            name: "gen.icosahedron",
            run: |h| mesh(h, &icosahedron(1.0, 2)),
        },
        Case {
            name: "gen.dodecahedron",
            run: |h| mesh(h, &dodecahedron(1.0, 1)),
        },
        Case {
            name: "bool.subtract",
            run: |h| boolean(h, CsgOp::Subtract),
        },
        Case {
            name: "bool.union",
            run: |h| boolean(h, CsgOp::Union),
        },
        Case {
            name: "bool.intersect",
            run: |h| boolean(h, CsgOp::Intersect),
        },
    ]
}

/// Every attribute's bits, the indices, and the counts that frame them.
pub fn mesh(h: &mut OutputHash, m: &Mesh) -> Result<(), HashError> {
    let f32s = |h: &mut OutputHash, label: &str, v: &mut dyn Iterator<Item = f32>| {
        h.str(label);
        let bits: Vec<u8> = v.flat_map(|x| x.to_bits().to_le_bytes()).collect();
        h.bytes(&bits);
    };
    if m.positions.iter().flatten().any(|x| x.is_nan()) {
        return Err(HashError::NaN);
    }
    f32s(h, "positions", &mut m.positions.iter().flatten().copied());
    f32s(h, "normals", &mut m.normals.iter().flatten().copied());
    f32s(h, "uvs", &mut m.uvs.iter().flatten().copied());
    f32s(h, "colors", &mut m.colors.iter().flatten().copied());
    h.str("indices");
    let bytes: Vec<u8> = m.indices.iter().flat_map(|i| i.to_le_bytes()).collect();
    h.bytes(&bytes);
    Ok(())
}

fn circle(r: f64, n: usize) -> Vec<DVec2> {
    (0..n)
        .map(|i| {
            let (s, c) = trig::sincos(i as f64 / n as f64 * trig::TAU);
            DVec2::new(r * c, r * s)
        })
        .collect()
}

fn gear_extrude() -> Mesh {
    let teeth = 10;
    let mut points = Vec::new();
    for i in 0..teeth {
        for (f, r) in [(0.0, 1.0), (0.2, 1.25), (0.45, 1.25), (0.65, 1.0)] {
            let (s, c) = trig::sincos((i as f64 + f) / teeth as f64 * trig::TAU);
            points.push(DVec2::new(r * c, r * s));
        }
    }
    let mut hole = circle(0.35, 24);
    hole.reverse();
    let g = ShapePoints {
        points,
        holes: vec![hole],
    };
    extrude(std::slice::from_ref(&g), &Extrude::default(), None)
}

fn vase() -> Mesh {
    lathe(&Lathe {
        points: (0..12)
            .map(|j| DVec2::new(0.3 + 0.2 * trig::sincos(j as f64 * 0.5).0, j as f64 * 0.1))
            .collect(),
        segments: 24,
        ..Default::default()
    })
}

fn horn() -> Mesh {
    let sample = |t: f64| {
        let (s, c) = trig::sincos(t * 2.0);
        (
            DVec3::new(c, t, s),
            DVec3::new(-2.0 * s, 1.0, 2.0 * c).normalize(),
        )
    };
    sweep(
        &Sweep {
            section: circle(0.3, 12),
            steps: 20,
            scale: vec![Key { t: 0.0, value: 1.0 }, Key { t: 1.0, value: 0.1 }],
            ..Default::default()
        },
        &ExtrudePath {
            sample: &sample,
            closed: false,
        },
    )
    .expect("the horn's path is valid")
}

/// A unit box with three.js `BoxGeometry`'s layout.
fn boxed() -> Mesh {
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ];
    let mut m = Mesh::default();
    for (n, u, v) in faces {
        let base = m.positions.len() as u32;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            m.positions.push(std::array::from_fn(|k| {
                (n[k] + u[k] * su + v[k] * sv) * 0.5
            }));
            m.normals.push(n);
            m.uvs.push([(su + 1.0) / 2.0, (1.0 - sv) / 2.0]);
        }
        m.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    m
}

fn sphere(r: f64) -> Mesh {
    let rows = 8;
    let points = (0..=rows)
        .map(|j| {
            let (s, c) = trig::sincos(j as f64 / rows as f64 * trig::PI);
            DVec2::new(if j == 0 || j == rows { 0.0 } else { r * s }, -r * c)
        })
        .collect();
    lathe(&Lathe {
        points,
        segments: 16,
        ..Default::default()
    })
}

fn boolean(h: &mut OutputHash, op: CsgOp) -> Result<(), HashError> {
    let tool = Placement {
        position: DVec3::new(0.5, 0.1, 0.05),
        ..Default::default()
    };
    match csg(&boxed(), Placement::default(), &sphere(0.2), tool, op) {
        Ok(m) => mesh(h, &m),
        Err(e) => {
            h.str(&format!("error: {e}"));
            Ok(())
        }
    }
}
