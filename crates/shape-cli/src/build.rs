//! `shape build <program.json>`: replays the program and reports what it
//! built, as JSON: the parts with their triangle counts and the joints.

use crate::args::{Args, Failure, failed, load};

pub fn run(a: Args) -> Result<String, Failure> {
    a.only(&[])?;
    let model = load(&a.program()?)?;
    let built = model.build().map_err(failed)?;
    let scene = built.scene();
    let parts: Vec<String> = scene
        .parts
        .iter()
        .map(|p| {
            let joint = p
                .joint
                .as_ref()
                .map_or("null".to_string(), |j| json_str(&j.name));
            format!(
                "{{\"name\":{},\"parent\":{},\"joint\":{},\"triangles\":{}}}",
                json_str(&p.name),
                json_str(&p.parent),
                joint,
                p.solid.triangles.len()
            )
        })
        .collect();
    Ok(format!(
        "{{\"name\":{},\"parts\":[{}]}}",
        json_str(&scene.name),
        parts.join(",")
    ))
}

/// A JSON string literal.
fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
