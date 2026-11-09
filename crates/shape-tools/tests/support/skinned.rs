//! Test support shared with the golden cases: a skinned GLB written by
//! hand.

/// A GLB of two triangles skinned to two bones (`hip`, `knee`): glTF's
/// skin, inverse bind matrices, `JOINTS_0` and `WEIGHTS_0`, written by
/// hand. With `skin_on_node` false, the weights are there but no node
/// names a skin.
pub fn skinned_glb(skin_on_node: bool) -> Vec<u8> {
    let mut bin: Vec<u8> = Vec::new();
    let f32s = |bin: &mut Vec<u8>, v: &[f32]| v.iter().for_each(|x| bin.extend(x.to_le_bytes()));
    // 0: positions, 4 × VEC3 (48 bytes).
    f32s(
        &mut bin,
        &[0.0, 0.0, 0.0, 0.1, 0.0, 0.0, 0.1, 0.5, 0.0, 0.0, 0.5, 0.0],
    );
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
    for ty in [0.0f32, -0.25] {
        f32s(
            &mut bin,
            &[
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, ty, 0.0, 1.0,
            ],
        );
    }
    let skin = if skin_on_node { r#","skin":0"# } else { "" };
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0,1]}}],
"nodes":[{{"name":"legs","mesh":0{skin}}},{{"name":"hip","children":[2]}},{{"name":"knee","translation":[0,0.25,0]}}],
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
