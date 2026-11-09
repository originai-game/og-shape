export default function (og) {
  const s = og.model({ name: "djembe" });
  const baseH = 360;

  const base = flaredBase(230, 128, baseH);
  const bowl = drumBowl();
  s.at(bowl, og.world.move(0, 0, baseH));
  s.assembly({ name: "djembe_assembly", children: [base, bowl] });
  return s;

  // A revolve about +y stood upright: profile y becomes world z.
  function upright(z = 0) {
    return og.frame({ origin: [0, 0, z], normal: [0, -1, 0], up: [0, 0, 1] });
  }

  function flaredBase(d, neckD, h) {
    const solid = s.cylinder({ name: "pedestal", radius: d / 2, radiusTop: neckD / 2, height: h });
    s.material(solid, { name: "painted_metal", color: "#9c3703", roughness: 0.8 });
    return s.group({ name: "flared_base", children: [solid] });
  }

  function drumBowl(neckR = 64, shoulderR = 118, middleR = 160, upperR = 187, rimR = 197,
                    shoulderZ = 23, middleZ = 74, upperZ = 157, h = 220, stripD = 7, stripCount = 16, collarH = 40) {
    const prof: [number, number][] = [[neckR, 0], [shoulderR, shoulderZ], [143, 50], [middleR, middleZ],
      [176, 129], [upperR, upperZ], [rimR, h]];
    const solid = s.revolve({ name: "bowl", profile: og.poly({ points: [[0, 0], ...prof, [0, h]] }), on: upright() });
    s.material(solid, { name: "painted_metal", color: "#9c3703", roughness: 0.8 });

    // Tension strips: one rod per ring frame, following the bowl's outline.
    const path = prof.map(([r, z]) => [r + stripD / 2, 0, z]);
    let strips = null;
    let i = 0;
    for (const f of s.ring({ n: stripCount, radius: 0 })) {
      const strip = s.sweep({ name: i++ ? `tension_strip_${i}` : "tension_strips", profile: og.circle({ radius: stripD / 2, segments: 8 }), path, on: f });
      if (strips) s.join(strips, strip); else strips = strip;
    }
    s.material(strips, { name: "rubber", color: "#000000" });

    const band = s.torus({ name: "lower_band", radius: shoulderR + stripD / 2, tube: stripD / 2, on: upright(shoulderZ - stripD / 2) });
    s.material(band, { name: "rubber", color: "#000000" });

    const collar = s.cylinder({ name: "neck_binding", radius: neckR + collarH * 0.15, radiusTop: neckR + stripD / 2,
      height: collarH, on: og.world.move(0, 0, -collarH + stripD) });
    s.material(collar, { name: "rubber", color: "#000000" });

    const head = drumHead();
    s.at(head, og.world.move(0, 0, h - 10));
    return s.group({ name: "drum_bowl", children: [solid, strips, band, collar, head] });
  }

  function drumHead(r = 202, skinR = 190, flatR = 172, h = 30, hoopH = 12) {
    const skin = s.revolve({ name: "skin", profile: og.poly({ points: [[0, 0], [skinR, 0], [skinR, hoopH], [flatR, h], [0, h]] }), on: upright() });
    const hoop = s.revolve({ name: "hoop", profile: og.poly({ points: [[skinR - 2, 0], [r - 2, 0], [r, hoopH / 2], [r - 3, hoopH], [skinR - 2, hoopH]] }), on: upright() });
    s.material(skin, { name: "leather", color: "#edd297", roughness: 0.9 });
    s.material(hoop, { name: "rubber", color: "#000000" });
    return s.group({ name: "drum_head", children: [skin, hoop] });
  }
}
