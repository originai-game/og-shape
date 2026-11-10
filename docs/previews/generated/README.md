# Generated meshes before and after the chain

`docs/research.md` §28. Each image: top row `shape preview`, bottom row
three.js (`crates/shape-wasm/ts/tools/render_glb.ts`); left the file as
generated, middle `shape process --budget 10000`, right `--budget 2000`,
the file's UVs and texture kept. For a person to look at, never compared
by pixel.

`place-*` (`docs/research.md` §31): top row the file read plainly (Y
up), bottom row after `shape place` with the generator's `up` and
`front`, `--pivot base` and a real size, beside a post 0.1, 1 or 10 m
tall; three views each, from the front round to the back
(`bench/generated/src/bin/place_previews.rs`).

Renders of meshes others generated and published:
- `hunyuan-*`, `trellis-*`, `spar3d-*`, `triposr-*`, `instantmesh-*`,
  `crm-*`, `unique3d-*`: Hi3DBench (3DTopia, MIT),
  huggingface.co/datasets/3DTopia/Hi3DBench; outputs of Hunyuan3D-2,
  TRELLIS, SPAR3D, TripoSR, InstantMesh, CRM and Unique3D.
- `meshy-*`: MESHY.AI 800 GLB 3D-Assets, curated by tfnn (MIT),
  huggingface.co/datasets/tfnn/MESHY.AI_800_GLB_3D-Assets_Categorised_and_Labelled;
  generated with Meshy by its users.

The URL and SHA-256 of each file are in `bench/generated/manifest.tsv`.
