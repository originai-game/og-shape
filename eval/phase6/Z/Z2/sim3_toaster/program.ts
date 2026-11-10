export default function (og) {
  const s = og.model({ name: "toaster" });
  const footH = 5, footSink = 1;
  const baseZ = footH - footSink;
  const bodyH = 179 - baseZ;
  const kw = 0.9843, dl = 0.8;             // fit to 155.11 x 306.55
  const slotW = 28, slotL = 230, slotDepth = 140, slotPitch = 64;
  const leverTopZ = 150, leverTravel = 59, tabDepth = 10, tabClear = 1.5;
  const dialZ = 45, buttonX = -36, buttonZ = 100;

  const housing = makeHousing();
  makeRacks();
  makeFeet();

  const endAt = (x, z) => s.snap(housing, { points: [[x, 300, z]], direction: [0, -1, 0] })[0].origin;

  // the end bulges out lower down: the tab clears the surface over its whole travel
  const leverEnd = endAt(0, leverTopZ);
  const lowEnd = endAt(0, leverTopZ - leverTravel - 5);
  const outY = Math.max(leverEnd[1], lowEnd[1]);
  const slotCz = leverTopZ - leverTravel / 2;
  s.pocket(housing, { profile: og.rect({ width: 6, height: leverTravel + 16, radius: 3 }), depth: 25,
    at: og.frame({ origin: [0, outY + 5, slotCz], normal: [0, 1, 0], up: [0, 0, 1] }) });
  const btnEnd = endAt(buttonX, buttonZ);
  s.pocket(housing, { profile: og.circle({ radius: 6.5, segments: 16 }), depth: 15,
    at: og.frame({ origin: [buttonX, btnEnd[1] + 5, buttonZ], normal: [0, 1, 0], up: [0, 0, 1] }) });

  const lever = makeLever(outY - leverEnd[1] + tabClear);
  const fL = og.frame({ origin: [0, leverEnd[1], leverTopZ] });
  s.at(lever, fL);
  s.joint(lever, { name: "lever_slide", parent: housing, kind: "prismatic", on: fL, axis: "-Z", range: [0, leverTravel] });

  const dial = makeDial();
  const dEnd = endAt(0, dialZ);
  const fD = og.frame({ origin: [0, dEnd[1] - 0.5, dialZ], normal: [0, 1, 0], up: [0, 0, 1] });
  s.at(dial, fD);
  s.joint(dial, { name: "dial_turn", parent: housing, kind: "continuous", on: fD, axis: "+Y" });

  const button = makeButton();
  const fB = og.frame({ origin: [buttonX, btnEnd[1], buttonZ], normal: [0, 1, 0], up: [0, 0, 1] });
  s.at(button, fB);
  s.joint(button, { name: "cancel_push", parent: housing, kind: "prismatic", on: fB, axis: "-Y", range: [0, 3] });

  s.material([lever, dial, button], { color: "#1c1c1e", roughness: 0.5 });
  return s;

  function makeHousing() {
    const sec = (w, l, r, z) => ({ profile: og.rect({ width: w * kw, height: l + dl, radius: r }), z });
    const h = s.loft({ name: "housing", on: og.world.move(0, 0, baseZ), steps: 6, curve: "spline", sections: [
      sec(146, 284, 30, 0),
      sec(155, 290, 34, 45),
      sec(150, 288, 32, 120),
      sec(134, 272, 26, bodyH - 12),
      sec(124, 262, 22, bodyH - 3),
      sec(112, 250, 16, bodyH),
    ] });
    s.pocket(h, { profile: og.rect({ width: slotW, height: slotL, radius: 4 }), depth: slotDepth,
      at: [-1, 1].map(k => og.world.move(k * slotPitch / 2, 0, 179)) });
    s.material(h, { color: "#9a9da1", metalness: 1, roughness: 0.35 });
    return h;
  }

  function makeRacks() {
    const z0 = 179 - slotDepth - 1, z1 = 165;
    let first = null;
    for (const sx of [-1, 1]) for (const side of [-1, 1]) for (let i = 0; i < 7; i++) {
      const x = sx * slotPitch / 2 + side * (slotW / 2 - 1.6);
      const y = -90 + i * 30;
      const w = s.wire({ name: first ? `rack_${sx}_${side}_${i}`.replace(/-/g, "m") : "bread_racks",
        path: [[x, y, z0], [x, y, z1]], diameter: 2, segments: 8 });
      if (first) s.merge(first, w); else first = w;
    }
    s.material(first, { color: "#c8c8c8", metalness: 1, roughness: 0.3 });
    return first;
  }

  function makeFeet() {
    let first = null;
    for (const [x, y] of [[-55, -115], [55, -115], [-55, 115], [55, 115]]) {
      const f = s.cylinder({ name: first ? `foot_${x}_${y}`.replace(/-/g, "m") : "feet", radius: 8, height: footH, segments: 16, on: og.world.move(x, y, 0) });
      if (first) s.merge(first, f); else first = f;
    }
    s.material(first, { color: "#111111", roughness: 0.9 });
    return first;
  }

  // origin on the slot at the top position; tab's back face at `back` (+Y)
  function makeLever(back) {
    const tab = s.box({ name: "lever_tab", width: 40, depth: tabDepth, height: 10, on: og.world.move(0, back + tabDepth / 2, -5) });
    s.fillet(tab, { radius: 2 });
    const stemLen = back + 10;
    const stem = s.box({ name: "lever_stem", width: 4, depth: stemLen, height: 6, on: og.world.move(0, back + 1 - stemLen / 2, -3) });
    s.join(tab, stem);
    return s.group({ name: "lever", children: [tab] });
  }

  function makeDial() {
    const knob = s.cylinder({ name: "dial_knob", radius: 14, height: 9, segments: 32 });
    s.fillet(knob, { radius: 1.5, edges: "top" });
    const ptr = s.box({ name: "dial_pointer", width: 5, depth: 26, height: 5, on: og.world.move(0, 0, 8.5) });
    s.fillet(ptr, { radius: 1.5, edges: "top" });
    s.join(knob, ptr);
    return s.group({ name: "dial", children: [knob] });
  }

  function makeButton() {
    const b = s.cylinder({ name: "cancel_button", radius: 5.5, height: 10, segments: 20, on: og.world.move(0, 0, -5) });
    s.fillet(b, { radius: 1.5, edges: "top" });
    return s.group({ name: "cancel", children: [b] });
  }
}
