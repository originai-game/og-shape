The mouse and pad are built and saved to `program.json`. The overall size measures 254 × 191 × 41 mm, matching the brief.

- **Parts:** each part is a closed solid in one piece, with 4,306 triangles in total.
  - **Pad:** a 4 mm thick rounded rectangle with 16 mm corners.
  - **Mouse shell:** 120 × 64 × 37 mm, highest just behind the middle.
  - **Oval button:** sits in a recess cut into the shell's top.
  - **Scroll wheel:** sits in a slot near the front and spins freely about the left-right axis.
- **Check:** I compared the result against the reference from an angle and from above, and it matches in shape and colour. I did not preview the wheel turned to a different position.

ITERATIONS: 5 run_typescript calls, 0 failed.

PROBLEMS:
- **`body` smoothing is misleading.** The guide says `smooth: true` makes the line pass through the points. In practice it put the shell's narrowest point (a "waist") at y ≈ 0, where my points made it widest (62 mm), and the shell bulged near the ends instead. My guess is that it smooths the points as a 2D curve, which overshoots when the curve rises steeply at the ends. I got around it by giving many points worked out from a formula, with smoothing off. The guide should say how the smoothing behaves, or warn about this.
- **`body` sizes aren't explained.** The guide doesn't say whether the `plan` values are full width or half width (they are full width). It also doesn't say that a line ending above zero leaves a flat end face, so the shell's ends were cut off square until I made both lines nearly zero there.
- **Cut-in features on curved tops.** No guide shows how to cut a shallow recess, like the button's, that follows a curved surface. `pocket` measures its depth from a frame you give it, so you have to estimate the surface height yourself.
- **Small gaps:**
  - The preview's `views` value of 1 with `azimuth` and `elevation` worked, but the guide's review section doesn't mention that you can preview a single view.
  - Turning a cylinder to lie along X (`turn(0, deg(90), 0)` to point +z along +X) needs working out by hand; the guide gives no example.
