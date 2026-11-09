export default function (og) {
  const s = og.model({ name: "toaster" });
  const width = 155, length = 285, height = 175, foot = 4; // x, y, z

  const shell = housing(width, length, height);
  s.at(shell, og.world.move(0, 0, foot));
  feet(shell);
  const wires = supports(shell);
  const endY = s.bbox(shell, "-Y").origin[1];

  // controls on the -Y end, at rest
  const leverPivot = og.frame({ origin: [0, endY + 3, 140] });
  const lever = leverTab();
  s.at(lever, leverPivot);
  s.joint(lever, { name: "lever_slide", parent: shell, kind: "prismatic", on: leverPivot, axis: "-Z", range: [0, 59] });

  const dialPivot = og.frame({ origin: [0, endY + 3, 40], normal: [0, -1, 0], up: [0, 0, 1] });
  const d = dial();
  s.at(d, dialPivot);
  s.joint(d, { name: "dial_turn", parent: shell, kind: "continuous", on: dialPivot, axis: "-Y" });

  const buttonPivot = og.frame({ origin: [42, endY + 3, 40], normal: [0, -1, 0], up: [0, 0, 1] });
  const b = button();
  s.at(b, buttonPivot);
  s.joint(b, { name: "cancel_push", parent: shell, kind: "prismatic", on: buttonPivot, axis: "+Y", range: [0, 3] });

  s.material([lever, d, b], { color: "#1e1e1e", roughness: 0.6 });
  return s;

  function housing(w, l, h, slotW = 30, slotL = 225, slotGap = 22, slotDepth = 140) {
    const body = s.loft({
      name: "housing",
      sections: [
        { profile: og.rect({ width: w, height: l, radius: 30 }), z: 0 },
        { profile: og.rect({ width: w - 4, height: l - 4, radius: 32 }), z: h * 0.35 },
        { profile: og.rect({ width: w - 30, height: l - 10, radius: 24 }), z: h },
      ],
    });
    s.fillet(body, { radius: 14, edges: "top" });
    const top = s.face(body, "top");
    for (const f of s.grid({ nx: 2, px: slotW + slotGap, on: top }))
      s.pocket(body, { profile: og.rect({ width: slotW, height: slotL, radius: 4 }), depth: slotDepth, at: f });
    // groove the lever runs in
    const groove = s.box({ name: "lever_groove", width: 6, depth: 20, height: 80, on: og.world.move(0, -l / 2, 70) });
    s.cut(body, groove);
    s.material(body, { color: "#9a9da0", metalness: 0.3, roughness: 0.45 });
    return body;
  }

  function feet(host, r = 8) {
    let k = 0;
    for (const f of s.grid({ nx: 2, ny: 2, px: width - 50, py: length - 60, on: og.world.move(0, 0, 0) })) {
      const ft = s.cylinder({ name: `foot_${k++}`, radius: r, height: foot + 0.5, on: og.frame({ origin: f.origin }) });
      s.material(ft, { color: "#111111", roughness: 0.9 });
    }
  }

  function supports(host, slotW = 30, slotGap = 22) {
    let rods = null, n = 0;
    const top = s.bbox(host, "+Z").origin[2];
    for (const sx of [-1, 1]) {
      for (const f of s.grid({ nx: 1, ny: 6, py: 36, on: og.world.move(sx * (slotW + slotGap) / 2, 0, top - 140) })) {
        for (const side of [-1, 1]) {
          const rod = s.cylinder({ name: `rod_${n++}`, radius: 1.2, height: 120, on: og.frame({ origin: [f.origin[0] + side * (slotW / 2 - 3), f.origin[1], f.origin[2]] }) });
          if (rods) s.join(rods, rod); else rods = rod;
        }
      }
    }
    s.material(rods, { color: "#c8c8c8", metalness: 1, roughness: 0.3 });
    return rods;
  }

  function leverTab() {
    const stem = s.box({ name: "lever_stem", width: 5, depth: 12, height: 8, on: og.world.move(0, 0, -4) });
    const tab = s.box({ name: "lever", width: 42, depth: 22, height: 12, on: og.world.move(0, -16, -6) });
    s.join(tab, stem);
    return s.group({ name: "lever_tab", children: [tab] });
  }

  function dial() {
    const knob = s.cylinder({ name: "dial", radius: 15, height: 12 });
    const grip = s.box({ name: "dial_grip", width: 6, depth: 30, height: 8, on: og.world.move(0, 0, 11.5) });
    s.join(knob, grip);
    return knob;
  }

  function button() {
    const btn = s.box({ name: "cancel_button", width: 18, depth: 10, height: 8 });
    return btn;
  }
}
