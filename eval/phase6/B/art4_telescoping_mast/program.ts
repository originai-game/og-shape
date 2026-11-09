export default function (og) {
  const s = og.model({ name: "telescoping_mast" });
  const width = 280, depth = 220, base_t = 20;
  const mast_y = -10;
  const mid_outer = 70, mid_wall = 7, mid_h = 1080;
  const top_outer = 50, top_wall = 6, top_h = 1400, top_z = 60;
  const mid_travel = 380, top_travel = 340;

  const support = supportFrame();
  const mid = mast("mid_mast", mid_outer, mid_wall, mid_h, "#8e9aa6");
  const midF = og.world.move(0, mast_y, base_t);
  s.at(mid, midF);
  s.joint(mid, { name: "mid_slide", parent: support, kind: "prismatic", on: midF, axis: "+Z", range: [0, mid_travel] });

  const top = topMast();
  const topF = og.world.move(0, mast_y, top_z);
  s.at(top, topF);
  s.joint(top, { name: "top_slide", parent: mid, kind: "prismatic", on: topF, axis: "+Z", range: [0, top_travel] });

  const stg = stage();
  const stageF = og.world.move(0, mast_y, top_z + top_h);
  s.at(stg, stageF);
  s.joint(stg, { name: "stage_turn", parent: top, kind: "revolute", on: stageF, axis: "+Z", range: [og.deg(-160), og.deg(160)] });
  return s;

  function supportFrame(rail_w = 200, rail_t = 12, rail_h = 1200, guide = 16, guide_x = 52, guide_h = 1100) {
    const base = s.box({ name: "support", width, depth, height: base_t });
    const railY = depth / 2 - rail_t / 2;
    const rail = s.box({ name: "rail", width: rail_w, depth: rail_t, height: rail_h, on: og.world.move(0, railY, base_t - 1) });
    s.join(base, rail);
    for (const sx of [-1, 1]) {
      const g = s.box({ name: sx < 0 ? "guide_l" : "guide_r", width: guide, depth: guide, height: guide_h, on: og.world.move(sx * guide_x, mast_y, base_t - 1) });
      s.join(base, g);
      for (const z of [320, 640, 960]) {
        const span = railY - mast_y;
        const c = s.box({ name: `cross_${sx < 0 ? "l" : "r"}_${z}`, width: guide, depth: span, height: guide, on: og.world.move(sx * guide_x, mast_y + span / 2, z) });
        s.join(base, c);
      }
    }
    s.material(base, { color: "#6f7378", roughness: 0.6 });
    return base;
  }

  function mast(name, outer, wall, h, color) {
    const prof = og.subtract(og.rect({ width: outer, height: outer }), og.rect({ width: outer - 2 * wall, height: outer - 2 * wall }));
    const m = s.extrude({ name, profile: prof, height: h });
    s.material(m, { color, metalness: 0.6, roughness: 0.4 });
    return m;
  }

  function topMast(collar = 64, collar_h = 24) {
    const m = mast("top_mast", top_outer, top_wall, top_h, "#b4b7bb");
    const c = s.box({ name: "collar", width: collar, depth: collar, height: collar_h, on: og.world.move(0, 0, top_h - collar_h) });
    s.join(m, c);
    return m;
  }

  function stage(r = 55, disk_h = 30, plate_t = 10, pin_r = 6, pin_h = 47) {
    const disk = s.cylinder({ name: "stage", radius: r, height: disk_h });
    const plate = s.box({ name: "plate", width: 2 * r, depth: 30, height: plate_t, on: og.world.move(0, 0, disk_h - 1) });
    s.join(disk, plate);
    const pin = s.cylinder({ name: "pin", radius: pin_r, height: pin_h, on: og.world.move(0, 0, disk_h + plate_t - 1) });
    s.join(disk, pin);
    s.material(disk, { color: "#3d4a5c", roughness: 0.5 });
    return disk;
  }
}
