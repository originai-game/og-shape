export default function (og) {
  const s = og.model({ name: "closet" });
  const width = 622.2, depth = 914.6, height = 2023; // overall size
  const wall = 20, doorT = 20, gap = 3;

  const body = caseBody(width, depth, height, wall);

  // door opening inside the case walls; doors sit flush with the front
  const openW = width - 2 * wall, openH = height - 2 * wall;
  const doorW = (openW - 3 * gap) / 2, doorH = openH - 2 * gap;
  const front = -depth / 2, z0 = wall + gap;

  const left = door("left_door", doorW, doorH, doorT, +1);
  const leftHinge = og.frame({ origin: [-width / 2 + wall, front, z0] });
  s.at(left, leftHinge);
  s.joint(left, { name: "left_door_hinge", parent: body, on: leftHinge, axis: "-Z", range: [0, og.deg(90)] });

  const right = door("right_door", doorW, doorH, doorT, -1);
  const rightHinge = og.frame({ origin: [width / 2 - wall, front, z0] });
  s.at(right, rightHinge);
  s.joint(right, { name: "right_door_hinge", parent: body, on: rightHinge, axis: "+Z", range: [0, og.deg(90)] });
  return s;

  function caseBody(w, d, h, t) {
    const b = s.box({ name: "case", width: w, depth: d, height: h });
    s.shell(b, { thickness: t, open: "front" });
    s.material(b, { color: "#5a5c58", metalness: 0.6, roughness: 0.5 });
    return b;
  }

  // Built about its hinge line: the z axis at the door's outer front edge.
  // side = +1 grows towards +x (left door), -1 towards -x (right door).
  function door(name, w, h, t, side) {
    const panel = s.box({ name: name + "_panel", width: w, depth: t, height: h,
      on: og.world.move(side * (w / 2 + gap), t / 2, 0) });
    s.material(panel, { color: "#4b4d4a", metalness: 0.6, roughness: 0.5 });
    return s.group({ name, children: [panel] });
  }
}
