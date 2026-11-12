//! Test support shared with the golden cases: skinned GLBs written by
//! hand.

/// A GLB of two triangles skinned to two bones (`hip`, `knee`): glTF's
/// skin, inverse bind matrices, `JOINTS_0` and `WEIGHTS_0`, written by
/// hand. With `skin_on_node` false, the weights are there but no node
/// names a skin.
pub fn skinned_glb(skin_on_node: bool) -> Vec<u8> {
    skinned_glb_under(skin_on_node, None)
}

/// The same legs under an armature node scaled by `armature`, as
/// exporters write centimetres (0.01): positions, bone offsets and
/// inverse bind matrices are in the armature's units, `1 / scale.x` times
/// larger, so the bind pose lands where the unscaled file's does.
pub fn skinned_glb_under(skin_on_node: bool, armature: Option<[f32; 3]>) -> Vec<u8> {
    skinned_glb_posed(skin_on_node, armature, None)
}

/// The same, saved posed: the knee node turned by the quaternion
/// `knee_turn` away from where its inverse bind matrix binds it.
pub fn skinned_glb_posed(
    skin_on_node: bool,
    armature: Option<[f32; 3]>,
    knee_turn: Option<[f32; 4]>,
) -> Vec<u8> {
    let unit = armature.map_or(1.0, |s| (1.0 / s[0]).round());
    let mut bin: Vec<u8> = Vec::new();
    let f32s = |bin: &mut Vec<u8>, v: &[f32]| v.iter().for_each(|x| bin.extend(x.to_le_bytes()));
    // 0: positions, 4 × VEC3 (48 bytes).
    let pos = [0.0, 0.0, 0.0, 0.1, 0.0, 0.0, 0.1, 0.5, 0.0, 0.0, 0.5, 0.0].map(|v: f32| v * unit);
    f32s(&mut bin, &pos);
    // 48: indices, 6 × u16 (12 bytes).
    for i in [0u16, 1, 2, 0, 2, 3] {
        bin.extend(i.to_le_bytes());
    }
    // 60: joints, 4 × u8 VEC4 (16 bytes).
    bin.extend([0u8, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]);
    // 76: weights, 4 × VEC4 (64 bytes).
    f32s(
        &mut bin,
        &[
            1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
        ],
    );
    // 140: inverse bind matrices, 2 × MAT4 (128 bytes).
    for ty in [0.0f32, -0.25 * unit] {
        f32s(
            &mut bin,
            &[
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, ty, 0.0, 1.0,
            ],
        );
    }
    let skin = if skin_on_node { r#","skin":0"# } else { "" };
    let knee = 0.25 * unit;
    let turn = knee_turn.map_or(String::new(), |q| {
        format!(r#","rotation":[{},{},{},{}]"#, q[0], q[1], q[2], q[3])
    });
    let (scene, armature) = match armature {
        Some(s) => (
            "[3]".to_string(),
            format!(
                r#",{{"name":"armature","scale":[{},{},{}],"children":[0,1]}}"#,
                s[0], s[1], s[2]
            ),
        ),
        None => ("[0,1]".to_string(), String::new()),
    };
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":{scene}}}],
"nodes":[{{"name":"legs","mesh":0{skin}}},{{"name":"hip","children":[2]}},{{"name":"knee","translation":[0,{knee},0]{turn}}}{armature}],
"skins":[{{"name":"rig","joints":[1,2],"inverseBindMatrices":4}}],
"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"JOINTS_0":2,"WEIGHTS_0":3}},"indices":1}}]}}],
"buffers":[{{"byteLength":{len}}}],
"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":48}},{{"buffer":0,"byteOffset":48,"byteLength":12}},
{{"buffer":0,"byteOffset":60,"byteLength":16}},{{"buffer":0,"byteOffset":76,"byteLength":64}},{{"buffer":0,"byteOffset":140,"byteLength":128}}],
"accessors":[{{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3"}},
{{"bufferView":1,"componentType":5123,"count":6,"type":"SCALAR"}},
{{"bufferView":2,"componentType":5121,"count":4,"type":"VEC4"}},
{{"bufferView":3,"componentType":5126,"count":4,"type":"VEC4"}},
{{"bufferView":4,"componentType":5126,"count":2,"type":"MAT4"}}]}}"#,
        len = bin.len()
    );
    let mut json = json.into_bytes();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let total = 12 + 8 + json.len() + 8 + bin.len();
    let mut out = b"glTF".to_vec();
    out.extend(2u32.to_le_bytes());
    out.extend((total as u32).to_le_bytes());
    out.extend((json.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(json);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(bin);
    out
}

/// A tube 100 mm tall, rings every 2 mm, skinned to a `root` bone at its
/// foot and a `knee` halfway up, weights blending over z ∈ [40, 60].
pub fn skinned_tube() -> shape_mesh::Solid {
    use shape_core::{DVec3, Frame};
    use shape_mesh::{Bone, Skin, Solid, Weights};
    let (rings, around, r) = (51usize, 32usize, 10.0);
    let mut s = Solid::default();
    let mut weights = Vec::new();
    let weight = |z: f64| {
        let t = ((z - 40.0) / 20.0).clamp(0.0, 1.0);
        Weights::from_raw([(0u32, 1.0 - t), (1u32, t)])
    };
    for i in 0..rings {
        let z = i as f64 * 2.0;
        for k in 0..around {
            let (sn, cs) =
                shape_core::trig::sin_cos(k as f64 / around as f64 * shape_core::trig::TAU);
            s.push_vertex(DVec3::new(r * cs, r * sn, z));
            weights.push(weight(z));
        }
    }
    let bottom = s.push_vertex(DVec3::ZERO);
    let top = s.push_vertex(DVec3::new(0.0, 0.0, 100.0));
    weights.extend([weight(0.0), weight(100.0)]);
    let id = |i: usize, k: usize| (i * around + k % around) as u32;
    for i in 0..rings - 1 {
        for k in 0..around {
            let (a, b, c, d) = (id(i, k), id(i, k + 1), id(i + 1, k + 1), id(i + 1, k));
            s.push_triangle([a, b, c], 0, &["tube"]);
            s.push_triangle([a, c, d], 0, &["tube"]);
        }
    }
    for k in 0..around {
        s.push_triangle([bottom, id(0, k + 1), id(0, k)], 1, &["bottom"]);
        s.push_triangle([top, id(rings - 1, k), id(rings - 1, k + 1)], 2, &["top"]);
    }
    s.skin = Some(Skin {
        bones: vec![
            Bone::new("root", None, Frame::IDENTITY),
            Bone::new(
                "knee",
                Some(0),
                Frame {
                    origin: DVec3::new(0.0, 0.0, 50.0),
                    ..Frame::IDENTITY
                },
            ),
        ],
        weights,
    });
    s.canonicalize().unwrap();
    s
}

/// [`skinned_glb`] with one animation, `bend`: the knee turns a quarter
/// turn about the file's z in one second (two keys, linear), and the hip
/// has a stepped translation channel that moves it 0.1 along x at 0.5 s.
/// With `armature`, under a scaled armature node as
/// [`skinned_glb_under`] writes it.
pub fn skinned_glb_animated(armature: Option<[f32; 3]>) -> Vec<u8> {
    let glb = skinned_glb_under(true, armature);
    let json_len = u32::from_le_bytes([glb[12], glb[13], glb[14], glb[15]]) as usize;
    let json = String::from_utf8(glb[20..20 + json_len].to_vec()).unwrap();
    let mut bin = glb[20 + json_len + 8..].to_vec();
    let old_len = bin.len();
    let unit = armature.map_or(1.0, |s| (1.0 / s[0]).round());
    let mut f32s = |v: &[f32]| v.iter().for_each(|x| bin.extend(x.to_le_bytes()));
    // Times 0, 1; rotations: none, a quarter turn about z.
    f32s(&[0.0, 1.0]);
    let h = std::f32::consts::FRAC_1_SQRT_2;
    f32s(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, h, h]);
    // Times 0, 0.5; translations of the hip: none, 0.1 along x.
    f32s(&[0.0, 0.5]);
    f32s(&[0.0, 0.0, 0.0, 0.1 * unit, 0.0, 0.0]);
    let at = |k: usize| old_len + k;
    let views = format!(
        r#",{{"buffer":0,"byteOffset":{},"byteLength":8}},{{"buffer":0,"byteOffset":{},"byteLength":32}},{{"buffer":0,"byteOffset":{},"byteLength":8}},{{"buffer":0,"byteOffset":{},"byteLength":24}}],
"accessors""#,
        at(0),
        at(8),
        at(40),
        at(48)
    );
    let accessors = r#",{"bufferView":5,"componentType":5126,"count":2,"type":"SCALAR"},
{"bufferView":6,"componentType":5126,"count":2,"type":"VEC4"},
{"bufferView":7,"componentType":5126,"count":2,"type":"SCALAR"},
{"bufferView":8,"componentType":5126,"count":2,"type":"VEC3"}],
"animations":[{"name":"bend","channels":[{"sampler":0,"target":{"node":2,"path":"rotation"}},
{"sampler":1,"target":{"node":1,"path":"translation"}}],
"samplers":[{"input":5,"output":6,"interpolation":"LINEAR"},{"input":7,"output":8,"interpolation":"STEP"}]}]}"#;
    let json = json.trim_end();
    let json = json
        .replace(
            &format!(r#""buffers":[{{"byteLength":{old_len}}}]"#),
            &format!(r#""buffers":[{{"byteLength":{}}}]"#, bin.len()),
        )
        .replacen("],\n\"accessors\"", &views, 1);
    let body = json.strip_suffix("]}").unwrap();
    let mut json = format!("{body}{accessors}").into_bytes();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let total = 12 + 8 + json.len() + 8 + bin.len();
    let mut out = b"glTF".to_vec();
    out.extend(2u32.to_le_bytes());
    out.extend((total as u32).to_le_bytes());
    out.extend((json.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(json);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(bin);
    out
}
