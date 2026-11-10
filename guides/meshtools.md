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
   bones), and every tool keeps the skin: weights follow the vertices
   through repair, decimation blends them, and `glb` writes it back.
   `glb_plain` alone refuses it (a plain GLB carries no skin). A
   skeleton under a uniformly scaled armature (centimetres)
   has the scale taken out; the import report (`skin_scales`) says so.
   UVs (the first set) and the base-colour texture (`map`) are kept
   too, and carried by every step below; `skipped` names the other
   material maps (normal, emissive, ...), which are not.
2. **Join** the parts into one solid (`join`). Each part's faces keep its
   name, so a part can still be found after joining. Skinned parts share
   one skeleton, merged by bone name; bones of one name that disagree, a
   scaled or mirrored skinned part, or skinned and unskinned parts
   together are an error that says which.
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
     when it could not (always on a skinned mesh: booleans carry no skin).
   - `min_edge: 0.001` collapses the slivers a union leaves.
4. **Decimate** to the budget (`decimate`, `target` or `ratio`), or make
   LODs (`lods`). The report's `error` is how far the surface moved, in
   mm. On a skinned mesh, the rows of vertices where weights change
   (joints) are kept by preference, so the mesh bends as before; the
   price is a little more surface error elsewhere at the same count
   (`docs/research.md` §11.5). `cross_bone_borders` turns this off for a
   mesh that will not be posed.
5. **Unwrap** (`unwrap`) only when the mesh has no UVs, or you will paint
   a new texture: unwrapping replaces the file's UVs, and its texture no
   longer fits. Raise `resolution` to the texture size you will bake at,
   so the padding is in its texels. `shape process` keeps a textured
   mesh's UVs unless given `--unwrap`.
6. **Measure** (`measure`) with the images and `intersections`; it
   measures the UVs the mesh carries. After repair, look at
   `uv_seam_fills` in its report: those hole fills stretch the texture
   across a seam. After decimation, `uv.overlapping_pairs` and
   `uv.flipped` should be what they were before (a file's own mirrored
   charts count as flipped).
7. **Preview** and look at it.
8. **Export** `glb_plain` for texturing (with `map` to keep the file's
   texture), or `glb` for parts, joints and skins (a part's
   `material.map`). To check a decimated skin, pose both meshes the same way
   (`shape-skin`'s `pose`) and compare them: the posed error should be no
   more than the rest error.

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
| `skinned`, `skin.bones` | the mesh has a skin | know it before `glb_plain`, which refuses it |
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

## Generated meshes (image- and text-to-3D)

Measured on 54 outputs of eight generators (`docs/research.md` §28):
- Their scale and pivot are the generator's: 0.75–2 m whatever the
  object, the origin at the box's centre. Read `pivot_from_base` and
  `bounds`.
- Read OBJ from TripoSR with `--z-up`. InstantMesh's OBJs are in
  neither frame; look at the preview.
- Keeping the file's UVs, decimation stops at about three triangles a
  UV island (thousands on an atlas of 2 000 islands). Below that,
  `--unwrap` reaches the budget and drops the texture.
- A hole filled across UV seams can cover half the texture: compare
  `uv.overlap` before and after repair.
- Only the base colour is kept: metallic-roughness and normal maps,
  vertex colours and the material's metallic and roughness factors
  are not (`skipped` names the maps and the colours).

## What repair cannot do

A single closed surface that folds through itself is untangled by
`fuse`. It keeps the space the surface winds round at least once (a
self-union), so `self_intersections` is 0 after it. An open surface that
folds through itself (one piece, `closed` false, `self_intersections`
above 0) cannot be untangled. Booleans refuse it. Close its holes first
(`max_hole_edges`), or get another mesh.

Untangling is slow on heavy noise: about 2 s for 30k triangles with
thousands of folds.
