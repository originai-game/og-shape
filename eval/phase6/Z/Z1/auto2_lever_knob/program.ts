export default function (og) {
  const s = og.model({ name: "lever_knob" });
  const R = 28.88;            // hex corner radius (57.76 across corners, along y)
  const blockH = 28;
  const knobH = 98.94 - blockH;
  const lug = 16, lugH = 20, recessDepth = 21;
  const slide = 19;

  const hex = (r) => og.rotate({ profile: og.polygon({ sides: 6, radius: r }), angle: og.deg(90) });

  const blk = block();
  const k = knob();
  const f = og.world.move(0, 0, blockH);
  s.at(k, f);
  s.joint(k, { name: "knob_slide", parent: blk, kind: "prismatic", on: f, axis: "-Z", range: [0, slide] });
  s.joint(k, { name: "knob_spin", parent: blk, kind: "continuous", on: f, axis: "+Z" });
  return s;

  function block() {
    const b = s.extrude({ name: "support_block", profile: hex(R), height: blockH });
    s.pocket(b, { profile: og.rect({ width: lug + 0.6, height: lug + 0.6 }), depth: recessDepth, at: s.face(b, "top") });
    s.pocket(b, { profile: og.circle({ radius: 15 }), depth: 4, at: s.face(b, "bottom") });
    s.material(b, { color: "#8a8d91", roughness: 0.6 });
    return b;
  }

  // Stepped bands: each band rises straight, then a short taper to the next, narrower one.
  function knob() {
    const bands = [[0, 1], [12, 0.98], [20, 0.94], [28, 0.875], [36, 0.79], [44, 0.68], [51, 0.56], [57, 0.45], [62, 0.36], [66, 0.29]];
    const sections = [];
    bands.forEach(([z, k], i) => {
      const zEnd = i + 1 < bands.length ? bands[i + 1][0] - 2.5 : knobH - 1.5;
      sections.push({ profile: hex(R * k), z });
      sections.push({ profile: hex(R * k), z: zEnd });
    });
    sections.push({ profile: hex(R * 0.22), z: knobH });
    const kb = s.loft({ name: "knob", sections });
    const l = s.box({ name: "lug", width: lug, depth: lug, height: lugH + 0.5, on: og.world.move(0, 0, -lugH) });
    s.join(kb, l);
    s.material(kb, { color: "#a7aaae", roughness: 0.5 });
    return kb;
  }
}
