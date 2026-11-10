The closet is built and saved as `program.json`. It's a grey metal case open at the front, with two inset doors side by side, each hinged at its outer edge and opening 90°. Its overall size is exactly 622.2 × 914.6 × 2023 mm, every part is a closed, single solid, and the whole model is 64 triangles. The preview matches the reference.

The size you gave makes the 914.6 mm side the front, so the doors are on the −X face and open toward −X. With both doors posed fully open (run 2), they swing outward and don't hit the case. In run 3 I rebuilt the right door on its own instead of mirroring the left one, so its hinge is named `door_right_hinge` rather than `door_left_hinge.001`. Its shape and position are the same as before, but I didn't pose it again after that change.

- **ITERATIONS:** 3 `run_typescript` calls, 0 failed.
- **PROBLEMS:**
  1. **Which face is the front:** the guide says to match the size per axis "even if the object's own front then faces another way". But it doesn't say how to decide which face the doors go on. The brief, the reference image and the guide's "front looks −Y" rule point different ways, and I had to work it out from the proportions.
  2. **Names on a mirrored copy:** `mirror` gives the copied joint and parts automatic names (`door_left_hinge.001`, `door_left_panel.001`) that don't follow the naming rules in §3. There's no option to rename them, so a left/right pair of doors with clean names has to be built twice.
  3. **Checking a pose:** the `preview` tool can't pose joints. To check an open door you have to call `s.pose` and `s.measure({ posed: true })` inside the program and print the result with `console.log`, which uses up a `run_typescript` call. The guide doesn't say that `console.log` output comes back in `logs`.
  4. **Joint axis signs:** the "seen from the front" examples for the axis sign assume the front faces −Y. For a front facing −X I had to work out the signs by hand.
