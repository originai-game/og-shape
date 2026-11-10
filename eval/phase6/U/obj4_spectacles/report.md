I built the spectacles in one `run_typescript` call, and the program is saved at `program.json`.

**What's in it:** two curved lenses, made by cutting each lens outline with a 150 mm-radius cylinder shell. The frame is one black part: both brow bars, the hinge blocks at their outer ends, and a thin arched wire bridge. Each nose pad is a small oval disc on a bent metal wire. The two temple arms are folded flat behind the lenses at rest. They tilt down and cross diagonally, as in the reference. Each arm has a hinge joint that opens 0–90°.

**Checks:**
- **Size:** the measured bounds are exactly 140 × 12.48 × 26.77 mm, matching the brief.
- **Solids:** every part is closed, one piece, with no non-manifold edges or degenerate triangles. The whole model is 2,116 triangles.
- **Looks:** the previews from the front, the sides and a high angle like the reference's match it.
- **Hinges:** I tested them on a cut-down copy of the program with both hinges posed open (1.5 rad), passed straight to `preview`. Both arms swing backward, the right way. The saved `program.json` has no pose in it.

The arms tilt about 9° downward, so they also point down a little when opened.

- **ITERATIONS:** 1 `run_typescript` call, 0 failed.
- **PROBLEMS:**
  1. The TypeScript guide doesn't say how to check a pose without saving it into the program. `pose` is "export and preview only", but `run_typescript` always writes `program_out`. Posing means either writing the pose into `program.json` and spending another run to remove it, or editing the JSON by hand. I edited the JSON, but `guides/program.md` was off-limits, so I had to guess the form of the `pose` op from the saved file.
  2. In the JSON form, ops refer to parts by a creation-order number. Neither guide says that parts used up by a `join` keep their numbers, and getting it wrong caused my one preview error ("was used up by an earlier operation").
  3. Placing a profile in a vertical plane depends on the frame's axis directions. The facing rule covers `og.frame` without `up` and face frames, but it never says what `x` becomes when you pass `normal` and `up` together. I had to work it out from x = y × z: `normal: [0,-1,0]` with `up: +Z` gives x = +X.
  4. The brief asks for a curved lens. The "curved panel = intersect of a slab and a big cylinder" hint worked, but the guide doesn't say whether to use a solid cylinder or a `tube`. Only a tube keeps the lens thickness even.
