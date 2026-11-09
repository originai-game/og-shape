export default function (og) {
  const s = og.model({ name: "lever_mechanism" });

  const block = supportBlock();
  s.at(block, og.world.move(-0.02, -0.05, 14.95).turn(0, 0, og.deg(31.09)));
  const base = s.group({ name: "base", children: [block] });

  const knob = leverKnob();
  const pivot = og.frame({ origin: [-0.02, -0.05, 98.87] }); // top of the knob, at rest
  s.at(knob, pivot);
  s.joint(knob, { name: "knob_slide", parent: base, kind: "prismatic", on: pivot, axis: "+Z", range: [-19.1, 0] });
  s.joint(knob, { name: "knob_spin", parent: base, kind: "continuous", on: pivot, axis: "+Z" });
  return s;

  // Hexagonal block centred on its origin (both sides), square recess on top, bore underneath.
  function supportBlock(d = 57.74, h = 30, opening = 10.1, boreD = 20, boreDepth = 6) {
    const b = s.extrude({ name: "support_block", profile: og.polygon({ sides: 6, radius: d / 2 }), height: h, bothSides: true });
    s.pocket(b, { profile: og.rect({ width: opening, height: opening }), depth: opening, at: s.face(b, "top") });
    s.hole(b, { diameter: boreD, depth: boreDepth, at: s.face(b, "bottom") });
    s.material(b, { color: "#8a8f96", roughness: 0.6 });
    return b;
  }

  // Stepped hexagonal knob; its origin is its top centre (the joint frame).
  function leverKnob(w = 57.76, h = 79.87, lug = 9.8) {
    const shift = -39.92;          // part centre below the pivot
    const bottom = -h / 2;
    const lugTop = bottom + lug;
    const bands = [
      [lugTop, -18, w / 2, 28],
      [-18, -10, 28, 26],
      [-10, 0, 26, 21],
      [0, 10, 21, 18.2],
      [10, 20, 18.2, 17.5],
      [20, 30, 17.5, 10],
      [30, 35, 10, 8.5],
      [35, 38, 8.5, 7],
      [38, h / 2, 7, 4.5],
    ];
    const turned = (z) => og.world.move(0, 0, z + shift).turn(0, 0, og.deg(-90));
    // The bands are contiguous: one loft through every band edge, linear between sections.
    const sections = [{ profile: og.polygon({ sides: 6, radius: bands[0][2] }), z: 0 }];
    for (const [, z1, , r1] of bands) sections.push({ profile: og.polygon({ sides: 6, radius: r1 }), z: z1 - lugTop });
    const body = s.loft({ name: "knob_body", sections, on: turned(lugTop) });
    // Lug overlaps the body by 0.5 mm so the join does not meet on a shared face.
    const stem = s.extrude({ name: "lug", profile: og.rect({ width: lug, height: lug }), height: lug + 0.5, on: turned(bottom) });
    s.join(body, stem);
    s.material(body, { color: "#a0a4aa", roughness: 0.5 });
    return s.group({ name: "knob", children: [body] });
  }
}
