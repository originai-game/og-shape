// Stream AG's runner (docs/research.md §28): every mesh fetched by
// fetch.ts through the release `shape` CLI, as a game asset would go.
//
//   cargo build --release -p shape-cli
//   bun bench/generated/fetch.ts DIR
//   bun bench/generated/run.ts DIR OUT [--only NAME,...] [--budgets 2000,10000,50000]
//                              [--merge-charts UV] [--modes keep,fresh]
//   bun bench/generated/table.ts OUT/results.jsonl
//
// Per file, into OUT/results.jsonl (one JSON object a line):
// - `file`: what the file itself says, read here from its bytes (glTF
//   JSON or OBJ/MTL text): material maps present, compression and other
//   extensions, vertex colours, UV sets, UV islands (triangles joined by
//   a shared corner: same position and same UV);
// - `import`, `measure`: `shape import` and `shape measure
//   --intersections`, as they print them;
// - `runs`: `shape process --budget B` for each budget, keeping the
//   file's UVs and texture (`keep`) and with `--unwrap` (`fresh`), each
//   with the repair the guide uses (weld 1 µm, floaters 5 %, non-manifold
//   edges split, holes up to 64 edges, fuse) and `--intersections`;
//   `--merge-charts UV` passes it to the keep runs (§32); the result's texture from
//   `shape import` of the output; wall time and peak memory (the child's
//   VmHWM, read every 5 ms);
// - `lods`: `shape decimate --ratio 0.5 --lods 3` of the largest `keep`
//   output.
// Nothing here judges; the table prints the numbers.
import { existsSync, mkdirSync, readFileSync, appendFileSync, writeFileSync } from "node:fs";
import { join, dirname, extname } from "node:path";
import { readManifest } from "./fetch.ts";

const here = dirname(new URL(import.meta.url).pathname);
const shape = join(here, "../../target/release/shape");
const REPAIR = ["--weld", "0.001", "--floaters", "0.05", "--non-manifold", "--holes", "64", "--fuse"];

function peakKiB(pid: number): number {
  try {
    const m = readFileSync(`/proc/${pid}/status`, "utf8").match(/VmHWM:\s+(\d+) kB/);
    return m ? Number(m[1]) : 0;
  } catch {
    return 0;
  }
}

type Run = { ok: boolean; ms: number; peak_mib: number; report?: any; error?: string };

async function cli(argv: string[], timeoutMs = 600_000): Promise<Run> {
  const t0 = performance.now();
  const child = Bun.spawn([shape, ...argv], { stdout: "pipe", stderr: "pipe" });
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
  if (code !== 0) {
    const error = err.trim() || (child.signalCode ? `killed (${child.signalCode}) after ${ms} ms` : `exit ${code}`);
    return { ok: false, ms, peak_mib, error };
  }
  return { ok: true, ms, peak_mib, report: out.trim() ? JSON.parse(out) : null };
}

// ── What the file says ─────────────────────────────────────────────────

type FileFacts = {
  format: string;
  bytes: number;
  generator_tag: string | null;
  maps: Record<string, number>; // material map → materials that have it
  extensions: string[];
  compression: string[];
  vertex_colours: boolean;
  uv_sets: number;
  uv_islands: number | null;
  materials: number;
  primitives: number;
};

const MAP_KEYS: [string, (m: any) => any][] = [
  ["base_colour", (m) => m.pbrMetallicRoughness?.baseColorTexture],
  ["metallic_roughness", (m) => m.pbrMetallicRoughness?.metallicRoughnessTexture],
  ["normal", (m) => m.normalTexture],
  ["occlusion", (m) => m.occlusionTexture],
  ["emissive", (m) => m.emissiveTexture],
];

function islands(n: number, tris: Uint32Array | number[], key: (v: number) => string): number {
  // Union-find over triangles through corners with the same key.
  const id = new Map<string, number>();
  const parent: number[] = [];
  const find = (a: number): number => {
    while (parent[a] !== a) {
      parent[a] = parent[parent[a]];
      a = parent[a];
    }
    return a;
  };
  const owner = new Map<number, number>();
  for (let t = 0; t < n; t++) parent.push(t);
  for (let t = 0; t < n; t++) {
    for (let c = 0; c < 3; c++) {
      const k = key(tris[3 * t + c]);
      let v = id.get(k);
      if (v === undefined) {
        v = id.size;
        id.set(k, v);
      }
      const o = owner.get(v);
      if (o === undefined) owner.set(v, t);
      else {
        const a = find(o);
        const b = find(t);
        if (a !== b) parent[a] = b;
      }
    }
  }
  let roots = 0;
  for (let t = 0; t < n; t++) if (find(t) === t) roots++;
  return roots;
}

function gltfFacts(bytes: Uint8Array): FileFacts {
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const jsonLen = dv.getUint32(12, true);
  const json = JSON.parse(new TextDecoder().decode(bytes.subarray(20, 20 + jsonLen)));
  const binAt = 20 + jsonLen + 8;
  const bin = bytes.subarray(binAt);
  const maps: Record<string, number> = {};
  for (const m of json.materials ?? []) {
    for (const [k, f] of MAP_KEYS) if (f(m)) maps[k] = (maps[k] ?? 0) + 1;
    for (const e of Object.keys(m.extensions ?? {})) maps[e] = (maps[e] ?? 0) + 1;
  }
  const extensions = [...new Set([...(json.extensionsUsed ?? []), ...(json.extensionsRequired ?? [])])].sort();
  const compression = extensions.filter((e) => /draco|meshopt/i.test(e));
  let vertex_colours = false;
  let uv_sets = 0;
  let primitives = 0;
  let uvIslands: number | null = 0;
  const acc = (i: number) => {
    const a = json.accessors[i];
    const v = json.bufferViews?.[a.bufferView];
    if (!v || v.buffer !== 0 || a.sparse) return null;
    const comps = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4 }[a.type as string]!;
    const size = { 5120: 1, 5121: 1, 5122: 2, 5123: 2, 5125: 4, 5126: 4 }[a.componentType as number]!;
    const stride = v.byteStride ?? comps * size;
    const off = (v.byteOffset ?? 0) + (a.byteOffset ?? 0);
    return { a, comps, size, stride, off };
  };
  for (const mesh of json.meshes ?? []) {
    for (const p of mesh.primitives ?? []) {
      primitives++;
      const at = p.attributes ?? {};
      if ("COLOR_0" in at) vertex_colours = true;
      uv_sets = Math.max(uv_sets, Object.keys(at).filter((k) => k.startsWith("TEXCOORD_")).length);
      if (uvIslands === null || !("TEXCOORD_0" in at) || p.extensions) {
        if (!("TEXCOORD_0" in at)) continue;
        uvIslands = null;
        continue;
      }
      const pos = acc(at.POSITION);
      const uv = acc(at.TEXCOORD_0);
      const ind = p.indices !== undefined ? acc(p.indices) : null;
      if (!pos || !uv || (p.indices !== undefined && !ind) || (p.mode ?? 4) !== 4) {
        uvIslands = null;
        continue;
      }
      const count = ind ? ind.a.count : pos.a.count;
      const tris = new Uint32Array(count);
      for (let k = 0; k < count; k++) {
        if (!ind) tris[k] = k;
        else {
          const o = bin.byteOffset + ind.off + k * ind.stride;
          const d = new DataView(bin.buffer);
          tris[k] = ind.size === 4 ? d.getUint32(o, true) : ind.size === 2 ? d.getUint16(o, true) : d.getUint8(o);
        }
      }
      const raw = (x: any, v: number) =>
        Buffer.from(bin.buffer, bin.byteOffset + x.off + v * x.stride, x.comps * x.size).toString("hex");
      uvIslands += islands(count / 3, tris, (v) => raw(pos, v) + raw(uv, v));
    }
  }
  return {
    format: "glb",
    bytes: bytes.length,
    generator_tag: json.asset?.generator ?? null,
    maps,
    extensions,
    compression,
    vertex_colours,
    uv_sets,
    uv_islands: uv_sets ? uvIslands : null,
    materials: (json.materials ?? []).length,
    primitives,
  };
}

function objFacts(path: string): FileFacts {
  const text = readFileSync(path, "utf8");
  const corners: string[] = [];
  let colours = false;
  let vt = 0;
  let mtllib: string | null = null;
  const usemtl = new Set<string>();
  for (const line of text.split("\n")) {
    if (line.startsWith("v ")) {
      if (!colours && line.trim().split(/\s+/).length >= 7) colours = true;
    } else if (line.startsWith("vt ")) vt++;
    else if (line.startsWith("f ")) {
      const c = line.slice(2).trim().split(/\s+/);
      for (let i = 1; i + 1 < c.length; i++) corners.push(c[0], c[i], c[i + 1]);
    } else if (line.startsWith("mtllib ")) mtllib = line.slice(7).trim();
    else if (line.startsWith("usemtl ")) usemtl.add(line.slice(7).trim());
  }
  const maps: Record<string, number> = {};
  if (mtllib && existsSync(join(dirname(path), mtllib))) {
    const keys: Record<string, string> = {
      map_kd: "base_colour",
      map_bump: "normal",
      bump: "normal",
      norm: "normal",
      map_ks: "specular",
      map_pr: "roughness",
      map_pm: "metallic",
      map_ke: "emissive",
      map_d: "alpha",
    };
    for (const line of readFileSync(join(dirname(path), mtllib), "utf8").split("\n")) {
      const k = line.trim().split(/\s+/)[0]?.toLowerCase();
      if (k && keys[k]) maps[keys[k]] = (maps[keys[k]] ?? 0) + 1;
    }
  }
  // An OBJ corner is "v/vt/vn": the same v and vt is the same corner.
  const ids = new Map<string, number>();
  const tris = corners.map((c) => {
    const [v, t] = c.split("/");
    const k = `${v}/${t ?? ""}`;
    let i = ids.get(k);
    if (i === undefined) {
      i = ids.size;
      ids.set(k, i);
    }
    return i;
  });
  return {
    format: "obj",
    bytes: text.length,
    generator_tag: null,
    maps,
    extensions: [],
    compression: [],
    vertex_colours: colours,
    uv_sets: vt > 0 ? 1 : 0,
    uv_islands: vt > 0 ? islands(tris.length / 3, tris, (v) => String(v)) : null,
    materials: usemtl.size,
    primitives: Math.max(1, usemtl.size),
  };
}

function meshFile(dir: string): string {
  const files = require("node:fs").readdirSync(dir) as string[];
  const m = files.find((f) => [".glb", ".gltf", ".obj", ".fbx"].includes(extname(f).toLowerCase()));
  if (!m) throw new Error(`${dir}: no mesh`);
  return join(dir, m);
}

function textureOut(imp: any): { images: number; decoded_bytes_with_mips: number; sizes: string[] } {
  const images = imp?.import?.images ?? [];
  let total = 0;
  for (const i of images) total += Math.round((i.width * i.height * 4 * 4) / 3);
  return { images: images.length, decoded_bytes_with_mips: total, sizes: images.map((i: any) => `${i.width}x${i.height}`) };
}

if (import.meta.main) {
  const [dir, out] = process.argv.slice(2);
  if (!dir || !out) {
    console.error("usage: bun bench/generated/run.ts DIR OUT [--only a,b] [--budgets 2000,10000,50000]");
    process.exit(2);
  }
  const opt = (k: string) => {
    const i = process.argv.indexOf(k);
    return i < 0 ? undefined : process.argv[i + 1];
  };
  const only = opt("--only")?.split(",");
  const budgets = (opt("--budgets") ?? "2000,10000,50000").split(",").map(Number);
  const merge = opt("--merge-charts");
  const modes = (opt("--modes") ?? "keep,fresh").split(",");
  mkdirSync(join(out, "glb"), { recursive: true });
  const results = join(out, "results.jsonl");
  if (!only) writeFileSync(results, "");
  for (const e of readManifest(join(here, "manifest.tsv"))) {
    if (only && !only.includes(e.name)) continue;
    const path = meshFile(join(dir, e.name));
    const bytes = new Uint8Array(readFileSync(path));
    const file = path.endsWith(".glb") ? gltfFacts(bytes) : objFacts(path);
    const imp = await cli(["import", path]);
    const meas = await cli(["measure", path, "--intersections"]);
    const runs: any[] = [];
    for (const mode of modes) {
      for (const b of budgets) {
        const o = join(out, "glb", `${e.name}.${mode}.${b}.glb`);
        const argv = ["process", path, "--budget", String(b), "-o", o, "--intersections", ...REPAIR];
        if (mode === "fresh") argv.push("--unwrap", "--resolution", "1024");
        else if (merge) argv.push("--merge-charts", merge);
        const r = await cli(argv);
        const tex = r.ok ? textureOut((await cli(["import", o])).report) : null;
        const rep = r.report;
        runs.push({
          mode,
          budget: b,
          ok: r.ok,
          error: r.error ?? null,
          ms: r.ms,
          peak_mib: r.peak_mib,
          repair: rep?.repair ?? null,
          decimate: rep?.decimate ?? null,
          unwrap: rep?.unwrap ?? null,
          measures: rep?.measures ?? null,
          texture_out: tex,
        });
        console.error(`${e.name} ${mode} ${b}: ${r.ok ? "ok" : r.error} ${r.ms} ms ${r.peak_mib} MiB`);
      }
    }
    const top = join(out, "glb", `${e.name}.keep.${budgets[budgets.length - 1]}.glb`);
    const lods = existsSync(top)
      ? await cli(["decimate", top, "--ratio", "0.5", "--lods", "3", "-o", join(out, "glb", `${e.name}.lod.glb`)])
      : null;
    const row = {
      name: e.name,
      generator: e.generator,
      category: e.category,
      licence: e.licence,
      file,
      import: imp.ok ? imp.report.import : { error: imp.error },
      import_ms: imp.ms,
      measure: meas.ok ? meas.report.whole : { error: meas.error },
      measure_parts: meas.ok ? meas.report.parts.length : null,
      measure_ms: meas.ms,
      runs,
      lods: lods
        ? lods.ok
          ? lods.report.levels.map((l: any) => ({
              triangles: l.decimate.triangles,
              error: l.decimate.error,
              closed: l.measures.closed,
            }))
          : { error: lods.error }
        : null,
    };
    appendFileSync(results, JSON.stringify(row) + "\n");
  }
}
