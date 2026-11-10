/**
 * Scale (research §19.6): one outside mesh of a million triangles or more
 * through `import → repair → decimate → unwrap → measure → export`, each
 * step a run of the release `shape` CLI, with its wall time and peak
 * memory (the child's `VmHWM`, sampled while it runs: Linux), and the whole
 * chain once more as `shape process`.
 *
 * Run (bun, from the repository root, after `cargo build --release -p
 * shape-cli`):
 *
 *   bun crates/shape-wasm/ts/tools/scale.ts <mesh.obj> <work dir> [--unit-mm MM] [--densify] [--budget N]
 *
 * `--densify` first splits every triangle into four at its edge
 * midpoints and moves each new vertex halfway to the mean of its two
 * opposite corners' midpoint (a light smoothing, so the new triangles are
 * not coplanar with their parent): a 250 k-triangle scan becomes 1 M.
 */

import { readFileSync, statSync, writeFileSync } from "node:fs";

const args = process.argv.slice(2);
const [input, work] = args;
if (!input || !work) throw new Error("usage: scale.ts <mesh.obj> <work dir> [--unit-mm MM] [--densify] [--budget N]");
const opt = (k: string) => {
  const i = args.indexOf(k);
  return i < 0 ? undefined : args[i + 1];
};
const unit = opt("--unit-mm") ?? "1";
const budget = opt("--budget") ?? "100000";
const shape = new URL("../../../../target/release/shape", import.meta.url).pathname;

function densify(src: string, dst: string): number {
  const v: number[][] = [];
  const f: number[][] = [];
  for (const line of readFileSync(src, "utf8").split("\n")) {
    if (line.startsWith("v ")) v.push(line.slice(2).trim().split(/\s+/).map(Number));
    else if (line.startsWith("f ")) {
      const ids = line.slice(2).trim().split(/\s+/).map((t) => Number.parseInt(t.split("/")[0], 10));
      for (let i = 1; i + 1 < ids.length; i++) f.push([ids[0] - 1, ids[i] - 1, ids[i + 1] - 1]);
    }
  }
  // One midpoint per edge, shared by its two triangles; its two
  // opposite corners pull it off the parent's plane.
  const mid = new Map<string, number>();
  const pull = new Map<number, number[]>();
  const edge = (a: number, b: number, c: number): number => {
    const key = a < b ? `${a},${b}` : `${b},${a}`;
    let m = mid.get(key);
    if (m === undefined) {
      m = v.length;
      v.push([0, 1, 2].map((k) => (v[a][k] + v[b][k]) / 2));
      mid.set(key, m);
      pull.set(m, []);
    }
    pull.get(m)!.push(c);
    return m;
  };
  const out: number[][] = [];
  for (const [a, b, c] of f) {
    const ab = edge(a, b, c);
    const bc = edge(b, c, a);
    const ca = edge(c, a, b);
    out.push([a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]);
  }
  for (const [m, opp] of pull) {
    if (opp.length !== 2) continue;
    const [p, q] = opp;
    const target = [0, 1, 2].map((k) => (v[p][k] + v[q][k]) / 2);
    // Towards the opposite corners' midpoint, a sixteenth of the way.
    v[m] = v[m].map((x, k) => x + (target[k] - x) / 16);
  }
  const lines = v.map((p) => `v ${p[0]} ${p[1]} ${p[2]}`);
  for (const t of out) lines.push(`f ${t[0] + 1} ${t[1] + 1} ${t[2] + 1}`);
  writeFileSync(dst, `${lines.join("\n")}\n`);
  return out.length;
}

type Step = { step: string; ms: number; maxRssMiB: number; report: any };
const steps: Step[] = [];

/** Peak resident memory of a running process (Linux: `VmHWM`), in KiB; 0 elsewhere or once it is gone. */
function peakKiB(pid: number): number {
  try {
    const m = readFileSync(`/proc/${pid}/status`, "utf8").match(/VmHWM:\s+(\d+) kB/);
    return m ? Number(m[1]) : 0;
  } catch {
    return 0;
  }
}

/**
 * Runs one step of the CLI. Its peak memory is the child's own `VmHWM`,
 * read every 5 ms while it runs (bun's `resourceUsage.maxRSS` is the
 * largest of all children so far, not this one's).
 */
async function run(step: string, argv: string[]): Promise<any> {
  const t0 = performance.now();
  const child = Bun.spawn([shape, ...argv], { stdout: "pipe", stderr: "pipe" });
  let peak = 0;
  const timer = setInterval(() => {
    peak = Math.max(peak, peakKiB(child.pid));
  }, 5);
  const [out, err, code] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);
  clearInterval(timer);
  const ms = performance.now() - t0;
  if (code !== 0) throw new Error(`${step}: ${err}`);
  let report: any = {};
  try {
    report = JSON.parse(out);
  } catch {
    report = { text: out.slice(0, 200) };
  }
  const maxRssMiB = peak / 1024;
  steps.push({ step, ms, maxRssMiB, report });
  console.log(`${step.padEnd(10)} ${(ms / 1000).toFixed(2).padStart(8)} s ${maxRssMiB.toFixed(0).padStart(6)} MiB`);
  return report;
}

let mesh = input;
if (args.includes("--densify")) {
  mesh = `${work}/dense.obj`;
  const t0 = performance.now();
  const n = densify(input, mesh);
  console.log(`densified to ${n} triangles (${(statSync(mesh).size / 2 ** 20).toFixed(0)} MiB OBJ) in ${((performance.now() - t0) / 1000).toFixed(1)} s`);
}
const obj = ["--unit-mm", unit];
const imp = await run("import", ["import", mesh, ...obj, "-o", `${work}/in.glb`]);
const rep = await run("repair", ["repair", `${work}/in.glb`, "-o", `${work}/fixed.glb`]);
const dec = await run("decimate", ["decimate", `${work}/fixed.glb`, "--target", budget, "-o", `${work}/low.glb`]);
const unw = await run("unwrap", ["unwrap", `${work}/low.glb`, "-o", `${work}/uv.glb`]);
const mea = await run("measure", ["measure", `${work}/uv.glb`]);
await run("export", ["export", `${work}/uv.glb`, "-o", `${work}/out.stl`]);
const proc = await run("process", ["process", mesh, ...obj, "--budget", budget, "-o", `${work}/processed.glb`]);
const pmea = await run("measure-in", ["measure", mesh, ...obj]);

console.log(
  JSON.stringify(
    {
      triangles_in: imp.triangles,
      repair: rep.repair,
      after_repair: rep.measures,
      decimate: dec.levels,
      unwrap: { charts: unw.charts ?? unw.unwrap?.charts },
      measure_keys: Object.keys(mea),
      process: { decimate: proc.decimate, unwrap: proc.unwrap?.charts, measures: proc.measures },
      measure_in: pmea.whole,
    },
    null,
    1,
  ),
);
writeFileSync(`${work}/scale.json`, JSON.stringify(steps.map((s) => ({ step: s.step, ms: Math.round(s.ms), maxRssMiB: Math.round(s.maxRssMiB) }))));
