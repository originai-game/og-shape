# og-shape

**3D modeling in Rust, built for LLMs.**

A model writes a 3D object as a short program: profiles, solids, booleans,
fillets, shells, text, named parts and joints. Faces and edges are reached by
name, never by computed coordinates. The program is the asset. The kernel
builds it into a mesh that is identical bit for bit on x86-64, aarch64 and
wasm32. The same kernel repairs, decimates, unwraps, measures, previews and
exports any mesh, wherever it came from.

og-shape is developed here, outside the engine, so that its work never
touches the engine's gate, landing queue or lanes. When its plan's merge
phase is reached (`docs/plan.md` §M), it moves into origin-engine. There its
Rust crates are vendored, its API becomes the IDL module `og.shape`, and its
tools become `og model` commands. The platform uses it only through a
released engine.

Status: every phase of `docs/plan.md` but the merge is done; the table in
its §Status is the record. The merge (phase M) waits for the owner. Run
the gate with `scripts/check.sh` (needs cargo-deny and wasmtime).

## What it makes

Each image is the kernel's own CPU preview (`shape preview`), no GPU.

| | | |
|---|---|---|
| ![toaster](docs/previews/readme/sim3_toaster.png) | ![spectacles](docs/previews/readme/obj4_spectacles.png) | ![washing machine](docs/previews/readme/washing_machine.png) |
| A toaster: a lofted housing with a filleted top, slots, a lever and a knob on joints. Written by an agent from a brief (`eval/phase6/Z/Z1/sim3_toaster`). | Spectacles with folding arms and lenses at opacity 0.4 (`…/obj4_spectacles`). | The washing machine of `golden/programs/`, posed: the door on a hinge, the drawer on a slide. |

Meshes from elsewhere go through the same kernel:

| | | |
|---|---|---|
| ![duck](docs/previews/uv/duck-full-half.png) | ![defects before](docs/previews/readme/mesh_before.png) | ![defects after](docs/previews/readme/mesh_after.png) |
| Khronos's Duck, as imported and decimated to half, its texture kept (three.js render, `docs/research.md` §17). | `golden/meshes/ball_defects.obj` as it comes: open, in 2 pieces, 79 faces flipped. | After `shape process --budget 1500`: closed, 1 piece, 1 500 triangles, 6 UV charts with no overlap (the lines are chart seams). |

`docs/previews/` holds more: the bevel cases (`phase3/`), a skinned
figure and Mixamo's Soldier posed (`phase5b/`), the models loaded and
moved in three.js (`validate/`), UVs under a checker (`uv/`).

## What has been checked

Run on `main` on 2026-11-10; each line says where the numbers come from.

| Check | Result | How |
|---|---|---|
| The same bytes on every host | 133 golden output hashes equal on x86-64 Linux, x86-64 Windows, aarch64 Linux, aarch64 macOS and wasm32 (wasmtime); the binding's GLB and PNG equal under bun and node | the gate in GitHub Actions, every push |
| Tests | 280 pass | `scripts/check.sh test` |
| An agent with only the guides and the MCP tools builds objects | 20 of 20 sand3d dataset objects built, every part closed, 24 of 24 joints as the reference, 2.0 builds an object, 19 ran on the first build | `eval/phase6`, run `Z1` (`docs/research.md` §22) |
| Those 20 programs on today's kernel | 20 of 20 rebuild and export to GLB; 119 of 119 parts closed, no non-manifold edge, no degenerate triangle | `shape measure` on each `program.json` |
| No part passes through itself | 139 of 140 built solids have no crossing triangle pair (exact predicates). One has 8: the mouse's shell, a `body` cut twice and filleted | `shape_bool::crossing_pairs` on every part |
| Bevels never return a wrong result | 17 948 fillets and chamfers on generator and boolean solids: 16 648 built, 1 300 refused with an error value, none returned crossing itself | `cargo run --release -p shape-bevel --example scan` |
| Booleans against outside meshes | 109 cases from sand3d, Thingi10K, synthetic defects and touching operands; no ok output crosses itself with the repair on | `bench/booleans` (`docs/research.md` §25.2) |
| The mesh chain on a damaged mesh | open, 2 pieces, 79 flipped faces → closed, 1 piece, on budget, unwrapped with no UV overlap | `shape process golden/meshes/ball_defects.obj` |
| Outside files | glTF, OBJ and FBX from three.js's and Khronos's samples import; textured and skinned ones keep UVs, bones and weights through repair and decimation to half | `docs/research.md` §9, §11.5, §17 |
| three.js reads the output | the washing machine's joints move and a skinned figure poses in headless Chromium; vertex positions match the kernel's own pose | `docs/research.md` §19 |
| A real MCP client | the official TypeScript SDK client and headless Claude Code drive every tool and resource | `docs/research.md` §19 |

Known limits, each an error value or a recorded gap, never a silent wrong
result unless said:
- One built part in the table above crosses itself and was returned
  without an error (the mouse's shell): to be traced.
- Booleans on faces a few ulps apart that are not one plane are refused
  (`CrossingResult`, `OpenTouching`); taking them as one surface is not
  built.
- 1 300 of the scan's bevels are refused: too large for the faces (426),
  a lone piece of a crease that does not fit (226), a twisted face (340),
  an edge with more than two faces (308).
- The unwrap is planar per chart, with no atlas packing; the preview
  draws no textures.
- Not read: Draco and meshopt-compressed glTF, morph targets, vertex
  colours. No USD or URDF export.
- The declarative `.ogt` source and `og cook` belong to the merge.

## Use it

From an agent, through MCP (`guides/mcp.md`):

```sh
cargo build --release -p shape-mcp
claude mcp add og-shape -- "$PWD/target/release/shape-mcp" --root /path/to/workspace
```

The agent then has `build`, `preview`, `measure` and `export` for model
programs, `run_typescript` for TypeScript ones (with bun and
`crates/shape-wasm/build.sh`), and `import`, `repair`, `decimate`,
`unwrap` and `process` for meshes from elsewhere. It reads how to write a
program from the server's resources (`guides/`).

From a shell, the same commands (`cargo run -p shape-cli -- help`):

```sh
shape build   model.json                     # parts and joints as JSON
shape preview model.json -o model.png        # a turntable, CPU-rendered
shape export  model.json -o model.glb        # or .obj, .stl
shape process scan.glb --budget 40000 --weld 0.001 --floaters 0.05 \
              --holes 64 --fuse -o asset.glb # repair, decimate, unwrap
```

## Read

- `docs/design.md`: goals, scope, API, architecture, determinism.
- `docs/plan.md`: phases, acceptance and the merge into the engine.
- `docs/research.md`: what has been verified so far: sand3d's real Blender
  dependencies, the library survey, the platform's current 3D pipeline,
  and what the engine already covers.
- `AGENTS.md`: rules for anyone working in this repository.

Licence: MIT (`LICENSE`). Third-party code keeps its own licence under
`third_party/`.
