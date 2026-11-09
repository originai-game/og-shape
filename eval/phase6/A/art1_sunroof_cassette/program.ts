export default function (og) {
  const s = og.model({ name: "sunroof_cassette" });
  const length = 1260, width = 840; // x, y
  const frameH = 30, rail = 50, cross = 60;
  const guideH = 8, carrierT = 4, glassT = 6; // frameH + guideH + carrierT + glassT = 48
  const openL = length - 2 * rail, openW = width - 2 * rail;
  const bayL = (openL - cross) / 2; // one opening, x
  const bayX = cross / 2 + bayL / 2; // centre of a bay

  const frame = cassetteFrame();

  const front = panel("front", bayL - 10, openW + 20);
  const fFront = og.frame({ origin: [-bayX, 0, 0] });
  s.at(front, fFront);
  s.joint(front, { name: "front_slide", parent: frame, kind: "prismatic", on: fFront, axis: "-X", range: [0, 40] });

  const rear = panel("rear", bayL - 10, openW + 20);
  const fRear = og.frame({ origin: [bayX, 0, 0] });
  s.at(rear, fRear);
  s.joint(rear, { name: "rear_slide", parent: frame, kind: "prismatic", on: fRear, axis: "+X", range: [0, 40] });
  return s;

  function cassetteFrame() {
    const f = s.box({ name: "frame", width: length, depth: width, height: frameH });
    const opening = s.box({ name: "opening", width: openL, depth: openW, height: frameH + 10, on: og.world.move(0, 0, -5) });
    s.cut(f, opening);
    const cm = s.box({ name: "crossmember", width: cross, depth: openW + 1, height: frameH });
    s.join(f, cm);
    for (const side of [-1, 1]) {
      const g = s.box({
        name: side < 0 ? "guide_left" : "guide_right",
        width: openL, depth: 12, height: guideH + 0.5,
        on: og.world.move(0, side * (openW / 2 + 20), frameH - 0.5),
      });
      s.join(f, g);
    }
    s.material(f, { color: "#1c1c1e", metalness: 0.8, roughness: 0.5 });
    return f;
  }

  // Built about its own origin: the bay centre, z = 0 at the frame's bottom.
  function panel(name, l, w) {
    const zc = frameH + guideH; // carrier bottom
    const carrier = s.box({ name: `${name}_carrier`, width: l - 20, depth: w - 20, height: carrierT, on: og.world.move(0, 0, zc) });
    const shoeY = openW / 2 - 6;
    for (const sx of [-1, 1]) for (const sy of [-1, 1]) {
      const shoe = s.box({
        name: `${name}_shoe_${sx < 0 ? "a" : "b"}${sy < 0 ? "l" : "r"}`,
        width: 40, depth: 14, height: guideH + 0.5,
        on: og.world.move(sx * (l / 2 - 50), sy * shoeY, frameH),
      });
      s.join(carrier, shoe);
    }
    const anchor = s.box({ name: `${name}_anchor`, width: 20, depth: 20, height: guideH + 0.5, on: og.world.move(0, -shoeY + 30, frameH) });
    s.join(carrier, anchor);
    s.material(carrier, { color: "#202024", roughness: 0.7 });
    const glass = s.box({ name: `${name}_glass`, width: l, depth: w, height: glassT, on: og.world.move(0, 0, zc + carrierT) });
    s.material(glass, { color: "#3d5566", metalness: 0.1, roughness: 0.1, opacity: 0.85 });
    return s.group({ name: `${name}_panel`, children: [carrier, glass] });
  }
}
