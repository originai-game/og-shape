The spectacles are built and saved as `program.json`. The overall size matches the brief exactly (140 × 12.48 × 26.77 mm), and every part measured closed and in one piece, with no broken or degenerate geometry, 2,972 triangles in all.

The model has two curved tinted lenses, a curved brow bar over each, a hinge block at each outer end, a thin arched bridge, and two nose pads on silver wire supports. The two temple arms are jointed at the hinges (right about −Z, left about +Z, each 0–95°). I didn't pose the joints to check that they open the right way and clear the frame.

At rest the arms lie folded behind the lenses and slope down so they cross, as in the reference image. The brief says "folded slightly", but its 12.48 mm depth only fits arms folded flat, so I followed the size and the picture. The preview from above matches the reference.

- **ITERATIONS:** 3 `run_typescript` calls, 1 failed. On the second call I passed a `path` to a `program.ts` file that didn't exist instead of `source`. That was my mistake, not the tool's.
- **PROBLEMS:**
  - The guides never say plainly how to make a part curved in plan, like a wrapped lens or brow. shape.md only says "a curved panel the `intersect` of a slab and a big cylinder". I had to work out that a hollow cylinder (a `tube`) with an axis along Z gives a shell of even thickness, and where to put that axis so the front sits at the right y.
  - The facing rule for `og.frame` with a sideways normal (normal +Y gives x = −X, y = +Z) is only in the TypeScript guide's Frame section. Extruding a 2D outline along y, as I did for the arms and lenses, depends on it, and it isn't shown with an example. I had to mirror the outline's x by hand to get the left arm right.
  - Joint "at rest" means the state described, but the guides don't cover a brief whose words ("folded slightly") disagree with its size, where only fully folded arms fit. shape.md says the numbers win over the picture; it doesn't say whether the numbers or the words decide the rest pose.
  - The `run_typescript` tool's description doesn't say that `path` must name a file that already exists, or that it can't be combined with `source`. That's minor.
