export default function (og) {
  const s = og.model({ name: "panoramic_sunroof_cassette" });
  const length = 1260, width = 840; // x, y
  const travel = 40, panelX = 310;

  const frame = cassetteFrame(length, width);
  const cross = crossmember(width);
  const fixed = s.group({ name: "fixed_frame", children: [frame, cross] });

  const ff = og.frame({ origin: [panelX, 0, 0] });
  const front = panel("front_panel");
  s.at(front, ff);
  s.joint(front, { name: "front_panel_slide", parent: fixed, kind: "prismatic", on: ff, axis: "+X", range: [-travel, 0] });

  const rf = og.frame({ origin: [-panelX, 0, 0] });
  const rear = panel("rear_panel");
  s.at(rear, rf);
  s.joint(rear, { name: "rear_panel_slide", parent: fixed, kind: "prismatic", on: rf, axis: "+X", range: [0, travel] });
  return s;

  // Side rails, headers and guide rails: one steel solid, z = 0..34.
  function cassetteFrame(l, w, railW = 80, h = 30, guideW = 28, guideT = 4) {
    const yRail = w / 2 - railW / 2, xHead = l / 2 - railW / 2;
    const f = s.box({ name: "cassette_frame", width: l, depth: railW, height: h, on: og.world.move(0, yRail, 0) });
    const pieces = [
      s.box({ name: "right_side_rail", width: l, depth: railW, height: h, on: og.world.move(0, -yRail, 0) }),
      s.box({ name: "front_header", width: railW, depth: w, height: h, on: og.world.move(xHead, 0, 0) }),
      s.box({ name: "rear_header", width: railW, depth: w, height: h, on: og.world.move(-xHead, 0, 0) }),
    ];
    const yGuide = yRail - 34; // 346: guide inner edge 6 mm inboard of the rail
    for (const [n, y] of [["left_guide_rail", yGuide], ["right_guide_rail", -yGuide]]) {
      pieces.push(s.box({ name: n, width: l, depth: guideW, height: guideT + 0.5, on: og.world.move(0, y, h - 0.5) }));
    }
    for (const p of pieces) s.join(f, p);
    s.material(f, { name: "steel", color: "#2e3036", metalness: 0.8, roughness: 0.4 });
    return f;
  }

  function crossmember(w, bw = 60, h = 30) {
    const b = s.box({ name: "center_crossmember", width: bw, depth: w - 160, height: h });
    s.material(b, { name: "steel_dark", color: "#26292e", metalness: 0.8, roughness: 0.4 });
    return b;
  }

  // Built about its own origin (panel centre, z from the frame's base).
  function panel(name, cw = 460, cd = 740, gw = 440, gd = 700) {
    const carrier = s.box({ name: name + "_carrier", width: cw, depth: cd, height: 4, on: og.world.move(0, 0, 38) });
    const glass = s.box({ name: name + "_glass", width: gw, depth: gd, height: 6, on: og.world.move(0, 0, 42) });
    const anchor = s.box({ name: name + "_cable_anchor", width: 80, depth: 120, height: 8, on: og.world.move(180, 0, 32) });
    s.material(carrier, { name: "plastic", color: "#14171a", roughness: 0.7 });
    s.material(glass, { name: "glass", color: "#5c8599", opacity: 0.42, roughness: 0.05 });
    s.material(anchor, { name: "aluminium", color: "#707378", metalness: 1, roughness: 0.4 });
    const kids = [carrier, glass, anchor];
    for (const [corner, x, y] of [["front_left", 170, 346], ["front_right", 170, -346], ["rear_left", -170, 346], ["rear_right", -170, -346]]) {
      const shoe = s.box({ name: name + "_" + corner + "_shoe", width: 50, depth: 26, height: 4, on: og.world.move(x, y, 34) });
      s.material(shoe, { name: "shoe_plastic", color: "#1f1f21", roughness: 0.7 });
      kids.push(shoe);
    }
    return s.group({ name, children: kids });
  }
}
