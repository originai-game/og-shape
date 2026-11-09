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
