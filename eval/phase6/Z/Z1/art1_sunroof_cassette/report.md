I built the sunroof cassette and saved it to `program.json` at rest (both slides at 0). It measures 1260 × 840 × 48 mm, matching the brief. Every part is a single closed solid with no broken edges, and the whole model is 1,122 triangles.

- **Frame:** one dark steel part made of the two side rails, front and rear headers, the centre crossmember and two thin guide rails along the sides.
- **Panels:** each glass panel sits on a dark carrier with four slide shoes and a cable anchor underneath.
- **Sliding:** the front panel slides 40 mm toward −X and the rear one 40 mm toward +X. I posed both at 40 mm and looked at the preview: they move the right way, stay inside the frame, and open the gaps by the crossmember that the reference shows. I only checked clearance by eye, not with the posed interference measure.
- **Orientation:** the brief puts the length along x, so I took "front" to mean the −X end. That doesn't match the guide's convention that the front faces −Y.

**ITERATIONS:** 3 run_typescript calls, 0 failed. The second run included the 40 mm pose to check it; the third removed it for the saved version.

**PROBLEMS:**
- `shape.md` says "the front looks −Y", but in this brief "front" and "rear" lie along the x length. The guide doesn't say how to read "forward" when the object's length runs along x, so I had to guess.
- A `s.pose(...)` call inside the program is saved into `program_out` as a `pose` op. The guides don't say this, so the only way to check a pose and still save the object at rest is an extra run with the pose removed. The `measure` and `preview` tools have no pose option, so this costs a run every time.
- `shape.md` §8 says to use `s.measure({ posed: true })` to check posed parts for collisions. `typescript.md` doesn't say where that result shows up when the program is run through `run_typescript`, which only reports the at-rest measures.
- The order of the frames `s.grid` returns (which index is which corner) isn't documented. I needed it to name the shoes.
