export default function (og) {
  const s = og.model({ name: "djembe" });
  const height = 600, rimR = 196, topZ = 570, neckZ = 300, neckR = 55;
  const upright = (z) => og.frame({ origin: [0, 0, z], normal: [0, -1, 0], up: [0, 0, 1] });
  // the bowl's outline above the neck, (radius, z): strips follow it
  const bowlLine = [[neckR, neckZ + 10], [110, neckZ + 40], [170, neckZ + 90], [188, neckZ + 140], [195, neckZ + 200], [rimR - 1, topZ]];

  body();
  strips(16);
  band();
  neckBinding();
  head();
  hoop();
  return s;

  function body(baseR = 120) {
    const prof = og.poly({ points: [[0, 0], [baseR, 0], [neckR, neckZ - 10], ...bowlLine, [0, topZ]] });
    const b = s.revolve({ name: "body", profile: prof, on: upright(0) });
    s.material(b, { color: "#7a2a0e", roughness: 0.6 });
    return b;
  }

  // One rod from (r0, z0) to (r1, z1) in the radial plane of a ring frame f (x outward).
  function rod(name, f, a, b, rad) {
    const dr = b[0] - a[0], dz = b[1] - a[1];
    const len = Math.sqrt(dr * dr + dz * dz);
    const tilt = Math.atan2(dr, dz);
    const on = f.move(a[0] + rad, 0, a[1]).turn(0, tilt, 0);
    return s.cylinder({ name, radius: rad, height: len + rad, segments: 8, on });
  }

  function strips(n, rad = 2.5) {
    const lines = bowlLine.slice(1, 5).concat([[rimR - 1, topZ - 20]]);
    const rods = [];
    let i = 0;
    for (const f of s.ring({ n, radius: 0 })) {
      for (let k = 0; k + 1 < lines.length; k++) rods.push(rod(`strip_${i}_${k}`, f, lines[k], lines[k + 1], rad));
      i++;
    }
    const g = s.group({ name: "strips", children: rods });
    s.material(g, { color: "#111111", roughness: 0.5 });
    return g;
  }

  function band() {
    const b = s.tube({ name: "band", outer: rimR + 2, inner: rimR - 4, height: 14, on: og.world.move(0, 0, topZ - 30) });
    s.material(b, { color: "#111111", roughness: 0.5 });
  }

  function neckBinding() {
    const b = s.tube({ name: "neck_binding", outer: neckR + 9, inner: neckR, height: 22, on: og.world.move(0, 0, neckZ - 12) });
    s.material(b, { color: "#111111", roughness: 0.5 });
  }

  function head() {
    const h = s.cylinder({ name: "head", radius: rimR - 2, height: height - topZ, on: og.world.move(0, 0, topZ) });
    s.fillet(h, { radius: 10, edges: "top" });
    s.material(h, { color: "#c9b88a", roughness: 0.8 });
  }

  function hoop(tube = 8) {
    const t = s.torus({ name: "hoop", radius: rimR, tube, on: upright(topZ + 12) });
    s.material(t, { color: "#111111", roughness: 0.4 });
  }
}
