I built the djembe in one `run_typescript` call, and the program is saved to `program.json`.

**Measurements:** the overall size is 407.9 × 407.9 × 600 mm, which matches the brief. Every part is closed, with no non-manifold edges or degenerate triangles. The whole model is about 6,000 triangles.

**Preview:** the four turntable views match the reference.

**Parts:**
- **`shell`:** the wooden base cone, neck and goblet bowl, turned as one piece.
- **`neck_binding`:** the black binding and band where the neck meets the bowl.
- **`strip_0`:** 18 black tension strips up the bowl. They are joined into one part, so its measure shows 18 separate pieces; that is expected.
- **`top_band`:** a black ring just under the head.
- **`head`:** the pale leather drum head.
- **`hoop`:** the black ring on top.

ITERATIONS: 1 `run_typescript` call, 0 failed.

PROBLEMS:
- §8 of shape.md says a part should have one piece unless meant otherwise, but neither guide says how to keep many repeated, non-touching details like the strips together. Joining them and accepting several pieces versus putting them in a group is left unclear.
- The TypeScript guide doesn't say whether `ring` with `radius: 0` returns a plain array that `forEach` can loop over, or whether a `wire` path is placed in the `on` frame's own space. Both worked as I guessed, but I had to guess.
- The torus `on` origin is the bottom of the tube, not its centre; you have to piece that together from "rests on z = 0, a torus's centre at z = tube". This was easy to get wrong when placing the hoop.
