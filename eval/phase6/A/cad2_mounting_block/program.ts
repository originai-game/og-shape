export default function (og) {
  const s = og.model({ name: "mounting_block" });
  const size = 80, chamfer = 2;
  const bossD = 30, bossH = 30, bore = 6;
  const ribL = 20, ribT = 4, ribH = 12;
  const sideHole = 5, margin = 12, pocketDepth = 20;
  const lap = 0.5;

  const block = s.box({ name: "block", width: size, depth: size, height: size });
  s.chamfer(block, { distance: chamfer });

  // boss on the top centre, sunk lap into the block
  const boss = s.cylinder({ name: "boss", radius: bossD / 2, height: bossH, on: s.face(block, "top").offset(-lap) });
  s.join(block, boss);

  // rib out of the +X side, built on the right face
  const right = s.face(block, "right");
  const rib = s.box({ name: "rib", width: ribT, depth: ribH, height: ribL, on: right.offset(-lap) });
  s.join(block, rib);

  s.hole(block, { diameter: bore, depth: bossH + 10, at: s.face(block, "boss.top") });

  // side holes: two on +X, two on -Y
  s.hole(block, { diameter: sideHole, depth: 15, at: s.grid({ nx: 2, px: 30, on: s.face(block, "block.right").move(0, 10) }) });
  s.hole(block, { diameter: sideHole, depth: 15, at: s.grid({ nx: 2, px: 30, on: s.face(block, "block.front").move(0, 10) }) });

  s.pocket(block, { profile: og.rect({ width: size - 2 * margin, height: size - 2 * margin }), depth: pocketDepth, at: s.face(block, "block.bottom") });

  s.material(block, { color: "#9a9a9a", metalness: 0.6, roughness: 0.4 });
  return s;
}
