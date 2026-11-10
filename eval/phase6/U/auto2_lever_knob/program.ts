export default function (og) {
  const s = og.model({ name: "lever_knob" });
  const R = 57.76 / 2;               // hex corner radius (corners on ±y)
  const sx = 50.54 / (57.76 * Math.cos(Math.PI / 6)); // stretch to match x size
  const total = 98.94;
  const blockH = 32;
  const knobH = total - blockH;
  const recess = 16, recessDepth = 10, lug = 15, lugH = 8;
  const boreD = 30, boreDepth = 4;

  const hex = (r) => og.scale({ profile: og.rotate({ profile: og.polygon({ sides: 6, radius: r }), angle: og.deg(90) }), x: sx });

  const blk = block();
  const k = knob();
  const top = og.world.move(0, 0, blockH);
  s.at(k, top);
  s.joint(k, { name: "knob_slide", parent: blk, kind: "prismatic", on: top, axis: "-Z", range: [0, 19] });
  s.joint(k, { name: "knob_spin", parent: blk, kind: "continuous", on: top, axis: "+Z" });
  return s;

  function block() {
    const b = s.extrude({ name: "support_block", profile: hex(R), height: blockH });
    s.pocket(b, { profile: og.rect({ width: recess, height: recess }), depth: recessDepth, at: s.face(b, "top") });
    s.pocket(b, { profile: og.circle({ radius: boreD / 2 }), depth: boreDepth, at: s.face(b, "bottom") });
    s.material(b, { color: "#8a8d91", roughness: 0.6 });
    return b;
  }

  // Stepped knob: tapered hexagonal bands stacked; each band starts 0.6 mm
  // inside the one below and a hair narrower, so the sides cross its top face.
  function knob() {
    const bands = [[0, 1.0], [8, 0.99], [16, 0.96], [24, 0.91], [31, 0.84], [38, 0.76], [45, 0.66], [51, 0.55], [56, 0.44], [61, 0.33], [64, 0.25], [knobH, 0.2]];
    let kb = null;
    for (let i = 0; i + 1 < bands.length; i++) {
      const [z0, f0] = bands[i], [z1, f1] = bands[i + 1];
      const lo = i === 0 ? z0 : z0 - 0.6;
      const flo = i === 0 ? f0 : f0 * 0.99;
      const band = s.extrude({ name: i === 0 ? "knob" : `band_${i}`, profile: hex(R * flo), height: z1 - lo, taper: 1 - f1 / flo, on: og.world.move(0, 0, lo) });
      if (kb) s.join(kb, band); else kb = band;
    }
    const l = s.box({ name: "lug", width: lug, depth: lug, height: lugH + 0.5, on: og.world.move(0, 0, -lugH) });
    s.join(kb, l);
    s.material(kb, { color: "#a7aaae", roughness: 0.5 });
    return kb;
  }
}
