export default function (og) {
  const s = og.model({ name: "telescoping_slide" });
  // Overall 1100 x 240 x 300; wall at +X, the slide runs out toward -X.
  const plateT = 30, plateW = 240, plateH = 300;
  const axisZ = plateH / 2;
  const wallX = 550 - plateT;          // wall plate's front face (-X side)
  const standoffL = 40;
  const sleeveL = 600, sleeveW = 80, sleeveH = 100, wall = 8;
  const middleL = 500, middleW = 60, middleH = 80, middleWall = 8;
  const bladeL = 600, bladeW = 26, bladeH = 36, blockSize = 40;
  const sleeveEnd = wallX - standoffL + 10;   // sleeve overlaps standoffs 10 mm
  const middleRest = 280, bladeRest = 80;     // back ends at rest (world x)

  const plate = wallPlate();
  const sleeve = outerSleeve();
  const mFrame = og.frame({ origin: [middleRest, 0, axisZ] });
  const middle = middleStage();
  s.at(middle, mFrame);
  s.joint(middle, { name: "middle_slide", parent: sleeve, kind: "prismatic", on: mFrame, axis: "-X", range: [0, 260] });
  const bFrame = og.frame({ origin: [bladeRest, 0, axisZ] });
  const blade = innerBlade();
  s.at(blade, bFrame);
  s.joint(blade, { name: "blade_slide", parent: middle, kind: "prismatic", on: bFrame, axis: "-X", range: [0, 220] });
  return s;

  // Along -X, profile x = world -Y, profile y = world +Z.
  function alongX(x) {
    return og.frame({ origin: [x, 0, axisZ], normal: [-1, 0, 0], up: [0, 0, 1] });
  }

  function wallPlate() {
    const p = s.box({ name: "wall_plate", width: plateT, depth: plateW, height: plateH, on: og.world.move(550 - plateT / 2, 0, 0) });
    s.chamfer(p, { distance: 3, edges: "side" });
    const front = s.face(p, "left");
    for (const dz of [-110, 110]) {
      const head = s.cylinder({ name: "screw_head", radius: 12, height: 7, on: front.move(0, dz).offset(-1) });
      s.join(p, head);
    }
    s.material(p, { color: "#b9b4a2", roughness: 0.8 });
    return p;
  }

  function outerSleeve() {
    const section = og.subtract(
      og.rect({ width: sleeveW, height: sleeveH }),
      og.rect({ width: sleeveW - 2 * wall, height: sleeveH - 2 * wall }),
      { translate: { profile: og.rect({ width: 2 * wall, height: 40 }), x: sleeveW / 2, y: 0 } },
    );
    const c = s.extrude({ name: "outer_sleeve", profile: section, height: sleeveL, on: alongX(sleeveEnd) });
    for (const dz of [sleeveH / 2 - 15, -(sleeveH / 2 - 15)]) {
      const st = s.box({ name: "standoff", width: standoffL, depth: 40, height: 30, on: og.world.move(wallX - standoffL / 2, 0, axisZ + dz - 15) });
      s.join(c, st);
    }
    s.material(c, { color: "#8a8d91", metalness: 0.8, roughness: 0.4 });
    return c;
  }

  // Built about its own origin: the back end at x = 0, on the slide axis.
  function middleStage() {
    const section = og.subtract(
      og.rect({ width: middleW, height: middleH }),
      og.rect({ width: middleW - 2 * middleWall, height: middleH - 2 * middleWall }),
    );
    const m = s.extrude({ name: "middle_section", profile: section, height: middleL, on: og.frame({ normal: [-1, 0, 0], up: [0, 0, 1] }) });
    s.material(m, { color: "#2a2b2e", metalness: 0.6, roughness: 0.5 });
    return s.group({ name: "middle_stage", children: [m] });
  }

  function innerBlade() {
    const b = s.extrude({ name: "inner_blade", profile: og.rect({ width: bladeW, height: bladeH }), height: bladeL, on: og.frame({ normal: [-1, 0, 0], up: [0, 0, 1] }) });
    const block = s.box({ name: "end_block", width: blockSize, depth: blockSize, height: blockSize, on: og.world.move(-bladeL - blockSize / 2 + 10, 0, -blockSize / 2) });
    s.join(b, block);
    s.material(b, { color: "#9a9da1", metalness: 0.8, roughness: 0.35 });
    return s.group({ name: "blade_stage", children: [b] });
  }
}
