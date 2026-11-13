//! Reports as JSON: keys in a fixed order, numbers as serde_json prints
//! them (shortest round-trip, the same on every host), no thresholds.
//!
//! Entry: [`object`], which lays out fields whose values are already JSON
//! text, so a kernel report (`MeshMeasures::to_json`) is embedded as the
//! kernel wrote it, never parsed and printed again. The other functions
//! write one kernel report each, in `shape-tools`' snake_case.

use serde_json::{Value, json};
use shape_tools::{
    Blocked, ColorSpace, DecimateReport, ImageInfo, Imported, Material, NonManifold, RepairReport,
    Uvs,
};

/// `{"k": v, …}` from keys and values that are JSON text already.
pub fn object(fields: &[(&str, String)]) -> String {
    let body: Vec<String> = fields
        .iter()
        .map(|(k, v)| format!("{}:{v}", Value::from(*k)))
        .collect();
    format!("{{{}}}", body.join(","))
}

/// `[a, b, …]` from values that are JSON text already.
pub fn array(items: &[String]) -> String {
    format!("[{}]", items.join(","))
}

/// A string as a JSON literal.
pub fn string(s: &str) -> String {
    Value::from(s).to_string()
}

/// What `shape_tools::repair` changed.
pub fn repair(r: &RepairReport) -> String {
    let fused = match &r.fused {
        None => Value::Null,
        Some(Ok(pieces)) => json!({ "pieces_before": pieces }),
        Some(Err(why)) => json!({ "refused": why }),
    };
    object(&[
        ("welded", r.welded.to_string()),
        ("degenerate", r.degenerate.to_string()),
        ("duplicates", r.duplicates.to_string()),
        ("floaters", r.floaters.to_string()),
        ("floater_triangles", r.floater_triangles.to_string()),
        ("flipped", r.flipped.to_string()),
        (
            "non_manifold",
            r.non_manifold.as_ref().map_or("null".into(), non_manifold),
        ),
        ("holes_filled", r.holes_filled.to_string()),
        ("large_holes_filled", r.large_holes_filled.to_string()),
        ("crossing_fills", r.crossing_fills.to_string()),
        ("round_fills", r.round_fills.to_string()),
        ("fill_triangles", r.fill_triangles.to_string()),
        ("uv_seam_fills", r.uv_seam_fills.to_string()),
        ("holes_left", r.holes_left.to_string()),
        (
            "thickened",
            r.thickened.map_or("null".into(), |t| {
                object(&[
                    ("pieces", t.pieces.to_string()),
                    ("skipped", t.skipped.to_string()),
                    ("triangles", t.triangles.to_string()),
                ])
            }),
        ),
        ("fused", fused.to_string()),
        ("pockets", r.pockets.to_string()),
        ("pockets_dropped", r.pockets_dropped.to_string()),
        (
            "pocket_triangles_dropped",
            r.pocket_triangles_dropped.to_string(),
        ),
        ("short_edges", r.short_edges.to_string()),
        (
            "self_intersections",
            r.self_intersections
                .map_or("null".into(), |n| n.to_string()),
        ),
    ])
}

/// What splitting non-manifold edges did.
fn non_manifold(n: &NonManifold) -> String {
    object(&[
        ("edges", n.edges.to_string()),
        ("split", n.split.to_string()),
        ("unpaired", n.unpaired.to_string()),
        ("left", n.left.to_string()),
        ("vertices", n.vertices.to_string()),
        ("moved", Value::from(n.moved).to_string()),
        ("crossings_added", n.crossings_added.to_string()),
        ("sheets_dropped", n.sheets_dropped.to_string()),
        (
            "sheet_triangles_dropped",
            n.sheet_triangles_dropped.to_string(),
        ),
    ])
}

/// What `shape_tools::decimate` did.
pub fn decimate(r: &DecimateReport) -> String {
    object(&[
        ("triangles_before", r.triangles_before.to_string()),
        ("triangles", r.triangles.to_string()),
        ("target", r.target.to_string()),
        ("error", Value::from(r.error).to_string()),
        ("folds_refused", r.folds_refused.to_string()),
        ("uv_islands", r.uv_islands.to_string()),
        ("uv_charts", r.uv_charts.to_string()),
        ("blocked", blocked(&r.blocked)),
    ])
}

/// Why the edges left after a decimation that missed its target cannot
/// collapse (all zero when it reached it).
fn blocked(b: &Blocked) -> String {
    object(&[
        ("fixed", b.fixed.to_string()),
        ("error", b.error.to_string()),
        ("link", b.link.to_string()),
        ("turn", b.turn.to_string()),
        ("uv", b.uv.to_string()),
        ("fold", b.fold.to_string()),
        ("not_retried", b.not_retried.to_string()),
    ])
}

/// What `shape_tools::unwrap` made.
pub fn unwrap(u: &Uvs) -> String {
    object(&[
        ("charts", u.chart_count.to_string()),
        ("scale", Value::from(u.scale).to_string()),
    ])
}

/// What an import read: its parts, images, and what it skipped.
pub fn imported(format: &str, m: &Imported) -> String {
    let parts: Vec<String> = m
        .parts
        .iter()
        .map(|p| {
            // `map` (the base-colour texture's name) only on a part that
            // has one.
            let mut fields = vec![
                ("name", string(&p.name)),
                ("material", string(&p.material.name)),
                ("triangles", p.solid.triangles.len().to_string()),
                ("vertices", p.solid.positions.len().to_string()),
                ("uvs", p.solid.uvs.is_some().to_string()),
            ];
            if let Some(t) = &p.material.map {
                fields.push(("map", string(&t.name)));
            }
            fields.push(("colors", p.solid.colors.is_some().to_string()));
            fields.push(("look", material(&p.material, &p.defaults, &[])));
            fields.push((
                "bones",
                p.solid.skin.as_ref().map_or("null".into(), |k| {
                    array(&k.bones.iter().map(|b| string(&b.name)).collect::<Vec<_>>())
                }),
            ));
            if let Some(m) = &p.solid.morphs {
                fields.push((
                    "morphs",
                    array(
                        &m.targets
                            .iter()
                            .map(|t| string(&t.name))
                            .collect::<Vec<_>>(),
                    ),
                ));
            }
            object(&fields)
        })
        .collect();
    // The file's clips, each once (parts skinned to one skeleton share
    // them): its name, length and keys.
    let mut clips: Vec<String> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for c in m.parts.iter().flat_map(|p| &p.clips) {
        if !seen.contains(&c.name.as_str()) {
            seen.push(&c.name);
            clips.push(object(&[
                ("name", string(&c.name)),
                ("duration", format!("{}", c.duration())),
                ("keys", c.times.len().to_string()),
            ]));
        }
    }
    let images: Vec<String> = m
        .images
        .iter()
        .map(|i| {
            object(&[
                ("name", string(&i.name)),
                ("format", string(&i.format)),
                ("width", i.width.map_or("null".into(), |w| w.to_string())),
                ("height", i.height.map_or("null".into(), |h| h.to_string())),
                ("file_bytes", i.file_bytes.to_string()),
                ("error", i.error.as_deref().map_or("null".into(), string)),
            ])
        })
        .collect();
    let triangles: usize = m.parts.iter().map(|p| p.solid.triangles.len()).sum();
    let mut fields = vec![
        ("format", string(format)),
        ("parts", array(&parts)),
        ("triangles", triangles.to_string()),
        (
            "skinned",
            m.parts.iter().any(|p| p.solid.skin.is_some()).to_string(),
        ),
    ];
    // `clips` only on a file that has some.
    if !clips.is_empty() {
        fields.push(("clips", array(&clips)));
    }
    fields.extend([
        ("images", array(&images)),
        ("polygons_triangulated", m.polygons_triangulated.to_string()),
        (
            "skipped",
            array(&m.skipped.iter().map(|s| string(s)).collect::<Vec<_>>()),
        ),
    ]);
    object(&fields)
}

/// A material's factors and maps, with the glTF factors its file left
/// unset (`defaults`) and what a step could not keep (`dropped`). Colours
/// are as stored, in `color_space` (`linear`: glTF's factors; `srgb`:
/// written converted to linear).
pub fn material(m: &Material, defaults: &[String], dropped: &[String]) -> String {
    let num = |x: f64| Value::from(x).to_string();
    let opt = |x: Option<f64>| x.map_or("null".into(), num);
    let rgb = |c: [f64; 3]| array(&c.map(num));
    let alpha_mode = if m.alpha_test.is_some() {
        "MASK"
    } else if m.transparent.unwrap_or(m.opacity.unwrap_or(1.0) < 1.0) {
        "BLEND"
    } else {
        "OPAQUE"
    };
    let maps: Vec<String> = m
        .maps()
        .into_iter()
        .map(|(kind, t)| {
            let i = ImageInfo::from_bytes(&t.name, &t.bytes);
            object(&[
                ("kind", string(kind)),
                ("name", string(&t.name)),
                ("format", string(&i.format)),
                ("width", i.width.map_or("null".into(), |w| w.to_string())),
                ("height", i.height.map_or("null".into(), |h| h.to_string())),
                ("file_bytes", i.file_bytes.to_string()),
            ])
        })
        .collect();
    let strings = |v: &[String]| array(&v.iter().map(|x| string(x)).collect::<Vec<_>>());
    object(&[
        ("name", string(&m.name)),
        (
            "color_space",
            string(match m.color_space {
                ColorSpace::Srgb => "srgb",
                ColorSpace::Linear => "linear",
            }),
        ),
        ("color", rgb(m.color.unwrap_or([1.0; 3]))),
        ("opacity", num(m.opacity.unwrap_or(1.0))),
        ("metalness", num(m.metalness)),
        ("roughness", num(m.roughness.unwrap_or(1.0))),
        ("emissive", rgb(m.emissive)),
        ("emissive_intensity", opt(m.emissive_intensity)),
        ("alpha_mode", string(alpha_mode)),
        ("alpha_cutoff", opt(m.alpha_test)),
        ("double_sided", m.double_sided.to_string()),
        ("normal_scale", opt(m.normal_scale)),
        ("ao_map_intensity", opt(m.ao_map_intensity)),
        ("maps", array(&maps)),
        ("defaults", strings(defaults)),
        ("dropped", strings(dropped)),
    ])
}
