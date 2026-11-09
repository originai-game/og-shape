export default function (og) {
  const s = og.model({ name: "long_reach_inspection_arm" });
  const hingeZ = 420; // top of the turntable collar, the first hinge plane
  const lengths = [700, 580, 480, 380];
  const range = [-1.35, 1.35];

  const DARK = { color: "#040405", roughness: 0.6 };
  const STEEL = { color: "#1f2124", metalness: 0.8, roughness: 0.4 };
  const ALU = { color: "#a8adb2", metalness: 0.9, roughness: 0.35 };
  const RUBBER = { color: "#010203", roughness: 0.9 };
  const ORANGE = { color: "#ff6b0f", roughness: 0.5 };
  const GLASS = { color: "#1a3852", metalness: 0.2, roughness: 0.05 };

  const base = basePart();
  let parent = base;
  let x = 0;
  lengths.forEach((L, i) => {
    const last = i === lengths.length - 1;
    const seg = segment(`seg${i}`, L, !last, last);
    const f = og.frame({ origin: [x, 0, hingeZ] });
    s.at(seg, f);
    s.joint(seg, { name: `hinge_${i}`, parent, on: f, axis: "+Z", range });
    parent = seg;
    x += L;
  });
  return s;

  // A box given by its centre, like sand3d's centred extrude.
  function box(name, w, d, h, cx, cy, cz, mat) {
    const b = s.box({ name, width: w, depth: d, height: h, on: og.world.move(cx, cy, cz - h / 2) });
    s.material(b, mat);
    return b;
  }

  // A cylinder given by its centre and radius; along x when `alongX`.
  function cyl(name, r, l, cx, cy, cz, mat, alongX = false) {
    const on = alongX
      ? og.frame({ origin: [cx - l / 2, cy, cz], normal: [1, 0, 0], up: [0, 0, 1] })
      : og.world.move(cx, cy, cz - l / 2);
    const c = s.cylinder({ name, radius: r, height: l, on });
    s.material(c, mat);
    return c;
  }

  // Hinge fork: two plates, a web behind, an orange cap. Returns [steel, cap].
  function fork(p, fx, z) {
    const lower = box(`${p}lower_plate`, 160, 130, 14, fx, 0, z + 7, STEEL);
    const upper = box(`${p}upper_plate`, 160, 130, 14, fx, 0, z + 53, STEEL);
    const web = box(`${p}fork_web`, 45, 130, 60, fx - 72, 0, z + 30, STEEL);
    s.join(lower, web);
    s.join(lower, upper);
    const cap = box(`${p}hinge_cap`, 110, 80, 7, fx, 0, z + 61.5, ORANGE);
    return [lower, cap];
  }

  function basePart() {
    const plate = box("floor_plate", 420, 300, 50, -75, 0, 25, DARK);
    const column = cyl("column", 52, 360, 0, 0, 225, DARK);
    s.join(plate, column);
    const collar = cyl("turntable_collar", 82, 55, 0, 0, 395, STEEL);
    const [forkSteel, cap] = fork("base_", 0, hingeZ);
    s.join(collar, forkSteel);
    return s.group({ name: "base", children: [plate, collar, cap] });
  }

  // Built about its own origin: the hinge axis is the z axis, beam along +x.
  function segment(p, L, distal, camera) {
    const kids = [];
    const beamEnd = L - (distal ? 75 : 35);
    const bl = beamEnd - 15;
    kids.push(cyl(`${p}_proximal_boss`, 37, 18, 0, 0, 30, STEEL));
    kids.push(box(`${p}_beam`, bl, 46, 26, 15 + bl / 2, 0, 30, ALU));
    kids.push(box(`${p}_cable_strip`, bl * 0.88, 14, 8, 30 + bl * 0.44, 0, 45, RUBBER));
    if (distal) kids.push(...fork(`${p}_distal_`, L, 0));
    if (camera) {
      kids.push(box(`${p}_sensor_neck`, 110, 64, 36, L - 12, 0, 30, STEEL));
      kids.push(box(`${p}_camera_body`, 145, 92, 76, L + 65, 0, 30, RUBBER));
      kids.push(cyl(`${p}_lens_barrel`, 34, 40, L + 153, 0, 30, STEEL, true));
      kids.push(cyl(`${p}_glass_face`, 25, 10, L + 176, 0, 30, GLASS, true));
      const leds = [[-32, 55], [32, 55], [-32, 5], [32, 5]].map(([y, z], i) => {
        const led = s.sphere({ name: `${p}_led_${i}`, radius: 8, on: og.world.move(L + 138, y, z - 8) });
        s.material(led, ORANGE);
        return led;
      });
      kids.push(...leds);
    }
    return s.group({ name: p === "seg0" ? "segment_0" : `segment_${p.slice(3)}`, children: kids });
  }
}
