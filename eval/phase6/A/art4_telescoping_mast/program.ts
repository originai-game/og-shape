export default function (og) {
  const s = og.model({ name: "telescoping_mast" });
  const width = 280, depth = 220, height = 1546; // overall size
  const baseT = 15;
  const midLen = 1085, midW = 50, midWall = 5;
  const topLen = 1000, topW = 36, topZ = 500;
  const midTravel = 380, topTravel = 340;

  const frame = support(width, depth, baseT);

  const midSeat = og.frame({ origin: [0, 0, baseT] });
  const mid = midMast(midLen, midW, midWall);
  s.at(mid, midSeat);
  s.joint(mid, { name: "mid_slide", parent: frame, kind: "prismatic", on: midSeat, axis: "+Z", range: [0, midTravel] });

  const topSeat = og.frame({ origin: [0, 0, topZ] });
  const top = topMast(topLen, topW);
  s.at(top, topSeat);
  s.joint(top, { name: "top_slide", parent: mid, kind: "prismatic", on: topSeat, axis: "+Z", range: [0, topTravel] });

  const stageSeat = og.frame({ origin: [0, 0, topZ + topLen] });
  const st = stage();
  s.at(st, stageSeat);
  s.joint(st, { name: "stage_turn", parent: top, kind: "revolute", on: stageSeat, axis: "+Z", range: [og.deg(-160), og.deg(160)] });
  return s;

  // Base plate, wall rail at the back (+Y), two guides with cross pieces.
  function support(w, d, t, railW = 200, railT = 15, railH = 1200, guideW = 16, guideX = 45) {
    const base = s.box({ name: "base_plate", width: w, depth: d, height: t });
    const railY = d / 2 - railT / 2;
    const rail = s.box({ name: "rail", width: railW, depth: railT, height: railH, on: og.world.move(0, railY, t - 0.5) });
    s.join(base, rail);
    const guideH = railH - 50;
    const crossD = railY - railT / 2 + guideW / 2 + 1; // from guide front to rail face, overlapping 1
    for (const gx of [-guideX, guideX]) {
      const g = s.box({ name: gx < 0 ? "guide_left" : "guide_right", width: guideW, depth: guideW, height: guideH, on: og.world.move(gx, 0, t - 0.5) });
      s.join(base, g);
      for (const z of [200, 500, 800, guideH - 20]) {
        const c = s.box({ name: "cross", width: guideW, depth: crossD, height: 20, on: og.world.move(gx, -guideW / 2 + crossD / 2, z) });
        s.join(base, c);
      }
    }
    const sleeve = s.extrude({
      name: "foot_sleeve",
      profile: og.subtract(og.rect({ width: 80, height: 80 }), og.rect({ width: midW + 2, height: midW + 2 })),
      height: 60,
      on: og.world.move(0, 0, t - 0.5),
    });
    s.join(base, sleeve);
    s.material(base, { color: "#8a8f94", metalness: 0.6, roughness: 0.5 });
    return s.group({ name: "support", children: [base] });
  }

  // Square tube, bottom on its origin.
  function midMast(len, w, wall) {
    const m = s.extrude({
      name: "mid_mast",
      profile: og.subtract(og.rect({ width: w, height: w }), og.rect({ width: w - 2 * wall, height: w - 2 * wall })),
      height: len,
    });
    s.material(m, { color: "#b4b7bb", metalness: 0.8, roughness: 0.35 });
    return m;
  }

  function topMast(len, w) {
    const m = s.box({ name: "top_mast", width: w, depth: w, height: len });
    s.material(m, { color: "#d0d3d6", metalness: 0.8, roughness: 0.3 });
    return m;
  }

  // Rotary stage about its own origin: collar, disc, plate, pin.
  function stage(collarH = 15, discR = 45, discH = 15, plateT = 6, pinH = 10) {
    const collar = s.cylinder({ name: "stage_collar", radius: 30, height: collarH });
    const disc = s.cylinder({ name: "stage_disc", radius: discR, height: discH, on: og.world.move(0, 0, collarH) });
    s.join(collar, disc);
    const plate = s.box({ name: "stage_plate", width: 70, depth: 30, height: plateT, on: og.world.move(0, 0, collarH + discH - 0.5) });
    s.join(collar, plate);
    const pin = s.cylinder({ name: "stage_pin", radius: 5, height: pinH + 0.5, on: og.world.move(20, 0, collarH + discH + plateT - 0.5) });
    s.join(collar, pin);
    s.material(collar, { color: "#5a6168", metalness: 0.7, roughness: 0.4 });
    return s.group({ name: "stage", children: [collar] });
  }
}
