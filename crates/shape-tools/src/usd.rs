//! Writes a scene as USD text (`.usda`): a mesh a part, its material as
//! a `UsdPreviewSurface`, its maps as files beside the stage
//! (`docs/research.md` §54).
//!
//! Entry: [`usda`]. The stage keeps the kernel's own space, said in its
//! header: Z up, `metersPerUnit` 0.001, so points are millimetres as
//! built. Meshes are triangles with a normal and a UV at every corner.
//! A skinned or morphing part is a `SkelRoot`: its bones a `Skeleton`,
//! its weights on the mesh, its morph targets `BlendShape`s, its clips
//! `SkelAnimation`s ([`usda_clips`]). The scene's own joints (hinges
//! and sliders between parts) are `UsdPhysics` joints between the
//! parts' prims. [`usdz`] packs a stage and its maps into one file.

use std::collections::BTreeSet;

use shape_mesh::SkinClip;
use shape_mesh::skin::{Mat4, affine_inverse, frame_inverse_matrix, frame_matrix, mat_mul};

use crate::export::{ColorSpace, ExportError, Material, Scene, check_joints, decompose, linear};
use crate::preview::{CREASE_DEGREES, corner_normals};

/// A scene as USD.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Usd {
    /// The stage, as `.usda` text.
    pub text: String,
    /// The maps the stage names: file name and bytes. They go beside
    /// the stage.
    pub textures: Vec<(String, Vec<u8>)>,
    /// What the scene holds that the stage does not.
    pub not_written: Vec<String>,
}

/// A prim name: letters, digits and underscores, not starting with a
/// digit, and not one already taken.
fn prim(name: &str, taken: &mut BTreeSet<String>) -> String {
    let mut id: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if id.is_empty() || id.starts_with(|c: char| c.is_ascii_digit()) {
        id.insert(0, '_');
    }
    while !taken.insert(id.clone()) {
        id.push('_');
    }
    id
}

fn f(v: f64) -> String {
    format!("{}", if v == 0.0 { 0.0 } else { v as f32 })
}

fn triple(v: [f64; 3]) -> String {
    format!("({}, {}, {})", f(v[0]), f(v[1]), f(v[2]))
}

fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `values` as a USD array, `per` to a line.
fn array(values: impl Iterator<Item = String>, per: usize) -> String {
    let mut out = String::from("[");
    for (i, v) in values.enumerate() {
        if i > 0 {
            out.push(',');
            out.push(if i % per == 0 { '\n' } else { ' ' });
        }
        out.push_str(&v);
    }
    out.push(']');
    out
}

/// One map as a `UsdUVTexture` under the material at `path`: its
/// shader's text, and the file to write, when USD can read the image.
#[allow(clippy::too_many_arguments)]
fn texture(
    path: &str,
    name: &str,
    file: &str,
    color: bool,
    scale: Option<[f64; 4]>,
    bias: Option<[f64; 4]>,
    outputs: &[&str],
) -> String {
    let four = |v: [f64; 4]| format!("({}, {}, {}, {})", f(v[0]), f(v[1]), f(v[2]), f(v[3]));
    let mut s = format!(
        "            def Shader {}\n            {{\n                uniform token info:id = \"UsdUVTexture\"\n                asset inputs:file = @./{file}@\n                float2 inputs:st.connect = <{path}/st.outputs:result>\n                token inputs:sourceColorSpace = \"{}\"\n                token inputs:wrapS = \"repeat\"\n                token inputs:wrapT = \"repeat\"\n",
        quoted(name),
        if color { "sRGB" } else { "raw" }
    );
    if let Some(v) = scale {
        s.push_str(&format!(
            "                float4 inputs:scale = {}\n",
            four(v)
        ));
    }
    if let Some(v) = bias {
        s.push_str(&format!(
            "                float4 inputs:bias = {}\n",
            four(v)
        ));
    }
    for o in outputs {
        let kind = if *o == "rgb" { "float3" } else { "float" };
        s.push_str(&format!("                {kind} outputs:{o}\n"));
    }
    s.push_str("            }\n");
    s
}

/// A whole number or a decimal, as short as it reads back.
fn d(v: f64) -> String {
    format!("{}", if v == 0.0 { 0.0 } else { v })
}

/// A matrix as USD writes one. USD multiplies row vectors, so its rows
/// are this column-major matrix's columns: the array as it stands, four
/// to a row, the translation last.
fn matrix(m: &Mat4) -> String {
    let row = |r: usize| {
        format!(
            "({}, {}, {}, {})",
            d(m[4 * r]),
            d(m[4 * r + 1]),
            d(m[4 * r + 2]),
            d(m[4 * r + 3])
        )
    };
    format!("( {}, {}, {}, {} )", row(0), row(1), row(2), row(3))
}

/// What a skinned or morphing part adds to its stage: the text of its
/// skeleton and clips (to go before its mesh, inside its `SkelRoot`),
/// and of the attributes and blend shapes inside its mesh.
struct Rig {
    before: String,
    inside: String,
    /// Its blend shapes, to go last inside the mesh.
    shapes: String,
    /// The last time code any clip reaches.
    end: f64,
}

/// Time codes a second: a clip's key at t seconds is at code t × this.
const CODES: f64 = 60.0;

/// The rig of part `name` (a solid with a skin, morph targets or
/// both), under the `SkelRoot` at `at`, its mesh named `mesh`.
fn rig(
    name: &str,
    s: &shape_mesh::Solid,
    at: &str,
    mesh: &str,
    clips: &[SkinClip],
    out: &mut Usd,
) -> Result<Rig, ExportError> {
    let none = shape_mesh::Skin::default();
    let skin = s.skin.as_ref().unwrap_or(&none);
    let n = skin.bones.len();
    let mut taken: BTreeSet<String> = ["Skel".to_string(), mesh.to_string()].into();
    // Joint paths: each bone's name under its parent's path.
    let mut bone_names = BTreeSet::new();
    let mut paths: Vec<String> = Vec::with_capacity(n);
    for b in &skin.bones {
        let own = prim(&b.name, &mut bone_names);
        paths.push(match b.parent {
            Some(p) => format!("{}/{own}", paths[p as usize]),
            None => own,
        });
    }
    let tokens = array(paths.iter().map(|p| quoted(p)), 4);
    let binds: Vec<Mat4> = skin
        .bones
        .iter()
        .map(|b| affine_inverse(&b.inverse_bind).unwrap_or_else(|| frame_matrix(&b.rest)))
        .collect();
    let rests: Vec<Mat4> = skin
        .bones
        .iter()
        .map(|b| match b.parent {
            Some(p) => mat_mul(
                &frame_inverse_matrix(&skin.bones[p as usize].rest),
                &frame_matrix(&b.rest),
            ),
            None => frame_matrix(&b.rest),
        })
        .collect();
    let targets: Vec<(String, &shape_mesh::Morph)> = {
        let mut names = BTreeSet::new();
        s.morphs
            .iter()
            .flat_map(|m| &m.targets)
            .map(|t| (prim(&t.name, &mut names), t))
            .collect()
    };
    let shape_tokens = array(targets.iter().map(|t| quoted(&t.0)), 6);
    // The clips, each a SkelAnimation; the first is the one bound.
    let mut animations = String::new();
    let mut bound = None;
    let mut end = 0.0f64;
    for clip in clips {
        let bad = |why: String| ExportError::BadClip {
            clip: clip.name.clone(),
            why,
        };
        let driven = !clip.morphs.is_empty();
        if clip.times.is_empty()
            || clip.poses.len() != clip.times.len()
            || clip.poses.iter().any(|p| p.len() != n)
            || clip.times.iter().any(|t| !t.is_finite())
            || clip.times.windows(2).any(|w| w[1] <= w[0])
        {
            return Err(bad(format!(
                "part {name:?}: its times must rise, with a matrix a bone at each"
            )));
        }
        if driven
            && (clip.morphs.len() != clip.times.len()
                || clip.morphs.iter().any(|r| r.len() != targets.len()))
        {
            return Err(bad(format!(
                "part {name:?}: a weight a morph target ({}) at each time",
                targets.len()
            )));
        }
        let id = prim(&clip.name, &mut taken);
        // Per key: each bone under its parent, as a translation, a
        // rotation and a scale.
        let (mut moves, mut turns, mut sizes) = (String::new(), String::new(), String::new());
        let mut last: Vec<[f64; 4]> = Vec::new();
        for (time, pose) in clip.times.iter().zip(&clip.poses) {
            let world: Vec<Mat4> = (0..n).map(|b| mat_mul(&pose[b], &binds[b])).collect();
            let mut keys = Vec::with_capacity(n);
            for b in 0..n {
                let local = match skin.bones[b].parent {
                    Some(p) => {
                        let up = affine_inverse(&world[p as usize]).ok_or_else(|| {
                            bad(format!(
                                "bone {:?} is flattened by the clip",
                                skin.bones[p as usize].name
                            ))
                        })?;
                        mat_mul(&up, &world[b])
                    }
                    None => world[b],
                };
                let mut key = decompose(&local);
                if let Some(before) = last.get(b) {
                    // The same rotation the short way from the last key.
                    let dot: f64 = (0..4).map(|k| key.1[k] * before[k]).sum();
                    if dot < 0.0 {
                        key.1 = key.1.map(|c| -c);
                    }
                }
                if !key
                    .0
                    .iter()
                    .chain(&key.1)
                    .chain(&key.2)
                    .all(|c| c.is_finite())
                {
                    return Err(bad(format!(
                        "bone {:?} is not finite in it",
                        skin.bones[b].name
                    )));
                }
                keys.push(key);
            }
            last = keys.iter().map(|k| k.1).collect();
            let code = d(time * CODES);
            end = end.max(time * CODES);
            moves.push_str(&format!(
                "                {code}: {},\n",
                array(keys.iter().map(|k| triple(k.0)), 6)
            ));
            // USD's quaternions lead with the real part.
            turns.push_str(&format!(
                "                {code}: {},\n",
                array(
                    keys.iter().map(|k| format!(
                        "({}, {}, {}, {})",
                        f(k.1[3]),
                        f(k.1[0]),
                        f(k.1[1]),
                        f(k.1[2])
                    )),
                    4
                )
            ));
            sizes.push_str(&format!(
                "                {code}: {},\n",
                array(keys.iter().map(|k| triple(k.2)), 6)
            ));
        }
        animations.push_str(&format!(
            "        def SkelAnimation {}\n        {{\n",
            quoted(&id)
        ));
        if n > 0 {
            animations.push_str(&format!(
                "            uniform token[] joints = {tokens}\n            float3[] translations.timeSamples = {{\n{moves}            }}\n            quatf[] rotations.timeSamples = {{\n{turns}            }}\n            half3[] scales.timeSamples = {{\n{sizes}            }}\n"
            ));
        }
        if driven {
            let mut weights = String::new();
            for (time, row) in clip.times.iter().zip(&clip.morphs) {
                weights.push_str(&format!(
                    "                {}: {},\n",
                    d(time * CODES),
                    array(row.iter().map(|w| f(*w)), 12)
                ));
            }
            animations.push_str(&format!(
                "            uniform token[] blendShapes = {shape_tokens}\n            float[] blendShapeWeights.timeSamples = {{\n{weights}            }}\n"
            ));
        }
        animations.push_str("        }\n\n");
        match &bound {
            None => bound = Some(id),
            Some(first) => out.not_written.push(format!(
                "{name}: clip {:?} is written but not bound (a skeleton plays one; {first:?} is)",
                clip.name
            )),
        }
    }
    if clips.is_empty() && targets.iter().any(|t| t.1.weight != 0.0) {
        out.not_written
            .push(format!("{name}: its morph targets' weights at rest"));
    }
    let mut before = String::from(
        "        def Skeleton \"Skel\" (\n            prepend apiSchemas = [\"SkelBindingAPI\"]\n        )\n        {\n",
    );
    before.push_str(&format!(
        "            uniform token[] joints = {tokens}\n            uniform matrix4d[] bindTransforms = {}\n            uniform matrix4d[] restTransforms = {}\n",
        array(binds.iter().map(matrix), 1),
        array(rests.iter().map(matrix), 1)
    ));
    if let Some(id) = &bound {
        before.push_str(&format!(
            "            rel skel:animationSource = <{at}/{id}>\n"
        ));
    }
    before.push_str("        }\n\n");
    before.push_str(&animations);
    // On the mesh: each point's bones and weights, the skeleton, the
    // blend shapes.
    let mut inside = String::new();
    if n > 0 {
        inside.push_str(&format!(
            "            int[] primvars:skel:jointIndices = {} (\n                elementSize = 4\n                interpolation = \"vertex\"\n            )\n            float[] primvars:skel:jointWeights = {} (\n                elementSize = 4\n                interpolation = \"vertex\"\n            )\n            matrix4d primvars:skel:geomBindTransform = {}\n",
            array(
                skin.weights.iter().flat_map(|w| w.bones).map(|b| b.to_string()),
                32
            ),
            array(skin.weights.iter().flat_map(|w| w.weights).map(f), 16),
            matrix(&frame_matrix(&shape_core::Frame::IDENTITY))
        ));
    }
    inside.push_str(&format!("            rel skel:skeleton = <{at}/Skel>\n"));
    if !targets.is_empty() {
        inside.push_str(&format!(
            "            uniform token[] skel:blendShapes = {shape_tokens}\n            rel skel:blendShapeTargets = {}\n",
            array(targets.iter().map(|t| format!("<{at}/{mesh}/{}>", t.0)), 4)
        ));
    }
    let mut shapes = String::new();
    for (id, t) in &targets {
        shapes.push_str(&format!(
            "\n            def BlendShape {}\n            {{\n                uniform vector3f[] offsets = {}\n            }}\n",
            quoted(id),
            array(t.deltas.iter().map(|q| triple([q.x, q.y, q.z])), 6)
        ));
    }
    Ok(Rig {
        before,
        inside,
        shapes,
        end,
    })
}

/// `scene` as a USD stage, its maps named `<prefix>_<material>_<kind>`.
pub fn usda(scene: &Scene, prefix: &str) -> Result<Usd, ExportError> {
    usda_clips(scene, prefix, &[])
}

/// [`usda`], with the clips of its skinned or morphing parts as
/// `SkelAnimation`s: `skin_clips` gives each such part's, by its name.
/// A skeleton plays one clip, so a part's first is bound and the rest
/// are written beside it and named in [`Usd::not_written`]. Time codes
/// run 60 a second.
pub fn usda_clips(
    scene: &Scene,
    prefix: &str,
    skin_clips: &[(&str, &[SkinClip])],
) -> Result<Usd, ExportError> {
    let checked = check_joints(scene)?;
    let mut end = 0.0f64;
    let mut out = Usd::default();
    let mut taken = BTreeSet::new();
    let root = prim(
        if scene.name.is_empty() {
            "scene"
        } else {
            &scene.name
        },
        &mut taken,
    );
    let mut text = format!(
        "#usda 1.0\n(\n    defaultPrim = {}\n    doc = \"written by og-shape\"\n    metersPerUnit = 0.001\n    upAxis = \"Z\"\n)\n\ndef Xform {}\n{{\n",
        quoted(&root),
        quoted(&root)
    );
    // Each part's prim, for the joints.
    let mut prim_of: std::collections::BTreeMap<&str, String> = std::collections::BTreeMap::new();
    let mut materials = String::new();
    // The stage's own scopes are not parts' names.
    let mut names: BTreeSet<String> = ["Materials".to_string(), "Joints".to_string()].into();
    let mut files = BTreeSet::new();
    let crease = shape_core::trig::cos(shape_core::trig::to_radians(CREASE_DEGREES));
    for p in &scene.parts {
        let s = p.solid;
        if s.triangles.is_empty() {
            continue;
        }
        let id = prim(&p.name, &mut names);
        let rigged = s.skin.is_some() || s.morphs.is_some();
        let mesh_from = text.len();
        prim_of.insert(
            p.name.as_str(),
            if rigged {
                format!("/{root}/{id}_rig/{id}")
            } else {
                format!("/{root}/{id}")
            },
        );
        let look = format!("/{root}/Materials/{id}");
        let n = s.triangles.len();
        let uvs = s.uvs.as_ref().filter(|u| u.len() == n);
        let colors = s.colors.as_ref().filter(|c| c.len() == n);
        text.push_str(&format!("    def Mesh {}\n    {{\n", quoted(&id)));
        text.push_str(&format!(
            "        int[] faceVertexCounts = {}\n",
            array((0..n).map(|_| "3".to_string()), 40)
        ));
        text.push_str(&format!(
            "        int[] faceVertexIndices = {}\n",
            array(s.triangles.iter().flatten().map(u32::to_string), 30)
        ));
        text.push_str(&format!(
            "        point3f[] points = {}\n",
            array(s.positions.iter().map(|q| triple([q.x, q.y, q.z])), 6)
        ));
        text.push_str(&format!(
            "        normal3f[] normals = {} (\n            interpolation = \"faceVarying\"\n        )\n",
            array(
                corner_normals(s, crease)
                    .iter()
                    .flatten()
                    .map(|q| triple([q.x, q.y, q.z])),
                6
            )
        ));
        if let Some(u) = uvs {
            text.push_str(&format!(
                "        texCoord2f[] primvars:st = {} (\n            interpolation = \"faceVarying\"\n        )\n",
                array(u.iter().flatten().map(|q| format!("({}, {})", f(q.x), f(q.y))), 8)
            ));
        }
        if let Some(c) = colors {
            text.push_str(&format!(
                "        color3f[] primvars:displayColor = {} (\n            interpolation = \"faceVarying\"\n        )\n",
                array(c.iter().flatten().map(|q| triple([q[0], q[1], q[2]])), 6)
            ));
            text.push_str(&format!(
                "        float[] primvars:displayOpacity = {} (\n            interpolation = \"faceVarying\"\n        )\n",
                array(c.iter().flatten().map(|q| f(q[3])), 20)
            ));
        }
        text.push_str("        uniform token subdivisionScheme = \"none\"\n");
        if p.material.double_sided {
            text.push_str("        uniform bool doubleSided = 1\n");
        }
        text.push_str(&format!(
            "        rel material:binding = <{look}>\n    }}\n\n"
        ));
        if rigged {
            // The mesh goes inside a SkelRoot, after its skeleton and
            // clips, with what binds it to them.
            let block = text.split_off(mesh_from);
            let at = format!("/{root}/{id}_rig");
            let clips = skin_clips
                .iter()
                .find(|c| c.0 == p.name)
                .map_or(&[][..], |c| c.1);
            let r = rig(&p.name, s, &at, &id, clips, &mut out)?;
            end = end.max(r.end);
            let block = block.trim_end().replacen(
                &format!("    def Mesh {}\n", quoted(&id)),
                &format!(
                    "    def Mesh {} (\n        prepend apiSchemas = [\"SkelBindingAPI\"]\n    )\n",
                    quoted(&id)
                ),
                1,
            );
            // One level deeper; the rig's own lines are at that depth
            // already: what binds the mesh goes before its material,
            // its blend shapes after.
            let deeper: String = block.lines().map(|l| format!("    {l}\n")).collect();
            let key = "            rel material:binding";
            let at_key = deeper.rfind(key).unwrap_or(deeper.len());
            let line_end = deeper[at_key..]
                .find('\n')
                .map_or(deeper.len(), |i| at_key + i + 1);
            text.push_str(&format!(
                "    def SkelRoot {}\n    {{\n{}{}{}{}{}{}    }}\n\n",
                quoted(&format!("{id}_rig")),
                r.before,
                &deeper[..at_key],
                r.inside,
                &deeper[at_key..line_end],
                r.shapes,
                &deeper[line_end..],
            ));
        }
        // The material: factors, and each map USD can read as a file.
        let m: &Material = &p.material;
        let to_linear = |c: f64| match m.color_space {
            ColorSpace::Srgb => linear(c),
            ColorSpace::Linear => c.clamp(0.0, 1.0),
        };
        let base = m.color.unwrap_or([1.0; 3]).map(to_linear);
        let strength = m.emissive_intensity.unwrap_or(1.0).max(0.0);
        let glow = m.emissive.map(|c| to_linear(c) * strength);
        let mut shaders = String::new();
        let mut inputs = String::new();
        let mut file_of = |kind: &str, t: &crate::Texture| -> Option<String> {
            let ext = match t.mime.as_str() {
                "image/png" => "png",
                "image/jpeg" => "jpg",
                _ => {
                    out.not_written.push(format!(
                        "{}: its {kind} {:?} (USD reads PNG and JPEG)",
                        p.name, t.name
                    ));
                    return None;
                }
            };
            if uvs.is_none() {
                out.not_written
                    .push(format!("{}: its {kind} (the part has no UVs)", p.name));
                return None;
            }
            let file = format!("{prefix}_{id}_{kind}.{ext}");
            if files.insert(file.clone()) {
                out.textures.push((file.clone(), t.bytes.clone()));
            }
            Some(file)
        };
        let wire = |input: &str, kind: &str, shader: &str, output: &str| {
            format!(
                "                {kind} inputs:{input}.connect = <{look}/{shader}.outputs:{output}>\n"
            )
        };
        match m.map.as_ref().and_then(|t| file_of("map", t)) {
            Some(file) => {
                let scale = Some([base[0], base[1], base[2], 1.0]);
                let body = texture(&look, "diffuse", &file, true, scale, None, &["rgb", "a"]);
                shaders.push_str(&body);
                inputs.push_str(&wire("diffuseColor", "color3f", "diffuse", "rgb"));
            }
            None => inputs.push_str(&format!(
                "                color3f inputs:diffuseColor = {}\n",
                triple(base)
            )),
        }
        let (metal, rough) = (
            m.metalness.clamp(0.0, 1.0),
            m.roughness.unwrap_or(1.0).clamp(0.0, 1.0),
        );
        match m.metalness_map.as_ref().and_then(|t| file_of("metalness_map", t)) {
            Some(file) => {
                let scale = Some([1.0, rough, metal, 1.0]);
                let body = texture(&look, "gloss", &file, false, scale, None, &["g", "b"]);
                shaders.push_str(&body);
                inputs.push_str(&wire("roughness", "float", "gloss", "g"));
                inputs.push_str(&wire("metallic", "float", "gloss", "b"));
            }
            None => inputs.push_str(&format!(
                "                float inputs:metallic = {}\n                float inputs:roughness = {}\n",
                f(metal),
                f(rough)
            )),
        }
        if let Some(file) = m.normal_map.as_ref().and_then(|t| file_of("normal_map", t)) {
            let k = m.normal_scale.unwrap_or(1.0);
            let body = texture(
                &look,
                "bump",
                &file,
                false,
                Some([2.0 * k, 2.0 * k, 2.0, 1.0]),
                Some([-k, -k, -1.0, 0.0]),
                &["rgb"],
            );
            shaders.push_str(&body);
            inputs.push_str(&wire("normal", "normal3f", "bump", "rgb"));
        }
        match m
            .emissive_map
            .as_ref()
            .and_then(|t| file_of("emissive_map", t))
        {
            Some(file) => {
                let scale = Some([glow[0], glow[1], glow[2], 1.0]);
                let body = texture(&look, "glow", &file, true, scale, None, &["rgb"]);
                shaders.push_str(&body);
                inputs.push_str(&wire("emissiveColor", "color3f", "glow", "rgb"));
            }
            None => inputs.push_str(&format!(
                "                color3f inputs:emissiveColor = {}\n",
                triple(glow)
            )),
        }
        if let Some(file) = m.ao_map.as_ref().and_then(|t| file_of("ao_map", t)) {
            let body = texture(&look, "shade", &file, false, None, None, &["r"]);
            shaders.push_str(&body);
            inputs.push_str(&wire("occlusion", "float", "shade", "r"));
        }
        inputs.push_str(&format!(
            "                float inputs:opacity = {}\n",
            f(m.opacity.unwrap_or(1.0).clamp(0.0, 1.0))
        ));
        if let Some(cut) = m.alpha_test {
            inputs.push_str(&format!(
                "                float inputs:opacityThreshold = {}\n",
                f(cut.clamp(0.0, 1.0))
            ));
        }
        let reader = if shaders.is_empty() {
            String::new()
        } else {
            "            def Shader \"st\"\n            {\n                uniform token info:id = \"UsdPrimvarReader_float2\"\n                string inputs:varname = \"st\"\n                float2 outputs:result\n            }\n".to_string()
        };
        materials.push_str(&format!(
            "        def Material {0}\n        {{\n            token outputs:surface.connect = <{look}/surface.outputs:surface>\n            def Shader \"surface\"\n            {{\n                uniform token info:id = \"UsdPreviewSurface\"\n{inputs}                token outputs:surface\n            }}\n{reader}{shaders}        }}\n",
            quoted(&id)
        ));
    }
    // The scene's joints, as UsdPhysics joints between the parts'
    // prims: where each turns or slides, and how far. The parts stand
    // as built, so a joint's frame is the same in both of its bodies:
    // its pivot, turned so that x is its axis.
    let mut joints = String::new();
    let mut joint_names = BTreeSet::new();
    for (p, (c, _)) in scene.parts.iter().zip(&checked) {
        let Some(c) = c else { continue };
        let kind = match c.kind {
            "revolute" | "continuous" => "PhysicsRevoluteJoint",
            "prismatic" => "PhysicsPrismaticJoint",
            "fixed" => "PhysicsFixedJoint",
            other => {
                out.not_written
                    .push(format!("joint {:?} ({other})", c.name));
                continue;
            }
        };
        let Some(child) = prim_of.get(p.name.as_str()) else {
            out.not_written
                .push(format!("joint {:?} (its part has no mesh)", c.name));
            continue;
        };
        let mut body = String::new();
        if let Some(parent) = prim_of.get(p.parent.as_str()) {
            body.push_str(&format!("            rel physics:body0 = <{parent}>\n"));
        }
        body.push_str(&format!("            rel physics:body1 = <{child}>\n"));
        let axis = c.local.map_or(shape_core::DVec3::X, |a| {
            c.pivot.x * a.x + c.pivot.y * a.y + c.pivot.z * a.z
        });
        // The turn that takes x to the axis: (1 + x·a, x × a), unit.
        let (w, v) = (1.0 + axis.x, [0.0, -axis.z, axis.y]);
        let length = (w * w + v[1] * v[1] + v[2] * v[2]).sqrt();
        let turn = if length < 1e-12 {
            "(0, 0, 0, 1)".to_string()
        } else {
            format!(
                "({}, {}, {}, {})",
                f(w / length),
                f(0.0),
                f(v[1] / length),
                f(v[2] / length)
            )
        };
        let at = triple([c.pivot.t.x, c.pivot.t.y, c.pivot.t.z]);
        if kind != "PhysicsFixedJoint" {
            body.push_str("            uniform token physics:axis = \"X\"\n");
        }
        for side in [0, 1] {
            body.push_str(&format!(
                "            point3f physics:localPos{side} = {at}\n            quatf physics:localRot{side} = {turn}\n"
            ));
        }
        if let (Some((lo, hi)), true) = (c.span, c.kind != "continuous") {
            // Degrees for a hinge; the stage's millimetres for a slide.
            let unit = if c.kind == "prismatic" {
                1000.0
            } else {
                180.0 / shape_core::trig::PI
            };
            body.push_str(&format!(
                "            float physics:lowerLimit = {}\n            float physics:upperLimit = {}\n",
                f(lo * unit),
                f(hi * unit)
            ));
        }
        if let Some((leader, _)) = &c.mimic {
            out.not_written.push(format!(
                "joint {:?} follows {leader:?} (USD has no such tie)",
                c.name
            ));
        }
        joints.push_str(&format!(
            "        def {kind} {}\n        {{\n{body}        }}\n",
            quoted(&prim(&c.name, &mut joint_names))
        ));
    }
    if !joints.is_empty() {
        text.push_str(&format!(
            "    def Scope \"Joints\"\n    {{\n{joints}    }}\n\n"
        ));
    }
    text.push_str(&format!(
        "    def Scope \"Materials\"\n    {{\n{materials}    }}\n}}\n"
    ));
    if end > 0.0 {
        text = text.replacen(
            "    upAxis = \"Z\"\n",
            &format!(
                "    upAxis = \"Z\"\n    timeCodesPerSecond = {}\n    startTimeCode = 0\n    endTimeCode = {}\n",
                d(CODES),
                d(end)
            ),
            1,
        );
    }
    out.text = text;
    Ok(out)
}

/// A stage and its maps as one `.usdz` file: a zip archive with
/// nothing compressed, the stage first under `stage_name` (a `.usda`
/// name), each file's bytes starting at a multiple of 64, as the
/// format asks.
pub fn usdz(usd: &Usd, stage_name: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut directory: Vec<u8> = Vec::new();
    let files = std::iter::once((stage_name, usd.text.as_bytes()))
        .chain(usd.textures.iter().map(|(n, b)| (n.as_str(), b.as_slice())));
    let mut count = 0u16;
    for (name, bytes) in files {
        let crc = crate::png::crc32(&[bytes]);
        let at = out.len();
        // Padding goes in an extra field (an id, a length, zeros), which
        // cannot be 1 to 3 bytes long.
        let mut pad = (64 - (at + 30 + name.len()) % 64) % 64;
        if (1..4).contains(&pad) {
            pad += 64;
        }
        let size = bytes.len() as u32;
        // Stored (method 0), dated 1980-01-01.
        let common = |out: &mut Vec<u8>| {
            out.extend_from_slice(&[10, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        };
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        common(&mut out);
        out.extend_from_slice(&(pad as u16).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        if pad > 0 {
            out.extend_from_slice(&[0x86, 0x19]);
            out.extend_from_slice(&((pad - 4) as u16).to_le_bytes());
            out.resize(out.len() + pad - 4, 0);
        }
        out.extend_from_slice(bytes);
        directory.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        directory.extend_from_slice(&[10, 0]);
        common(&mut directory);
        // No extra field or comment, disk 0, no attributes.
        directory.extend_from_slice(&[0; 12]);
        directory.extend_from_slice(&(at as u32).to_le_bytes());
        directory.extend_from_slice(name.as_bytes());
        count += 1;
    }
    let start = out.len() as u32;
    out.extend_from_slice(&directory);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&(directory.len() as u32).to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}
