export default function (og) {
  const s = og.model({ name: "telescoping_slide" });
  // world: wall plate at +X end (x 0..-30), slide runs out along -X to x = -1100
  const plateT = 30, plateW = 240, plateH = 300;
  const zc = plateH / 2;            // slide centre height
  const standoffL = 40;
  const outerX0 = -plateT - standoffL, outerL = 500;
  const outerW = 60, outerH = 80, wall = 4, slotH = 30;
  const midX0 = -120, midL = 600, midW = 48, midH = 68, midWall = 3;
  const bladeX0 = -300, bladeX1 = -1070, bladeW = 12, bladeH = 16;
  const tip = 30;

  const B = (name, x0, x1, y0, y1, z0, z1) =>
    s.box({ name, width: Math.abs(x1 - x0), depth: y1 - y0, height: z1 - z0,
      on: og.world.move((x0 + x1) / 2, (y0 + y1) / 2, z0) });

  // wall plate + screws
  const plate = B("wall_plate", -plateT, 0, -plateW / 2, plateW / 2, 0, plateH);
  s.chamfer(plate, { distance: 3, edges: "side" });
  s.material(plate, { color: "#c9c4b0", roughness: 0.8 });
  const screwFrames = [og.world.move(-plateT + 0.5, 80, 245), og.world.move(-plateT + 0.5, 80, 55)]
    .map(f => og.frame({ origin: f.origin, normal: [-1, 0, 0] }));
  const screws = screwFrames.map((f, i) => {
    const sc = s.cylinder({ name: `screw_head_${i + 1}`, radius: 11, height: 6, segments: 20, on: f });
    s.material(sc, { color: "#5a5d61", metalness: 1, roughness: 0.4 });
    return sc;
  });

  // standoffs between plate and sleeve
  const so1 = B("standoff_upper", outerX0 - 1, -plateT + 0.5, -15, 15, zc + 10, zc + 35);
  const so2 = B("standoff_lower", outerX0 - 1, -plateT + 0.5, -15, 15, zc - 35, zc - 10);
  s.join(so1, so2);
  s.material(so1, { color: "#7d8085", metalness: 0.8, roughness: 0.5 });

  // outer C-channel sleeve, open slot on its -Y face
  const outer = B("outer_sleeve", outerX0 - outerL, outerX0, -outerW / 2, outerW / 2, zc - outerH / 2, zc + outerH / 2);
  const bore = B("outer_bore", outerX0 - outerL - 1, outerX0 + 1, -outerW / 2 + wall, outerW / 2 - wall, zc - outerH / 2 + wall, zc + outerH / 2 - wall);
  s.cut(outer, bore);
  const slot = B("outer_slot", outerX0 - outerL - 1, outerX0 - wall - 20, -outerW / 2 - 1, -outerW / 2 + wall + 1, zc - slotH / 2, zc + slotH / 2);
  s.cut(outer, slot);
  s.material(outer, { color: "#8e9399", metalness: 0.9, roughness: 0.35 });

  // middle section
  const mid = B("middle_section", midX0 - midL, midX0, -midW / 2, midW / 2, zc - midH / 2, zc + midH / 2);
  const midBore = B("middle_bore", midX0 - midL - 1, midX0 + 1, -midW / 2 + midWall, midW / 2 - midWall, zc - midH / 2 + midWall, zc + midH / 2 - midWall);
  s.cut(mid, midBore);
  s.material(mid, { color: "#2a2b2e", metalness: 0.6, roughness: 0.5 });
  const midG = s.group({ name: "middle", children: [mid] });
  s.at(midG, og.world);
  s.joint(midG, { name: "middle_slide", parent: outer, kind: "prismatic", on: og.world, axis: "-X", range: [0, 260] });

  // inner blade with end block
  const blade = B("inner_blade", bladeX1 - 0.5, bladeX0, -bladeW / 2, bladeW / 2, zc - bladeH / 2, zc + bladeH / 2);
  const block = B("end_block", bladeX1 - tip, bladeX1, -tip / 2, tip / 2, zc - tip / 2, zc + tip / 2);
  s.join(blade, block);
  s.material(blade, { color: "#9aa0a6", metalness: 0.9, roughness: 0.3 });
  const innerG = s.group({ name: "inner", children: [blade] });
  s.at(innerG, og.world);
  s.joint(innerG, { name: "inner_slide", parent: midG, kind: "prismatic", on: og.world, axis: "-X", range: [0, 220] });

  return s;
}
