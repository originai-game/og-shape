export default function (og) {
  const s = og.model({ name: "bracket_block" });
  const length = 80, width = 60, thickness = 20;

  const block = bracket(length, width, thickness);
  s.material(block, { color: "#9a9ea3", metalness: 0.2, roughness: 0.5 });
  return s;

  function bracket(L, W, T, leg = 20, r = 8, chamfer = 2,
                   pocketL = 30, pocketW = 20, pocketD = 16, holeD = 8, holeInset = 20) {
    const x0 = -L / 2, y0 = -W / 2;
    const pts = [
      [x0, y0], [L / 2, y0], [L / 2, y0 + leg], [x0 + leg, y0 + leg], [x0 + leg, W / 2], [x0, W / 2],
    ].map(([x, y]) => ({ x, y, radius: r }));
    const b = s.extrude({ name: "bracket", profile: og.poly({ points: pts }), height: T });
    s.chamfer(b, { distance: chamfer, edges: "bottom" });
    // pocket across the long leg, next to the corner, open on the top
    const pocket = s.box({ name: "pocket", width: pocketL, depth: pocketW, height: pocketD + 1,
      on: og.world.move(x0 + leg + pocketL / 2, y0 + leg / 2, T - pocketD) });
    s.cut(b, pocket);
    // through hole at the end of the long leg
    s.hole(b, { diameter: holeD, at: [og.world.move(L / 2 - holeInset, y0 + leg / 2, T)] });
    return b;
  }
}
