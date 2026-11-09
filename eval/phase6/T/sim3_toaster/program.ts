export default function (og) {
  const s = og.model({ name: "toaster" });
  // Overall: housing 154 x 280 x 179, controls on the +Y end.
  const w = 154, d = 280, h = 179;

  const housing = body(w, d, h);

  // Browning dial: turns about +Y. Pivot on the dial's axis.
  const dialPivot = og.frame({ origin: [0, 138, 48.5] });
  const knob = dial();
  s.at(knob, dialPivot);
  s.joint(knob, { name: "dial_spin", parent: housing, kind: "continuous", on: dialPivot, axis: "+Y" });

  // Cancel button: pushes in 3 mm along -Y.
  const buttonPivot = og.frame({ origin: [-31.5, 135.5, 39] });
  const button = cancelButton();
  s.at(button, buttonPivot);
  s.joint(button, { name: "cancel_button_push", parent: housing, kind: "prismatic", on: buttonPivot, axis: "+Y", range: [-3, 0] });

  // Lever tab: slides down 59 mm.
  const leverPivot = og.frame({ origin: [0, 135.5, 140] });
  const tab = lever();
  s.at(tab, leverPivot);
  s.joint(tab, { name: "lever_slide", parent: housing, kind: "prismatic", on: leverPivot, axis: "+Z", range: [-59, 0] });

  return s;

  function body(w, d, h, floor = 8, t = 2, slotW = 44, slotD = 158, slotY = -8, pitch = 52, bottom = 30, wirePitch = 26, trayW = 68, trayD = 194, channelH = 99) {
    // Rounded housing narrowing towards the top: (z, width, depth).
    const levels = [[floor, w - 6, d - 6], [25, w, d], [68, w - 2 * t, d - 3], [137, 143, 271], [162, 140, 255], [175, 120, 224], [h, 103, 205]];
    const skin = s.loft({
      name: "housing",
      sections: levels.map(([z, sw, sd]) => ({ profile: og.rect({ width: sw, height: sd, radius: 0.18 * sw }), z })),
    });
    s.shell(skin, { thickness: t });

    const base = s.box({ name: "base_floor", width: 103, depth: d - 2 * t, height: 2 * t, on: og.world.move(0, 0, bottom - floor - t) });
    const tray = s.extrude({ name: "bottom_tray", profile: og.rect({ width: trayW, height: trayD, radius: floor }), height: bottom - 2 * floor, on: og.world.move(0, 0, floor) });
    for (const y of [-pitch, pitch]) {
      s.pocket(tray, { profile: og.slot({ length: bottom - t, width: 2 * t }), depth: 3 * t, at: s.grid({ nx: 2, ny: floor, px: trayW / 2, py: floor, on: s.face(tray, "bottom").move(0, y) }) });
    }
    const guide = s.box({ name: "lever_guide", width: 3 * t, depth: (d - slotD) / 2, height: channelH - 3 * t, on: og.world.move(0, (d + slotD) / 4, channelH - bottom) });
    s.material([base, tray, guide], { name: "steel", color: "#656565", metalness: 1, roughness: 0.5 });

    const liners = [];
    const wires = [];
    [-pitch / 2, pitch / 2].forEach((x, i) => {
      const liner = s.box({ name: `slot_liner_${i}`, width: slotW + t, depth: slotD, height: h - bottom, on: og.world.move(x, slotY, bottom) });
      const opening = s.extrude({ name: `slot_opening_${i}`, profile: og.rect({ width: slotW - t, height: slotD - 6 * t, radius: t }), height: h, on: og.world.move(x, slotY, bottom + t) });
      s.cut(skin, s.copy(opening, `slot_opening_copy_${i}`));
      s.cut(liner, opening);
      s.material(liner, { name: "steel", color: "#55575a", metalness: 1, roughness: 0.5 });
      liners.push(liner);
      let j = 0;
      for (const f of s.grid({ nx: 2, ny: 5, px: bottom, py: wirePitch, on: og.world.move(x, slotY, 0) })) {
        const wire = s.sweep({
          name: `bread_support_${i}_${j++}`,
          profile: og.circle({ radius: 1, segments: 8 }),
          path: [[0, 0, bottom], [0, 0, h - 16], [2 * t, 0, h - 10]],
          on: og.frame({ origin: f.origin }),
        });
        s.material(wire, { name: "chrome", color: "#9a9a9a", metalness: 1, roughness: 0.2 });
        wires.push(wire);
      }
    });

    const feet = [];
    let k = 0;
    for (const f of s.grid({ nx: 2, ny: 2, px: 116, py: 219 })) {
      const foot = s.cylinder({ name: `foot_${k++}`, radius: 7, height: floor + t, on: og.frame({ origin: f.origin }) });
      s.material(foot, { name: "rubber", color: "#252525", roughness: 0.9 });
      feet.push(foot);
    }
    s.material(skin, { name: "brushed_steel", color: "#b9babc", metalness: 1, roughness: 0.4 });
    return s.group({ name: "body", children: [skin, ...liners, ...feet, ...wires, base, tray, guide] });
  }

  // Built about its axis: the dial turns about its own y axis at the origin.
  function dial(dw = 31, dd = 26, rim = 3) {
    // dial origin is the joint at world y = 138; the rim face sits at y = 130.
    const axis = (y) => og.frame({ origin: [0, y, 0], normal: [0, 1, 0], up: [0, 0, 1] });
    const knob = s.cylinder({ name: "dial_rim", radius: dw / 2, height: dd / 2, on: axis(-8) });
    const grip = s.extrude({ name: "dial_grip", profile: og.rect({ width: dw - rim, height: dw / 3, radius: rim }), height: dd / 2 + rim, on: axis(-8 + dd / 2 - rim) });
    s.join(knob, grip);
    s.material(knob, { name: "plastic", color: "#45474a", roughness: 0.6 });
    return s.group({ name: "dial", children: [knob] });
  }

  function cancelButton(bw = 17, bd = 13, bh = 6) {
    const b = s.extrude({ name: "cancel_button", profile: og.rect({ width: bw, height: bd, radius: bh / 3 }), height: bh });
    s.material(b, { name: "plastic", color: "#45474a", roughness: 0.6 });
    return s.group({ name: "cancel_button_group", children: [b] });
  }

  function lever(lw = 44, ld = 61, lh = 12, tabD = 29, bevel = 5, stemW = 6, recess = 1) {
    const tab = s.extrude({ name: "lever_tab", profile: og.rect({ width: lw, height: tabD, radius: bevel + recess }), height: lh, on: og.world.move(0, (ld - tabD) / 2, 0) });
    s.chamfer(tab, { distance: bevel, edges: "top" });
    s.pocket(tab, { profile: og.rect({ width: lw - 2 * bevel, height: tabD - 2 * bevel, radius: recess }), depth: recess, at: s.face(tab, "top") });
    const stem = s.box({ name: "lever_stem", width: stemW, depth: ld - tabD + bevel, height: lh, on: og.world.move(0, -tabD / 2 + bevel / 2, 0) });
    s.join(tab, stem);
    s.material(tab, { name: "plastic", color: "#414346", roughness: 0.6 });
    return s.group({ name: "lever", children: [tab] });
  }
}
