//! The resources: the authoring guides an agent reads before it writes a
//! program, served as Markdown. They are the files under `guides/`,
//! compiled in, so the server needs no checkout.
//!
//! Entry: [`list`] (for `resources/list`) and [`read`] (for
//! `resources/read`).

use serde_json::{Value, json};

struct Guide {
    uri: &'static str,
    name: &'static str,
    title: &'static str,
    description: &'static str,
    text: &'static str,
}

const GUIDES: [Guide; 3] = [
    Guide {
        uri: "og-shape://guides/program.md",
        name: "program.md",
        title: "The program JSON: format and operations",
        description: "How a model program is written as JSON ({name, ops}): ids, frames, \
            profiles, and every operation with its fields. What `build`, `preview`, `measure` \
            and `export` take.",
        text: include_str!("../../../guides/program.md"),
    },
    Guide {
        uri: "og-shape://guides/typescript.md",
        name: "typescript.md",
        title: "The TypeScript API (og-shape binding)",
        description: "The authoring language: og.model, the Shape's methods, frames and \
            profiles, as `run_typescript` runs them.",
        text: include_str!("../../../guides/typescript.md"),
    },
    Guide {
        uri: "og-shape://guides/meshtools.md",
        name: "meshtools.md",
        title: "Mesh tools: from an outside mesh to a usable asset",
        description: "The chain import → repair → decimate → unwrap → measure → preview → \
            export, settings that worked, and how to read the measures.",
        text: include_str!("../../../guides/meshtools.md"),
    },
];

/// The `resources/list` result.
pub fn list() -> Value {
    let resources: Vec<Value> = GUIDES
        .iter()
        .map(|g| {
            json!({
                "uri": g.uri,
                "name": g.name,
                "title": g.title,
                "description": g.description,
                "mimeType": "text/markdown",
                "size": g.text.len(),
            })
        })
        .collect();
    json!({ "resources": resources })
}

/// The `resources/read` result for `uri`, or None if there is no such
/// resource.
pub fn read(uri: &str) -> Option<Value> {
    let g = GUIDES.iter().find(|g| g.uri == uri)?;
    Some(json!({
        "contents": [{ "uri": g.uri, "mimeType": "text/markdown", "text": g.text }],
    }))
}
