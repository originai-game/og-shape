export default function (og) {
  const s = og.model({ name: "lever_knob" });
  const width = 50.54, depth = 57.76, height = 98.94;
  const R = depth / 2;                       // hexagon corner radius, corners on ±y
  const sx = width / (Math.sqrt(3) * R);     // stretch x to the stated width
  const blockH = 30;
  const knobH = height - blockH;
  const slide = 19;

  const block = supportBlock();
  const pivot = og.world.move(0, 0, blockH);
  const k = knob();
  s.at(k, pivot);
  s.joint(k, { name: "knob_slide", parent: block, kind: "prismatic", on: pivot, axis: "-Z", range: [0, slide] });
  s.joint(k, { name: "knob_spin", parent: block, kind: "continuous", on: pivot, axis: "+Z" });
  return s;

  function hex(r) {
    return og.scale({ profile: og.rotate({ profile: og.polygon({ sides: 6, radius: r }), angle: og.deg(90) }), x: sx });
  }

  function supportBlock() {
    const b = s.extrude({ name: "support_block", profile: hex(R), height: blockH });
    s.pocket(b, { profile: og.rect({ width: 17, height: 17 }), depth: 12, at: s.face(b, "top") });
    s.hole(b, { diameter: 30, depth: 3, at: s.face(b, "bottom") });
    s.material(b, { color: "#8a8d91", metalness: 0.6, roughness: 0.5 });
    return b;
  }

  // Built about its own origin: the bottom centre, on the spin axis.
  function knob() {
    const bands = [
      [0, 1.0], [12, 1.0], [15, 0.98], [24, 0.98], [27, 0.93], [35, 0.93],
      [38, 0.84], [45, 0.84], [48, 0.7], [54, 0.7], [57, 0.54], [61, 0.54],
      [64, 0.38], [knobH - 1.5, 0.38], [knobH, 0.3],
    ];
    const body = s.loft({ name: "knob", sections: bands.map(([z, f]) => ({ profile: hex(R * f), z })) });
    const lug = s.box({ name: "lug", width: 16, depth: 16, height: 11, on: og.world.move(0, 0, -10) });
    s.join(body, lug);
    s.material(body, { color: "#a0a3a7", metalness: 0.6, roughness: 0.45 });
    return s.group({ name: "knob_assembly", children: [body] });
  }
}
