export default function (og) {
  const s = og.model({ name: "mounting_block" });
  const size = 80, chamferD = 2;
  const bossD = 30, bossH = 30, bossHole = 6;
  const ribOut = 20, ribThick = 4, ribTall = 12;
  const sideHole = 5, sideHoleDepth = 15, sideHoleSpacing = 30;
  const pocketMargin = 12, pocketDepth = 20;
  const sink = 0.5;

  const block = mountingBlock();
  return s;

  function mountingBlock() {
    const b = s.box({ name: "mounting_block", width: size, depth: size, height: size });
    s.chamfer(b, { distance: chamferD });
    const boss = s.cylinder({ name: "boss", radius: bossD / 2, height: bossH, on: s.face(b, "top").offset(-sink) });
    s.join(b, boss);
    const rib = s.box({ name: "rib", width: ribThick, depth: ribTall, height: ribOut,
      on: s.face(b, "left").move(0, size / 2 - ribTall / 2 - 6).offset(-sink) });
    s.join(b, rib);
    s.hole(b, { diameter: bossHole, depth: bossH, at: og.frame({ origin: [0, 0, size + bossH - sink] }) });
    s.hole(b, { diameter: sideHole, depth: sideHoleDepth, at: s.grid({ nx: 2, px: sideHoleSpacing, on: s.face(b, "front").move(0, 5) }) });
    s.hole(b, { diameter: sideHole, depth: sideHoleDepth, at: s.grid({ nx: 2, px: sideHoleSpacing, on: s.face(b, "left").move(0, 5) }) });
    s.pocket(b, { profile: og.rect({ width: size - 2 * pocketMargin, height: size - 2 * pocketMargin }), depth: pocketDepth, at: s.face(b, "bottom") });
    s.material(b, { color: "#9a9a9a", metalness: 0.6, roughness: 0.4 });
    return b;
  }
}
