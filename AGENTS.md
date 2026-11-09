# og-shape repository guidance

Design: `docs/design.md`. Plan and status: `docs/plan.md`. Verified research:
`docs/research.md`. Each rule is stated once, here or in those files.

## What this is

- A Rust kernel that origin-engine will absorb (`docs/plan.md` §M). Write it
  so that the merge only vendors crates and translates the API into the
  engine's IDL. Nothing in the design should need to change at the merge.
- It depends on no engine crate and no platform code. The engine's
  conventions still apply (origin-engine `AGENTS.md`):
  - zero-value desc structs;
  - errors as values;
  - no hidden state;
  - one documented entry per module;
  - three.js vocabulary where it fits;
  - no taste verdicts: tools report measurements and findings, and the
    agent judges.
- The engine ships no skills. What an author needs beyond a signature goes
  into guides that will become `og api` entries, under `guides/`.

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
  Unicode-3.0 (Unicode data tables). Never use
  GPL, LGPL or MPL code, and never read GPL source to port it. Blender's
  published design notes are fine; its source files are not. Vendored code
  keeps its licence under `third_party/<name>/`.
- **Names survive operations.** Faces and edges are reached by name, never
  by computed coordinates. Every operation states how it carries names
  through (`docs/design.md` §4).

## Building and testing

- Toolchain: `rust-toolchain.toml`, pinned to the same nightly as
  origin-engine. A move changes both repositories together.
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
