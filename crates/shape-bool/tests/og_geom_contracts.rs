#![allow(missing_docs, clippy::disallowed_types)]
//! og-geom's contract tests (copied, `third_party/og-geom/SOURCE`):
//! og-geom against three.js's counts, closed-manifold topology, analytic
//! volumes and determinism.

use glam::{DQuat, DVec2, DVec3};
use shape_bool::*;
use shape_gen::extrude::ExtrudePath;
use shape_gen::*;
use std::collections::HashMap;

// ── helpers ─────────────────────────────────────────────────────────────────

/// A box of three.js `BoxGeometry(w, h, d)`'s layout: four vertices a face,
/// two triangles each, counter-clockwise seen from outside.
fn boxed(w: f32, h: f32, d: f32) -> Mesh {
    let (x, y, z) = (w / 2.0, h / 2.0, d / 2.0);
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
            let p: [f32; 3] =
                std::array::from_fn(|k| (n[k] + u[k] * su + v[k] * sv) * [x, y, z][k]);
            m.positions.push(p);
            m.normals.push(n);
            m.uvs.push([(su + 1.0) / 2.0, (1.0 - sv) / 2.0]);
        }
        m.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    m
}

/// A sphere as a lathe of a half circle (closed: the poles and the seam
/// meet exactly).
fn sphere(r: f64, segments: u32, rows: u32) -> Mesh {
    let points = (0..=rows)
        .map(|j| {
            let (s, c) = trig::sincos(j as f64 / rows as f64 * trig::PI);
            DVec2::new(if j == 0 || j == rows { 0.0 } else { r * s }, -r * c)
        })
        .collect();
    lathe(&Lathe {
        points,
        segments,
        ..Default::default()
    })
}

/// Euler characteristic of the mesh welded by position, after checking that
/// every edge is used exactly twice, once each way.
fn closed_manifold(m: &Mesh) -> i64 {
    let mut id: HashMap<[u32; 3], u32> = HashMap::new();
    let w: Vec<u32> = m
        .positions
        .iter()
        .map(|p| {
            let key = p.map(|x| (x + 0.0).to_bits());
            let n = id.len() as u32;
            *id.entry(key).or_insert(n)
        })
        .collect();
    let mut directed: HashMap<(u32, u32), u32> = HashMap::new();
    let mut faces = 0i64;
    for t in m.indices.as_chunks::<3>().0.iter() {
        let t = [w[t[0] as usize], w[t[1] as usize], w[t[2] as usize]];
        assert!(
            t[0] != t[1] && t[1] != t[2] && t[0] != t[2],
            "a degenerate triangle {t:?}"
        );
        faces += 1;
        for k in 0..3 {
            *directed.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
        }
    }
    for (&(a, b), &n) in &directed {
        assert_eq!(n, 1, "edge {a}-{b} is used {n} times in one direction");
        assert_eq!(directed.get(&(b, a)), Some(&1), "edge {a}-{b} has no twin");
    }
    let edges = directed.len() as i64 / 2;
    id.len() as i64 - edges + faces
}

fn circle(cx: f64, cy: f64, r: f64, n: usize, clockwise: bool) -> Vec<DVec2> {
    (0..n)
        .map(|i| {
            let a = i as f64 / n as f64 * trig::TAU;
            let (s, c) = trig::sincos(if clockwise { -a } else { a });
            DVec2::new(cx + r * c, cy + r * s)
        })
        .collect()
}

/// A gear: `teeth` trapezoid teeth round a disc, a round hole in the middle.
fn gear(teeth: usize) -> ShapePoints {
    let mut points = Vec::new();
    for i in 0..teeth {
        for (f, r) in [(0.0, 1.0), (0.2, 1.25), (0.45, 1.25), (0.65, 1.0)] {
            let a = (i as f64 + f) / teeth as f64 * trig::TAU;
            let (s, c) = trig::sincos(a);
            points.push(DVec2::new(r * c, r * s));
        }
    }
    ShapePoints {
        points,
        holes: vec![circle(0.0, 0.0, 0.35, 24, true)],
    }
}

fn fnv(bits: &[u32]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in bits {
        for byte in b.to_le_bytes() {
            h ^= byte as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
    h
}

// ── counts equal to three.js's ──────────────────────────────────────────────

#[test]
fn lathe_counts_are_three_js_s() {
    for (n, segments) in [(3usize, 12u32), (10, 7), (2, 1)] {
        let points = (0..n)
            .map(|j| DVec2::new(0.5 + j as f64 * 0.1, j as f64 * 0.2))
            .collect();
        let m = lathe(&Lathe {
            points,
            segments,
            ..Default::default()
        });
        // LatheGeometry: (segments + 1) columns of the points, two triangles a quad.
        assert_eq!(m.vertex_count(), (segments as usize + 1) * n);
        assert_eq!(m.triangle_count(), 2 * segments as usize * (n - 1));
    }
}

#[test]
fn polyhedra_counts_are_three_js_s_at_detail_0_to_2() {
    type Solid = fn(f64, u32) -> Mesh;
    let solids: [(&str, Solid, usize); 4] = [
        ("icosahedron", icosahedron, 20),
        ("dodecahedron", dodecahedron, 36),
        ("octahedron", octahedron, 8),
        ("tetrahedron", tetrahedron, 4),
    ];
    for (name, make, faces) in solids {
        for detail in 0..=2u32 {
            let m = make(1.5, detail);
            // Each face splits into (detail + 1)² triangles, unindexed.
            let tris = faces * (detail as usize + 1).pow(2);
            assert_eq!(m.triangle_count(), tris, "{name} detail {detail}");
            assert_eq!(m.vertex_count(), 3 * tris, "{name} detail {detail}");
            for p in &m.positions {
                let r = DVec3::from(p.map(f64::from)).length();
                assert!((r - 1.5).abs() < 1e-6, "{name}: a vertex off the sphere");
            }
            // Closed: a sphere, Euler characteristic 2.
            assert_eq!(closed_manifold(&m), 2, "{name} detail {detail}");
            assert!(m.volume() > 0.0, "{name} faces out");
        }
    }
}

#[test]
fn extrude_counts_are_three_js_s_and_it_is_closed() {
    let g = gear(12);
    let (outline, hole) = (g.points.len(), g.holes[0].len());
    let ring = outline + hole;
    // earcut: n points with h holes take n + 2h - 2 triangles.
    let lid = ring + 2 - 2;
    for (bevel, steps, segs) in [
        (false, 1u32, 0u32),
        (false, 3, 0),
        (true, 2, 3),
        (true, 1, 1),
    ] {
        let o = Extrude {
            depth: 0.4,
            steps,
            bevel_enabled: bevel,
            bevel_thickness: 0.06,
            bevel_size: 0.05,
            bevel_segments: segs.max(1),
            ..Default::default()
        };
        let m = extrude(std::slice::from_ref(&g), &o, None);
        let layers = steps + if bevel { 2 * segs } else { 0 };
        let tris = 2 * lid + 2 * ring * layers as usize;
        assert_eq!(m.triangle_count(), tris, "bevel {bevel} steps {steps}");
        assert_eq!(m.vertex_count(), 3 * tris);
        // One hole through it: a torus, Euler characteristic 0.
        assert_eq!(closed_manifold(&m), 0, "bevel {bevel} steps {steps}");
        assert!(m.volume() > 0.0, "it faces out");
    }
}

#[test]
fn extruding_a_square_makes_its_prism() {
    let sq = ShapePoints {
        points: vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(2.0, 0.0),
            DVec2::new(2.0, 1.0),
            DVec2::new(0.0, 1.0),
        ],
        holes: vec![],
    };
    let m = extrude(
        &[sq],
        &Extrude {
            depth: 3.0,
            bevel_enabled: false,
            ..Default::default()
        },
        None,
    );
    assert_eq!(closed_manifold(&m), 2);
    assert!((m.volume() - 6.0).abs() < 1e-5, "{}", m.volume());
}

#[test]
fn extrude_along_a_path_follows_it() {
    // A quarter circle in the xz plane; the section a small square.
    let sample = |t: f64| {
        let (s, c) = trig::sincos(t * trig::PI / 2.0);
        (DVec3::new(2.0 * c, 0.0, 2.0 * s), DVec3::new(-s, 0.0, c))
    };
    let sq = ShapePoints {
        points: circle(0.0, 0.0, 0.2, 4, false),
        holes: vec![],
    };
    let path = ExtrudePath {
        sample: &sample,
        closed: false,
    };
    let m = extrude(
        &[sq],
        &Extrude {
            steps: 16,
            ..Default::default()
        },
        Some(&path),
    );
    assert_eq!(closed_manifold(&m), 2);
    let end = m.positions.iter().map(|p| p[2]).fold(f32::MIN, f32::max);
    assert!((end - 2.2).abs() < 0.05, "it reaches z 2 ({end})");
}

// ── volumes ─────────────────────────────────────────────────────────────────

#[test]
fn a_lathed_circle_is_pappus_s_torus() {
    // A circle of radius 0.5 two from the axis: 2 pi R × pi r².
    let (big, small) = (2.0, 0.5);
    let n = 96;
    let points: Vec<DVec2> = (0..=n)
        .map(|j| {
            let (s, c) = trig::sincos(j as f64 / n as f64 * trig::TAU);
            DVec2::new(big + small * c, small * s)
        })
        .collect();
    let m = lathe(&Lathe {
        points,
        segments: 128,
        ..Default::default()
    });
    let want = trig::TAU * big * trig::PI * small * small;
    let v = m.volume().abs();
    assert!((v - want).abs() / want < 2e-3, "{v} against {want}");
}

#[test]
fn box_minus_box_is_its_analytic_volume_and_closed() {
    // A unit box through a corner of a 2 m box: 0.6 × 0.7 × 0.8 cut away.
    let a = boxed(2.0, 2.0, 2.0);
    let b = boxed(1.0, 1.0, 1.0);
    let place = Placement {
        position: DVec3::new(0.9, 0.8, 0.7),
        ..Default::default()
    };
    let m = csg(&a, Placement::default(), &b, place, CsgOp::Subtract).unwrap();
    assert_eq!(closed_manifold(&m), 2);
    let want = 8.0 - 0.6 * 0.7 * 0.8;
    assert!(
        (m.volume() - want).abs() < 1e-5,
        "{} against {want}",
        m.volume()
    );
    let u = csg(&a, Placement::default(), &b, place, CsgOp::Union).unwrap();
    assert_eq!(closed_manifold(&u), 2);
    assert!(
        (u.volume() - (8.0 + 1.0 - 0.336)).abs() < 1e-5,
        "{}",
        u.volume()
    );
    let i = csg(&a, Placement::default(), &b, place, CsgOp::Intersect).unwrap();
    assert_eq!(closed_manifold(&i), 2);
    assert!((i.volume() - 0.336).abs() < 1e-5, "{}", i.volume());
}

#[test]
fn csg_of_a_cube_and_a_sphere_is_closed_and_adds_up() {
    let a = boxed(2.0, 2.0, 2.0);
    let b = sphere(0.6, 24, 12);
    let rotated = Placement {
        position: DVec3::new(0.85, 0.3, -0.2),
        quaternion: DQuat::from_euler(glam::EulerRot::XYZ, 0.3, 0.7, 0.1),
        scale: DVec3::new(1.0, 1.2, 0.9),
    };
    let none = Placement::default();
    let (va, vb) = (a.volume(), b.volume() * 1.2 * 0.9);
    let u = csg(&a, none, &b, rotated, CsgOp::Union).unwrap();
    let i = csg(&a, none, &b, rotated, CsgOp::Intersect).unwrap();
    let s = csg(&a, none, &b, rotated, CsgOp::Subtract).unwrap();
    // The ellipsoid pokes out through the +x and +y faces but leaves the
    // edge between them: what the bite leaves is a bridge of cube over a
    // tunnel, a solid of genus one (Euler characteristic 0).
    for (name, m, euler) in [("union", &u, 2), ("intersect", &i, 2), ("subtract", &s, 0)] {
        assert_eq!(closed_manifold(m), euler, "{name}");
        assert!(m.volume() > 0.0, "{name} faces out");
    }
    // V(A ∪ B) + V(A ∩ B) = V(A) + V(B), and V(A − B) = V(A) − V(A ∩ B).
    assert!(
        (u.volume() + i.volume() - va - vb).abs() < 1e-4,
        "{} {} {va} {vb}",
        u.volume(),
        i.volume()
    );
    assert!((s.volume() - (va - i.volume())).abs() < 1e-4);
    // A sphere wholly inside: a cavity, two closed shells.
    let hollow = csg(&a, none, &b, Placement::default(), CsgOp::Subtract).unwrap();
    assert_eq!(closed_manifold(&hollow), 4);
    assert!((hollow.volume() - (8.0 - b.volume())).abs() < 1e-5);
}

#[test]
fn csg_keeps_one_of_two_coincident_faces() {
    // Two boxes side by side, touching along a face: their union is a 2×1×1 box.
    let a = boxed(1.0, 1.0, 1.0);
    let at = |x: f64| Placement {
        position: DVec3::new(x, 0.0, 0.0),
        ..Default::default()
    };
    let u = csg(&a, at(0.0), &a, at(1.0), CsgOp::Union).unwrap();
    assert_eq!(closed_manifold(&u), 2);
    assert!((u.volume() - 2.0).abs() < 1e-5, "{}", u.volume());
    // A box cut flush with a face: a notch open to the outside.
    let big = boxed(2.0, 2.0, 2.0);
    let notch = Placement {
        position: DVec3::new(0.5, 0.5, 0.0),
        ..Default::default()
    };
    let s = csg(&big, Placement::default(), &a, notch, CsgOp::Subtract).unwrap();
    assert_eq!(closed_manifold(&s), 2);
    assert!((s.volume() - 7.0).abs() < 1e-5, "{}", s.volume());
}

#[test]
fn the_sweep_is_closed_and_tapers() {
    let sample = |t: f64| (DVec3::new(0.0, t * 2.0, 0.0), DVec3::Y);
    let path = ExtrudePath {
        sample: &sample,
        closed: false,
    };
    let m = sweep(
        &Sweep {
            section: circle(0.0, 0.0, 0.5, 16, false),
            steps: 8,
            scale: vec![Key { t: 0.0, value: 1.0 }, Key { t: 1.0, value: 0.2 }],
            twist: vec![Key { t: 0.0, value: 0.0 }, Key { t: 1.0, value: 1.0 }],
            caps: true,
        },
        &path,
    )
    .unwrap();
    assert_eq!(closed_manifold(&m), 2);
    assert!(m.volume() > 0.0);
    // The top ring is a fifth of the bottom's radius.
    let top = m
        .positions
        .iter()
        .filter(|p| p[1] > 1.999)
        .map(|p| (p[0] * p[0] + p[2] * p[2]).sqrt())
        .fold(0.0, f32::max);
    assert!((top - 0.1).abs() < 1e-4, "{top}");
}

#[test]
fn merge_mirrors_and_keeps_parts_facing_out() {
    let b = boxed(1.0, 1.0, 1.0);
    let at = Placement {
        position: DVec3::new(2.0, 0.0, 0.0),
        ..Default::default()
    };
    let m = merge(&[
        Part {
            mesh: &b,
            placement: at,
            mirror: Mirror::default(),
        },
        Part {
            mesh: &b,
            placement: at,
            mirror: Mirror {
                x: true,
                ..Default::default()
            },
        },
    ])
    .unwrap();
    assert_eq!(m.triangle_count(), 24);
    assert!(
        (m.volume() - 2.0).abs() < 1e-5,
        "the mirrored half faces out too"
    );
    assert!(m.positions.iter().any(|p| p[0] < -2.4));
}

// ── determinism ─────────────────────────────────────────────────────────────

fn everything() -> Vec<Mesh> {
    let g = gear(10);
    let bevelled = extrude(std::slice::from_ref(&g), &Extrude::default(), None);
    let vase = lathe(&Lathe {
        points: (0..12)
            .map(|j| DVec2::new(0.3 + 0.2 * trig::sincos(j as f64 * 0.5).0, j as f64 * 0.1))
            .collect(),
        segments: 24,
        ..Default::default()
    });
    let sample = |t: f64| {
        let (s, c) = trig::sincos(t * 2.0);
        (
            DVec3::new(c, t, s),
            DVec3::new(-2.0 * s, 1.0, 2.0 * c).normalize(),
        )
    };
    let horn = sweep(
        &Sweep {
            section: circle(0.0, 0.0, 0.3, 12, false),
            steps: 20,
            scale: vec![Key { t: 0.0, value: 1.0 }, Key { t: 1.0, value: 0.1 }],
            ..Default::default()
        },
        &ExtrudePath {
            sample: &sample,
            closed: false,
        },
    )
    .unwrap();
    let dice = csg(
        &boxed(1.0, 1.0, 1.0),
        Placement::default(),
        &sphere(0.2, 16, 8),
        Placement {
            position: DVec3::new(0.5, 0.1, 0.05),
            ..Default::default()
        },
        CsgOp::Subtract,
    )
    .unwrap();
    vec![
        bevelled,
        vase,
        horn,
        icosahedron(1.0, 2),
        dodecahedron(1.0, 1),
        dice,
    ]
}

#[test]
fn two_builds_are_byte_identical() {
    let (a, b) = (everything(), everything());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.bits(), y.bits());
    }
    // The bits themselves, recorded on x86_64 Linux: a host that rounds
    // differently (a fused multiply-add, a libm sine) fails here.
    let hashes: Vec<String> = a
        .iter()
        .map(|m| format!("{:016x}", fnv(&m.bits())))
        .collect();
    assert_eq!(
        hashes,
        [
            "b03f283171de7f75",
            "be6b1fa2121783ef",
            "7133bb6f67635d0f",
            "dd8e55eccf0c7012",
            "a6bf8294bc6c52f2",
            "003c474fdb6b2aeb"
        ],
        "extrude, lathe, sweep, icosahedron, dodecahedron, csg"
    );
}

// ── og.Path2 and og.Shape flattened in Rust, as the TypeScript runtime does ─

fn fnv_points(points: &[DVec2]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for p in points {
        for v in [p.x, p.y] {
            for byte in v.to_le_bytes() {
                h ^= byte as u64;
                h = h.wrapping_mul(0x100_0000_01b3);
            }
        }
    }
    h
}

/// The same calls as packages/engine/src/geom.ts's og.Path2 and og.Shape
/// make, with the counts and FNV hashes of the f64 points bun printed for
/// them (angles written as numbers, as a `type model` part names them): a
/// `.ogt` shape and a TypeScript one flatten to the same bits.
// The angles are written as numbers on purpose: a `.ogt` names them so.
#[allow(clippy::approx_constant)]
#[test]
fn path2_flattens_to_the_typescript_runtime_s_points() {
    let v = |x: f64, y: f64| DVec2::new(x, y);
    let mut arcs = Path2::new();
    arcs.move_to(1.0, 0.0)
        .absarc(
            0.0,
            0.0,
            1.0,
            -0.7853981633974483,
            3.9269908169872414,
            false,
        )
        .line_to(0.2, -0.4)
        .arc(0.1, 0.1, 0.3, 0.0, 3.141592653589793, true);
    let mut curves = Path2::new();
    curves
        .move_to(0.0, 0.0)
        .quadratic_curve_to(0.5, 1.0, 1.0, 0.0)
        .bezier_curve_to(v(1.2, -0.5), v(1.8, 0.4), 2.0, 0.0)
        .spline_thru(&[v(2.5, 0.5), v(3.0, -0.2), v(3.4, 0.1)])
        .close_path();
    let mut ellipse = Path2::new();
    ellipse
        .absellipse(v(0.5, 0.25), v(2.0, 1.0), 0.3, -0.3, true, 0.7)
        .ellipse(
            v(1.0, 1.0),
            v(0.5, 0.25),
            -0.7853981633974483,
            0.7853981633974483,
            false,
            0.0,
        );
    let mut shape = Shape::default();
    shape
        .path
        .absarc(0.0, 0.0, 1.0, 0.0, 6.283185307179586, false);
    let mut hole = Path2::new();
    hole.absarc(0.0, 0.0, 0.3, 0.0, 6.283185307179586, true);
    shape.holes.push(hole);
    let e = shape.extract_points(12);
    let mut holed = e.points.clone();
    holed.extend_from_slice(&e.holes[0]);
    let mut from_points = Path2::new();
    from_points.set_from_points(&[v(0.0, 0.0), v(1.0, 0.0), v(1.0, 1.0), v(1.0, 1.0)]);
    let got: Vec<(usize, String)> = [
        arcs.get_points(5),
        curves.get_points(7),
        ellipse.get_points(12),
        holed,
        from_points.get_points(3),
    ]
    .iter()
    .map(|p| (p.len(), format!("{:016x}", fnv_points(p))))
    .collect();
    let want: Vec<(usize, String)> = [
        (23, "ea23d9a8a5cb3a2a"),
        (44, "33e053fbf600b013"),
        (50, "93b7c449378d949c"),
        (50, "5c8c6a9a4e4fec9e"),
        (3, "328e239e448cbc18"),
    ]
    .iter()
    .map(|(n, h)| (*n, h.to_string()))
    .collect();
    assert_eq!(
        got, want,
        "arcs, curves, ellipse, a shape with a hole, set_from_points"
    );
}
