export default function (og) {
  const s = og.model({ name: "bollard" });
  const footR = 157, height = 960;
  const col = bollard(footR, height);
  s.material(col, { color: "#2b2d2f", roughness: 0.55 });
  return s;

  // One turned body: foot, neck, column, recessed top.
  function bollard(fr, h, footH = 16, neckR = 134, neckH = 30, colR = 125, rimW = 18, recess = 12) {
    const ir = colR - rimW;
    const profile = og.poly({ points: [
      [0, 0],
      [fr, 0],
      { x: fr, y: footH, radius: 5 },
      { x: neckR, y: footH + 4, radius: 4 },
      { x: neckR, y: neckH, radius: 3 },
      [colR, neckH + 6],
      { x: colR, y: h, radius: 6 },
      { x: ir, y: h, radius: 4 },
      { x: ir - 8, y: h - recess, radius: 4 },
      [0, h - recess],
    ] });
    return s.revolve({ name: "bollard", profile, on: og.frame({ origin: [0, 0, 0], normal: [0, -1, 0], up: [0, 0, 1] }) });
  }
}
