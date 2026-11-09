export default function (og) {
  const s = og.model({ name: "telescoping_slide" });
  // Wall face at x = 0; the slide runs out toward -X along the axis y = 0, z = axisZ.
  const plateT = 20, plateW = 240, plateH = 300;
  const axisZ = plateH / 2;
  const standoff = 40;                              // wall to sleeve
  const outerL = 500, outerW = 60, outerH = 50, wall = 4;
  const midL = 500, midOut = 160;                   // middle sticks out of the sleeve at rest
  const midW = outerW - 2 * wall - 2, midH = outerH - 2 * wall - 2;
  const innerL = 560, innerS = 14, block = 24, innerOut = 356;
  const midTravel = 260, innerTravel = 220;

  const plate = wallPlate();
  const screws = screwHeads();
  const outer = outerSleeve();
  const base = s.group({ name: "base", children: [plate, screws, outer] });

  const midFrame = og.frame({ origin: [-standoff - outerL - midOut, 0, axisZ - midH / 2] });
  const mid = middle();
  s.at(mid, midFrame);
  s.joint(mid, { name: "middle_slide", kind: "prismatic", parent: base, on: midFrame, axis: "-X", range: [0, midTravel] });

  const innerFrame = og.frame({ origin: [midFrame.origin[0] - innerOut, 0, axisZ - innerS / 2] });
  const inner = innerBlade();
  s.at(inner, innerFrame);
  s.joint(inner, { name: "inner_slide", kind: "prismatic", parent: mid, on: innerFrame, axis: "-X", range: [0, innerTravel] });
  return s;

  function wallPlate() {
    const p = s.box({ name: "wall_plate", width: plateT, depth: plateW, height: plateH, on: og.world.move(plateT / 2, 0, 0) });
    s.material(p, { color: "#b9b4a4", roughness: 0.8 });
    return p;
  }

  function screwHeads() {
    const heads = [];
    for (const z of [axisZ + 100, axisZ - 100]) {
      heads.push(s.cylinder({ name: z > axisZ ? "screw_heads" : "screw_low", radius: 10, height: 6,
        on: og.frame({ origin: [0, 0, z], normal: [-1, 0, 0] }) }));
    }
    s.join(heads[0], heads[1]);
    s.material(heads[0], { color: "#6b6e70", metalness: 0.9, roughness: 0.4 });
    return heads[0];
  }

  function outerSleeve() {
    const cx = -standoff - outerL / 2;
    const o = s.box({ name: "outer_sleeve", width: outerL, depth: outerW, height: outerH, on: og.world.move(cx, 0, axisZ - outerH / 2) });
    const bore = s.box({ name: "outer_bore", width: outerL + 2, depth: outerW - 2 * wall, height: outerH - 2 * wall,
      on: og.world.move(cx - 1, 0, axisZ - outerH / 2 + wall) });
    s.cut(o, bore);
    const slot = s.box({ name: "outer_slot", width: outerL + 2, depth: wall * 3, height: outerH / 3,
      on: og.world.move(cx - 1, -outerW / 2, axisZ - outerH / 6) });
    s.cut(o, slot); // the C-channel's opening on the front side
    for (const dz of [-1, 1]) {
      const st = s.box({ name: dz < 0 ? "standoff_low" : "standoff_high", width: standoff + 1, depth: 30, height: 12,
        on: og.world.move(-standoff / 2 - 0.5, 0, axisZ + dz * (outerH / 2 - 8) - 6) });
      s.join(o, st);
    }
    s.material(o, { color: "#9a9da0", metalness: 0.8, roughness: 0.4 });
    return o;
  }

  // Origin at its outer (-X) end, running along +X.
  function middle() {
    const m = s.box({ name: "middle_section", width: midL, depth: midW, height: midH, on: og.world.move(midL / 2, 0, 0) });
    const bore = s.box({ name: "middle_bore", width: midL + 2, depth: midW - 2 * wall, height: midH - 2 * wall,
      on: og.world.move(midL / 2, 0, wall) });
    s.cut(m, bore);
    s.material(m, { color: "#7d8083", metalness: 0.8, roughness: 0.4 });
    return s.group({ name: "middle", children: [m] });
  }

  // Origin at the blade's outer end; the end block sits beyond it in -X.
  function innerBlade() {
    const b = s.box({ name: "inner_blade", width: innerL, depth: innerS, height: innerS, on: og.world.move(innerL / 2 - 0.5, 0, 0) });
    const e = s.box({ name: "end_block", width: block, depth: block, height: block,
      on: og.world.move(-block / 2, 0, innerS / 2 - block / 2) });
    s.join(b, e);
    s.material(b, { color: "#4a4d50", metalness: 0.8, roughness: 0.4 });
    return s.group({ name: "inner", children: [b] });
  }
}
