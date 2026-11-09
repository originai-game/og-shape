export default function (og) {
  const s = og.model({ name: "djembe" });
  const height = 600, hoopR = 196, hoopTube = 7.955; // hoop outer = 203.955 (half of 407.91)
  const footR = 115, neckR = 65, neckZ = 330, bowlR = 192, rimZ = 575;
  const upright = (z) => og.frame({ origin: [0, 0, z], normal: [0, -1, 0], up: [0, 0, 1] });

  const shell = body();
  const hd = head();
  const hp = hoop();
  const bd = band();
  const nb = neckBinding();
  return s;

  function body() {
    const profile = og.poly({ points: [
      [0, 0], [footR, 0], [neckR, neckZ], [neckR + 5, neckZ + 10], [110, 360], [160, 400],
      [185, 450], [bowlR, 520], [bowlR, rimZ], [0, rimZ],
    ] });
    const b = s.revolve({ name: "body", profile, on: upright(0) });
    s.material(b, { color: "#6b2a10", roughness: 0.6 });
    return b;
  }

  function head(t = 20) {
    const h = s.cylinder({ name: "head", radius: bowlR + 2, height: t, on: og.world.move(0, 0, rimZ - 4) });
    s.material(h, { color: "#c8b88a", roughness: 0.8 });
    return h;
  }

  function hoop() {
    const z = height - hoopTube;
    const t = s.torus({ name: "hoop", radius: hoopR, tube: hoopTube, on: upright(z) });
    s.material(t, { color: "#111111", roughness: 0.5 });
    return t;
  }

  function band(n = 20) {
    const b = s.tube({ name: "band", outer: bowlR + 4, inner: bowlR - 6, height: 16, on: og.world.move(0, 0, rimZ - 20) });
    const off = 3;
    const pts = [[110 + off, 0, 360], [160 + off, 0, 400], [185 + off, 0, 450], [bowlR + off, 0, 520], [bowlR + off, 0, rimZ - 10]];
    for (const f of s.ring({ n, radius: 0 })) {
      const st = s.sweep({ name: "strip", profile: og.circle({ radius: 2, segments: 6 }), path: pts, on: f });
      s.join(b, st);
    }
    s.material(b, { color: "#111111", roughness: 0.5 });
    return b;
  }

  function neckBinding() {
    const nb = s.tube({ name: "neck_binding", outer: neckR + 12, inner: neckR - 5, height: 22, on: og.world.move(0, 0, neckZ - 2) });
    s.material(nb, { color: "#111111", roughness: 0.5 });
    return nb;
  }
}
