The mouse and pad model is finished and saved to `program.json`. It's 254 × 191 × 41 mm, matching the brief, with about 9,600 triangles.

- **Pad:** a 3 mm thick rounded rectangle.
- **Mouse shell:** 114 mm long, 63 mm wide and 38 mm tall. It has a slot for the scroll wheel and an oval recess.
- **Button:** a separate oval sitting on the floor of the recess.
- **Scroll wheel:** spins freely about the left-right (X) axis through the joint `wheel_spin`.

Every part is closed and in one piece. With the wheel turned, nothing passes through anything else; the mouse sits on the pad and the button on the recess floor, touching but not sunk in. One small flaw remains: the front end of the mouse narrows a little sharply where its outline closes to a point.

**ITERATIONS:** 6 run_typescript calls, 1 of which failed.

**PROBLEMS:**
- **Body ends:** the guide says a body's end is flat "where `plan` or the height is not 0", which suggests setting only the width (`plan`) to 0 makes a rounded end. It doesn't: the run fails with "only one view closes", because the top and bottom lines must also meet at the same height there. The guide doesn't say this, or how to round off a body's end. I worked it out from the error.
- **Smooth bottom line:** the guide warns that smooth lines overshoot between steep points, but not that a flat stretch of the bottom line can dip below its own points. Mine sank about 2.7 mm into the pad, and only the measured bounds showed it.
- **Interference check:** the guide doesn't explain what `asBuilt: false` with nonzero `crossings` means for a part that turns. Here it was a real overlap (the wheel dipped below its slot), not a rounding effect, but the guide's wording made me wonder.
- **Preview tool:** it refuses `path` and `program` together. The guides don't mention this, but the error message made it clear.
