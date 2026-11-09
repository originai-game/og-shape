#!/usr/bin/env bash
# Downloads the Thingi10K meshes listed in thingi10k.txt from Hugging Face
# into DIR/<id>/{vertices,facets}.npy, for `--thingi DIR`.
set -euo pipefail
dir=${1:?usage: fetch-thingi10k.sh DIR}
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$dir"
grep -v '^#' "$here/thingi10k.txt" | while read -r id _; do
  [ -f "$dir/$id/vertices.npy" ] && continue
  mkdir -p "$dir/$id"
  curl -sfL -o "$dir/$id.npz" "https://huggingface.co/datasets/Thingi10K/Thingi10K/resolve/main/npz/$id.npz"
  unzip -oq "$dir/$id.npz" -d "$dir/$id" && rm "$dir/$id.npz"
done
