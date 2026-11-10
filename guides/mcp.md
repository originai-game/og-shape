# The MCP server: og-shape for any agent

`shape-mcp` serves the kernel to any MCP client (Claude Code, Claude
Desktop, Cursor, others) over stdio. An agent writes a model as a program,
builds it, looks at its preview, measures it and exports it; or takes a
mesh from elsewhere through import, repair, decimate and unwrap. Every
report, image and file comes from the kernel: the same bytes on every host.

## Build it

```sh
cargo build --release -p shape-mcp        # target/release/shape-mcp
crates/shape-wasm/build.sh                # only for run_typescript (needs wasm-bindgen-cli 0.2.130)
```

`run_typescript` also needs [bun](https://bun.sh) on the PATH (or
`--bun <path>`). Without bun or the built binding, that one tool answers
an error value saying what is missing; the others work.

## Run it

```text
shape-mcp --root <dir> [--max-bytes N] [--bun <path>] [--binding <dir>]
          [--ts-timeout SECONDS] [--no-typescript]
```

- `--root`: the workspace. Every path a tool takes is relative to it (or
  absolute inside it); anything outside, `..` above it or a link out of
  it is refused.
- `--max-bytes`: the largest file read, and inline program, in bytes
  (64 MiB by default).
- `--binding`: the TypeScript binding's directory; by default the
  checkout the server was built from (`crates/shape-wasm/ts`).
- `--no-typescript`: refuse `run_typescript`. A TypeScript program is code
  and runs with the server's permissions, not confined to the root.

## Register it

Claude Code (`--scope project` writes the project's `.mcp.json`, to share
it; leave it out to keep it to yourself):

```sh
claude mcp add --scope project og-shape -- \
  /path/to/og-shape/target/release/shape-mcp --root /path/to/workspace
```

Or by hand, in `.mcp.json` at the project root (Claude Code) or
`claude_desktop_config.json` (Claude Desktop); other clients take the same
`command` and `args`:

```json
{
  "mcpServers": {
    "og-shape": {
      "command": "/path/to/og-shape/target/release/shape-mcp",
      "args": ["--root", "/path/to/workspace"]
    }
  }
}
```

## What it offers

Tools (arguments are in each tool's schema):

| tool | takes | gives |
|---|---|---|
| `build` | `program` (JSON) or `path` (.json) | parts, joints, measures |
| `preview` | a program or a mesh file; `views`, `width`, `height`, `columns`, `elevation`, `azimuth` (radians), `no_edges`, `no_ground`, `out`? | a PNG (image content) |
| `measure` | a program or a mesh file; `intersections` | measures, no thresholds |
| `export` | a program or a mesh file; `out` (.glb, .obj, .stl) | the file |
| `import` | a mesh file; `out`? | parts, materials, UVs, bones, images, what was skipped |
| `repair`, `decimate`, `unwrap` | a mesh file and the tool's settings; `out`? | what changed, measures after |
| `process` | a mesh file; `budget`, `repair`, `unwrap`, `out`? | the whole chain's report |
| `run_typescript` | `source` or `path` (.ts); `program_out`? | parts, measures, the recorded program |

Resources, to read before writing a program:
`og-shape://guides/shape.md` (how to model an object),
`og-shape://guides/program.md` (the JSON form),
`og-shape://guides/typescript.md` (the TypeScript API) and
`og-shape://guides/meshtools.md` (the mesh tools and their measures).

## The loop

1. Read the guide for the form you write.
2. `build` (JSON) or `run_typescript` (TypeScript, with `program_out` to
   keep the program).
3. `preview` the program and look; `measure` it. Fix the program, not
   the mesh.
4. `export` GLB.
