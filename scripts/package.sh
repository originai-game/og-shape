#!/usr/bin/env bash
# Packs a release for this host into dist/ (.github/workflows/release.yml):
#
#   scripts/package.sh            shape and shape-mcp, built here
#   scripts/package.sh binding    the TypeScript binding (src/ and the
#                                 built pkg/), for `shape-mcp --binding`
#
# Each archive holds LICENSE, README.md, CHANGELOG.md, guides/ and
# third_party/ (the notices of what the binaries carry).
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
host=$(rustc -vV | sed -n 's/^host: //p')
mkdir -p dist

pack() {
  local name=$1 dir=dist/$1
  cp LICENSE README.md CHANGELOG.md "$dir/"
  cp -R guides third_party "$dir/"
  if [[ $host == *windows* ]]; then
    (cd dist && 7z a -tzip -bso0 "$name.zip" "$name")
    echo "dist/$name.zip"
  else
    tar -C dist -czf "dist/$name.tar.gz" "$name"
    echo "dist/$name.tar.gz"
  fi
  rm -rf "$dir"
}

case "${1:-}" in
  "")
    cargo build --release --locked -p shape-cli -p shape-mcp
    name=og-shape-$version-$host
    rm -rf "dist/$name" && mkdir -p "dist/$name"
    exe=""
    [[ $host == *windows* ]] && exe=.exe
    cp "target/release/shape$exe" "target/release/shape-mcp$exe" "dist/$name/"
    pack "$name"
    ;;
  binding)
    crates/shape-wasm/build.sh
    name=og-shape-$version-binding
    rm -rf "dist/$name" && mkdir -p "dist/$name"
    cp -R crates/shape-wasm/ts/src crates/shape-wasm/ts/pkg "dist/$name/"
    pack "$name"
    ;;
  *)
    echo "usage: scripts/package.sh [binding]" >&2
    exit 2
    ;;
esac
