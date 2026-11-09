export default function (og) {
  const s = og.model({ name: "hinged_straps" });
  const length = 914.4, depth = 5.72, height = 76.2;
  const strapH = 41;                       // one strap, plate + curl
  const rise = height - strapH;            // the second strap sits this much higher
  const pinInset = 12, pinZ = strapH / 2;  // pin near the left end, mid-height

  const fixed = strap("fixed_strap");
  const pivot = og.frame({ origin: [-length / 2 + pinInset, 0, rise + pinZ], normal: [0, -1, 0], up: [0, 0, 1] });
  const swing = swingStrap();
  s.at(swing, og.frame({ origin: pivot.origin }));
  s.joint(swing, { name: "strap_spin", parent: fixed, kind: "continuous", on: og.frame({ origin: pivot.origin }), axis: "+Y" });
  return s;

  // A strap about its own origin: bottom centre of the plate, length along x.
  function strap(name, at = og.world, curlR = depth / 2, wall = 1.2, plateT = 1.0, holeD = 4, holes = 7, pitch = 140) {
    const curlZ = strapH - curlR;
    const plate = s.box({ name, width: length, depth: plateT, height: curlZ + 0.5,
      on: at.move(0, -(curlR - (wall - plateT) / 2 - plateT / 2) , 0) });
    s.hole(plate, { diameter: holeD, at: s.grid({ nx: holes, px: pitch, on: s.face(plate, "front") }) });
    const curl = s.tube({ name: name + "_curl", outer: curlR, inner: curlR - wall, height: length,
      on: og.frame({ origin: at.move(-length / 2, 0, curlZ).origin, normal: [1, 0, 0] }) });
    s.join(plate, curl);
    s.material(plate, { color: "#4a4a4c", metalness: 0.8, roughness: 0.45 });
    return plate;
  }

  // The swinging strap, built with its pin at the group's origin.
  function swingStrap() {
    const p = strap("swing_strap", og.world.move(length / 2 - pinInset, 0, -pinZ));
    return s.group({ name: "swing", children: [p] });
  }
}
