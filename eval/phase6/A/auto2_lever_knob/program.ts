export default function (og) {
  const s = og.model({ name: "lever_knob" });
  const R = 28.88;            // hex circumradius (corners along y)
  const blockH = 30, knobH = 68.94, travel = 19;

  const block = support(R, blockH);
  const knob = knobPart(R, knobH);
  const seatF = og.frame({ origin: s.face(block, "top").origin }).turn(0, 0, og.deg(90));
  s.at(knob, seatF);
  s.joint(knob, { name: "knob_slide", parent: block, kind: "prismatic", on: seatF, axis: "+Z", range: [-travel, 0] });
  s.joint(knob, { name: "knob_spin", parent: block, kind: "continuous", on: seatF, axis: "+Z" });
  return s;

  function hex(r) { return og.polygon({ sides: 6, radius: r }); }

  function support(r, h, recess = 16, recessDepth = 22, bore = 30, boreDepth = 3) {
    const b = s.extrude({ name: "block", profile: hex(r), height: h, on: og.world.turn(0, 0, og.deg(90)) });
    s.pocket(b, { profile: og.rect({ width: recess, height: recess }), depth: recessDepth, at: s.face(b, "top") });
    s.hole(b, { diameter: bore, depth: boreDepth, at: [s.face(b, "bottom")] });
    s.material(b, { color: "#8a8d91", roughness: 0.6 });
    return b;
  }

  function knobPart(r, h, lug = 15, lugH = 18) {
    const bands = [[0, 0.97], [0.15, 0.99], [0.3, 0.93], [0.45, 0.82], [0.6, 0.66], [0.72, 0.5], [0.85, 0.36], [0.95, 0.26], [1, 0.18]];
    const body = s.loft({ name: "knob", sections: [{ profile: hex(r * bands[0][1]), z: 0 }, { profile: hex(r * bands[1][1]), z: bands[1][0] * h }] });
    const pieces = [body];
    for (let i = 1; i < bands.length - 1; i++) {
      const [z0, k0] = bands[i], [z1, k1] = bands[i + 1];
      const band = s.loft({ name: `band_${i}`, sections: [{ profile: hex(r * k0), z: 0 }, { profile: hex(r * k1), z: (z1 - z0) * h }], on: og.world.move(0, 0, z0 * h) });
      s.material(band, { color: "#a0a3a7", roughness: 0.5 });
      pieces.push(band);
    }
    const l = s.box({ name: "lug", width: lug, depth: lug, height: lugH + 0.5, on: og.world.move(0, 0, -lugH) });
    s.join(body, l);
    s.material(body, { color: "#a0a3a7", roughness: 0.5 });
    return s.group({ name: "knob_assy", children: pieces });
  }
}
