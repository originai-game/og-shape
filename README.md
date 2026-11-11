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

| | |
|---|---|
| ![generated police car](docs/previews/generated/meshy-car_police-colour.png) | ![generated cabinet](docs/previews/generated/spar3d-120-colour.png) |
| A Meshy police car (37 000 triangles, a 2048² JPEG map) in `shape preview`: top before it drew maps, bottom with its base-colour map sampled per pixel (`docs/research.md` §30). | A SPAR3D cabinet, the same way: the wood inside the grey shell shows only with the map. Its normal map is not drawn. |
| ![generated horse](docs/previews/generated/triposr-420-colour.png) | ![generated bear](docs/previews/generated/hunyuan-300-colour.png) |
| A TripoSR horse, coloured only by its vertex colours: top before the preview drew them, bottom after (§30). | A Hunyuan3D-2 bear (150 000 triangles), the same way, with its base-colour map. |
| ![placed horse](docs/previews/generated/merged-triposr-420.png) | ![placed police car](docs/previews/generated/merged-meshy-car_police.png) |
| The same horse through the whole chain: `shape process --budget 10000` with its colours, stood up from TripoSR's Z-up frame on its base at 2 m (`--up +z --front +x --pivot base --height 2m`), then previewed (§33). | The police car the same way: 10 000 triangles, its maps shrunk to 1024² (`--texture-max 1024`), 4.8 m long. |
| ![baked house](docs/previews/generated/bake-hunyuan-5.png) | ![baked robot](docs/previews/generated/bake-meshy-robot_mech1.png) |
| A Hunyuan3D-2 house (150 000 triangles): top the source, middle keeping its UVs (stuck at 7 266 triangles for a budget of 2 000), bottom `--bake 1024` at 2 000 triangles with its look baked into a new atlas (§34). | Meshy's robot the same way: keeping UVs it stops at 3 384 triangles with its legs damaged; baked, 2 000 triangles in its shape and colours. |

`docs/previews/` holds more: the bevel cases (`phase3/`), a skinned
figure and Mixamo's Soldier posed (`phase5b/`), the models loaded and
moved in three.js (`validate/`), UVs under a checker (`uv/`), twelve
generated meshes before and after the chain, thirteen before and
after the preview drew their colour, thirteen through the whole
chain with their look and placement, and sixteen kept and baked at
2 000 triangles (`generated/`).

## What has been checked

Run on `main` on 2026-11-10; each line says where the numbers come from.

| Check | Result | How |
|---|---|---|
| The same bytes on every host | 159 golden output hashes equal on x86-64 Linux, x86-64 Windows, aarch64 Linux, aarch64 macOS and wasm32 (wasmtime); the binding's GLB and PNG equal under bun and node | the gate in GitHub Actions, every push |
| Tests | 334 pass | `scripts/check.sh test` |
| An agent with only the guides and the MCP tools builds objects | 20 of 20 sand3d dataset objects built, every part closed, 24 of 24 joints as the reference, 2.0 builds an object, 19 ran on the first build | `eval/phase6`, run `Z1` (`docs/research.md` §22) |
| Those 20 programs on today's kernel | 20 of 20 rebuild and export to GLB; 119 of 119 parts closed, no non-manifold edge, no degenerate triangle | `shape measure` on each `program.json` |
| Every committed program | 105 of 107 build (`eval/phase6/**`, `golden/programs/`); the two that do not are a refusal a golden case hashes on purpose and a trial program with a joint declared twice | `shape build` on each |
| No part passes through itself | 140 of 140 built solids have no crossing triangle pair (exact predicates). The mouse's shell had 8: its move into the world folded a needle, now repaired (`docs/research.md` §26) | `crates/shape-model/tests/built_parts.rs`, `shape_bool::crossing_pairs` on every part |
| Bevels never return a wrong result | 17 948 fillets and chamfers on generator and boolean solids: 16 648 built, 1 300 refused with an error value, none returned crossing itself | `cargo run --release -p shape-bevel --example scan` |
| Booleans against outside meshes | 109 cases from sand3d, Thingi10K, synthetic defects and touching operands; no ok output crosses itself with the repair on | `bench/booleans` (`docs/research.md` §25.2) |
| Real generated meshes | 54 files from 8 image- and text-to-3D generators (Hunyuan3D-2, TRELLIS, SPAR3D, TripoSR, InstantMesh, CRM, Unique3D, Meshy): within budget and closed keeping their UVs and texture 39, 51 and 52 at 2 000, 10 000 and 50 000 triangles, 51, 52 and 52 with fresh UVs (the 2 open ones are TRELLIS meshes; Unique3D's chair keeps its vertex-colour borders and misses 2 000, which `--cross-color-borders` reaches at 7.4 mm); decimation folds none through itself (outputs crossing themselves at 10k: 2, both inputs that crossed); 143 of 154 booleans on the results closed, 7 refused with a cause, 4 missed; at 10 000 triangles all 54 keep their material factors, the 40 textured ones every map and the 14 vertex-coloured ones their colours, and `--texture-max 1024` brings the 16 files over 16 MiB of texture under 11 MiB | `bench/generated` (`docs/research.md` §28, §29, §32, §33) |
| Baking generated meshes to a game's budget | `process --bake 1024`: all 54 within 2 000 and 10 000 triangles (52 closed; the 2 TRELLIS meshes open after repair), every one with a baked base-colour and normal map (15 with metallic-roughness), 1 texel without a source in 108 runs; keeping UVs, 39 and 53. Colour error against the source (unlit, 256², 0–255) where both reach 2 000: 3.6 baked, 5.6 kept (median); 3–4 s and about 50 MiB for a 1024² bake | `bench/generated/bake.ts` (`docs/research.md` §34) |
| Generated meshes placed for a game | all 54 stand on z = 0 with the origin at the base's centre, at the size asked to 4e-8, facing front, with each generator's frame given as `up` and `front` (TripoSR `+z`/`+x`, InstantMesh `-z`/`+x`, SPAR3D and CRM `+y`/`-z`); no simple reading of the axis measures stands more than 22 up | `bench/generated/place.ts` (`docs/research.md` §31) |
| The mesh chain on a damaged mesh | open, 2 pieces, 79 flipped faces → closed, 1 piece, on budget, unwrapped with no UV overlap | `shape process golden/meshes/ball_defects.obj` |
| Outside files | glTF, OBJ and FBX from three.js's and Khronos's samples import; textured and skinned ones keep UVs, bones and weights through repair and decimation to half | `docs/research.md` §9, §11.5, §17 |
| three.js reads the output | the washing machine's joints move and a skinned figure poses in headless Chromium; vertex positions match the kernel's own pose | `docs/research.md` §19 |
| A real MCP client | the official TypeScript SDK client and headless Claude Code drive every tool and resource | `docs/research.md` §19 |

Known limits. Each is an error value or a recorded gap; the one marked
silent returns a result with no error:
- **Silent, measured:** a boolean or edit whose result still crosses
  itself after the repair is returned without an error, with the count in
  its report (`crossingPairs`, `BoolReport::crossing_pairs`,
  `shape_edit::EditReport`). No committed program, golden case other than
  `selfunion.check` (its inputs cross by design), bench case, or swept
  `hole`, `pocket`, `seat` or `dent` leaves any; 76 of 4 920 swept
  shells do (950 before), where a rounded corner, chamfer or fillet
  narrower than the wall folds the inner solid beyond what `self_union`
  untangles (`docs/research.md` §27).
- 1 300 of the scan's bevels are refused: too large for the faces (426),
  a lone piece of a crease that does not fit (226), a twisted face (340),
  an edge with more than two faces (308).
- The unwrap is planar per chart. The preview
  draws base-colour maps (PNG, baseline JPEG), vertex colours and the
  emissive colour, but no normal, metallic-roughness or emissive maps,
  and no metalness or roughness factors.
- Carrying vertex colours through the fuse step costs memory: `process`
  on Unique3D's 620 000- and 820 000-triangle files peaks at 3.0–3.3 GiB
  (1.8 GiB for the bear without its colours, `docs/research.md` §33).
- Keeping a file's UVs, decimation stops at about three triangles a UV
  island: 14 of 54 generated meshes miss 2 000 triangles (the report's
  `blocked` says why); merging rigidly matching islands lowers that
  3–25 %. `--bake` reaches the budget by resampling the look into a new
  atlas: where keeping also reaches it (10 000 triangles) the bake looks
  less like the file (median colour error 2.9 against 1.6 of 255), its
  maps double a single-map file's texture memory, and its normal map is
  in this kernel's tangent frame (written as `TANGENT`), not MikkTSpace
  (`docs/research.md` §34).
- Repair leaves open, and reports, non-manifold edges that no pairing of
  their faces separates and holes over 64 edges that no flat clear fill
  closes: 2 of TRELLIS's 6 meshes (`docs/research.md` §32).
- Not read: Draco and meshopt-compressed glTF, morph targets, a second
  UV set. No USD or URDF export.
- An imported mesh's normals are recomputed, smooth within a surface: a
  normal map baked against the file's normals shades a median 1–1.6°
  differently after repair alone. The texture budget writes PNG only (no
  KTX2 or Basis); neither it nor the preview decodes progressive JPEG; a
  plain GLB has one material, so parts whose materials differ lose
  theirs (reported).
- `place` turns, sizes and moves a mesh to its base, but does not
  choose its up or front: the author says them, per generator
  (`guides/meshtools.md`). A program's export has no pivot option; the
  program places its own parts.
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
`unwrap`, `place` and `process` for meshes from elsewhere. It reads how to write a
program from the server's resources (`guides/`).

From a shell, the same commands (`cargo run -p shape-cli -- help`):

```sh
shape build   model.json                     # parts and joints as JSON
shape preview model.json -o model.png        # a turntable, CPU-rendered
shape export  model.json -o model.glb        # or .obj, .stl
shape process scan.glb --budget 40000 --weld 0.001 --floaters 0.05 \
              --non-manifold --holes 64 --fuse -o asset.glb
                                             # repair, decimate, unwrap
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
