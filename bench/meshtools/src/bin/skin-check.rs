//! Imports skinned GLB files, reports their skins (bones, the uniform
//! scale taken out, how well the file's bind matrices agree), then poses
//! one bone and compares the result with the file's bind pose turned the
//! same way, computed from its own node and inverse bind matrices as read
//! by the independent `gltf` crate (`docs/research.md` §9.9).
//!
//! ```text
//! cargo run --release --bin skin-check -- FILE.glb...
//! ```

use glam::DVec3;
use shape_skin::{Pose, Turn, pose};
use shape_tools::{Import, import, no_files};

type M = [[f64; 4]; 4];

fn mul(a: &M, b: &M) -> M {
    let mut out = [[0.0; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            out[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    out
}

fn apply(m: &M, p: DVec3) -> DVec3 {
    let v = [p.x, p.y, p.z, 1.0];
    let q: Vec<f64> = (0..3).map(|r| (0..4).map(|c| m[c][r] * v[c]).sum()).collect();
    DVec3::new(q[0], q[1], q[2])
}

/// A general 4×4 inverse (Gauss–Jordan with partial pivoting).
fn inverse(m: &M) -> M {
    let mut a = [[0.0; 8]; 4];
    for r in 0..4 {
        for c in 0..4 {
            a[r][c] = m[c][r];
        }
        a[r][4 + r] = 1.0;
    }
    for c in 0..4 {
        let p = (c..4).max_by(|&x, &y| a[x][c].abs().total_cmp(&a[y][c].abs())).unwrap();
        a.swap(c, p);
        let d = a[c][c];
        for k in 0..8 {
            a[c][k] /= d;
        }
        for r in 0..4 {
            if r != c {
                let f = a[r][c];
                for k in 0..8 {
                    a[r][k] -= f * a[c][k];
                }
            }
        }
    }
    let mut out = [[0.0; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            out[c][r] = a[r][4 + c];
        }
    }
    out
}

fn f64m(m: [[f32; 4]; 4]) -> M {
    m.map(|c| c.map(f64::from))
}

fn main() {
    let angle = 0.5;
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).expect("read");
        let ours = match import(&bytes, &Import::default(), &no_files) {
            Ok(i) => i,
            Err(e) => {
                println!("{path}: refused: {e}");
                continue;
            }
        };
        for s in &ours.skin_scales {
            println!(
                "{path}: part {:?}: scale {} taken out; inverse bind matrices agree to {:.2e} mm; uneven {:.1e}; nodes stand {:.2e} mm from the bind pose",
                s.part, s.scale, s.bind_error, s.uneven, s.node_offset
            );
        }
        let g = gltf::Gltf::from_slice(&bytes).expect("gltf");
        let blob = g.blob.clone().expect("GLB");
        let nodes: Vec<gltf::Node> = g.nodes().collect();
        let mut parent = vec![None; nodes.len()];
        for n in &nodes {
            for c in n.children() {
                parent[c.index()] = Some(n.index());
            }
        }
        for part in ours.parts.iter().filter(|p| p.solid.skin.is_some()) {
            let skin = part.solid.skin.as_ref().unwrap();
            let Some(node) = nodes
                .iter()
                .find(|n| n.name() == Some(part.name.as_str()) && n.skin().is_some())
            else {
                println!("  part {:?}: no node of that name with a skin", part.name);
                continue;
            };
            let gskin = node.skin().unwrap();
            let joints: Vec<usize> = gskin.joints().map(|j| j.index()).collect();
            // The bone to turn: the one with the most bones below it, short
            // of the root.
            let below = |b: usize| skin.bones.iter().filter(|x| {
                let mut at = x.parent;
                while let Some(p) = at {
                    if p as usize == b { return true; }
                    at = skin.bones[p as usize].parent;
                }
                false
            }).count();
            let turned = (1..skin.bones.len()).max_by_key(|&b| (below(b), usize::MAX - b)).unwrap_or(0);
            let bone_name = &skin.bones[turned].name;
            let jnode = joints
                .iter()
                .copied()
                .find(|&j| nodes[j].name() == Some(bone_name.as_str()))
                .expect("joint node");
            // The file, posed by its own matrices: the joint's local turned
            // about its own +X.
            let (sn, cs) = (libm::sin(angle), libm::cos(angle));
            let rx: M = [[1.0, 0.0, 0.0, 0.0], [0.0, cs, sn, 0.0], [0.0, -sn, cs, 0.0], [0.0, 0.0, 0.0, 1.0]];
            let world = |mut i: usize, turned: bool| -> M {
                let local = |n: usize| {
                    let l = f64m(nodes[n].transform().matrix());
                    if turned && n == jnode { mul(&l, &rx) } else { l }
                };
                let mut m = local(i);
                while let Some(p) = parent[i] {
                    m = mul(&local(p), &m);
                    i = p;
                }
                m
            };
            let ibms: Vec<M> = gskin
                .reader(|_| Some(&blob))
                .read_inverse_bind_matrices()
                .map(|it| it.map(f64m).collect())
                .unwrap_or_else(|| vec![[[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]; joints.len()]);
            // The file's bind pose, from its own matrices: `bind` is
            // `world · inverse bind` of the bone weighting the most
            // vertices; each joint's bind world is `bind · inverse bind⁻¹`.
            // The turned joint and every joint below it move by
            // `BW_k · R · BW_k⁻¹`.
            let mut counts = vec![0usize; joints.len()];
            for prim in node.mesh().unwrap().primitives() {
                let r = prim.reader(|_| Some(&blob));
                let js: Vec<[u16; 4]> = r.read_joints(0).unwrap().into_u16().collect();
                let ws: Vec<[f32; 4]> = r.read_weights(0).unwrap().into_f32().collect();
                for (j, w) in js.iter().zip(&ws) {
                    for k in 0..4 {
                        if w[k] > 0.0 {
                            counts[j[k] as usize] += 1;
                        }
                    }
                }
            }
            let main = (0..joints.len()).max_by(|&a, &b| counts[a].cmp(&counts[b]).then(b.cmp(&a))).unwrap();
            let bind = mul(&world(joints[main], false), &ibms[main]);
            let bw: Vec<M> = ibms.iter().map(|m| mul(&bind, &inverse(m))).collect();
            let kj = joints.iter().position(|&j| j == jnode).unwrap();
            let motion = mul(&mul(&bw[kj], &rx), &inverse(&bw[kj]));
            let below = |j: usize| {
                let mut at = Some(joints[j]);
                while let Some(n) = at {
                    if n == jnode {
                        return true;
                    }
                    at = parent[n];
                }
                false
            };
            let skinning: Vec<M> = (0..joints.len())
                .map(|j| {
                    let w = if below(j) { mul(&motion, &bw[j]) } else { bw[j] };
                    mul(&w, &ibms[j])
                })
                .collect();
            let mut file: Vec<DVec3> = Vec::new();
            for prim in node.mesh().unwrap().primitives() {
                let r = prim.reader(|_| Some(&blob));
                let pos: Vec<[f32; 3]> = r.read_positions().unwrap().collect();
                let js: Vec<[u16; 4]> = r.read_joints(0).unwrap().into_u16().collect();
                let ws: Vec<[f32; 4]> = r.read_weights(0).unwrap().into_f32().collect();
                for ((p, j), w) in pos.iter().zip(&js).zip(&ws) {
                    let v = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
                    let total: f64 = w.iter().map(|&x| x as f64).sum();
                    let mut q = DVec3::ZERO;
                    for k in 0..4 {
                        if w[k] > 0.0 {
                            q += apply(&skinning[j[k] as usize], v) * (w[k] as f64 / total);
                        }
                    }
                    // glTF metres, Y up → kernel mm, Z up.
                    file.push(DVec3::new(q.x, -q.z, q.y) * 1000.0);
                }
            }
            let posed = pose(
                &part.solid,
                &Pose {
                    turns: vec![Turn {
                        bone: bone_name.clone(),
                        axis: DVec3::X,
                        angle,
                    }],
                },
            )
            .expect("pose");
            let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
            let mut worst: f64 = 0.0;
            let mut moved: f64 = 0.0;
            for ((p, q), r) in posed.positions.iter().zip(&file).zip(&part.solid.positions) {
                worst = worst.max((*p - *q).length());
                moved = moved.max((*p - *r).length());
                lo = lo.min(*q);
                hi = hi.max(*q);
            }
            println!(
                "  part {:?}: {} bones, {} vertices; turned {:?} by {angle} rad (moves up to {:.1} mm); our pose vs the file's: max {:.2e} mm over a {:.0} mm model",
                part.name,
                skin.bones.len(),
                part.solid.positions.len(),
                bone_name,
                moved,
                worst,
                (hi - lo).length()
            );
        }
    }
}
