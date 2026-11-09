export default function (og) {
  const s = og.model({ name: "side_wall_mast" });
  const midSeat = 160, topSeat = 280, stageSeat = 1030; // each part's seat on its parent
  const midTravel = 380, topTravel = 340, stageTurn = og.deg(162);

  const support = supportFrame();

  const f1 = og.world.move(0, 0, midSeat);
  const mid = midMast();
  s.at(mid, f1);
  s.joint(mid, { name: "support_to_mid", parent: support, kind: "prismatic", on: f1, axis: "+Z", range: [0, midTravel] });

  const f2 = og.world.move(0, 0, midSeat + topSeat);
  const top = topMast();
  s.at(top, f2);
  s.joint(top, { name: "mid_to_top", parent: mid, kind: "prismatic", on: f2, axis: "+Z", range: [0, topTravel] });

  const f3 = og.world.move(0, 0, midSeat + topSeat + stageSeat);
  const stage = rotaryStage();
  s.at(stage, f3);
  s.joint(stage, { name: "top_to_stage", parent: top, kind: "revolute", on: f3, axis: "+Z", range: [-stageTurn, stageTurn] });

  return s;

  // A block of width x depth x height centred on (x, y, zc), as sand3d's centred extrude.
  function blk(name, width, depth, height, x, y, zc) {
    return s.box({ name, width, depth, height, on: og.world.move(x, y, zc - height / 2) });
  }
  function disc(name, d, height, x, y, zc) {
    return s.cylinder({ name, radius: d / 2, height, on: og.world.move(x, y, zc - height / 2) });
  }

  function supportFrame() {
    const body = blk("support_frame_body", 280, 180, 30, -40, -20, 15);
    const rail = blk("support_rail", 20, 220, 1180, -150, -20, 590);
    const foot = blk("support_foot", 130, 100, 80, -30, -10, 70);
    const inL = blk("support_inner_left", 14, 50, 1040, -43, -15, 580);
    const inR = blk("support_inner_right", 14, 50, 1040, 43, -15, 580);
    const crosses = [[50, 150], [45, 560], [50, 1000]].map(([h, z], i) =>
      blk(["support_cross_low", "support_cross_mid", "support_cross_high"][i], 190, 25, h, -45, -52, z));
    for (const piece of [rail, foot, inL, inR, ...crosses]) s.join(body, piece);
    const g = s.group({ name: "support_frame", children: [body] });
    s.material(g, { name: "steel", color: "#3d454d", metalness: 0.6, roughness: 0.5 });
    return g;
  }

  // Built about its seat: z = 0 is where it sits on the support.
  function midMast() {
    const body = blk("mid_mast_body", 44, 24, 1120, 0, -24, 560);
    const railL = blk("mid_rail_left", 14, 40, 920, -29, -12, 560);
    const railR = blk("mid_rail_right", 14, 40, 920, 29, -12, 560);
    const capLow = blk("mid_cap_low", 72, 24, 50, 0, -24, 75);
    const capHigh = blk("mid_cap_high", 72, 24, 50, 0, -24, 1045);
    for (const piece of [railL, railR, capLow, capHigh]) s.join(body, piece);
    const g = s.group({ name: "mid_mast", children: [body] });
    s.material(g, { color: "#8a939b", metalness: 0.6, roughness: 0.45 });
    return g;
  }

  function topMast() {
    const body = blk("top_mast_body", 32, 24, 920, 0, 0, 460);
    const railL = blk("top_rail_left", 6, 20, 520, -19, -4, 300);
    const railR = blk("top_rail_right", 6, 20, 520, 19, -4, 300);
    const cap = blk("top_cap", 90, 60, 50, 0, -5, 945);
    const cartridge = disc("cartridge", 80, 60, 0, -5, 1000);
    for (const piece of [railL, railR, cap, cartridge]) s.join(body, piece);
    const g = s.group({ name: "top_mast", children: [body] });
    s.material(g, { color: "#b4b7bb", metalness: 0.7, roughness: 0.4 });
    return g;
  }

  function rotaryStage() {
    const body = disc("rotary_stage_body", 150, 18, 0, 0, 9);
    const plate = blk("stage_plate", 120, 80, 18, 30, 0, 27);
    const tab = blk("stage_tab", 34, 44, 18, -70, 0, 27);
    const pin = disc("stage_pin", 20, 40, 60, 0, 56);
    for (const piece of [plate, tab, pin]) s.join(body, piece);
    const g = s.group({ name: "rotary_stage", children: [body] });
    s.material(g, { color: "#6d7780", metalness: 0.6, roughness: 0.45 });
    return g;
  }
}
