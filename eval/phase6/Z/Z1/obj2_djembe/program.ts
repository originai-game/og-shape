export default function (og) {
  const s = og.model({ name: "djembe" });
  const H = 600, headR = 197, hoopTube = 7, hoopR = 203.955 - hoopTube;
  const neckZ = 320, bowlTopZ = 560, baseR = 105, neckR = 62;
  const black = { color: "#141414", roughness: 0.6 };

  const shell = woodBody();
  const head = s.cylinder({ name: "head", radius: headR, height: 36, on: og.world.move(0, 0, bowlTopZ - 1) });
  s.fillet(head, { radius: 6, edges: "top" });
  s.material(head, { color: "#c8b88a", roughness: 0.8 });

  const hoop = s.torus({ name: "hoop", radius: hoopR, tube: hoopTube, axis: "+Z", on: og.world.move(0, 0, H - 2 * hoopTube) });
  s.material(hoop, black);
  const rimBand = s.torus({ name: "rim_band", radius: headR + 1, tube: 5, axis: "+Z", on: og.world.move(0, 0, bowlTopZ - 12) });
  s.material(rimBand, black);
  const neckBand = s.tube({ name: "neck_band", outer: 104, inner: 58, height: 22, on: og.world.move(0, 0, neckZ - 4) });
  s.material(neckBand, black);
  const strips = tensionStrips(24);
  return s;

  function woodBody() {
    const prof = og.poly({ points: [
      [0, 0], [baseR, 0], [neckR, neckZ], [100, neckZ + 15], [150, 380],
      [185, 450], [196, 520], [196, bowlTopZ], [0, bowlTopZ],
    ] });
    const b = s.revolve({ name: "shell", profile: prof, axis: "+Z", segments: 64 });
    s.material(b, { color: "#6b2a10", roughness: 0.55 });
    return b;
  }

  function tensionStrips(n) {
    const path = [[103, 0, neckZ + 14], [153, 0, 380], [188, 0, 450], [199, 0, 520], [199, 0, bowlTopZ + 2]];
    const frames = s.ring({ n, radius: 0, on: og.world });
    const parts = frames.map((f, i) => s.wire({ name: `strip_${i}`, path, diameter: 4, segments: 8, on: f }));
    const g = s.group({ name: "tension_strips", children: parts });
    s.material(g, black);
    return g;
  }
}
