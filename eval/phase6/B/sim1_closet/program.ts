export default function (og) {
  const s = og.model({ name: "closet" });
  const width = 622.2, depth = 914.6, height = 2023;
  const wall = 20, doorT = 18, gap = 3;
  const doorW = (width - 2 * wall - 3 * gap) / 2;
  const doorH = height - 2 * wall - 2 * gap;
  const grey = "#5a5c58";

  const box = body(width, depth, height);

  // hinge lines: front outer edges of the opening, at rest
  const leftHinge = og.frame({ origin: [-width / 2 + wall + gap, -depth / 2, wall + gap] });
  const rightHinge = og.frame({ origin: [width / 2 - wall - gap, -depth / 2, wall + gap] });

  const left = door("left_door", +1);
  s.at(left, leftHinge);
  s.joint(left, { name: "left_door_hinge", parent: box, on: leftHinge, axis: "-Z", range: [0, og.deg(90)] });

  const right = door("right_door", -1);
  s.at(right, rightHinge);
  s.joint(right, { name: "right_door_hinge", parent: box, on: rightHinge, axis: "+Z", range: [0, og.deg(90)] });
  return s;

  function body(w, d, h) {
    const b = s.box({ name: "case", width: w, depth: d, height: h });
    s.shell(b, { thickness: wall, open: "front" });
    s.material(b, { color: grey, metalness: 0.6, roughness: 0.5 });
    return b;
  }

  // Built about its hinge: the hinge line on z, door grows toward +X (dir=+1) or -X (dir=-1) and back into +Y.
  function door(name, dir) {
    const panel = s.box({ name: name + "_panel", width: doorW, depth: doorT, height: doorH, on: og.world.move(dir * doorW / 2, doorT / 2, 0) });
    s.material(panel, { color: "#4b4d4a", metalness: 0.6, roughness: 0.5 });
    return s.group({ name, children: [panel] });
  }
}
