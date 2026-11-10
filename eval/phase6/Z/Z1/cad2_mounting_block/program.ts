export default function (og) {
  const s = og.model({ name: "mounting_block" });
  const size = 80, edge = 2, sink = 0.5;
  const bossD = 30, bossH = 30, boreD = 6;
  const ribLen = 20, ribT = 4, ribH = 12;
  const holeD = 5, holeDepth = 12;
  const margin = 12, pocketDepth = 20;

  const block = s.box({ name: "mounting_block", width: size, depth: size, height: size });
  s.chamfer(block, { distance: edge });
  const top = s.face(block, "top");
  const left = s.face(block, "left");
  const front = s.face(block, "front");
  const bottom = s.face(block, "bottom");

  // sunk 0.5 mm so it fuses: top at 109.5 as the brief's bounding box says
  const boss = s.cylinder({ name: "boss", radius: bossD / 2, height: bossH, on: top.offset(-sink) });
  s.join(block, boss);

  const rib = s.box({ name: "rib", width: ribT, depth: ribH, height: ribLen, on: left.move(0, 28).offset(-sink) });
  s.join(block, rib);

  s.hole(block, { diameter: boreD, depth: bossH, at: top.offset(bossH - sink) });

  s.hole(block, { diameter: holeD, depth: holeDepth, at: [
    left.move(-14, 4), left.move(14, -6),
    front.move(-14, 4), front.move(14, -6),
  ] });

  s.pocket(block, { profile: og.rect({ width: size - 2 * margin, height: size - 2 * margin }), depth: pocketDepth, at: bottom });

  s.material(block, { color: "#a7abb0", metalness: 0.8, roughness: 0.4 });
  return s;
}
