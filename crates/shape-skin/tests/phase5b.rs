//! Phase 5b part 1's acceptance (`docs/plan.md`): a stylised figure,
//! written as a program, skinned from its named regions, posed with an
//! elbow and a knee bent 90°, and measured: closed, and how far any
//! triangle turns from where the skin moves it. `cargo run --release -p
//! shape-skin --example phase5b -- docs/previews/phase5b` writes the rest
//! and posed previews and the GLB.

#[path = "figure/mod.rs"]
mod figure;

use shape_skin::{Pose, measure_pose, pose, skin};

#[test]
fn the_figure_skins_and_bends_without_tearing() {
    let body = figure::body();
    let m = body.measure();
    assert!(m.closed && m.components == 1, "{m:?}");
    let s = skin(&body, &figure::rig()).unwrap();
    let sk = s.skin.as_ref().unwrap();
    sk.check(s.positions.len()).unwrap();
    assert!(s.measure().closed);
    for p in [Pose::default(), figure::bend(), figure::wave()] {
        let pm = measure_pose(&s, &p).unwrap();
        println!("{pm:?}");
        assert!(pm.closed && pm.non_manifold_edges == 0);
        assert_eq!(pm.collapsed, 0);
    }
    let posed = pose(&s, &figure::bend()).unwrap();
    assert!(posed.measure().closed);
}

/// The figure as a scene of one skinned part.
fn scene(s: &shape_mesh::Solid) -> shape_tools::Scene<'_> {
    let mut part = shape_tools::Part::new("figure", s);
    part.material.color = Some([0.85, 0.62, 0.45]);
    shape_tools::Scene {
        name: "figure".into(),
        parts: vec![part],
    }
}

type M4 = [[f64; 4]; 4];

fn m4(m: [[f32; 4]; 4]) -> M4 {
    m.map(|c| c.map(f64::from))
}

/// Column-major product `a · b`.
fn mul(a: &M4, b: &M4) -> M4 {
    let mut m = [[0.0; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            m[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    m
}

fn apply(m: &M4, p: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|r| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r])
}

/// A turn about a unit axis, as a column-major matrix.
fn rotation(k: [f64; 3], angle: f64) -> M4 {
    let (s, c) = shape_core::trig::sin_cos(angle);
    let r = |v: [f64; 3]| {
        let d = k[0] * v[0] + k[1] * v[1] + k[2] * v[2];
        let x = [
            k[1] * v[2] - k[2] * v[1],
            k[2] * v[0] - k[0] * v[2],
            k[0] * v[1] - k[1] * v[0],
        ];
        [0, 1, 2].map(|i| v[i] * c + x[i] * s + k[i] * d * (1.0 - c))
    };
    let [x, y, z] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(r);
    [
        [x[0], x[1], x[2], 0.0],
        [y[0], y[1], y[2], 0.0],
        [z[0], z[1], z[2], 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// Kernel (mm, Z up) to glTF (m, Y up).
fn gltf_space(p: shape_core::DVec3) -> [f64; 3] {
    [p.x * 0.001, p.z * 0.001, -p.y * 0.001]
}

#[test]
fn the_glb_reads_back_with_its_skin_and_poses_like_the_kernel() {
    let s = skin(&figure::body(), &figure::rig()).unwrap();
    let sk = s.skin.as_ref().unwrap();
    let bytes = shape_tools::glb(&scene(&s)).unwrap();
    assert_eq!(bytes, shape_tools::glb(&scene(&s)).unwrap());
    let doc = gltf::Gltf::from_slice(&bytes).unwrap();
    let blob = doc.blob.clone().expect("a BIN chunk");
    let skin_ = doc.skins().next().expect("a skin");
    let joints: Vec<gltf::Node> = skin_.joints().collect();
    assert_eq!(joints.len(), sk.bones.len());
    for (j, b) in joints.iter().zip(&sk.bones) {
        assert_eq!(j.name(), Some(b.name.as_str()));
    }
    let ibm: Vec<M4> = skin_
        .reader(|_| Some(blob.as_slice()))
        .read_inverse_bind_matrices()
        .unwrap()
        .map(m4)
        .collect();
    // Each node's parent, to compose world matrices.
    let mut parent = vec![None; doc.nodes().count()];
    for n in doc.nodes() {
        for c in n.children() {
            parent[c.index()] = Some(n.index());
        }
    }
    let skinned = doc.nodes().find(|n| n.skin().is_some()).unwrap();
    let prim = skinned.mesh().unwrap().primitives().next().unwrap();
    let reader = prim.reader(|_| Some(blob.as_slice()));
    let pos: Vec<[f32; 3]> = reader.read_positions().unwrap().collect();
    let jw: Vec<[u16; 4]> = reader.read_joints(0).unwrap().into_u16().collect();
    let ww: Vec<[f32; 4]> = reader.read_weights(0).unwrap().into_f32().collect();
    assert_eq!((pos.len(), jw.len()), (ww.len(), ww.len()));
    for w in &ww {
        assert!((w.iter().sum::<f32>() - 1.0).abs() < 2e-7, "{w:?}");
    }
    // The kernel vertex behind each GLB vertex, by its f32 rest position.
    let key = |p: [f64; 3]| p.map(|x| (x as f32).to_bits());
    let by_rest: std::collections::BTreeMap<[u32; 3], usize> = s
        .positions
        .iter()
        .enumerate()
        .map(|(i, &p)| (key(gltf_space(p)), i))
        .collect();
    for pose_ in [shape_skin::Pose::default(), figure::bend(), figure::wave()] {
        let posed = pose(&s, &pose_).unwrap();
        // Bone node locals with the pose's turns: a kernel turn about a
        // bone-local axis a is a turn about y_up(a) in the node's frame.
        let mut local: Vec<M4> = doc.nodes().map(|n| m4(n.transform().matrix())).collect();
        for t in &pose_.turns {
            let j = &joints[sk.bone(&t.bone).unwrap() as usize];
            let a = t.axis.normalize();
            local[j.index()] = mul(&local[j.index()], &rotation([a.x, a.z, -a.y], t.angle));
        }
        let world = |mut i: usize| {
            let mut m = local[i];
            while let Some(p) = parent[i] {
                m = mul(&local[p], &m);
                i = p;
            }
            m
        };
        let joint_m: Vec<M4> = joints
            .iter()
            .zip(&ibm)
            .map(|(j, ibm)| mul(&world(j.index()), ibm))
            .collect();
        let mut at_rest: std::collections::BTreeMap<[u32; 3], [f64; 3]> = Default::default();
        let mut worst = 0.0f64;
        for i in 0..pos.len() {
            let p = pos[i].map(f64::from);
            let mut q = [0.0; 3];
            for k in 0..4 {
                let r = apply(&joint_m[jw[i][k] as usize], p);
                (0..3).for_each(|c| q[c] += r[c] * f64::from(ww[i][k]));
            }
            // Copies of one vertex (split at real edges) land together:
            // the exported mesh cannot tear.
            let k = pos[i].map(f32::to_bits);
            if let Some(prev) = at_rest.insert(k, q) {
                assert_eq!(prev, q, "a seam tore at {:?}", pos[i]);
            }
            let v = by_rest[&k];
            let e = gltf_space(posed.positions[v]);
            worst = worst.max((0..3).map(|c| (e[c] - q[c]).abs()).fold(0.0, f64::max));
        }
        // f32 positions and matrices: a few micrometres at most.
        assert!(worst < 5e-6, "GLB and kernel poses differ by {worst} m");
    }
}
