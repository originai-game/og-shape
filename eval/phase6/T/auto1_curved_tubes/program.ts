export default function (og) {
  const s = og.model({ name: "curved_tube_assembly" });

  // Stationary lower tube.
  const tube3 = curvedTube("curved_tube_3");
  s.at(tube3, og.world.move(17, 0, 868.51).turn(0, 0, og.deg(10.61)));
  const stationary = s.group({ name: "stationary_tube", children: [tube3] });
  s.material(stationary, { color: "#9a9a9a", roughness: 0.5 });

  // Rotating upper pair, spinning about the vertical axis on top of the lower tube.
  const spin = og.world.move(0, 90.77, 1219.6).turn(0, 0, og.deg(10.61));
  const rotating = rotatingTubes("rotating_tubes");
  s.at(rotating, spin);
  s.joint(rotating, { name: "rotating_tubes_spin", parent: stationary, kind: "continuous", on: spin, axis: "+Z" });

  return s;

  // sand3d rotations are extrinsic X, then Y, then Z: R = Rz * Rx, i.e. turn Z first, then X.
  function rotatingTubes(name) {
    const t1 = curvedTube(`${name}_curved_tube_1`);
    s.at(t1, og.world.move(0, 0, -960.91).turn(0, 0, og.deg(-100.61)).turn(og.deg(-90), 0, 0));
    const t2 = curvedTube(`${name}_curved_tube_2`);
    s.at(t2, og.world.move(0, 0, 258.74).turn(0, 0, og.deg(-100.61)).turn(og.deg(90), 0, 0));
    const g = s.group({ name, children: [t1, t2] });
    s.material(g, { color: "#9a9a9a", roughness: 0.5 });
    return g;
  }

  // A C-shaped pipe of two 90 degree elbows, meeting tangentially at (0, -depth/2, 0).
  // Ends open at z = +-height/2, pointing along z.
  function curvedTube(name, width = 254, depth = 517.47, height = 702.17, bore = 211.84) {
    const R = height / 2;
    const cy = -depth / 2;
    const upper = elbow(`${name}`, R, width, bore, cy, R, -1);
    const lower = elbow(`${name}_lower`, R, width, bore, cy, -R, +1);
    // The two elbows meet tangentially in a cusp, where the kernel's join leaves the
    // result open; they stay two parts of one colour in one group.
    return s.group({ name: `${name}_group`, children: [upper, lower] });
  }

  // Quarter torus about the x axis centred at (0, cy, cz), kept in y >= cy and
  // on the side of cz given by dir (-1: below, +1: above).
  function elbow(name, R, d, bore, cy, cz, dir) {
    // Torus axis (its own +y) along world x. Turned half a facet about that axis so
    // that no ring of vertices lies on the quadrant's cutting planes.
    const centre = og.frame({ origin: [0, cy, cz], normal: [0, 0, 1], up: [1, 0, 0] }).turn(0, og.deg(360 / 64 / 2), 0);
    const outer = s.torus({ name, radius: R, tube: d / 2, on: centre });
    const inner = s.torus({ name: `${name}_bore`, radius: R, tube: bore / 2, on: centre });
    s.cut(outer, inner);
    const extent = R + d / 2;
    const quadrant = s.box({
      name: `${name}_quadrant`, width: 2 * d, depth: extent, height: extent,
      on: og.world.move(0, cy + extent / 2, dir < 0 ? cz - extent : cz),
    });
    s.intersect(outer, quadrant);
    return outer;
  }
}
