The toaster is built and saved to `program.json`. Its overall size matches the brief exactly: 155.11 × 306.55 × 179 mm.

The pose check came before the final run, and I didn't pose the final version again. In that check, with the lever pulled down 59 mm and the button pushed in 3 mm, the lever tab crossed the housing in 4 places and the button only touched. I then moved the tab 0.3 mm off the end face to clear those crossings.

- **Housing:** a brushed-steel shell that narrows and rounds towards the top, with two bread slots cut 115 mm deep. Every measured part is closed and has no defects. The whole model is 4,346 triangles.
- **Wire supports:** 20 thin rods along the long walls of the slots.
- **Feet:** four small black rubber feet.
- **Controls:** all on the front end (−Y):
  - a lever tab that slides down 59 mm in a guide slot;
  - a browning dial with a grip bar that turns freely;
  - a cancel button that pushes in 3 mm.

In the previews it matches the reference.

**ITERATIONS:** 5 `run_typescript` calls, 1 failed. The third run was a check with the joints posed, and its program was overwritten by the final run without poses. The fourth run failed because a constant was read before it was declared.

**PROBLEMS:**
- The order of declarations behaved inconsistently. My first run read `leverZ` inside `makeHousing()` before declaring it, and it built correctly. The same order failed with a ReferenceError in the fourth run. The guide's rule ("declare shared numbers before the first call") is right, but the first run working anyway was misleading.
- The guides don't say how to read the posed check from inside a program. I printed `s.measure({ posed: true })` with `console.log`, but its interference fields (`crossings`, `touching`, `inside`) aren't explained anywhere. A part resting flat against a face showed up as 4 crossings, so I couldn't tell real overlap from contact.
- The guides don't say how to check a pose without changing the saved program. Since `program_out` must always be `program.json`, every posed check overwrites it and takes another run to undo.
- Loft side faces are all called `side`, so a single end wall can't be reached by name. I had to build frames from coordinates (`og.frame` with `normal`) to place the controls, although §3 says to reach faces by name, never by coordinates.
