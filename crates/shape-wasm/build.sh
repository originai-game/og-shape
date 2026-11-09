#!/usr/bin/env bash
# Builds the wasm module and its wasm-bindgen glue into ts/pkg for the
# TypeScript binding. Needs wasm-bindgen-cli at the crate's exact version:
#   cargo install --locked wasm-bindgen-cli --version 0.2.130
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo build --release --locked --target wasm32-unknown-unknown -p shape-wasm
wasm-bindgen --target web --out-dir crates/shape-wasm/ts/pkg \
  target/wasm32-unknown-unknown/release/shape_wasm.wasm
