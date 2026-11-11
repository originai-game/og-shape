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
| `preview` | a program or a mesh file; `views`, `width`, `height`, `columns`, `elevation` (over 0, up to π/2), `azimuth` (radians), `no_edges`, `no_ground`, `fov` (perspective, over 0, up to π/3), `unlit`, `each` (one PNG a view, with its camera), `uv` and `uv_size` (the UV layout of each material instead), `pose`?, `out`? | PNGs (image content); a report: `images` (each with what it `shows`: cameras, or `uv_min` and `tiles`) and `undrawn` maps |
| `measure` | a program or a mesh file; `intersections`, `posed`, `pose`?, `axes` | measures, no thresholds; `axes`: a mesh's extents, area facing each way, where its mass sits |
| `export` | a program or a mesh file; `out` (.glb, .obj, .stl), `pose`? | the file |
| `import` | a mesh file; `out`? | parts, materials, UVs, bones, images, what was skipped |
| `repair`, `decimate`, `unwrap` | a mesh file and the tool's settings; `out`? | what changed, measures after |
| `place` | a mesh file; `up`, `front`, `turn`, one of `longest`, `height`, `width`, `depth` (`"0.9m"`) or `scale`, `pivot` (`"base"`), `out`? | what was done, and the axis measures before and after |
| `process` | a mesh file; `budget`, `repair`, `unwrap`, `merge_charts`, `cross_color_borders`, `fresh_uvs`, `texture_max`, `bake`, `bake_distance`, `place`?, `out`? | the whole chain's report, with the material written and each map's size before and after; with `bake` (texels), fresh UVs packed into one atlas and the look baked into new maps, and a `bake` key: what was baked, texels with no source, distances, the normal map's angles (`guides/meshtools.md`, "Keep the UVs or bake") |
| `run_typescript` | `source` or `path` (a .ts file already there); `program_out`? | parts, measures, the recorded program, `console.log` output as `logs` |

Every tool that reads a mesh file takes `unit_mm` (OBJ), and `up` and
`front` (OBJ and glTF: the file's own axes that are the object's up and
front, `"+x"` … `"-z"`) or `z_up`.

`pose` (`{"lid_hinge": 1.2}`: radians, or mm for a slide) poses a
program's joints for that call only; the program stays as it is.
`measure` with `pose` or `posed` measures at the pose, with each joint's
seating and the parts that meet.

Resources, to read before writing a program:
`og-shape://guides/shape.md` (how to model an object: read it first),
`og-shape://guides/program.md` (the JSON form),
`og-shape://guides/typescript.md` (the TypeScript API) and
`og-shape://guides/meshtools.md` (the mesh tools and their measures).

Long calls: `process` and `run_typescript` send `notifications/progress`
before each step when the request carries `_meta.progressToken` (SDK
clients do this when given a progress callback), and stop on
`notifications/cancelled` before their next step (`run_typescript` stops
bun at once); a cancelled call is not answered. `ping` is answered while
a call runs. Other tools are one kernel call each and run to the end.

## The loop

1. Read the guide for the form you write.
2. `build` (JSON) or `run_typescript` (TypeScript, with `program_out` to
   keep the program).
3. `preview` the program and look; `measure` it. Fix the program, not
   the mesh.
4. `export` GLB.
