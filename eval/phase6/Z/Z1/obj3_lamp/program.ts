export default function (og) {
  const s = og.model({ name: "lamp" });
  const capW = 223, pedH = 100, pedBot = 130, pedTop = 100;
  const plateH = 20, stepW = 150, stepH = 30;
  const bar = 28, top = 598.5, railEnd = 595 - capW / 2;
  const frameY = -90, frameZ = 470, linkX0 = 30;

  const ped = pedestal();
  const cap = capPart();
  const arm = armPart();
  return s;

  function pedestal() {
    const p = s.extrude({ name: "pedestal", profile: og.rect({ width: pedBot, height: pedBot }), height: pedH, taper: 1 - pedTop / pedBot });
    s.material(p, { color: "#7a1e1e", roughness: 0.7 });
    return p;
  }

  function capPart() {
    const plate = s.box({ name: "cap", width: capW, depth: capW, height: plateH, on: og.world.move(0, 0, pedH) });
    const step = s.extrude({ name: "cap_step", profile: og.rect({ width: stepW, height: stepW }), height: stepH, taper: 0.5, on: og.world.move(0, 0, pedH + plateH - 1) });
    s.join(plate, step);
    s.material(plate, { color: "#555a5e", roughness: 0.6 });
    return plate;
  }

  function armPart() {
    const z0 = pedH + plateH + stepH - 2;
    const post = s.box({ name: "arm", width: bar, depth: bar, height: top - z0, on: og.world.move(0, 0, z0) });
    const rail = s.box({ name: "rail", width: railEnd + bar / 2, depth: bar, height: bar, on: og.world.move((railEnd - bar / 2) / 2, 0, top - bar) });
    s.join(post, rail);
    const zr = top - bar / 2;
    const frame = s.sweep({
      name: "frame",
      profile: og.rect({ width: bar, height: bar }),
      path: [[linkX0, 0, zr], [linkX0, frameY, frameZ], [railEnd - bar / 2, frameY, frameZ], [railEnd - bar / 2, 0, zr]],
      up: [0, 0, 1],
    });
    s.join(post, frame);
    s.material(post, { color: "#a89f6a", roughness: 0.6 });
    return post;
  }
}
