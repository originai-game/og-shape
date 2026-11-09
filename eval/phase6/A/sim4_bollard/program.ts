export default function (og) {
  const s = og.model({ name: "bollard" });
  const footRadius = 157, height = 960;

  const body = bollard(footRadius, height);
  s.material(body, { color: "#2b2d2f", roughness: 0.6 });
  return s;

  // One turned profile: foot, neck, column, recessed top. Built upright on the world origin.
  function bollard(rf, h, footH = 20, neckR = 128, neckH = 15, colR = 122, rimW = 22, recess = 12) {
    const top = h;
    const profile = og.poly({ points: [
      [0, 0],
      { x: rf, y: 0, radius: 4 },
      { x: rf, y: footH, radius: 4 },
      [neckR, footH],
      [neckR, footH + neckH],
      [colR, footH + neckH],
      { x: colR, y: top, radius: 4 },
      { x: colR - rimW, y: top, radius: 3 },
      [colR - rimW - recess, top - recess],
      [0, top - recess],
    ] });
    return s.revolve({ name: "body", profile, on: og.frame({ origin: [0, 0, 0], normal: [0, -1, 0], up: [0, 0, 1] }) });
  }
}
