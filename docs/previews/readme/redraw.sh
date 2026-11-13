#!/usr/bin/env bash
# Draws every image of the README's gallery again with `shape preview`.
#
#   docs/previews/readme/redraw.sh FILES [OUT]
#
# FILES holds the outside files the gallery shows, which are not kept
# here: RobotExpressive.glb and Soldier.glb (three.js examples/models/gltf),
# Samba.fbx (three.js examples/models/fbx), AnimatedMorphCube.glb (Khronos
# glTF-Sample-Assets). OUT is where the images go (default: this
# directory). Needs python3 with Pillow to lay frames side by side.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
files=${1:?the directory of outside files}
out=${2:-$here}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cd "$root"
cargo build -q --release -p shape-cli
B=$root/target/release/shape
one() { "$B" preview "$1" -o "$2" --views 1 "${@:3}" >/dev/null; }
row() { # row OUT WIDTH HEIGHT FILE...: the files side by side, each scaled to WIDTH × HEIGHT
  python3 -I - "$@" <<'PY'
import sys
from PIL import Image
out, w, h, names = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), sys.argv[4:]
ims = [Image.open(n) for n in names]
ims = [i if i.size == (w, h) else i.resize((w, h), Image.LANCZOS) for i in ims]
sheet = Image.new("RGB", (w * len(ims), h), (255, 255, 255))
for k, i in enumerate(ims):
    sheet.paste(i, (k * w, 0))
sheet.save(out, optimize=True)
PY
}

# A file's node animation and morph targets.
P="--width 720 --height 600 --azimuth 0.5 --no-ground"
one "$files/RobotExpressive.glb" "$tmp/g0.png" $P
one "$files/RobotExpressive.glb" "$tmp/g1.png" $P --clip Dance --time 1.0
one "$files/RobotExpressive.glb" "$tmp/g2.png" $P --clip Wave --time 1.0 --morph Surprised=1
one "$files/RobotExpressive.glb" "$tmp/g3.png" $P --clip Running --time 0.3
row "$out/node_animation.png" 360 300 "$tmp"/g{0,1,2,3}.png
P="--width 720 --height 560 --azimuth 0.6"
one "$files/AnimatedMorphCube.glb" "$tmp/m0.png" $P
one "$files/AnimatedMorphCube.glb" "$tmp/m1.png" $P --time 1.0
one "$files/AnimatedMorphCube.glb" "$tmp/m2.png" $P --time 3.0
one "$files/AnimatedMorphCube.glb" "$tmp/m3.png" $P --morph target0=0.5,target1=0.5
row "$out/morph_targets.png" 360 280 "$tmp"/m{0,1,2,3}.png

# Programs.
for n in sim3_toaster obj4_spectacles; do
  one "eval/phase6/Z/Z1/$n/program.json" "$out/$n.png" --width 640 --height 480 --azimuth 0.7 --elevation 0.45
done
one golden/programs/washing_machine.json "$out/washing_machine.png" --width 640 --height 480 --azimuth 0.7 --elevation 0.35
one "$here/src/subdivide.json" "$out/subdivide.png" --width 800 --height 520 --azimuth 0.6 --elevation 0.6
"$B" collide eval/phase6/Z/Z1/obj1_octagonal_cup/program.json --hulls 16 --max-vertices 16 -o "$tmp/cup.glb" >/dev/null
one "$tmp/cup.glb" "$out/cup_colliders.png" --width 640 --height 480 --azimuth 0.7 --elevation 0.6
one "$here/src/loft_along.json" "$out/loft_along.png" --width 900 --height 520 --elevation 0.75 --azimuth 0.5
one "$here/src/thick_shells.json" "$out/thick_shells.png" --width 900 --height 520 --elevation 0.6 --azimuth -0.35
"$B" preview "$here/src/shell_crossing.json" -o "$out/shell_crossing.png" --views 2 --width 720 --height 520 --azimuth 0.6 --elevation 0.9 >/dev/null
one "$here/src/shell_pieces.json" "$out/shell_pieces.png" --width 1100 --height 520 --elevation 0.5 --azimuth -0.25

# The clip `open`, at four times.
k=0
for t in 0 0.4 0.8 1.4; do
  python3 -I - golden/programs/washing_machine_clips.json "$tmp/wm$k.json" "$t" <<'PY'
import json, sys
p = json.load(open(sys.argv[1]))
p["ops"].append({"op": "pose", "clip": "open", "time": float(sys.argv[3])})
json.dump(p, open(sys.argv[2], "w"))
PY
  one "$tmp/wm$k.json" "$tmp/wm$k.png" --width 320 --height 300 --azimuth 0.7 --elevation 0.35
  k=$((k + 1))
done
row "$out/washing_machine_clip.png" 320 300 "$tmp"/wm{0,1,2,3}.png

# Meshes from elsewhere.
one golden/meshes/ball_defects.obj "$out/mesh_before.png" --width 640 --height 480 --azimuth 0.7 --elevation 0.45
"$B" process golden/meshes/ball_defects.obj --budget 1500 --weld 0.001 --floaters 0.05 --holes 64 --fuse -o "$tmp/ball.glb" >/dev/null
one "$tmp/ball.glb" "$out/mesh_after.png" --width 640 --height 480 --azimuth 0.7 --elevation 0.45
"$B" preview "$files/Samba.fbx" -o "$tmp/sm.png" --frames 6 --width 190 --height 300 --no-edges >/dev/null
"$B" preview "$files/Soldier.glb" -o "$tmp/so.png" --clip Run --frames 6 --width 190 --height 300 --azimuth 3.6 >/dev/null
row "$tmp/sm_row.png" 190 300 "$tmp"/sm.{0,1,2,3,4,5}.png
row "$tmp/so_row.png" 190 300 "$tmp"/so.{0,1,2,3,4,5}.png
python3 -I - "$out/clips_posed.png" "$tmp/sm_row.png" "$tmp/so_row.png" <<'PY'
import sys
from PIL import Image
rows = [Image.open(n) for n in sys.argv[2:]]
sheet = Image.new("RGB", (rows[0].width, sum(r.height for r in rows)))
y = 0
for r in rows:
    sheet.paste(r, (0, y))
    y += r.height
sheet.save(sys.argv[1], optimize=True)
PY
"$B" repair "$here/src/leaf.obj" -o "$tmp/leaf0.glb" >/dev/null
"$B" repair "$here/src/leaf.obj" --weld 0.001 --thicken 1.2 -o "$tmp/leaf1.glb" >/dev/null
one "$tmp/leaf0.glb" "$out/leaf_open.png" --width 700 --height 460 --elevation 0.35 --azimuth 0.9
one "$tmp/leaf1.glb" "$out/leaf_thick.png" --width 700 --height 460 --elevation 0.35 --azimuth 0.9
