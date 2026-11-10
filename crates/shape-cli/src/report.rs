//! Reports as JSON: keys in a fixed order, numbers as serde_json prints
//! them (shortest round-trip, the same on every host), no thresholds.
//!
//! Entry: [`object`], which lays out fields whose values are already JSON
//! text, so a kernel report (`MeshMeasures::to_json`) is embedded as the
//! kernel wrote it, never parsed and printed again. The other functions
//! write one kernel report each, in `shape-tools`' snake_case.

use serde_json::{Value, json};
use shape_tools::{DecimateReport, Imported, RepairReport, Uvs};

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
        ("holes_filled", r.holes_filled.to_string()),
        ("fill_triangles", r.fill_triangles.to_string()),
        ("uv_seam_fills", r.uv_seam_fills.to_string()),
        ("holes_left", r.holes_left.to_string()),
        ("fused", fused.to_string()),
        ("short_edges", r.short_edges.to_string()),
        (
            "self_intersections",
            r.self_intersections
                .map_or("null".into(), |n| n.to_string()),
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
                ("material", string(&p.material)),
                ("triangles", p.solid.triangles.len().to_string()),
                ("vertices", p.solid.positions.len().to_string()),
                ("uvs", p.solid.uvs.is_some().to_string()),
            ];
            if let Some(t) = &p.map {
                fields.push(("map", string(&t.name)));
            }
            fields.push((
                "bones",
                p.solid.skin.as_ref().map_or("null".into(), |k| {
                    array(&k.bones.iter().map(|b| string(&b.name)).collect::<Vec<_>>())
                }),
            ));
            object(&fields)
        })
        .collect();
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
    object(&[
        ("format", string(format)),
        ("parts", array(&parts)),
        ("triangles", triangles.to_string()),
        (
            "skinned",
            m.parts.iter().any(|p| p.solid.skin.is_some()).to_string(),
        ),
        ("images", array(&images)),
        ("polygons_triangulated", m.polygons_triangulated.to_string()),
        (
            "skipped",
            array(&m.skipped.iter().map(|s| string(s)).collect::<Vec<_>>()),
        ),
    ])
}
