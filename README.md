# og-shape

A deterministic mesh-modeling kernel for Origin Engine, written in Rust.

With og-shape, an agent writes a 3D object as a short program: profiles, solids,
booleans, fillets, shells, text, named parts and joints. The program is the
asset. The kernel builds it into a mesh that is identical bit for bit on
x86-64, aarch64 and wasm32. The same tool also repairs, decimates, unwraps,
measures, previews and exports meshes, so the platform's asset pipeline no
longer needs Blender.

og-shape is developed here, outside the engine, so that its work never
touches the engine's gate, landing queue or lanes. When its plan's merge
phase is reached (`docs/plan.md` §M), it moves into origin-engine. There its
Rust crates are vendored, its API becomes the IDL module `og.shape`, and its
tools become `og model` commands. The platform uses it only through a
released engine.

Status: phase 0. `shape-core` and the gate exist. Run the gate with
`scripts/check.sh` (needs cargo-deny and wasmtime).

- `docs/design.md`: goals, scope, API, architecture, determinism.
- `docs/plan.md`: phases, acceptance and the merge into the engine.
- `docs/research.md`: what has been verified so far: sand3d's real Blender
  dependencies, the library survey, the platform's current 3D pipeline,
  and what the engine already covers.
- `AGENTS.md`: rules for anyone working in this repository.

Licence: MIT (`LICENSE`). Third-party code keeps its own licence under
`third_party/`.
