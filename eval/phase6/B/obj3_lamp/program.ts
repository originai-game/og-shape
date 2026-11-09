export default function (og) {
  const s = og.model({ name: "lamp" });
  // overall 595 x 223 x 598.5
  const capW = 223, capH = 25;
  const pedBottom = 140, pedTop = 95, pedH = 130;
  const bar = 24;                 // yellow bar section
  const topZ = 598.5;
  const railEnd = 595 - capW / 2; // rail runs along +X from the post
  const drop = 60, reach = 90;    // front bar of the frame: down and toward -Y

  const ped = pedestal();
  const c = cap();
  const arm = stand();
  return s;

  function pedestal() {
    const p = s.extrude({ name: "pedestal", profile: og.rect({ width: pedBottom, height: pedBottom }), height: pedH, taper: pedTop / pedBottom });
    s.material(p, { color: "#7a1e1e", roughness: 0.7 });
    return p;
  }

  function cap(step = 150, stepH = 30, collar = 40, collarH = 15) {
    const slab = s.box({ name: "cap", width: capW, depth: capW, height: capH, on: og.world.move(0, 0, pedH) });
    const pyr = s.extrude({ name: "cap_step", profile: og.rect({ width: step, height: step }), height: stepH, taper: 0.55, on: og.world.move(0, 0, pedH + capH - 0.5) });
    s.join(slab, pyr);
    const col = s.box({ name: "cap_collar", width: collar, depth: collar, height: collarH, on: og.world.move(0, 0, pedH + capH + stepH - 1) });
    s.join(slab, col);
    s.material(slab, { color: "#55585c", roughness: 0.6 });
    return slab;
  }

  function stand() {
    const postBase = pedH + capH + 30 + 15 - 1 - 0.5;
    const railZ = topZ - bar / 2;
    const post = s.box({ name: "arm", width: bar, depth: bar, height: railZ - postBase, on: og.world.move(0, 0, postBase) });
    const rail = s.box({ name: "rail", width: railEnd + bar / 2, depth: bar, height: bar, on: og.world.move((railEnd - bar / 2) / 2, 0, topZ - bar) });
    s.join(post, rail);
    const x0 = 30, x1 = railEnd - bar / 2;
    const front = s.box({ name: "frame_bar", width: x1 - x0 + bar, depth: bar, height: bar, on: og.world.move((x0 + x1) / 2, -reach, railZ - drop - bar / 2) });
    s.join(post, front);
    for (const [i, x] of [x0, x1].entries()) {
      const link = s.sweep({ name: `frame_link_${i}`, profile: og.rect({ width: bar, height: bar }), path: [[x, 2, railZ + 2], [x, -reach - 2, railZ - drop - 2]] });
      s.join(post, link);
    }
    s.material(post, { color: "#c8b560", roughness: 0.6 });
    return post;
  }
}
