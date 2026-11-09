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

Status: phases 0 to 3 done; 4 and 5 in progress (`docs/plan.md`). Run the
gate with `scripts/check.sh` (needs cargo-deny and wasmtime).

- `docs/design.md`: goals, scope, API, architecture, determinism.
- `docs/plan.md`: phases, acceptance and the merge into the engine.
- `docs/research.md`: what has been verified so far: sand3d's real Blender
  dependencies, the library survey, the platform's current 3D pipeline,
  and what the engine already covers.
- `AGENTS.md`: rules for anyone working in this repository.

Licence: MIT (`LICENSE`). Third-party code keeps its own licence under
`third_party/`.
