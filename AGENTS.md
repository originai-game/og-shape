# og-shape repository guidance

Design: `docs/design.md`. Plan and status: `docs/plan.md`. Verified research:
`docs/research.md`. Each rule is stated once, here or in those files.

## What this is

- A Rust kernel for 3D modeling, its own project (MIT). It is not being
  merged into origin-engine (the owner, 2026-11-11; `docs/plan.md` §M
  records what that merge would have been). It ships as crates, the
  `shape` command line, the MCP server and the wasm binding.
- It depends on no engine crate and no platform code. Its conventions are
  the engine's, kept as its own:
  - zero-value desc structs;
  - errors as values;
  - no hidden state;
  - one documented entry per module;
  - three.js vocabulary where it fits;
  - no taste verdicts: tools report measurements and findings, and the
    agent judges.
- It ships no skills. What an author needs beyond a signature goes into
  guides, under `guides/` (the MCP server serves them as resources).

## Non-negotiable

- **Rust only** in the kernel. It ships as native code and as wasm32. No C
  or C++ through FFI, no Python, no Blender, no GPU.
- **Deterministic on every host.** The same program and the same inputs
  give the same bytes on x86-64, aarch64 and wasm32. In practice:
  - Use f64 arithmetic with `+ - * /` and `sqrt` only. Take all other maths
    from `trig` (pure Rust, based on `libm`). Never use `std`'s `sin`,
    `cos`, `atan2`, `powf` or `exp`. Never call `mul_add`.
  - Use no SIMD paths: glam's scalar types, or `DVec3`.
  - Output never depends on the iteration order of a `HashMap` or
    `HashSet`. Use `BTreeMap`, `IndexMap`, or sorted vectors.
  - No rayon or threads in the kernel.
  - Never hash or compare NaN bits.
  - Decide every geometric side with exact predicates (Shewchuk's, from the
    `robust` crate) or with integer arithmetic.
  - The determinism gate (`docs/plan.md` §Gate) proves all of this on all
    three targets. A claim that a crate is deterministic is checked by the
    gate, never assumed.
- **Permissive licences only:** MIT, Apache-2.0, BSD, zlib, ISC, and
  Unicode-3.0 (Unicode data tables). This project is MIT, and code under
  GPL, LGPL or MPL cannot be made part of an MIT work: never use it,
  whether copied, ported, translated or rewritten to its structure.
  Vendored code keeps its licence under `third_party/<name>/`.
  - GPL source (Blender's) may be read to understand what a tool does and
    to compare results (the owner, 2026-11-11). What is then written here
    is written from the published idea (a paper, design notes, the
    behaviour seen), in this project's own structure and names, and its
    commit says which source was read and what was taken from it: an
    idea, never code.
- **Names survive operations.** Faces and edges are reached by name, never
  by computed coordinates. Every operation states how it carries names
  through (`docs/design.md` §4).

## Building and testing

- Toolchain: `rust-toolchain.toml`, a pinned nightly.
- No GPU is needed for anything here. The preview renderer is a CPU
  rasterizer.
- Tests run in seconds. Golden fixtures are output hashes, not images.
  Images are written for a person to look at and are never compared
  pixel by pixel.
- Keep a test only when it fails on a plausible regression.

## Landing

- Every change lands on `main` through a reviewed commit with the gate
  green. Never push `main` with the gate red.
- Every change an author would see goes into `CHANGELOG.md` under
  Unreleased.
