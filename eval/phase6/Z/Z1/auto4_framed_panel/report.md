I built the framed panel in one run, and its overall size matches the brief: 1016 × 35.56 × 685.8 mm. The program is saved as `program.json`.

- **Panel:** a dark box, 1016 × 25 × 685.8 mm, standing upright.
- **Frame:** a light-grey rectangle with a 23 mm border and 10.56 mm thick, sitting on the front of the panel.
- **Joint:** the frame is hinged on the panel's top front edge with a continuous joint called `frame_spin`, turning about −X.
- **Checks:** both solids are closed, in one piece each, with no non-manifold edges and 44 triangles in total. The preview matches the reference.
- **Not checked:** I never posed the joint, so I haven't seen the frame turn. Because it spins all the way round its hinge, it will pass through the panel. The guide says that is allowed for a spinning joint.

ITERATIONS: 1 run_typescript call, 0 failed.

PROBLEMS:
- The axis direction is unclear for a part that spins all the way round. The guide gives sign rules only for doors, lids and flaps, so I picked −X by analogy with the flap.
- "Spins" in the brief could mean a full spin or a limited hinge. The guide doesn't say how to read that, so I chose a continuous joint.
- The guide never says that `og.frame({ origin })` without a normal keeps the world axes. I relied on the facing rule, and the recorded program confirmed it.
- The guide doesn't say how to set the x direction of a frame made with `normal` and `up`. I had to work it out (here x came out as +X).

Otherwise none.
