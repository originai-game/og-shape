export default function (og) {
  const s = og.model({ name: "djembe" });
  const outerRadius = 407.91 / 2, height = 600;
  const hoopTube = 8;
  const headTop = height - 4;
  const rimZ = 560; // top of the wooden shell
  // wooden outline (radius, z), base to rim
  const outline = [[115, 0], [62, 300], [70, 312], [110, 325], [150, 345], [178, 380], [192, 430], [196, 500], [195, rimZ]];
  const black = { color: "#151515", roughness: 0.6 };

  const shell = wood();
  const head = skin();
  const hoop = ring();
  const band = s.tube({ name: "lacing", outer: 202, inner: 192, height: 22, on: og.world.move(0, 0, rimZ - 26) });
  const binding = s.tube({ name: "neck_binding", outer: 76, inner: 58, height: 24, on: og.world.move(0, 0, 296) });
  s.join(band, binding);
  for (const strip of tensionStrips(16)) s.join(band, strip);
  s.material(band, black);
  return s;

  function wood() {
    const pts = [[0, 0], ...outline, [0, rimZ]];
    const w = s.revolve({ name: "shell", profile: og.poly({ points: pts }), axis: "+Z" });
    s.material(w, { color: "#6b2a10", roughness: 0.7 });
    return w;
  }

  function skin() {
    const h = s.cylinder({ name: "head", radius: 196, height: headTop - rimZ, on: og.world.move(0, 0, rimZ) });
    s.fillet(h, { radius: 6, edges: "top" });
    s.material(h, { color: "#cdb98a", roughness: 0.8 });
    return h;
  }

  function ring() {
    const t = s.torus({ name: "hoop", radius: outerRadius - hoopTube, tube: hoopTube, axis: "+Z", on: og.world.move(0, 0, height - 2 * hoopTube) }); // a +Z torus sits on z = 0
    s.material(t, black);
    return t;
  }

  // Strips follow the outline from the neck binding to the band.
  function tensionStrips(n) {
    const d = 4;
    const path = outline.filter(([r, z]) => z >= 312 && z <= rimZ - 10).map(([r, z]) => [r + d / 2 - 0.5, 0, z]);
    path.unshift([66, 0, 306]); // starts inside the neck binding
    path.push([196 + d / 2 - 0.5, 0, rimZ - 10]); // ends inside the band
    const frames = s.ring({ n, radius: 0, start: og.deg(2.8), on: og.world });
    const parts = frames.map((f, i) => s.wire({ name: `strip_${i}`, path, diameter: d, on: f }));
    return parts;
  }
}
