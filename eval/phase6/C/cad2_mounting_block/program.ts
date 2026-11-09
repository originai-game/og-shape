export default function (og) {
  const s = og.model({ name: "mounting_block" });
  const side = 80, edgeChamfer = 2;
  const bossD = 30, bossH = 30, bossHole = 6;
  const ribX = 20, ribY = 4, ribZ = 12;
  const holeD = 5, holeDepth = 15;
  const margin = 12, pocketDepth = 20;
  const sink = 0.5;

  const block = body();
  s.material(block, { color: "#9a9ea3", metalness: 0.8, roughness: 0.4 });
  return s;

  function body() {
    const b = s.box({ name: "block", width: side, depth: side, height: side });
    s.chamfer(b, { distance: edgeChamfer });
    const top = s.face(b, "top");
    const left = s.face(b, "left");
    const front = s.face(b, "front");
    const bottom = s.face(b, "bottom");

    const boss = s.cylinder({ name: "boss", radius: bossD / 2, height: bossH, on: top.offset(-sink) });
    const bossTop = s.face(boss, "top");
    // rib on the -X side, near the top: height runs out of the face (x), width along -Y, depth along z
    const rib = s.box({ name: "rib", width: ribY, depth: ribZ, height: ribX, on: left.move(0, side / 2 - 2 * ribZ).offset(-sink) });
    s.join(b, boss);
    s.join(b, rib);

    s.hole(b, { diameter: bossHole, at: bossTop, depth: bossH });
    s.hole(b, { diameter: holeD, depth: holeDepth, at: [
      left.move(-20, 0), left.move(10, 12),
      front.move(-15, 5), front.move(15, 5),
    ] });
    s.pocket(b, { profile: og.rect({ width: side - 2 * margin, height: side - 2 * margin }), depth: pocketDepth, at: bottom });
    return b;
  }
}
