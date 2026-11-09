export default function (og) {
  const s = og.model({ name: "sunroof_cassette" });
  // overall size: 1260 x 840 x 48, length along x, car front toward -X
  const length = 1260, width = 840, frameH = 30;
  const rail = 60, cross = 80;                  // border and crossmember widths
  const openX = (length - 2 * rail - cross) / 2; // 520 per opening in x
  const openY = width - 2 * rail;               // 720
  const panelX = cross / 2 + openX / 2;         // 300: panel centre from x = 0
  const carrierH = 12, glassT = 6;              // 30 + 12 + 6 = 48
  const travel = 40;

  const cassette = frame();
  const front = panel("front", -1);
  const rear = panel("rear", +1);
  const fFront = og.frame({ origin: [-panelX, 0, frameH] });
  const fRear = og.frame({ origin: [panelX, 0, frameH] });
  s.at(front, fFront);
  s.at(rear, fRear);
  s.joint(front, { name: "front_slide", parent: cassette, kind: "prismatic", on: fFront, axis: "-X", range: [0, travel] });
  s.joint(rear, { name: "rear_slide", parent: cassette, kind: "prismatic", on: fRear, axis: "+X", range: [0, travel] });
  return s;

  function frame(guideW = 16, guideH = 12) {
    const f = s.box({ name: "frame", width: length, depth: width, height: frameH });
    for (const sx of [-1, 1]) {
      const hole = s.box({ name: sx < 0 ? "front_opening" : "rear_opening", width: openX, depth: openY, height: frameH + 2,
        on: og.world.move(sx * panelX, 0, -1) });
      s.cut(f, hole);
    }
    for (const sy of [-1, 1]) {
      const g = s.box({ name: sy < 0 ? "left_guide" : "right_guide", width: length - 2 * rail + 2, depth: guideW, height: guideH,
        on: og.world.move(0, sy * (openY / 2 - guideW / 2 + 1), 0) });
      s.join(f, g);
    }
    s.material(f, { name: "steel", color: "#2a2c30", metalness: 0.8, roughness: 0.45 });
    return f;
  }

  // Built about its own origin: the panel centre on the frame's top face.
  function panel(name, dir, border = 30) {
    const cw = openX + 2 * border - 10, cd = openY + 2 * border - 10; // 570 x 770
    const carrier = s.extrude({ name: `${name}_carrier`, height: carrierH,
      profile: og.subtract(og.rect({ width: cw, height: cd }), og.rect({ width: cw - 2 * border, height: cd - 2 * border })) });
    const shoeY = openY / 2 - 8 + 1; // over the guide rails
    for (const sx of [-1, 1]) for (const sy of [-1, 1]) {
      const shoe = s.box({ name: `${name}_shoe`, width: 40, depth: 14, height: frameH - 12 + 1,
        on: og.world.move(sx * (cw / 2 - border - 40), sy * shoeY, -(frameH - 12)) });
      s.join(carrier, shoe);
    }
    // the shoes stand inside the carrier ring's inner edge; the anchor hangs at the outer end
    const anchor = s.box({ name: `${name}_anchor`, width: 24, depth: 30, height: 11,
      on: og.world.move(dir * (cw / 2 - border / 2), 0, -10) });
    s.join(carrier, anchor);
    s.material(carrier, { name: "carrier", color: "#151515", roughness: 0.7 });
    const glass = s.box({ name: `${name}_glass`, width: cw - 10, depth: cd - 10, height: glassT, on: og.world.move(0, 0, carrierH) });
    s.fillet(glass, { radius: 2, edges: "top" });
    s.material(glass, { name: "glass", color: "#3d5866", metalness: 0.2, roughness: 0.1, opacity: 0.85 });
    return s.group({ name: `${name}_panel`, children: [carrier, glass] });
  }
}
