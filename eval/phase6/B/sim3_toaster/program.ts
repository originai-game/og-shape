export default function (og) {
  const s = og.model({ name: "toaster" });
  // size: 155 x 306.5 x 179 (x, y, z); long axis along y, controls on the -Y end
  const footH = 5, bodyH = 179, bodyD = 282, bodyW = 155;
  const halfD = bodyD / 2;
  const slotW = 34, slotL = 230, slotDepth = 140, slotX = 28;
  const leverZ = 140, leverTravel = 59;

  const housing = body();
  const feet = feetGroup();
  const dial = browningDial(endFrame(-22, 40));
  const button = cancelButton(endFrame(30, 40));
  const lever = leverTab(endFrame(0, leverZ));

  s.joint(dial, { name: "dial_turn", parent: housing, kind: "continuous", on: endFrame(-22, 40), axis: "-Y" });
  s.joint(button, { name: "button_push", parent: housing, kind: "prismatic", on: endFrame(30, 40), axis: "+Y", range: [0, 3] });
  s.joint(lever, { name: "lever_slide", parent: housing, kind: "prismatic", on: endFrame(0, leverZ), axis: "-Z", range: [0, leverTravel] });
  return s;

  // a frame on the -Y end face, +z out of it, x = world +X
  function endFrame(x, z) {
    return og.frame({ origin: [x, -halfD, z], normal: [0, -1, 0], up: [0, 0, 1] });
  }

  function body() {
    const sec = (w, d, r, z) => ({ profile: og.rect({ width: w, height: d, radius: r }), z });
    const h = s.loft({
      name: "housing",
      on: og.world.move(0, 0, footH - 1),
      sections: [
        sec(bodyW - 16, bodyD - 10, 30, 0),
        sec(bodyW - 2, bodyD - 2, 36, 12),
        sec(bodyW, bodyD, 38, 35),
        sec(bodyW - 12, bodyD, 38, 145),
        sec(bodyW - 24, bodyD - 8, 34, 166),
        sec(bodyW - 40, bodyD - 24, 26, bodyH - footH + 1),
      ],
    });
    const top = s.face(h, "top");
    s.pocket(h, { profile: og.rect({ width: slotW, height: slotL, radius: 4 }), depth: slotDepth, at: [top.move(-slotX, 0), top.move(slotX, 0)] });
    // lever guide slot on the end face
    s.pocket(h, { profile: og.rect({ width: 8, height: leverTravel + 14, radius: 4 }), depth: 8, at: endFrame(0, leverZ - leverTravel / 2).offset(0.5) });
    // wire supports: rods on each long slot wall
    const floorZ = 179 - slotDepth;
    for (const sx of [-slotX, slotX]) {
      for (const side of [-1, 1]) {
        const wx = sx + side * (slotW / 2 - 0.5);
        for (const f of s.grid({ nx: 6, px: 36, on: og.world.move(wx, 0, floorZ - 1).turn(0, 0, og.deg(90)) })) {
          const rod = s.box({ name: "wire", width: 2, depth: 2, height: slotDepth - 25, on: f });
          s.join(h, rod);
        }
      }
    }
    s.material(h, { name: "brushed_steel", color: "#8c8f93", metalness: 0.8, roughness: 0.45 });
    return h;
  }

  function feetGroup() {
    const feet = [];
    const g = s.grid({ nx: 2, ny: 2, px: bodyW - 50, py: bodyD - 70, on: og.world });
    let i = 0;
    for (const f of g) feet.push(s.cylinder({ name: `foot_${i++}`, radius: 9, height: footH, on: f }));
    const grp = s.group({ name: "feet", children: feet });
    s.material(grp, { name: "rubber", color: "#1c1c1c", roughness: 0.9 });
    return grp;
  }

  function browningDial(f) {
    const knob = s.cylinder({ name: "dial", radius: 14, height: 9, on: f.offset(-1) });
    const grip = s.box({ name: "dial_grip", width: 26, depth: 7, height: 8, on: f.offset(7) });
    s.join(knob, grip);
    s.material(knob, { name: "black_plastic", color: "#18181a", roughness: 0.5 });
    return knob;
  }

  function cancelButton(f) {
    const b = s.box({ name: "cancel_button", width: 14, depth: 9, height: 9, on: f.offset(-1) });
    s.fillet(b, { radius: 2, edges: "top" });
    s.material(b, { name: "black_plastic", color: "#18181a", roughness: 0.5 });
    return b;
  }

  function leverTab(f) {
    const stem = s.box({ name: "lever", width: 6, depth: 6, height: 14, on: f.offset(-6) });
    const tab = s.box({ name: "lever_tab", width: 40, depth: 14, height: 17.5, on: f.offset(7) });
    s.fillet(tab, { radius: 3, edges: "top" });
    s.join(stem, tab);
    s.material(stem, { name: "black_plastic", color: "#18181a", roughness: 0.5 });
    return stem;
  }
}
