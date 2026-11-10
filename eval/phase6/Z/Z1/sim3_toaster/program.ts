export default function (og) {
  const s = og.model({ name: "toaster" });
  const W = 155.11, L = 290, footH = 4, H = 175; // housing width, length, height (above feet)
  const endY = -L / 2; // control end (front, -Y)
  const slotW = 34, slotL = 230, slotGap = 60, slotDepth = 115;
  const leverZ = footH + 116, ctrlZ = footH + 30;

  const housing = makeHousing();
  makeWires();
  makeFeet();

  // lever tab: slides down 59 mm
  const lever = makeLever();
  const leverF = og.frame({ origin: [0, endY, leverZ] });
  s.at(lever, leverF);
  s.joint(lever, { name: "lever_slide", parent: housing, kind: "prismatic", on: leverF, axis: "-Z", range: [0, 59] });

  // browning dial: turns
  const dial = makeDial();
  const dialF = og.frame({ origin: [-22, endY, ctrlZ] });
  s.at(dial, dialF);
  s.joint(dial, { name: "dial_turn", parent: housing, kind: "continuous", on: dialF, axis: "-Y" });

  // cancel button: pushes in 3 mm
  const button = makeButton();
  const btnF = og.frame({ origin: [28, endY, ctrlZ] });
  s.at(button, btnF);
  s.joint(button, { name: "cancel_push", parent: housing, kind: "prismatic", on: btnF, axis: "+Y", range: [0, 3] });
  return s;

  function endFrame(x, z) {
    return og.frame({ origin: [x, endY, z], normal: [0, -1, 0], up: [0, 0, 1] });
  }

  function makeHousing() {
    const sec = (w, l, r, z) => ({ profile: og.rect({ width: w, height: l, radius: r }), z });
    const h = s.loft({
      name: "housing",
      sections: [
        sec(146, 282, 36, 0),
        sec(W - 3, L - 3, 40, 8),
        sec(W, L, 42, 22),
        sec(W, L, 42, 120),
        sec(W - 4, L - 4, 40, 145),
        sec(W - 14, L - 10, 36, 160),
        sec(W - 28, L - 20, 30, 169),
        sec(W - 48, L - 34, 22, H),
      ],
      on: og.world.move(0, 0, footH),
    });
    const top = s.face(h, "top");
    s.pocket(h, { profile: og.rect({ width: slotW, height: slotL, radius: 3 }), depth: slotDepth, at: s.grid({ nx: 2, px: slotGap, on: top }) });
    s.pocket(h, { profile: og.rect({ width: 8, height: 80, radius: 4 }), depth: 15, at: endFrame(0, leverZ - 34) });
    s.pocket(h, { profile: og.rect({ width: 13, height: 11, radius: 1 }), depth: 9, at: endFrame(28, ctrlZ) });
    s.material(h, { color: "#8e9296", metalness: 0.85, roughness: 0.38 });
    return h;
  }

  function makeWires() {
    const zBot = footH + H - slotDepth + 1, hgt = slotDepth - 18;
    let first = null;
    for (const sx of [-1, 1]) for (const side of [-1, 1]) for (let i = 0; i < 5; i++) {
      const x = sx * slotGap / 2 + side * (slotW / 2 - 1.5);
      const y = -80 + i * 40;
      const c = s.cylinder({ name: first ? `wire_${sx}_${side}_${i}` : "wire_supports", radius: 1.1, height: hgt, segments: 8, on: og.world.move(x, y, zBot) });
      if (first) s.merge(first, c); else first = c;
    }
    s.material(first, { color: "#c8c8c8", metalness: 0.9, roughness: 0.3 });
    return first;
  }

  function makeFeet() {
    let first = null;
    for (const fx of [-1, 1]) for (const fy of [-1, 1]) {
      const f = s.cylinder({ name: first ? `foot_${fx}_${fy}` : "feet", radius: 8, height: footH + 0.5, segments: 16, on: og.world.move(fx * 50, fy * 110, 0) });
      if (first) s.merge(first, f); else first = f;
    }
    s.material(first, { color: "#1a1a1a", roughness: 0.9 });
    return first;
  }

  // built about its origin: the point on the end face where it rides
  function makeLever() {
    const gap = 0.3, out = 16.55 - gap;
    const tab = s.box({ name: "lever_tab", width: 40, depth: out, height: 12, on: og.world.move(0, -out / 2 - gap, -6) });
    s.fillet(tab, { radius: 2.5 });
    const stem = s.box({ name: "lever_stem", width: 6, depth: 13, height: 8, on: og.world.move(0, 5, -4) });
    s.join(tab, stem);
    s.material(tab, { color: "#151515", roughness: 0.5 });
    return s.group({ name: "lever", children: [tab] });
  }

  function makeDial() {
    const knob = s.cylinder({ name: "dial", radius: 14, height: 9, segments: 32, on: og.frame({ normal: [0, -1, 0], up: [0, 0, 1] }).offset(-1) });
    s.fillet(knob, { radius: 1.5, edges: "top" });
    const grip = s.box({ name: "dial_grip", width: 24, depth: 6, height: 6.5, on: s.face(knob, "top").offset(-1) });
    s.fillet(grip, { radius: 1.5, edges: "top" });
    s.join(knob, grip);
    s.material(knob, { color: "#151515", roughness: 0.5 });
    return s.group({ name: "dial_group", children: [knob] });
  }

  function makeButton() {
    const b = s.box({ name: "cancel_button", width: 12, depth: 12, height: 10, on: og.world.move(0, 0, -5) });
    s.fillet(b, { radius: 1, edges: "side" });
    s.material(b, { color: "#202020", roughness: 0.6 });
    return s.group({ name: "button", children: [b] });
  }
}
