#!/usr/bin/env bash
# The gate (docs/plan.md §Gate).
#
#   scripts/check.sh            lint, test, golden, wasm
#   scripts/check.sh <step>...  only the steps named
#   scripts/check.sh bless      rewrite golden/hashes.txt from this host
#
# Steps: lint, test, golden, wasm. CI runs lint once and test, golden and
# wasm on every target.
set -euo pipefail
cd "$(dirname "$0")/.."

GOLDEN=golden/hashes.txt
OUT=target/golden

lint() {
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets --locked -- -D warnings
  if ! command -v cargo-deny >/dev/null; then
    echo "cargo-deny is missing: cargo install --locked cargo-deny" >&2
    exit 1
  fi
  cargo deny --locked check licenses bans sources
}

test() {
  cargo test --workspace --locked
}

compare() {
  local label=$1 file=$2
  if diff -u "$GOLDEN" "$file"; then
    echo "golden: $label matches $GOLDEN"
  else
    echo "golden: $label differs from $GOLDEN (lines above)" >&2
    exit 1
  fi
}

golden() {
  mkdir -p "$OUT"
  cargo run --release --locked -q -p shape-golden > "$OUT/native.txt"
  compare native "$OUT/native.txt"
}

wasm() {
  # The library must also build for the browser target.
  cargo build --release --locked --target wasm32-unknown-unknown -p shape-core
  cargo build --release --locked --target wasm32-wasip1 -p shape-golden
  if ! command -v wasmtime >/dev/null; then
    echo "wasmtime is missing: https://wasmtime.dev" >&2
    exit 1
  fi
  mkdir -p "$OUT"
  wasmtime run target/wasm32-wasip1/release/shape-golden.wasm > "$OUT/wasm32.txt"
  compare wasm32 "$OUT/wasm32.txt"
}

bless() {
  mkdir -p "$(dirname "$GOLDEN")"
  cargo run --release --locked -q -p shape-golden > "$GOLDEN"
  echo "golden: wrote $GOLDEN; say in the commit and CHANGELOG.md why it moved"
}

steps=("$@")
if [ ${#steps[@]} -eq 0 ]; then
  steps=(lint test golden wasm)
fi
for step in "${steps[@]}"; do
  case "$step" in
    lint | test | golden | wasm | bless) echo "== $step"; "$step" ;;
    *) echo "unknown step: $step" >&2; exit 2 ;;
  esac
done
