export default function (og) {
  const s = og.model({ name: "lever_knob" });
  const height = 98.94;
  const hexR = 57.76 / 2;          // corner radius, corners on ±y
  const blockH = 25;
  const knobH = height - blockH;
  const recess = 17, recessDepth = 10;
  const lug = 16, lugH = 8;

  const block = supportBlock();
  const top = og.frame({ origin: s.bbox(block, "+Z").origin });
  const k = knob();
  s.at(k, top);
  s.joint(k, { name: "knob_slide", parent: block, kind: "prismatic", on: top, axis: "-Z", range: [0, 19] });
  s.joint(k, { name: "knob_spin", parent: block, kind: "continuous", on: top, axis: "+Z" });
  return s;

  function hex(r) {
    return { rotate: { profile: og.polygon({ sides: 6, radius: r }), angle: og.deg(90) } };
  }

  function supportBlock() {
    const b = s.extrude({ name: "block", profile: hex(hexR), height: blockH });
    s.pocket(b, { profile: og.rect({ width: recess, height: recess }), depth: recessDepth, at: s.face(b, "top") });
    s.pocket(b, { profile: og.circle({ radius: 12 }), depth: 3, at: s.face(b, "bottom") });
    s.material(b, { color: "#8a8d91", roughness: 0.6 });
    return b;
  }

  // Built about its own origin: the bottom face of the knob at z = 0.
  function knob() {
    const bands = [
      [0, 1.0], [12, 1.0], [20, 0.97], [28, 0.92], [36, 0.85],
      [44, 0.76], [52, 0.64], [59, 0.5], [65, 0.36], [70, 0.26], [knobH, 0.22],
    ];
    let body = null;
    for (let i = 0; i + 1 < bands.length; i++) {
      const [z0, f0] = bands[i], [z1, f1] = bands[i + 1];
      const lap = i === 0 ? 0 : 0.5;
      const band = s.extrude({
        name: i === 0 ? "knob" : `band_${i}`, profile: hex(hexR * f0), height: z1 - z0 + lap,
        taper: 1 - f1 / f0, on: og.world.move(0, 0, z0 - lap),
      });
      if (body) s.join(body, band); else body = band;
    }
    const l = s.box({ name: "lug", width: lug, depth: lug, height: lugH + 0.5, on: og.world.move(0, 0, -lugH) });
    s.join(body, l);
    s.material(body, { color: "#a3a6aa", roughness: 0.5 });
    return body;
  }
}
