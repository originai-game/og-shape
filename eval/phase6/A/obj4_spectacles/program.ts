export default function (og) {
  const s = og.model({ name: "spectacles" });
  // Planned size 140 x ~12.5 x ~26.8, temples folded at rest.
  const lensW = 54, lensH = 22, lensGap = 12; // gap between the inner lens edges
  const lensX = lensGap / 2 + lensW / 2;      // lens centre x
  const browH = 5, browD = 7;
  const endW = 8;                             // hinge block at the outer end of each brow
  const halfW = 70;                           // half the overall width
  const templeL = 125, templeT = 2, templeH = 4;
  const hingeZ = lensH + browH / 2 - 0.5;

  const parts = [];
  for (const side of [-1, 1]) {
    parts.push(lens(side), brow(side), pad(side));
  }
  parts.push(bridge());
  const front = s.group({ name: "front", children: parts });

  for (const side of [-1, 1]) {
    const yOff = side > 0 ? browD - 0.5 : browD - 0.5 + templeT + 0.3;
    const hinge = og.frame({ origin: [side * halfW, yOff, hingeZ] });
    const t = temple(side);
    s.at(t, hinge);
    s.joint(t, {
      name: side > 0 ? "right_hinge" : "left_hinge",
      parent: front, on: hinge, axis: side > 0 ? "-Z" : "+Z", range: [0, og.deg(95)],
    });
  }
  return s;

  // A meniscus lens: a slab cut between two spheres sharing one centre behind it.
  function lens(side, R = 80, t = 2) {
    const cx = side * lensX, cz = lensH / 2;
    const look = og.frame({ origin: [cx, 8, cz], normal: [0, -1, 0], up: [0, 0, 1] });
    const l = s.extrude({ name: side > 0 ? "right_lens" : "left_lens",
      profile: og.rect({ width: lensW, height: lensH, radius: 7 }), height: 9, on: look });
    const outer = s.sphere({ name: "lens_outer", radius: R, on: og.world.move(cx, R, cz) });
    s.intersect(l, outer);
    const inner = s.sphere({ name: "lens_inner", radius: R - t, on: og.world.move(cx, R, cz) });
    s.cut(l, inner);
    s.material(l, { color: "#2a2d33", roughness: 0.1, metalness: 0.2, opacity: 0.75 });
    return l;
  }

  function brow(side) {
    const inner = lensGap / 2 + 2;
    const w = halfW - endW - inner;
    const b = s.box({ name: side > 0 ? "right_brow" : "left_brow", width: w, depth: browD, height: browH,
      on: og.world.move(side * (inner + w / 2), browD / 2 - 0.5, lensH - 0.5) });
    s.fillet(b, { radius: 1.5, edges: "side" });
    const end = s.box({ name: "end_piece", width: endW + 0.5, depth: browD, height: browH + 1,
      on: og.world.move(side * (halfW - endW / 2 - 0.25), browD / 2 - 0.5, lensH - 1.5) });
    s.join(b, end);
    s.material(b, { color: "#111111", roughness: 0.4 });
    return b;
  }

  // A thin wire arch between the inner top corners of the lenses.
  function bridge(r = lensGap / 2 + 1, wire = 0.8) {
    const b = s.revolve({ name: "bridge", profile: og.poly({ points: [[r - wire, -wire], [r + wire, -wire], [r + wire, wire], [r - wire, wire]] }),
      angle: og.deg(180), on: og.world.move(0, 1.5, lensH - 7).turn(0, og.deg(180), 0) });
    s.material(b, { color: "#9a9ca0", metalness: 1, roughness: 0.3 });
    return b;
  }

  // Wire support from the lens's inner edge back to a small pad.
  function pad(side) {
    const x = side * (lensGap / 2 - 1);
    const wire = s.cylinder({ name: side > 0 ? "right_pad" : "left_pad", radius: 0.5, height: 5,
      on: og.frame({ origin: [x, 1.5, 9], normal: [0, 1, 0], up: [0, 0, 1] }) });
    const disc = s.cylinder({ name: "pad_disc", radius: 2.5, height: 1, radiusTop: 2.5,
      on: og.frame({ origin: [x + side * 0.5, 6, 8], normal: [-side, 0, 0], up: [0, 0, 1] }) });
    s.join(wire, disc);
    s.material(wire, { color: "#c8ccd0", metalness: 0.6, roughness: 0.3 });
    return wire;
  }

  // Built about its hinge at the origin, folded: it runs inward along x behind the lenses.
  function temple(side) {
    const arm = s.box({ name: side > 0 ? "right_temple_arm" : "left_temple_arm", width: templeL, depth: templeT, height: templeH,
      on: og.world.move(-side * templeL / 2, templeT / 2, -templeH / 2) });
    s.fillet(arm, { radius: 0.8, edges: "side" });
    s.material(arm, { color: "#111111", roughness: 0.4 });
    return s.group({ name: side > 0 ? "right_temple" : "left_temple", children: [arm] });
  }
}
