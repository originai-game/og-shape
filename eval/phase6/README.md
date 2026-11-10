# Phase 6 evaluation: the authoring guide

Twenty objects from sand3d's datasets, rebuilt by agents as og-shape
TypeScript programs. Results and failure modes: `docs/research.md` §13.

## Layout

- `briefs/<object>/`: what the agents were given besides the guides: a
  short description with the overall size (`brief.md`) and the dataset's
  render of the object (`reference.png`).
- `references.json`: each object's dataset, id, joint count, reference
  size (from its OpenUSD file) and attribution.
- `A/`, `T/`, `B/`, `C/`: one directory per run and object, with the
  agent's final `program.ts`, the recorded `program.json`, `preview.png`
  (four views from −Y, +X, +Y, −X), `measure.json` and `runs.log` (one
  line per build the agent ran).
  - `A`: description and picture, with the first guide.
  - `T`: the sand3d program to translate (plus description and picture),
    with the first guide: the easier baseline.
  - `B`: description and picture, with the second guide (the fixes from
    `A`'s failure modes).
  - `C`: five of the hardest objects again, with the final guide.
  - `M`: five objects whose agents hit the gaps stream M closed
    (research §14.4).
  - `U`: `M`'s five objects through the MCP server (`shape-mcp`), by
    headless Claude Code agents whose only tools were the server's and
    Read, with the guides read as MCP resources (research §19.7). Each
    also has the agent's answer (`report.md`) and its turns and tokens
    (`usage.json`).
  - `Z/Z1`, `Z/Z2`: run `Z` (research §22): all 20 objects through the
    MCP server as `U` was, with each failed build's text (`errors.log`)
    and the surface distance to the reference mesh (`distance.json`);
    `Z2` the four objects with a failed build or more than three builds,
    with the revised guides. `Z/repro/`: the kernel gaps it found.
    `Z/distance.ts` also wrote `distance.json` for runs `B` and `U`.
- `run.ts`: the runner the agents used:
  `bun eval/phase6/run.ts <program.ts> <out dir> [--pose joint=value …]`.
- `mcp_run.ts`: runs `U`'s and `Z`'s driver: `node eval/phase6/mcp_run.ts
  <scratch dir> [model] [object …] [--summary] [--run=<dir>] [--jobs=<n>]`.
- `Z/fetch_references.ts`: downloads the reference OpenUSD files
  `references.json` names and checks their SHA-256 (not committed);
  `bun eval/phase6/Z/distance.ts <refs dir> <run dir>` measures.

The previews and measures were regenerated from each `program.ts` after
the runs; they are the same bytes as the agents saw. `program.json`
replays with `shape build/preview/export`; run `B`'s are replayed by the
binding's tests.

## Data and licences

The objects come from these datasets on Hugging Face, by Sandflow, each
under CC BY 4.0:

- `Sandflow/sand3d-cad` (Zero-To-CAD and CADFS parts),
- `Sandflow/sand3d-articraft` (Articraft Data, Matt Zhou et al.),
- `Sandflow/sand3d-simready` (NVIDIA SimReady assets),
- `Sandflow/sand3d-automate` (AutoMate, Jones et al., 2021),
- `Sandflow/sand3d-objaverse` (Objaverse models by their Sketchfab
  authors).

Each object's own source and attribution, as the dataset states it, is
in `references.json`. The reference images are the datasets' renders,
unchanged. The briefs are written here; the programs are the agents' own
work, written from the briefs (runs `A`, `B`, `C`) or translated from the
dataset's sand3d program (run `T`), so they are adaptations of the
datasets' objects and carry the same attribution. The sand3d programs
themselves are not copied here. sand3d (Apache-2.0) was read at
`84c980a` for its assembly rules (`docs/guidelines.md`), which
`guides/shape.md` follows in its own words.
