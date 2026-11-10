//! A plain GLB of one solid: one node, one mesh, one primitive, one
//! material, positions, normals and (if the solid has them) texture
//! coordinates, with the base-colour texture if one is given: what a
//! texturing tool takes once a mesh is closed, joined, triangulated,
//! under its triangle budget and unwrapped.

use std::collections::BTreeMap;
use std::fmt;

use shape_core::DVec3;
use shape_mesh::Solid;

use crate::export::{ExportError, check_texture, gltf_uv, texture_json};
use crate::image::Texture;
use crate::json::{Json, f32n, nums};
use crate::surface::{METRES, y_up};

/// What to write besides the geometry. The zero value writes a mesh named
/// `mesh` with no texture. Texture coordinates are the solid's own
/// ([`Solid::uvs`], from import or [`crate::unwrap()`]).
#[derive(Clone, Copy, Debug, Default)]
pub struct PlainGlb<'a> {
    /// The node's and the mesh's name; "" is `mesh`.
    pub name: &'a str,
    /// The material's base-colour texture (three.js `map`): PNG or JPEG,
    /// on a solid with UVs.
    pub map: Option<&'a Texture>,
}

/// Why a plain GLB could not be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlainGlbError {
    /// A position or UV is NaN or infinite.
    NotFinite,
    /// The UVs are for a different number of triangles.
    UvMismatch,
    /// The solid is skinned and this writer writes no skin (phase 5b).
    Skinned,
    /// The texture cannot be written (not PNG or JPEG, or no UVs): why.
    BadTexture(String),
}

impl fmt::Display for PlainGlbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlainGlbError::NotFinite => f.write_str("a position or UV is not finite"),
            PlainGlbError::Skinned => f.write_str(
                "the solid is skinned and a plain GLB carries no skin, so it is refused",
            ),
            PlainGlbError::UvMismatch => {
                f.write_str("the UVs were made for a different number of triangles")
            }
            PlainGlbError::BadTexture(why) => write!(f, "the texture: {why}"),
        }
    }
}

impl std::error::Error for PlainGlbError {}

/// The solid as a plain GLB: metres, Y up (as [`crate::glb`]), normals
/// smooth within a surface, one vertex per (position, surface, UV), u32
/// indices, three.js's default material. UVs are written with v flipped
/// (glTF's origin is the top left). The bytes depend only on the input.
/// A skinned solid is refused: a plain GLB promises one mesh with no
/// skin, and dropping the bones would break it silently. [`crate::glb`]
/// writes skins.
pub fn glb_plain(s: &Solid, d: &PlainGlb) -> Result<Vec<u8>, PlainGlbError> {
    if s.skin.is_some() {
        return Err(PlainGlbError::Skinned);
    }
    if s.positions.iter().any(|p| !p.is_finite()) {
        return Err(PlainGlbError::NotFinite);
    }
    let uvs = match &s.uvs {
        Some(u) if u.len() != s.triangles.len() => return Err(PlainGlbError::UvMismatch),
        Some(u) => {
            if u.iter().flatten().any(|q| !q.is_finite()) {
                return Err(PlainGlbError::NotFinite);
            }
            Some(u)
        }
        None => None,
    };
    if let Some(t) = d.map {
        check_texture(t, "", uvs.is_some()).map_err(|e| match e {
            ExportError::BadTexture { why, .. } => PlainGlbError::BadTexture(why),
            _ => PlainGlbError::BadTexture(e.to_string()),
        })?;
    }
    // Normals per (vertex, surface).
    let mut normal: BTreeMap<(u32, u32), DVec3> = BTreeMap::new();
    for (i, t) in s.triangles.iter().enumerate() {
        let [a, b, c] = t.map(|v| s.positions[v as usize]);
        let n = (b - a).cross(c - a);
        for &v in t {
            *normal.entry((v, s.surfaces[i])).or_default() += n;
        }
    }
    let mut ids: BTreeMap<(u32, u32, u64, u64), u32> = BTreeMap::new();
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nrm: Vec<[f32; 3]> = Vec::new();
    let mut tex: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::with_capacity(s.triangles.len() * 3);
    for (i, t) in s.triangles.iter().enumerate() {
        for (k, &v) in t.iter().enumerate() {
            let uv = uvs.map_or([0.0f32; 2], |u| gltf_uv(u[i][k]));
            let key = (
                v,
                s.surfaces[i],
                u64::from(uv[0].to_bits()),
                u64::from(uv[1].to_bits()),
            );
            let next = pos.len() as u32;
            let id = *ids.entry(key).or_insert(next);
            if id == next {
                let p = y_up(s.positions[v as usize]) * METRES;
                pos.push([p.x as f32, p.y as f32, p.z as f32]);
                let n = normal[&(v, s.surfaces[i])];
                let len = n.length();
                let n = y_up(if len > 0.0 { n / len } else { DVec3::Z });
                nrm.push([n.x as f32, n.y as f32, n.z as f32]);
                tex.push(uv);
            }
            indices.push(id);
        }
    }
    let name = if d.name.is_empty() { "mesh" } else { d.name };
    let mut bin: Vec<u8> = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let mut add =
        |bin: &mut Vec<u8>, data: Vec<u8>, target: u64, acc: Vec<(&'static str, Json)>| {
            let start = bin.len();
            bin.extend_from_slice(&data);
            views.push(Json::Obj(vec![
                ("buffer", Json::Int(0)),
                ("byteOffset", Json::Int(start as u64)),
                ("byteLength", Json::Int(data.len() as u64)),
                ("target", Json::Int(target)),
            ]));
            let mut fields = vec![("bufferView", Json::Int(views.len() as u64 - 1))];
            fields.extend(acc);
            accessors.push(Json::Obj(fields));
            accessors.len() as u64 - 1
        };
    let floats = |v: &[f32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
    let mut attributes = Vec::new();
    if !pos.is_empty() {
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for p in &pos {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let flat: Vec<f32> = pos.iter().flatten().copied().collect();
        let a = add(
            &mut bin,
            floats(&flat),
            34962,
            vec![
                ("componentType", Json::Int(5126)),
                ("count", Json::Int(pos.len() as u64)),
                ("type", Json::Str("VEC3".into())),
                ("min", Json::Arr(lo.iter().map(|&v| f32n(v)).collect())),
                ("max", Json::Arr(hi.iter().map(|&v| f32n(v)).collect())),
            ],
        );
        attributes.push(("POSITION", Json::Int(a)));
        let flat: Vec<f32> = nrm.iter().flatten().copied().collect();
        let a = add(
            &mut bin,
            floats(&flat),
            34962,
            vec![
                ("componentType", Json::Int(5126)),
                ("count", Json::Int(nrm.len() as u64)),
                ("type", Json::Str("VEC3".into())),
            ],
        );
        attributes.push(("NORMAL", Json::Int(a)));
        if uvs.is_some() {
            let flat: Vec<f32> = tex.iter().flatten().copied().collect();
            let a = add(
                &mut bin,
                floats(&flat),
                34962,
                vec![
                    ("componentType", Json::Int(5126)),
                    ("count", Json::Int(tex.len() as u64)),
                    ("type", Json::Str("VEC2".into())),
                ],
            );
            attributes.push(("TEXCOORD_0", Json::Int(a)));
        }
    }
    let mut doc = vec![
        (
            "asset",
            Json::Obj(vec![
                ("version", Json::Str("2.0".into())),
                ("generator", Json::Str("og-shape".into())),
            ]),
        ),
        ("scene", Json::Int(0)),
        (
            "scenes",
            Json::Arr(vec![Json::Obj(vec![(
                "nodes",
                Json::Arr(vec![Json::Int(0)]),
            )])]),
        ),
    ];
    if attributes.is_empty() {
        doc.push((
            "nodes",
            Json::Arr(vec![Json::Obj(vec![("name", Json::Str(name.into()))])]),
        ));
    } else {
        let ia = add(
            &mut bin,
            indices.iter().flat_map(|i| i.to_le_bytes()).collect(),
            34963,
            vec![
                ("componentType", Json::Int(5125)),
                ("count", Json::Int(indices.len() as u64)),
                ("type", Json::Str("SCALAR".into())),
            ],
        );
        doc.push((
            "nodes",
            Json::Arr(vec![Json::Obj(vec![
                ("name", Json::Str(name.into())),
                ("mesh", Json::Int(0)),
            ])]),
        ));
        doc.push((
            "meshes",
            Json::Arr(vec![Json::Obj(vec![
                ("name", Json::Str(name.into())),
                (
                    "primitives",
                    Json::Arr(vec![Json::Obj(vec![
                        ("attributes", Json::Obj(attributes)),
                        ("indices", Json::Int(ia)),
                        ("material", Json::Int(0)),
                        ("mode", Json::Int(4)),
                    ])]),
                ),
            ])]),
        ));
        doc.push((
            "materials",
            Json::Arr(vec![Json::Obj(vec![
                ("name", Json::Str(name.into())),
                (
                    "pbrMetallicRoughness",
                    Json::Obj(match d.map {
                        Some(_) => vec![
                            ("baseColorFactor", nums(&[1.0, 1.0, 1.0, 1.0])),
                            ("baseColorTexture", Json::Obj(vec![("index", Json::Int(0))])),
                            ("metallicFactor", Json::Num(0.0)),
                            ("roughnessFactor", Json::Num(1.0)),
                        ],
                        None => vec![
                            ("baseColorFactor", nums(&[1.0, 1.0, 1.0, 1.0])),
                            ("metallicFactor", Json::Num(0.0)),
                            ("roughnessFactor", Json::Num(1.0)),
                        ],
                    }),
                ),
            ])]),
        ));
        doc.push(("accessors", Json::Arr(accessors)));
        doc.extend(texture_json(
            &d.map.into_iter().collect::<Vec<_>>(),
            &mut bin,
            &mut views,
        ));
        doc.push(("bufferViews", Json::Arr(views)));
        doc.push((
            "buffers",
            Json::Arr(vec![Json::Obj(vec![(
                "byteLength",
                Json::Int(bin.len() as u64),
            )])]),
        ));
    }
    let mut text = String::new();
    Json::Obj(doc).write(&mut text);
    let mut json = text.into_bytes();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let with_bin = !bin.is_empty();
    let total = 12 + 8 + json.len() + if with_bin { 8 + bin.len() } else { 0 };
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json);
    if with_bin {
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin);
    }
    Ok(out)
}
