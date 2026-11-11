// Stream AM (docs/research.md §34): the 54 generated meshes through
// `shape process` keeping their UVs (`keep`, today's default) and with
// `--bake 1024` (fresh UVs packed into one atlas, the look baked), at
// 2 000 and 10 000 triangles; the look's error of each against the
// repaired source (`bake_look`, the unlit preview from fixed views); time
// and peak memory of every run.
//
//   cargo build --release -p shape-cli
//   (cd bench/generated && cargo build --release --bin bake_look)
//   bun bench/generated/bake.ts DIR OUT [--only a,b] [--budgets 2000,10000]
//                               [--sheets a,b,...]
//   bun bench/generated/bake-table.ts OUT/bake.jsonl
//
// DIR as fetch.ts's. Per file: `shape repair` with the guide's repair
// (the bake's source, written to OUT/rep/NAME.glb), then per budget a
// keep run and a bake run (OUT/glb/NAME.MODE.B.glb), then `bake_look`
// of the source against both. `--sheets` names files whose 2 000
// sheet (source, keep, bake; three lit views each) goes to
// OUT/sheets/NAME.png. One JSON line per file in OUT/bake.jsonl.
import { mkdirSync, readFileSync, appendFileSync, writeFileSync, readdirSync } from "node:fs";
import { join, dirname, extname } from "node:path";
import { readManifest } from "./fetch.ts";

const here = dirname(new URL(import.meta.url).pathname);
const shape = process.env.SHAPE ?? join(here, "../../target/release/shape");
const look = process.env.BAKE_LOOK ?? join(here, "target/release/bake_look");
const REPAIR = ["--weld", "0.001", "--floaters", "0.05", "--non-manifold", "--holes", "64", "--fuse"];

function peakKiB(pid: number): number {
  try {
    const m = readFileSync(`/proc/${pid}/status`, "utf8").match(/VmHWM:\s+(\d+) kB/);
    return m ? Number(m[1]) : 0;
  } catch {
    return 0;
  }
}

type Run = { ok: boolean; ms: number; peak_mib: number; out: string; error?: string };

async function run(bin: string, argv: string[], timeoutMs = 900_000): Promise<Run> {
  const t0 = performance.now();
  const child = Bun.spawn([bin, ...argv], { stdout: "pipe", stderr: "pipe" });
  let peak = 0;
  const timer = setInterval(() => {
    peak = Math.max(peak, peakKiB(child.pid));
  }, 5);
  const kill = setTimeout(() => child.kill(), timeoutMs);
  const [out, err, code] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  clearInterval(timer);
  clearTimeout(kill);
  const ms = Math.round(performance.now() - t0);
  const peak_mib = Math.round(peak / 1024);
  if (code !== 0) return { ok: false, ms, peak_mib, out, error: err.trim() || `exit ${code}` };
  return { ok: true, ms, peak_mib, out };
}

function meshFile(dir: string): string {
  const m = readdirSync(dir).find((f) => [".glb", ".gltf", ".obj", ".fbx"].includes(extname(f).toLowerCase()));
  if (!m) throw new Error(`${dir}: no mesh`);
  return join(dir, m);
}

// GPU memory of the maps a report's material lists: RGBA8 with mips.
function textureMiB(material: any): number {
  let b = 0;
  for (const m of material?.maps ?? []) if (m.width && m.height) b += (m.width * m.height * 16) / 3;
  return b / 2 ** 20;
}

if (import.meta.main) {
  const [dir, out] = process.argv.slice(2);
  if (!dir || !out) {
    console.error("usage: bun bench/generated/bake.ts DIR OUT [--only a,b] [--budgets 2000,10000] [--sheets a,b]");
    process.exit(2);
  }
  const opt = (k: string) => {
    const i = process.argv.indexOf(k);
    return i < 0 ? undefined : process.argv[i + 1];
  };
  const only = opt("--only")?.split(",");
  const sheets = opt("--sheets")?.split(",") ?? [];
  const budgets = (opt("--budgets") ?? "2000,10000").split(",").map(Number);
  for (const d of ["rep", "glb", "sheets"]) mkdirSync(join(out, d), { recursive: true });
  const results = join(out, "bake.jsonl");
  if (!only) writeFileSync(results, "");
  for (const e of readManifest(join(here, "manifest.tsv"))) {
    if (only && !only.includes(e.name)) continue;
    const path = meshFile(join(dir, e.name));
    const rep = join(out, "rep", `${e.name}.glb`);
    const r0 = await run(shape, ["repair", path, ...REPAIR, "-o", rep]);
    const runs: any[] = [];
    for (const b of budgets) {
      const outs: Record<string, string> = {};
      for (const mode of ["keep", "bake"]) {
        const o = join(out, "glb", `${e.name}.${mode}.${b}.glb`);
        outs[mode] = o;
        const argv = ["process", path, "--budget", String(b), "-o", o, ...REPAIR];
        if (mode === "bake") argv.push("--bake", "1024");
        const r = await run(shape, argv);
        const rep = r.ok ? JSON.parse(r.out) : null;
        runs.push({
          mode,
          budget: b,
          ok: r.ok,
          error: r.error ?? null,
          ms: r.ms,
          peak_mib: r.peak_mib,
          triangles: rep?.measures?.triangles ?? null,
          closed: rep?.measures?.closed ?? null,
          error_mm: rep?.decimate?.error ?? null,
          diagonal: rep?.measures?.diagonal ?? null,
          unwrap: mode === "bake" ? rep?.unwrap ?? null : null,
          bake: rep?.bake ?? null,
          maps: (rep?.material?.maps ?? []).map((m: any) => m.kind),
          colors: rep?.measures?.colors != null,
          texture_mib: textureMiB(rep?.material),
        });
        console.error(`${e.name} ${mode} ${b}: ${r.ok ? "ok" : r.error} ${r.ms} ms ${r.peak_mib} MiB`);
      }
      const argv = [rep, outs.keep, outs.bake];
      if (b === budgets[0] && sheets.includes(e.name)) argv.push("--sheet", join(out, "sheets", `${e.name}.png`));
      const l = r0.ok ? await run(look, argv) : null;
      const lines = l?.ok ? l.out.trim().split("\n").map((x) => JSON.parse(x)) : [];
      for (const [k, mode] of ["keep", "bake"].entries()) {
        const row = runs.find((x) => x.mode === mode && x.budget === b);
        row.look = lines[k] ? { mean: lines[k].mean, p95: lines[k].p95, both: lines[k].both, one: lines[k].one } : null;
      }
    }
    const row = {
      name: e.name,
      generator: e.generator,
      repair: { ok: r0.ok, ms: r0.ms, peak_mib: r0.peak_mib },
      runs,
    };
    appendFileSync(results, JSON.stringify(row) + "\n");
  }
}
