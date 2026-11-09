export default function (og) {
  const s = og.model({ name: "lever_knob" });
  const blockR = 28.88, blockH = 30;           // hex to the corners (y), height
  const knobH = 98.94 - blockH;                // knob above the block top
  const lugW = 16, lugH = 18, recessDepth = 20;
  const slide = 19;

  const block = supportBlock(blockR, blockH);
  const seat = og.frame({ origin: s.bbox(block, "+Z").origin });
  const knob = leverKnob(blockR, knobH);
  s.at(knob, seat);
  s.joint(knob, { name: "knob_slide", parent: block, kind: "prismatic", on: seat, axis: "-Z", range: [0, slide] });
  s.joint(knob, { name: "knob_spin", parent: block, kind: "continuous", on: seat, axis: "+Z" });
  return s;

  function hex(r) {
    return { rotate: { profile: og.polygon({ sides: 6, radius: r }), angle: og.deg(90) } };
  }

  function supportBlock(r, h, bore = 22, boreDepth = 3) {
    const b = s.extrude({ name: "support_block", profile: hex(r), height: h });
    s.pocket(b, { profile: og.rect({ width: lugW + 1, height: lugW + 1 }), depth: recessDepth, at: s.face(b, "top") });
    s.pocket(b, { profile: og.circle({ radius: bore / 2 }), depth: boreDepth, at: s.face(b, "bottom") });
    s.material(b, { color: "#8a8d91", metalness: 0.6, roughness: 0.4 });
    return b;
  }

  // Built about its own origin: the seat on the block top at z = 0.
  function leverKnob(r, h) {
    const bands = [
      [0.00, 0.95], [0.22, 0.99], [0.36, 0.97], [0.48, 0.88],
      [0.60, 0.74], [0.70, 0.58], [0.80, 0.46], [0.90, 0.34], [1.00, 0.22],
    ];
    // One tapered hexagonal band between each pair of sections, each sunk 0.5 mm into the one below.
    let body = null;
    for (let i = 0; i + 1 < bands.length; i++) {
      const [z0, k0] = bands[i], [z1, k1] = bands[i + 1];
      const sink = i === 0 ? 0 : 0.5;
      const band = s.extrude({ name: i === 0 ? "knob" : `band_${i}`, profile: hex(r * k0), height: (z1 - z0) * h + sink, taper: 1 - k1 / k0, on: og.world.move(0, 0, z0 * h - sink) });
      if (body) s.join(body, band); else body = band;
    }
    const lug = s.box({ name: "lug", width: lugW, depth: lugW, height: lugH + 1, on: og.world.move(0, 0, -lugH) });
    s.join(body, lug);
    s.material(body, { color: "#b0b3b8", metalness: 0.5, roughness: 0.35 });
    return s.group({ name: "knob_assembly", children: [body] });
  }
}
