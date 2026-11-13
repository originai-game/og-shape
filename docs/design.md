# og-shape design

## 1. Goal

3D modeling in Rust, built for LLMs.

An agent asks for an object ("a washing machine whose door opens") and
writes it as a short program that reads like the object's construction:
named parts, named faces and edges, joints that say how the parts move, and
for a figure, the bones that bend it. The kernel builds that program into a
game-ready mesh, the same bytes on every host. The program is the asset.
The agent changes the model by editing the program.

The kernel's job is its own capability: what a program can say, and what
the kernel does with any mesh it is given. Where an agent's meshes come
from otherwise is the agent's concern, not the kernel's. Every mesh, built
or imported, gets the same tools:
- repair, decimate and UV-unwrap;
- measure;
- render a turntable preview;
- export (GLB, OBJ, STL).

A skinned mesh (bones and weights) keeps its skin through every tool, or
the tool refuses it. It is never broken silently.

What it is not:
- A CAD kernel. There are no NURBS and no B-rep, only meshes.
- A sculpting tool or a retopology tool.
- A rigger for arbitrary meshes. Bones come from the program that builds
  the mesh (§4, skinning), and imported skins are carried through.

## 2. Scope of the first release

| Family | Operations | Comes from |
|---|---|---|
| Profiles (2D) | rect (with corner radius), circle, ellipse, slot, polygon, poly (points with sharp/smooth flags), arc, tangent arc, spline, superellipse, text; union, subtract, intersect, offset, stroke, mirror, hull | sand3d's vocabulary; engine Shape/Path |
| Solids and paths | extrude (taper, twist, both sides, `until` a face), revolve, sweep (rotation-minimizing frames), loft, body, wire, helix; box, cylinder, tube, cone, sphere, torus, dome, wedge | sand3d; og-geom extrude/lathe/sweep |
| Edits | cut, join, intersect, split, hole (plain, counterbore, countersink), pocket, seat, fillet, chamfer, soften, shell, dent, merge, copy, scale, delete | sand3d |
| Placement | Frame, face(name), edge(name), bbox, local, at, transform, grid, ring, along, snap | sand3d |
| Parts | gear (involute), thread, knurl | sand3d |
| Articulation | joint (revolute, continuous, prismatic, fixed, floating; axis, range, mimic), pose | sand3d |
| Assembly | group, assembly, attach, units | sand3d |
| Materials | material (engine `type model` material fields: glTF metallic-roughness by three.js names) | engine |
| Mesh tools | import (glb/gltf/obj/fbx), weld, remove floaters and degenerate faces, apply transforms, triangulate, join into one node, decimate (ratio or triangle target, with LODs), UV unwrap, measure, preview, export (GLB, OBJ, STL) | platform `asset_process` (`docs/research.md` §3) |

Not in the first release: USD and URDF export, runtime (in-game)
generation, remeshing to quads. Each can be added later without changing
the API. Atlas packing on a texel grid and baking a source mesh's maps,
vertex colours and shading normals onto a decimated mesh's new atlas are
in (`docs/research.md` §34): mesh tools, like the rest of the table's
last row.

## 3. How an author uses it

- **The source.** An author writes a program: JSON (`{name, ops}`,
  `guides/program.md`), or TypeScript through the binding over the wasm
  build (`guides/typescript.md`), which records the same program. Both
  go through the same kernel.

  ```ts
  const plate = shape.extrude({ name: "plate", profile: shape.rect({ width: 80, height: 40, radius: 4 }), depth: 6 });
  shape.hole(plate, { diameter: 6.6, on: shape.grid({ x: 2, y: 1, pitchX: 50 }) });
  shape.fillet(plate, { radius: 1, edges: "top" });
  ```

- **Where the result lives.** The program is the asset, kept wherever
  its project keeps sources. `shape export` (the MCP `export` tool, the
  binding's `glb`) writes a GLB with parts, joints as nodes, materials
  and clips; `shape process` and `shape decimate` make the levels of
  detail and the texture budget a game needs.
- **When it runs.** At asset build time only, for now. The kernel also
  runs in wasm, so in-game generation can be added later as more API
  members, without a second implementation.

## 4. Data model

- **Solid:** a closed, oriented triangle mesh in f64. It carries:
  - per-face attributes: material, surface id, operand id;
  - names: sets of face tags and edge tags;
  - optionally, UVs: one texture coordinate per triangle corner, so a
    seam is two corners of a vertex with different UVs (v up; glTF's v
    is flipped on the way in and out).
  - optionally, vertex colours: linear RGBA per triangle corner, kept
    exactly as UVs are (`docs/research.md` §29).
  
  Positions and UVs are f64 inside and rounded to f32 only on output. A
  solid without UVs orders and hashes exactly as before UVs existed.
- **UVs through operations** (`docs/research.md` §17):
  - Import keeps the first UV set and the base-colour texture (as the
    file's bytes); GLB export writes both.
  - Repair, apply transform and join keep each triangle's UVs; a filled
    hole takes its rim's, and a fill across a seam is reported.
  - Decimation keeps seams as feature lines and moves each corner's UV
    with its vertex, on its own side of any seam.
  - Booleans interpolate UVs inside each source face, so cut faces keep
    their mapping; faces from an operand without UVs are counted.
  - Generators give UVs in the model's unit on request (caps planar,
    walls by arc length round and along); a program asks with `uvs`.
    Bevels re-fit trimmed faces and give new faces the map of the old
    face each faces most (`docs/research.md` §21).
  - Edits build their tools with UVs; a tool without them on a solid
    with them is refused, and a cut's tool UVs are mirrored so the
    cavity reads right.
  - A tool that cannot keep UVs refuses or reports; it never drops them
    silently.
- **Materials of imported meshes** (`docs/research.md` §29): an imported
  part keeps its material's factors as the file means them (glTF's
  defaults where it leaves one unset) and its five glTF maps, by
  three.js's names, through every tool that keeps its UVs; replacing the
  UVs drops the maps and says so.
- **Names.** Every face and edge carries a set of tags. sand3d's rules are
  the starting point (`docs/research.md` §1.3):
  - Generators name caps and walls (`top`, `bottom`, `start`, `end`,
    `side`, `rim`, `seam`).
  - A boolean names the edges along the cut `cut`, plus each operand's
    name, and keeps each operand's face names, qualified as
    `<part>.<tag>`.
  - A fillet or chamfer names its new faces `fillet` or `chamfer`, and
    re-tags the boundary edges with the cap names they touch.
  - A named edge left between two coplanar faces loses its name.
  - Lookup returns every element carrying any of the names given. A miss
    is an error value that lists the names that exist.
- **Part:** a named solid with a material and a placement.
- **Skin (`docs/plan.md` phase 5b):** a skeleton of named bones
  and, per vertex, up to four bone weights.
  - Bones come from the program: a part or a named region becomes a bone,
    a joint becomes its pivot. Weights blend smoothly across the seams the
    program names.
  - Imported skins are kept: bones, inverse bind matrices, and weights;
    a uniform armature scale is taken out of the bones. Weights are
    interpolated through repair and decimation, which keeps bone-weight
    borders. Rigid placement is baked into the bones, and join merges
    skeletons by bone name.
  - GLB export writes a glTF skin.
- **Morph targets:** optional, on imported meshes: named per-vertex
  offsets, each with a rest weight (`Solid::morphs`). They follow the
  vertices as a skin's weights do (a weld takes the mean, a collapse
  the blend, a placement turns and scales them), a clip may drive their
  weights, and GLB export writes them. The kernel's own operations
  refuse a solid that has them.
- **Group / assembly:** a tree of parts. A **joint** connects a child to a
  parent at a frame, with a kind, an axis, a range and an optional mimic.
- **Units:** millimetres in programs by default; metres on export. All
  tolerances are fixed in millimetres, whatever the unit.

## 5. Architecture

A Cargo workspace. Each crate has one job and a small public surface.

| Crate | Job | Built on |
|---|---|---|
| `shape-core` | f64 vectors and frames, `trig` (libm), exact predicates, stable sort, the determinism rules in one place | glam (scalar), libm, robust |
| `shape-2d` | profiles, 2D booleans, offset, stroke, triangulation with holes, curve flattening, glyph outlines | i_overlay (integer core), spade (CDT on exact points), own Bézier flattening, ttf-parser (skrifa pulls in Unicode-3.0 crates, which are not on the licence list) |
| `shape-mesh` | half-edge mesh, names, welding, measures, canonical ordering | — |
| `shape-gen` | the generators: extrude, revolve, sweep, loft, body, wire, helix, primitives, gear, thread, knurl | og-geom's generators, copied at a pinned engine commit, plus new ones |
| `shape-bool` | mesh booleans and split | og-geom's exact-predicate CSG, or manifold-rust; chosen in phase 0 |
| `shape-bevel` | fillet, chamfer, soften | written here, clean-room, from Blender's published Bevel V2 design |
| `shape-edit` | shell, dent, hole, pocket, seat, merge | the crates above |
| `shape-tools` | import, repair, decimate and LODs, UV unwrap, measure, preview, export | own glTF, OBJ and FBX readers, own QEM decimator, own unwrap, own CPU rasterizer, own UASTC and ETC1S codecs, miniz_oxide, ruzstd, image-webp (`docs/research.md` §9, §50, §57, §58) |
| `shape-model` | parts, groups, joints, materials, the program API in Rust | all |
| `shape-wasm` | wasm32 build and the TypeScript binding | wasm-bindgen |
| `shape-cli` | `shape build / export / measure / preview / import / repair / decimate / unwrap / process`, the tools as commands; its commands are a library the MCP server calls | all |
| `shape-mcp` | an MCP server on stdio: the CLI's commands as tools, a TypeScript program run through the binding and replayed natively, the guides as resources, paths confined to a workspace | `shape-cli`, serde_json |

Why og-geom's code is copied and not depended on:
- og-geom already has deterministic extrude, lathe, sweep, frames, `trig`,
  and an exact-predicate CSG.
- Copying it at a pinned engine commit keeps this repository free of
  engine dependencies.
- The commit it was copied from is recorded in `third_party/og-geom/SOURCE`.

## 6. Determinism

The rules are in `AGENTS.md`. The design consequences:
- Every operation ends with canonical ordering: vertices by position, then
  edges and faces by vertex index. This makes the output independent of
  internal visiting order.
- Segment counts depend only on the arguments: a chord tolerance in
  millimetres, clamped to a minimum and a maximum per turn.
- Coordinates that come within a weld distance of a target's coordinates
  are snapped onto them before a boolean.
- An operation that cannot honour its input returns an error value. It
  never returns a best effort silently. Examples: a boolean on a
  non-manifold input, or a fillet radius larger than the faces allow.
- The gate hashes every operation's output on every target
  (`docs/plan.md` §Gate).

## 7. Tool outputs

- **measure:** triangle and vertex counts; bounds; pivot; surface area;
  volume; closed or open; non-manifold edges; degenerate faces; floaters;
  UV coverage and overlap; texture memory once decoded. Reported as JSON
  measurements with no thresholds. The guides carry the thresholds.
- **preview:** a turntable of N views to PNG from a deterministic CPU
  rasterizer: flat and Lambert shading, edges, a ground shadow. It runs
  with no GPU, for an agent to look at.
- **export:** GLB with materials. Joints go into node extras in the shape
  sand3d writes (`docs/research.md` §1.5), so outside tools can read them.
  OBJ and STL hold meshes only.

## 8. The merge into origin-engine (dropped)

Not going ahead (the owner, 2026-11-11): og-shape is its own project.
This section records what the merge would have been.

Done once, at plan phase M:
1. Vendor the crates under `crates/og-shape*` in origin-engine and replace
   og-geom with them.
2. Write `crates/og-api/api/shape.ogt`. The TypeScript `og.shape` is
   generated for both hosts.
3. Extend `type model` with the new operations.
4. Make `og cook` build programs.
5. Add `og model export / measure / preview / repair / decimate / unwrap`.
6. Move `guides/` here to `crates/og-api/guides/`.
7. Ship it in a minor release with changelog fragments.

The platform then switches `asset_process` to the `og model` commands and
drops Blender from the asset image (platform ask, drafted at the merge).
