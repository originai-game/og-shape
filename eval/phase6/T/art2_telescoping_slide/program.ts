export default function (og) {
  const s = og.model({ name: "wall_backed_three_stage_extension_slide" });
  const steel = "#adb3b8";

  const outer = outerSleeve();
  const midSeat = og.frame({ origin: [200, 0, 0] });
  const middle = middleSection();
  s.at(middle, midSeat);
  s.joint(middle, { name: "outer_to_middle", parent: outer, kind: "prismatic", on: midSeat, axis: "+X", range: [0, 260] });

  const innerSeat = og.frame({ origin: [440, 0, 0] });
  const inner = innerSection();
  s.at(inner, innerSeat);
  s.joint(inner, { name: "middle_to_inner", parent: middle, kind: "prismatic", on: innerSeat, axis: "+X", range: [0, 220] });

  return s;

  // A block x by y by z centred on (cx, cy, cz).
  function block(name, x, y, z, cx, cy, cz) {
    return s.box({ name, width: x, depth: y, height: z, on: og.world.move(cx, cy, cz - z / 2) });
  }

  function outerSleeve(len = 560) {
    const plate = block("wall_plate", 40, 240, 300, -20, 0, 0);
    s.material(plate, { name: "plastic", color: "#dbd6c7" });

    // C-channel running out from the wall, with its rear socket.
    const channel = block("outer_channel", 30, 95, 85, 15, 0, 0);
    s.join(channel, block("outer_web", len, 6, 70, len / 2, -39, 0));
    s.join(channel, block("outer_top_flange", len, 84, 6, len / 2, 0, 32));
    s.join(channel, block("outer_bottom_flange", len, 84, 6, len / 2, 0, -32));
    s.join(channel, block("outer_upper_lip", len, 6, 18, len / 2, 39, 23));
    s.join(channel, block("outer_lower_lip", len, 6, 18, len / 2, 39, -23));
    s.material(channel, { name: "steel", color: steel, metalness: 1, roughness: 0.4 });

    const up = block("upper_standoff", 80, 26, 30, 40, -55, 75);
    s.material(up, { name: "steel", color: steel, metalness: 1, roughness: 0.4 });
    const low = s.mirror(up, { plane: og.world, name: "lower_standoff" });

    const screwUp = s.cylinder({ name: "wall_screw_1", radius: 15, height: 8, on: og.frame({ origin: [0, 0, 105], normal: [1, 0, 0] }) });
    s.material(screwUp, { name: "screw", color: "#75787a", metalness: 1, roughness: 0.5 });
    const screwLow = s.mirror(screwUp, { plane: og.world, name: "wall_screw_0" });

    return s.group({ name: "outer_sleeve", children: [plate, channel, up, low, screwUp, screwLow] });
  }

  function middleSection(len = 620, cx = 270) {
    const spine = block("middle_section_body", len, 44, 18, cx, 3, 0);
    s.join(spine, block("middle_web", len, 6, 52, cx, -22, 0));
    s.join(spine, block("top_glide", len, 30, 6, cx, -5, 26));
    s.join(spine, block("bottom_glide", len, 30, 6, cx, -5, -26));
    s.join(spine, block("rear_stop_tab", 18, 52, 48, -40, 3, 0));
    s.material(spine, { name: "dark_steel", color: "#2e3338", metalness: 1, roughness: 0.4 });
    return s.group({ name: "middle_section", children: [spine] });
  }

  function innerSection() {
    const blade = block("inner_blade", 580, 16, 12, 300, 33, 0);
    s.join(blade, block("terminal_end", 30, 40, 40, 605, 35, 0));
    s.material(blade, { name: "steel", color: steel, metalness: 1, roughness: 0.4 });
    return s.group({ name: "inner_section", children: [blade] });
  }
}
