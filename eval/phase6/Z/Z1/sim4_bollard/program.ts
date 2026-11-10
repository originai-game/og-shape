export default function (og) {
  const s = og.model({ name: "bollard" });
  const footR = 157, footH = 14, footTop = 24;
  const neckR = 116, neckTop = 46;
  const colR = 122, H = 960;
  const lip = 14, recess = 12;

  const profile = og.poly({ points: [
    [0, 0],
    [footR, 0],
    { x: footR, y: footH, radius: 3 },
    { x: neckR + 10, y: footTop, radius: 4 },
    [neckR, footTop],
    [neckR, neckTop],
    [colR, neckTop],
    { x: colR, y: H, radius: 4 },
    { x: colR - lip, y: H, radius: 2 },
    { x: colR - lip - 6, y: H - recess, radius: 3 },
    [0, H - recess],
  ] });
  const body = s.revolve({ name: "bollard", profile, axis: "+Z", segments: 64 });
  s.material(body, { color: "#2b2d2f", roughness: 0.6 });
  return s;
}
