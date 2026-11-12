//! Writes a scene as USD text (`.usda`): a mesh a part, its material as
//! a `UsdPreviewSurface`, its maps as files beside the stage
//! (`docs/research.md` §54).
//!
//! Entry: [`usda`]. The stage keeps the kernel's own space, said in its
//! header: Z up, `metersPerUnit` 0.001, so points are millimetres as
//! built. Meshes are triangles with a normal and a UV at every corner.
//! Joints, skins, morph targets and clips are not written.

use std::collections::BTreeSet;

use crate::export::{ColorSpace, ExportError, Material, Scene, check_joints, linear};
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

/// `scene` as a USD stage, its maps named `<prefix>_<material>_<kind>`.
pub fn usda(scene: &Scene, prefix: &str) -> Result<Usd, ExportError> {
    let checked = check_joints(scene)?;
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
    if checked.iter().any(|(c, _)| c.is_some()) {
        out.not_written
            .push("joints (the parts stand as built)".into());
    }
    let mut materials = String::new();
    let mut names = BTreeSet::new();
    let mut files = BTreeSet::new();
    let crease = shape_core::trig::cos(shape_core::trig::to_radians(CREASE_DEGREES));
    for p in &scene.parts {
        let s = p.solid;
        if s.skin.is_some() {
            out.not_written.push(format!("{}: its skin", p.name));
        }
        if s.morphs.is_some() {
            out.not_written
                .push(format!("{}: its morph targets", p.name));
        }
        if s.triangles.is_empty() {
            continue;
        }
        let id = prim(&p.name, &mut names);
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
    text.push_str(&format!(
        "    def Scope \"Materials\"\n    {{\n{materials}    }}\n}}\n"
    ));
    out.text = text;
    Ok(out)
}
