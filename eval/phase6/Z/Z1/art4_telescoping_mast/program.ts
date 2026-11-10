export default function (og) {
  const s = og.model({ name: "telescoping_mast" });
  const W = 280, D = 220, H = 1546;
  const baseT = 15;
  const mastY = 30;               // mast axis y
  const midW = 50, midWall = 4, midZ0 = 16, midLen = 1084;  // 16..1100
  const topW = 36, topZ0 = 300, topLen = 1158, headH = 30;  // 300..1458, head to 1488
  const stageR = 55, stageH = 20, plateT = 8, pinH = 30;     // 1488 + 58 = 1546
  const rail = { w: 180, t: 10, h: 1150 };

  const frame = supportFrame();
  const mid = midMast();
  const top = topMast();
  const stage = rotaryStage();

  const midSlide = og.world.move(0, mastY, 0);
  s.joint(mid, { name: "mid_mast_slide", parent: frame, kind: "prismatic", on: midSlide, axis: "+Z", range: [0, 380] });
  s.joint(top, { name: "top_mast_slide", parent: mid, kind: "prismatic", on: midSlide, axis: "+Z", range: [0, 340] });
  const stageF = og.world.move(0, mastY, topZ0 + topLen + headH);
  s.at(stage, stageF);
  s.joint(stage, { name: "stage_rotate", parent: top, on: stageF, axis: "+Z", range: [og.deg(-160), og.deg(160)] });
  return s;

  function supportFrame() {
    const base = s.box({ name: "support_frame", width: W, depth: D, height: baseT });
    const wall = s.box({ name: "rail", width: rail.w, depth: rail.t, height: rail.h, on: og.world.move(0, D / 2 - rail.t / 2, baseT - 1) });
    s.join(base, wall);
    const gx = 40, gw = 20, gy0 = 10, gy1 = 75;
    for (const sx of [-1, 1]) {
      const g = s.box({ name: sx < 0 ? "guide_l" : "guide_r", width: gw, depth: gy1 - gy0, height: rail.h - 10, on: og.world.move(sx * gx, (gy0 + gy1) / 2, baseT - 1) });
      s.join(base, g);
    }
    for (const z of [320, 660, 1000]) {
      const c = s.box({ name: "cross_" + z, width: 140, depth: 32, height: 22, on: og.world.move(0, 86, z) });
      s.join(base, c);
    }
    const collar = s.extrude({ name: "collar", profile: og.subtract(og.rect({ width: 74, height: 74 }), og.rect({ width: midW + 2, height: midW + 2 })), height: 80, on: og.world.move(0, mastY, baseT - 1) });
    s.join(base, collar);
    s.material(base, { color: "#8a8f96", metalness: 0.6, roughness: 0.5 });
    return base;
  }

  function midMast() {
    const m = s.extrude({ name: "mid_mast", profile: og.subtract(og.rect({ width: midW, height: midW }), og.rect({ width: midW - 2 * midWall, height: midW - 2 * midWall })), height: midLen, on: og.world.move(0, mastY, midZ0) });
    s.material(m, { color: "#b8bcc2", metalness: 0.8, roughness: 0.35 });
    return m;
  }

  function topMast() {
    const t = s.box({ name: "top_mast", width: topW, depth: topW, height: topLen, on: og.world.move(0, mastY, topZ0) });
    const head = s.box({ name: "head", width: 64, depth: 64, height: headH + 1, on: og.world.move(0, mastY, topZ0 + topLen - 1) });
    s.join(t, head);
    s.material(t, { color: "#d0d3d8", metalness: 0.8, roughness: 0.3 });
    return t;
  }

  function rotaryStage() {
    const disk = s.cylinder({ name: "stage_disk", radius: stageR, height: stageH, segments: 48 });
    const plate = s.box({ name: "stage_plate", width: 110, depth: 40, height: plateT + 1, on: og.world.move(0, 0, stageH - 1) });
    s.join(disk, plate);
    const pin = s.cylinder({ name: "stage_pin", radius: 6, height: pinH + 1, segments: 16, on: og.world.move(0, 0, stageH + plateT - 1) });
    s.join(disk, pin);
    s.material(disk, { color: "#5a6270", metalness: 0.7, roughness: 0.4 });
    return s.group({ name: "rotary_stage", children: [disk] });
  }
}
