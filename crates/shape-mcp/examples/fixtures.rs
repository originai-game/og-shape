//! Writes mesh files for driving the server with a real MCP client
//! (`crates/shape-mcp/ts/client/client.ts`, research §19.3): the binary
//! FBX the importer's tests read (a unit cube in centimetres with two
//! materials and an embedded PNG), into the directory given.
//!
//! `cargo run -p shape-mcp --example fixtures -- <dir>`

#[path = "../../shape-tools/tests/support/fbx.rs"]
#[allow(dead_code)]
mod fbx;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args().nth(1).ok_or("usage: fixtures <dir>")?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(format!("{dir}/cube.fbx"), fbx::binary_fbx())?;
    Ok(())
}
