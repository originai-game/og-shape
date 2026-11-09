// Lamp on a rail stand, translated from the sand3d program `railstand`.
// World positions follow the sand3d translations (pedestal at z = -558).
export default function (og) {
  const s = og.model({ name: "railstand" });

  const base = [282.5, -52.5, -558];        // pedestal origin
  const capAt = [282.5, -52, -439];         // tiered cap origin
  const railAt = add(capAt, [-243, 10, 181]); // post rail origin
  const frameAt = add(railAt, [0, 71, 157.5]); // open frame origin

  const ped = pedestal(base);
  const cap = tieredCap(capAt);
  const rail = postRail(railAt);
  const frame = openFrame(frameAt);
  s.assembly({ name: "railstand", children: [ped, cap, rail, frame] });
  return s;

  function add(a, b) { return [a[0] + b[0], a[1] + b[1], a[2] + b[2]]; }
  function at(o, x = 0, y = 0, z = 0) { return og.world.move(o[0] + x, o[1] + y, o[2] + z); }

  // A square section that changes from `lower` to `upper` over `height`
  // (sand3d's tapered extrude of a square).
  function J(a, b, _tag) { s.join(a, b); }

  function squareDraft(name, lower, upper, height, on) {
    return s.loft({
      name, on,
      sections: [
        { profile: og.rect({ width: lower, height: lower }), z: 0 },
        { profile: og.rect({ width: upper, height: upper }), z: height },
      ],
    });
  }

  // A w x d rectangle centred at (0, y) in the profile plane.
  function rectAt(w, d, y) {
    w -= 0.2; d -= 0.2; // inset: coplanar faces with the bars fail the join
    return og.poly({ points: [[-w / 2, y - d / 2], [w / 2, y - d / 2], [w / 2, y + d / 2], [-w / 2, y + d / 2]] });
  }

  function pedestal(o, w = 109, h = 150, footH = 34, neck = 77) {
    const foot = squareDraft("pedestal", 0.5, w, footH, at(o));
    const body = squareDraft("pedestal_body", w, neck, h - footH, at(o, 0, 0, footH - 0.1));
    J(foot, body, "ped");
    s.material(foot, { name: "painted_metal", color: "#a53f3f" });
    return foot;
  }

  function tieredCap(o, w = 208, h = 197, neck = 76, shoulder = 124, tier = 118, crown = 86, collar = 28) {
    const cap = squareDraft("cap", neck, shoulder, 48, at(o, 0, 0, 31));
    const pieces = [
      squareDraft("lip", shoulder, w, 3.2, at(o, 0, 0, 78.9)),
      squareDraft("edge", w, w - 10, 13.2, at(o, 0, 0, 81.9)),
      squareDraft("ledge", w - 10, tier, 3.2, at(o, 0, 0, 94.9)),
      squareDraft("tier", tier, crown, 35.2, at(o, 0, 0, 97.9)),
      squareDraft("peak", crown, collar, 28.2, at(o, 0, 0, 132.9)),
      s.box({ name: "collar", width: collar, depth: 32, height: h - 157, on: at(o, -8, 6, 157) }),
    ];
    for (const [i, p] of pieces.entries()) J(cap, p, "cap" + i);
    s.material(cap, { name: "painted_metal", color: "#7e7e7e" });
    return cap;
  }

  function postRail(o, w = 496, d = 34, h = 282, bar = 16, postW = 26, postX = 235.5, endW = 24, linkH = 112, shift = 51) {
    const rail = s.box({ name: "post_rail", width: w, depth: d, height: bar, on: at(o, 0, 0, h - bar) });
    const post = s.box({ name: "post", width: postW, depth: 31, height: h, on: at(o, postX, -2, 0) });
    const end = s.loft({
      name: "end_link", on: at(o, -w / 2 + endW / 2, 0, h - linkH),
      sections: [
        { profile: rectAt(endW, d, shift), z: 0 },
        { profile: rectAt(endW, d, 0), z: linkH },
      ],
    });
    J(rail, post, "post");
    J(rail, end, "end");
    s.material(rail, { name: "painted_metal", color: "#e7d690" });
    return rail;
  }

  function openFrame(o, w = 496, h = 141, bar = 20, depth = 34, endW = 24, shift = 42) {
    const frame = s.box({ name: "open_frame", width: w, depth, height: bar, on: at(o, 0, shift / 2, h - bar) });
    const bottom = s.box({ name: "frame_bottom", width: w, depth, height: bar, on: at(o, 0, -shift / 2, 0) });
    const link = s.loft({
      name: "slanted_link", on: at(o, -w / 2 + endW / 2, 0, 0),
      sections: [
        { profile: rectAt(endW, depth, -shift / 2), z: 0 },
        { profile: rectAt(endW, depth, shift / 2), z: h },
      ],
    });
    const other = s.loft({
      name: "slanted_link_right", on: at(o, w / 2 - endW / 2, 0, 0),
      sections: [
        { profile: rectAt(endW, depth, -shift / 2), z: 0 },
        { profile: rectAt(endW, depth, shift / 2), z: h },
      ],
    });
    J(frame, bottom, "fb");
    J(frame, link, "fl");
    J(frame, other, "fo");
    s.material(frame, { name: "painted_metal", color: "#e7d690" });
    return frame;
  }
}
