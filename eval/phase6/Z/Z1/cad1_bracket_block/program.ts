export default function (og) {
  const s = og.model({ name: "bracket_block" });
  const lenX = 80, lenY = 60, leg = 20, thick = 20, r = 8, chamfer = 2;
  const pocketL = 30, pocketW = 20, pocketDepth = 16, holeD = 8;
  const x0 = -lenX / 2, x1 = lenX / 2, y0 = -lenY / 2, y1 = lenY / 2;

  // L outline: long leg along x at the front, short leg along y at +X
  const outline = og.poly({ points: [
    { x: x0, y: y0, radius: r },
    { x: x1, y: y0, radius: r },
    { x: x1, y: y1, radius: r },
    { x: x1 - leg, y: y1, radius: r },
    { x: x1 - leg, y: y0 + leg },
    { x: x0, y: y0 + leg, radius: r },
  ] });
  const plate = s.extrude({ name: "bracket", profile: outline, height: thick });
  s.chamfer(plate, { distance: chamfer, edges: "bottom" });

  // pocket next to the corner, across the full leg width
  const legY = y0 + leg / 2;
  const pocketX = x1 - leg - pocketL / 2;
  s.pocket(plate, { profile: og.rect({ width: pocketL, height: pocketW + 4 }), depth: pocketDepth,
    at: og.frame({ origin: [pocketX, legY, thick] }) });

  // through hole at the end of the long leg
  s.hole(plate, { diameter: holeD, at: og.frame({ origin: [x0 + leg / 2, legY, thick] }) });

  s.material(plate, { color: "#a0a4a8", metalness: 0.8, roughness: 0.4 });
  return s;
}
