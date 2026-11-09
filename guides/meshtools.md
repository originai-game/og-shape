# Mesh tools: from an outside mesh to a usable asset

The kernel's mesh tools take a mesh it did not build (a scan, another
tool's GLB, an OBJ, an FBX) and make it fit for texturing, booleans and
the engine. They report measurements and never judge; this guide says
what to look for.

## The chain

```text
import → join → repair → decimate → unwrap → measure → preview → export
```

1. **Import** the file (`import`). Read what was skipped
   (`Imported::skipped`): points, lines, NURBS, morph targets and blend
   shapes are not read. A Draco- or meshopt-compressed glTF is refused:
   get an uncompressed one. A **skinned** mesh (bones and weights) is
   read whole, in its bind pose (`measure` says `skinned` and lists the
   bones). Repair, join, decimate and `glb_plain` refuse it rather than
   break the skin; `glb` writes it back, and unwrap, measure and preview
   take it. A skeleton under a uniformly scaled armature (centimetres)
   has the scale taken out; the import report (`skin_scales`) says so.
2. **Join** the parts into one solid (`join`). Each part's faces keep its
   name, so a part can still be found after joining.
3. **Repair** (`repair`). Settings that worked on the acceptance meshes
   (`docs/research.md` §9.5):
   - `weld: 0.001` (1 µm): closes the cracks a seam-split or soup export
     leaves. Keep `weld_all` off unless the mesh is a soup of separate
     sheets meant to be one.
   - `floaters.fraction: 0.05`: drops specks under 5 % of the largest
     piece. Look at `measure`'s `largest_pieces` first: eyes, teeth and
     buttons are pieces too.
   - `max_hole_edges: 64`: closes holes up to 64 edges round. Fills are
     named `fill`; preview them.
   - `fuse: true` joins interpenetrating closed parts; the report says
     when it could not.
   - `min_edge: 0.001` collapses the slivers a union leaves.
4. **Decimate** to the budget (`decimate`, `target` or `ratio`), or make
   LODs (`lods`). The report's `error` is how far the surface moved, in
   mm.
5. **Unwrap** (`unwrap`). Raise `resolution` to the texture size you will
   bake at, so the padding is in its texels.
6. **Measure** (`measure`) with the UVs, the images and `intersections`.
7. **Preview** and look at it.
8. **Export** `glb_plain` for texturing, or `glb` for parts and joints.

## Reading the measures

| measure | what it says | what to want |
|---|---|---|
| `closed` | every edge has two faces, wound opposite ways | `true` for anything a boolean, a 3D print or a texturing service takes |
| `boundary_loops` | holes left | 0, unless the object is a sheet |
| `non_manifold_edges`, `inconsistent_edges` | edges with three faces or more; faces that disagree | 0 |
| `self_intersections` | pairs of faces passing through each other | 0 for booleans; a few are harmless for rendering |
| `floaters`, `largest_pieces` | pieces besides the largest | only the ones the object should have |
| `degenerate_triangles`, `duplicate_triangles` | faces with no area; repeated faces | 0 |
| `shortest_edge` | in mm; below about 1e-5 mm, f32 export merges vertices | well above 1e-5 |
| `triangles` | the budget | at most the budget (a texturing tool's input limit is often about 40 000) |
| `skinned`, `skin.bones` | the mesh has a skin | know it before running a tool that refuses it |
| `pivot_from_base` | where the origin sits relative to the base's centre | 0 for props that stand on the ground |
| `uv.overlapping_pairs`, `uv.flipped`, `uv.outside` | UV faces over each other, mirrored, or outside the square | 0 for baking and painting |
| `uv.coverage` | how much of the square the charts fill | the higher the better; 0.1–0.8 on the acceptance meshes, 0.5 on average, lowest on thin parts |
| `texture.decoded_bytes_with_mips` | GPU memory the textures take | within the game's budget |

## A budget, for example a texturing tool's input

Texturing tools typically take a closed, joined, triangulated plain GLB
under a triangle budget, unwrapped: `closed`, `triangles` within the
budget, one part, and `glb_plain`'s output (one node, one mesh, one
triangle primitive with UVs). Every kernel solid is triangles, so
"triangulated" always holds.

## What repair cannot do

A single surface that folds through itself (one piece, closed, but with
`self_intersections` above 0 after `fuse`) cannot be untangled here.
Booleans on it are not reliable. Get another mesh, or remesh it outside
the kernel.
