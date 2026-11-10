// Stream AJ's runner (docs/research.md §31): every mesh fetched by
// fetch.ts placed by `shape place` as a game wants it, standing on its
// base at a real size, facing front.
//
//   cargo build --release -p shape-cli
//   bun bench/generated/fetch.ts DIR
//   bun bench/generated/place.ts DIR OUT
//
// Per file, into OUT/place.jsonl, and the tables on stdout:
// - `before`: `shape measure FILE --axes`, the file read plainly (OBJ
//   and glTF Y up), whole-file axis measures;
// - `place`: `shape place FILE --up U --front F --pivot base --DIM L -o
//   OUT/place/NAME.glb`, with the generator's frame (FRAMES) and the
//   object's real size (SIZES);
// - `after`: `shape measure OUT/place/NAME.glb --axes`, the written file
//   read back;
// - `readings`: for three simple readings of the `before` numbers, the
//   up each would choose, against the up the generator's frame gives.
// Nothing here judges; the tables print the numbers. The sizes are a
// person's guess of the real object, written down so they can be argued
// with.
import { existsSync, mkdirSync, readdirSync, writeFileSync, appendFileSync } from "node:fs";
import { join, dirname, extname } from "node:path";
import { readManifest } from "./fetch.ts";

const here = dirname(new URL(import.meta.url).pathname);
const shape = join(here, "../../target/release/shape");

// The file's own axes that are up and front, per generator, found by
// previewing the cabinet (120), the horse (420) and the armchair (480) of
// each: the side the prompt image shows is the front.
export const FRAMES: Record<string, { up: string; front: string }> = {
  "Hunyuan3D-2": { up: "+y", front: "+z" },
  TRELLIS: { up: "+y", front: "+z" },
  SPAR3D: { up: "+y", front: "-z" },
  CRM: { up: "+y", front: "-z" },
  TripoSR: { up: "+z", front: "+x" },
  InstantMesh: { up: "-z", front: "+x" },
  Unique3D: { up: "+y", front: "+z" },
  Meshy: { up: "+y", front: "+z" },
};

// Real sizes: which extent, in metres. Hi3DBench's prompts by id; Meshy's
// by name.
const PROMPTS: Record<string, [string, number, string]> = {
  "5": ["height", 6.0, "a house"],
  "30": ["height", 0.1, "four mugs"],
  "120": ["height", 1.2, "a dish cabinet"],
  "300": ["height", 0.35, "a sitting teddy bear"],
  "420": ["height", 2.0, "a galloping horse, head up"],
  "480": ["height", 1.2, "a carved armchair"],
};
const MESHY: Record<string, [string, number, string]> = {
  airliner_toy: ["longest", 0.3, "a toy airliner"],
  boat: ["longest", 4.0, "a rowing boat"],
  barn: ["height", 8.0, "a barn"],
  cottage: ["height", 6.0, "a cottage"],
  cat1: ["height", 0.3, "a cat"],
  tree1: ["height", 6.0, "a tree"],
  palm_tree: ["height", 8.0, "a palm tree"],
  army_man: ["height", 1.8, "a soldier"],
  businessman: ["height", 1.8, "a man"],
  robot_mech1: ["height", 2.5, "a mech"],
  chair_wooden: ["height", 0.9, "a wooden chair"],
  chest2: ["longest", 0.9, "a chest"],
  sword1: ["longest", 1.0, "a sword"],
  car_police: ["longest", 4.8, "a police car"],
  bus_school: ["longest", 11.0, "a school bus"],
  burger3: ["longest", 0.12, "a burger"],
};

export function size(name: string): [string, number, string] {
  if (name.startsWith("meshy-")) return MESHY[name.slice(6)];
  return PROMPTS[name.split("-").pop()!];
}

function cli(argv: string[]): any {
  const r = Bun.spawnSync([shape, ...argv], { stdout: "pipe", stderr: "pipe" });
  if (r.exitCode !== 0) throw new Error(`shape ${argv.join(" ")}: ${r.stderr.toString()}`);
  return JSON.parse(r.stdout.toString());
}

function meshFile(dir: string): string {
  const m = readdirSync(dir).find((f) => [".glb", ".gltf", ".obj", ".fbx"].includes(extname(f).toLowerCase()));
  if (!m) throw new Error(`${dir}: no mesh`);
  return join(dir, m);
}

const AXES = ["+x", "-x", "+y", "-y", "+z", "-z"];
const opposite = (a: string) => (a[0] === "+" ? "-" : "+") + a[1];

// Three readings of the numbers an agent sees, each naming an up.
export function readings(a: any): Record<string, string> {
  const f = a.facing;
  // Mass low: the axis the mass sits furthest from the middle along; up
  // is away from the side it sits on.
  let k = 0;
  const m = a.mass.in_box;
  for (const j of [1, 2]) if (Math.abs(m[j] - 0.5) > Math.abs(m[k] - 0.5)) k = j;
  const mass_low = (m[k] < 0.5 ? "+" : "-") + "xyz"[k];
  // Most area facing down: up is opposite the direction most area faces.
  let most = AXES[0];
  for (const d of AXES) if (f[d] > f[most]) most = d;
  // A flat base: up is opposite the direction that faces more than its
  // opposite by the most.
  let flat = AXES[0];
  for (const d of AXES) if (f[d] - f[opposite(d)] > f[flat] - f[opposite(flat)]) flat = d;
  return { mass_low, most_area_down: opposite(most), flat_base_down: opposite(flat) };
}

if (import.meta.main) {
  const [dir, out] = process.argv.slice(2);
  if (!dir || !out) {
    console.error("usage: bun bench/generated/place.ts DIR OUT");
    process.exit(2);
  }
  mkdirSync(join(out, "place"), { recursive: true });
  const results = join(out, "place.jsonl");
  writeFileSync(results, "");
  const rows: any[] = [];
  for (const e of readManifest(join(here, "manifest.tsv"))) {
    const path = meshFile(join(dir, e.name));
    const frame = FRAMES[e.generator];
    const [dim, metres, what] = size(e.name);
    const before = cli(["measure", path, "--axes"]).axes;
    const glb = join(out, "place", `${e.name}.glb`);
    const t0 = performance.now();
    const placed = cli([
      "place", path, "--up", frame.up, "--front", frame.front,
      "--pivot", "base", `--${dim}`, `${metres}m`, "-o", glb,
    ]);
    const ms = Math.round(performance.now() - t0);
    const after = cli(["measure", glb, "--axes"]).axes;
    // The up the frame gives, in the plain reading's axes.
    const truth = placed.place.up;
    const read = readings(before);
    const extent = (a: any) => (dim === "longest" ? Math.max(...a.size) : a.size[{ height: 2, width: 0, depth: 1 }[dim]!]);
    const row = {
      name: e.name,
      generator: e.generator,
      category: e.category,
      what,
      dim,
      metres,
      frame,
      truth,
      readings: read,
      before,
      place: placed.place,
      after,
      // The written GLB (f32, metres) read back.
      base_z: after.pivot_from_base[2],
      base_offset: Math.hypot(...after.pivot_from_base),
      extent_m: extent(after) / 1000,
      longest_before_m: Math.max(...before.size) / 1000,
      ms,
    };
    rows.push(row);
    appendFileSync(results, JSON.stringify(row) + "\n");
  }
  table(rows);
}

export function table(rows: any[]) {
  const by = new Map<string, any[]>();
  for (const r of rows) by.set(r.generator, [...(by.get(r.generator) ?? []), r]);
  const fmt = (x: number, d = 2) => x.toFixed(d);
  console.log("| generator | files | frame (up, front) | longest side before (m) | origin above base before (of height) | origin from base after (max, mm) | size asked = size after (files; worst rel. error) | height after (m, range) |");
  console.log("|---|---|---|---|---|---|---|---|");
  for (const [g, rs] of by) {
    const lb = rs.map((r) => r.longest_before_m);
    const frac = rs.map((r) => r.before.pivot_from_base[2] / r.before.size[2]);
    const off = Math.max(...rs.map((r) => r.base_offset));
    const rel = rs.map((r) => Math.abs(r.extent_m - r.metres) / r.metres);
    const ok = rel.filter((x) => x < 1e-6).length;
    const h = rs.map((r) => r.after.size[2] / 1000);
    console.log(
      `| ${g} | ${rs.length} | ${rs[0].frame.up}, ${rs[0].frame.front} | ${fmt(Math.min(...lb))}–${fmt(Math.max(...lb))} | ${fmt(Math.min(...frac))}–${fmt(Math.max(...frac))} | ${off.toExponential(1)} | ${ok}; ${Math.max(...rel).toExponential(1)} | ${fmt(Math.min(...h))}–${fmt(Math.max(...h))} |`,
    );
  }
  console.log();
  console.log("| reading | right up (of 54) | " + [...by.keys()].join(" | ") + " |");
  console.log("|---|---|" + [...by.keys()].map(() => "---").join("|") + "|");
  for (const k of ["mass_low", "most_area_down", "flat_base_down"]) {
    const right = (rs: any[]) => rs.filter((r) => r.readings[k] === r.truth).length;
    console.log(`| ${k} | ${right(rows)} | ` + [...by.values()].map((rs) => `${right(rs)}/${rs.length}`).join(" | ") + " |");
  }
  const asRead = (rs: any[]) => rs.filter((r) => r.truth === "+z").length;
  console.log(`| as read (Y up) | ${asRead(rows)} | ` + [...by.values()].map((rs) => `${asRead(rs)}/${rs.length}`).join(" | ") + " |");
  console.log();
  console.log("| file | true up (as read) | mass_low | most_area_down | flat_base_down | mass in box (x, y, z) |");
  console.log("|---|---|---|---|---|---|");
  for (const r of rows.filter((r) => /^(triposr|instantmesh)-(420|480)$/.test(r.name))) {
    console.log(
      `| ${r.name} | ${r.truth} | ${r.readings.mass_low} | ${r.readings.most_area_down} | ${r.readings.flat_base_down} | ${r.before.mass.in_box.map((x: number) => fmt(x)).join(", ")} |`,
    );
  }
  console.log();
  console.log("| file | object | asked | height after (m) | longest after (m) |");
  console.log("|---|---|---|---|---|");
  for (const r of rows) {
    console.log(`| ${r.name} | ${r.what} | ${r.dim} ${r.metres} m | ${fmt(r.after.size[2] / 1000)} | ${fmt(Math.max(...r.after.size) / 1000)} |`);
  }
}
