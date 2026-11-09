export default function (og) {
  const s = og.model({ name: "lamp" });
  const width = 595, depth = 223, height = 598.5; // overall size

  const baseW = depth, pedH = 110, slabH = 25, stepH = 30;
  const bar = 24; // post and rail section
  const postX = -width / 2 + baseW / 2; // post on the base's centre
  const capTop = pedH + slabH + stepH;

  const ped = pedestal(140, 90, pedH);
  const top = cap(baseW, slabH, 130, 60, stepH);
  s.at(top, og.world.move(postX, 0, pedH));
  s.at(ped, og.world.move(postX, 0, 0));

  const a = arm(height - capTop, width / 2 - postX + bar / 2);
  const swivel = og.frame({ origin: [postX, 0, capTop] });
  s.at(a, swivel);
  s.joint(a, { name: "arm_swivel", parent: top, kind: "continuous", on: swivel, axis: "+Z" });

  const f = frame(470, 120);
  const pivot = og.frame({ origin: [width / 2 - 470 / 2, 0, height - bar / 2] });
  s.at(f, pivot);
  s.joint(f, { name: "frame_tilt", parent: a, on: pivot, axis: "+X", range: [og.deg(-45), og.deg(45)] });
  return s;

  function pedestal(bottom, topW, h) {
    const p = s.loft({ name: "pedestal", sections: [
      { profile: og.rect({ width: bottom, height: bottom }), z: 0 },
      { profile: og.rect({ width: topW, height: topW }), z: h },
    ] });
    s.material(p, { color: "#7a1f1f", roughness: 0.7 });
    return p;
  }

  function cap(w, h, stepBottom, stepTop, sh) {
    const slab = s.box({ name: "cap", width: w, depth: w, height: h });
    const step = s.loft({ name: "cap_step", sections: [
      { profile: og.rect({ width: stepBottom, height: stepBottom }), z: 0 },
      { profile: og.rect({ width: stepTop, height: stepTop }), z: sh },
    ], on: og.world.move(0, 0, h - 0.5) });
    s.join(slab, step);
    s.material(slab, { color: "#5a5c60", roughness: 0.6 });
    return slab;
  }

  // Built about the post's foot; the rail runs along +x from the post.
  function arm(postH, railL) {
    const post = s.box({ name: "arm", width: bar, depth: bar, height: postH });
    const rail = s.box({ name: "rail", width: railL, depth: bar, height: bar,
      on: og.world.move(railL / 2 - bar / 2, 0, postH - bar) });
    s.join(post, rail);
    s.material(post, { color: "#b5ad6e", roughness: 0.6 });
    return post;
  }

  // Built about the pivot (rail axis); hangs down and to the front at 45 degrees.
  function frame(w, h, t = 20) {
    const ring = og.subtract(og.rect({ width: w, height: h }), og.rect({ width: w - 2 * t, height: h - 2 * t }));
    const k = 0.70710678;
    const plane = og.frame({ origin: [0, -k * h / 2, -k * h / 2], normal: [0, k, -k], up: [0, -k, -k] });
    const r = s.extrude({ name: "frame", profile: ring, height: t, bothSides: true, on: plane });
    s.material(r, { color: "#b5ad6e", roughness: 0.6 });
    return s.group({ name: "frame_group", children: [r] });
  }
}
