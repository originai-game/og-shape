export default function (og) {
  const s = og.model({ name: "mounting_block" });
  const cube = 80, overlap = 0.5;
  const ribLength = 20, ribThickness = 4, ribHeight = 12;
  const bossRadius = 15, bossHeight = 30, boreDiameter = 6;
  const chamferDistance = 2;
  const mountDiameter = 5, mountOffset = 10;
  const pocketMargin = 12, pocketDepth = 20;
  const baseX = -ribLength / 2 + overlap / 2; // centres the whole block in x

  const block = base();
  s.material(block, { color: "#9a9a9a", metalness: 0.2, roughness: 0.5 });
  return s;

  function base() {
    const b = s.box({ name: "block", width: cube, depth: cube, height: cube, on: og.world.move(baseX, 0, 0) });
    s.chamfer(b, { distance: chamferDistance });

    // four through holes in the side faces, at mid height + offset
    const zh = cube / 2 + mountOffset;
    const sideX = [-mountOffset, mountOffset].map((y) =>
      og.frame({ origin: [baseX + cube / 2, y, zh], normal: [1, 0, 0] }));
    const sideY = [-mountOffset, mountOffset].map((x) =>
      og.frame({ origin: [x, cube / 2, zh], normal: [0, 1, 0] }));
    s.hole(b, { diameter: mountDiameter, at: [...sideX, ...sideY] });

    // rib on the +X side
    const rib = s.box({
      name: "rib", width: ribLength, depth: ribThickness, height: ribHeight,
      on: og.world.move(baseX + cube / 2 - overlap + ribLength / 2, 0, cube / 2 - ribHeight / 2),
    });
    s.join(b, rib);

    // boss on top, with a bore
    const boss = s.cylinder({ name: "boss", radius: bossRadius, height: bossHeight, on: og.world.move(baseX, 0, cube - overlap) });
    s.join(b, boss);
    s.hole(b, { diameter: boreDiameter, depth: bossHeight, at: og.frame({ origin: [baseX, 0, cube - overlap + bossHeight], normal: [0, 0, 1] }) });

    // square pocket up into the bottom
    const pocket = s.box({
      name: "pocket", width: cube - 2 * pocketMargin, depth: cube - 2 * pocketMargin,
      height: pocketDepth + 2 * chamferDistance, on: og.world.move(baseX, 0, -chamferDistance),
    });
    s.cut(b, pocket);
    return b;
  }
}
