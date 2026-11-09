export default function (og) {
  const s = og.model({ name: "hinged_straps" });
  const length = 914.4, height = 41, curlD = 5.72, plateT = 1.2, totalH = 76.2;
  const pinInset = 20; // pin distance from the strap's left end

  const plateY = curlD / 2 - plateT / 2 - 0.2; // plate face just inside the curl
  const fixed = strap("fixed_strap", -plateY, 0);
  const swing = strap("swing_strap", plateY, totalH - height);

  // pin on the swinging strap's left end, mid height, axis along y
  const pin = og.frame({ origin: [-length / 2 + pinInset, 0, totalH - height + height / 2] });
  s.joint(swing, { name: "strap_pivot", parent: fixed, kind: "continuous", on: pin, axis: "+Y" });
  return s;

  // A strap: plate along x with a rolled curl on its top edge, built in world place.
  function strap(name, plateY, z0, holes = 7, holeD = 4) {
    const plate = s.box({ name, width: length, depth: plateT, height: height - curlD / 2,
      on: og.world.move(0, plateY, z0) });
    const curl = s.tube({ name: name + "_curl", outer: curlD / 2, inner: curlD / 2 - plateT, height: length,
      on: og.frame({ origin: [-length / 2, 0, z0 + height - curlD / 2], normal: [1, 0, 0], up: [0, 0, 1] }) });
    s.join(plate, curl);
    s.hole(plate, { diameter: holeD, at: s.grid({ nx: holes, px: (length - 2 * pinInset) / (holes - 1),
      on: s.face(plate, "front") }) });
    s.material(plate, { color: "#4a4a4a", metalness: 0.8, roughness: 0.45 });
    return plate;
  }
}
