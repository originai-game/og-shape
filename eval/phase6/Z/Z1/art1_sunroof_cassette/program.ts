export default function (og) {
  const s = og.model({ name: "sunroof_cassette" });
  const L = 1260, W = 840, H = 48;
  const frameH = 30, railW = 60, headerW = 60, crossW = 80;
  const guideW = 21, guideH = 34, guideY = W / 2 - railW - guideW / 2 + 1;
  const shoeH = 6, carrierT = 4, glassT = H - guideH - shoeH - carrierT; // 4
  const panelX = 325, carrierL = 510, carrierW = 2 * guideY, glassL = 500, glassW = 740;
  const travel = 40;

  const frame = cassetteFrame();

  const front = panel("front");
  const fF = og.world.move(-panelX, 0, guideH);
  s.at(front, fF);
  s.joint(front, { name: "front_panel_slide", parent: frame, kind: "prismatic", on: fF, axis: "-X", range: [0, travel] });

  const rear = panel("rear");
  const fR = og.world.move(panelX, 0, guideH);
  s.at(rear, fR);
  s.joint(rear, { name: "rear_panel_slide", parent: frame, kind: "prismatic", on: fR, axis: "+X", range: [0, travel] });
  return s;

  function cassetteFrame() {
    const inner = W - 2 * railW + 2;
    const f = s.box({ name: "frame", width: L, depth: railW, height: frameH, on: og.world.move(0, -(W / 2 - railW / 2), 0) });
    const parts = [
      s.box({ name: "side_rail_r", width: L, depth: railW, height: frameH, on: og.world.move(0, W / 2 - railW / 2, 0) }),
      s.box({ name: "front_header", width: headerW, depth: inner, height: frameH, on: og.world.move(-(L / 2 - headerW / 2), 0, 0) }),
      s.box({ name: "rear_header", width: headerW, depth: inner, height: frameH, on: og.world.move(L / 2 - headerW / 2, 0, 0) }),
      s.box({ name: "crossmember", width: crossW, depth: inner, height: frameH, on: og.world }),
      s.box({ name: "guide_rail_l", width: L - 2 * headerW + 2, depth: guideW, height: guideH, on: og.world.move(0, -guideY, 0) }),
      s.box({ name: "guide_rail_r", width: L - 2 * headerW + 2, depth: guideW, height: guideH, on: og.world.move(0, guideY, 0) }),
    ];
    for (const p of parts) s.join(f, p);
    s.material(f, { color: "#1d1f22", metalness: 0.6, roughness: 0.5 });
    return f;
  }

  // Built about its own origin: centre of the panel, z = 0 on the guide-rail tops.
  function panel(tag) {
    const sign = tag === "front" ? -1 : 1;
    const carrier = s.box({ name: `${tag}_carrier`, width: carrierL, depth: carrierW, height: carrierT, on: og.world.move(0, 0, shoeH) });
    s.material(carrier, { color: "#2a2c30", roughness: 0.7 });
    const shoes = [];
    for (const [i, f] of s.grid({ nx: 2, ny: 2, px: carrierL - 80, py: 2 * guideY }).entries()) {
      const shoe = s.box({ name: `${tag}_shoe_${i + 1}`, width: 40, depth: 16, height: shoeH + 0.5, on: f });
      s.material(shoe, { color: "#111111", roughness: 0.8 });
      shoes.push(shoe);
    }
    // Cable anchor: a block under the carrier's outer end, inboard of the left guide rail.
    const anchor = s.box({ name: `${tag}_cable_anchor`, width: 30, depth: 16, height: 8.5, on: og.world.move(sign * (carrierL / 2 - 30), -(guideY - guideW / 2 - 12), shoeH - 8) });
    s.material(anchor, { color: "#3a3a3a", metalness: 0.7, roughness: 0.4 });
    const glass = s.box({ name: `${tag}_glass`, width: glassL, depth: glassW, height: glassT, on: og.world.move(0, 0, shoeH + carrierT) });
    s.fillet(glass, { radius: 10, edges: "side" });
    s.material(glass, { color: "#3c5a6b", metalness: 0.1, roughness: 0.1, opacity: 0.85 });
    return s.group({ name: `${tag}_panel`, children: [carrier, ...shoes, anchor, glass] });
  }
}
