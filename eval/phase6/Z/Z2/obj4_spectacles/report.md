I built the spectacles in 2 `run_typescript` calls, neither of which failed, and saved the program as `program.json`.

**What's in the model:**
- **Lenses:** two curved, tinted lenses, shaped by intersecting a rounded outline with a sphere of radius 100 mm.
- **Frame:** a black brow bar over each lens, following the lens curve, a thin wire bridge, and a block at each outer end where the hinge sits.
- **Nose pads:** a flattened pad on a metal wire support on each side.
- **Temple arms:** each folds on a hinge at the outer end of its brow (`right_hinge` and `left_hinge`, 0 to 95°). As in the picture, at rest they lie folded behind the lenses, tilted down 7° so they cross.

**Checks:**
- **Size:** 140 × 11.1 × 26.5 mm against the brief's 140 × 12.48 × 26.77, so it is about 1.4 mm too shallow front to back.
- **Parts:** every part is a single closed solid, about 5,400 triangles in all.
- **Hinges:** opened to 90°, both arms swing back to the rear as they should.
- **Overlaps:** with the arms open, the right arm passes through its hinge block, because the pivot sits inside the block. The lenses, brows, bridge and pad wires are separate parts that overlap on purpose.

ITERATIONS: 2 run_typescript calls, 0 failed.

PROBLEMS:
- The guide doesn't say which way an arm that is tilted at rest swings when its axis is a world direction. I set the axis to world Z, and an arm tilted down 7° then swings in a cone around world Z rather than about its own tilted line. It worked here, but I had to guess.
- The facing rule describes face frames and `og.frame` without `up`, but doesn't say how `og.frame({ normal, up })` sets x. I had to work out the cross product myself to place a profile drawn upright with the front facing −Y. One example would fix this.
- The guide doesn't explain that `asBuilt: false` in a posed measure's interference report means the parts don't meet at rest, so the overlap comes from the pose. It took some reading to tell this apart from the overlaps that are there on purpose.
