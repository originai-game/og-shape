# Changelog

## Unreleased

- Repository created: design, plan and verified research.
- Cargo workspace and `shape-core`: `DVec3` and `Frame`, `trig` (libm),
  exact predicates (`orient2d`, `orient3d`, `incircle`, `insphere`),
  canonical ordering, and the SHA-256 output hash, which refuses NaN.
- The determinism lint: `clippy.toml` denies `std` transcendental maths,
  `mul_add`, `powi`, threads, `HashMap`/`HashSet` and glam's SIMD types.
- The gate: `scripts/check.sh` (fmt, clippy, cargo-deny licences and bans,
  tests, golden hashes natively and as wasm32 under wasmtime), and GitHub
  Actions on x86-64 Linux, x86-64 Windows and aarch64 Linux. The first
  golden hashes cover `shape-core`.
- `shape-gen` and `shape-bool`: origin-engine's og-geom copied at
  `d4f4d4d` (`third_party/og-geom/SOURCE`): three.js's extrude, lathe,
  sweep, polyhedra, merge, and exact-predicate CSG and fracture. og-geom's
  own contract tests pass unchanged, including its recorded output hashes,
  so the copy gives the engine's bytes. Golden cases added for each.
- Phase 0 decision: booleans stay on og-geom's exact-predicate CSG;
  manifold-rust is dropped (`docs/research.md` §5). The benchmark is in
  `bench/booleans`.
- `shape-mesh`: `Solid` (f64 triangles with surfaces, face names and edge
  names), name lookup that lists existing names on a miss, canonical
  ordering, measures, and the output hash.
- Plan: aarch64 macOS joins the gate later; phase 0 benchmarks booleans on
  synthetic AI-mesh defects instead of provider outputs, which re-check the
  decision in phase 5.
