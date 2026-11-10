The mouse and pad model is built and saved to `program.json`. It measures 254 × 191 × 41.13 mm against the brief's 254 × 191 × 41. The height is 0.13 mm over because the mouse top's smooth curve rises slightly higher than the points I gave it.

- **Pad:** 3 mm thick with rounded corners.
- **Mouse shell:** a smooth body sitting on the pad, with a slot for the scroll wheel and an oval recess holding a small button.
- **Scroll wheel:** a separate part that spins freely about the left-right axis.

Every part measures closed, one piece and with no broken edges, and the whole model is 7,886 triangles. The previews look like the reference. I didn't preview the wheel turned to another position. Because it spins freely, that check matters less.

- **ITERATIONS:** 4 run_typescript calls, 0 failed.
- **PROBLEMS:**
  - The guides don't say how `body` reads its `plan` and `top` points. It isn't stated whether the y values are absolute positions along the part or start at 0, or whether `plan` is the full width or the half-width. I had to work both out from the measures.
  - The guides warn that a `loft` with curves can overshoot, but not that `body` with `smooth: true` does too. The top rose about 3.3–4 mm above its highest point, so I had to adjust it by trial to hit the 41 mm height.
  - `shape.md` warns that a later `at` replaces an `on`, but its examples are hinges on an edge. A part turned with `on` and then placed at its pivot with `at` hits the same problem. My first wheel came out standing upright, and wrapping it in a group fixed it.
  - There is no clear way to cut a recess into a curved surface. `pocket` measures depth from a flat frame, so I placed the frame above the surface and worked out the depth by hand.
  - `typescript.md` doesn't say which way a cylinder lies after `turn(0, 90°, 0)`. I got its axis along x right, but only by reasoning it out.
