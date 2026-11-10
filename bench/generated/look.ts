// Stream AH's measure (docs/research.md §29): what of a file's look —
// material factors, maps, vertex colours — reaches the output of the
// chain, and what the texture budget does to texture memory.
//
//   bun bench/generated/look.ts DIR OUT SHAPE [--texture-max PX] [--only a,b]
//
// Per file, `SHAPE process FILE --budget 10000` with the guide's repair
// (keeping the file's UVs), into OUT/look.jsonl: the input's effective
// material (glTF's defaults applied, or the MTL's), its maps and whether
// it has vertex colours; the output GLB's material, maps (kind, size,
// decoded bytes with mips) and COLOR_0; the wall time. SHAPE may be an
// older binary (without --texture-max): the "before" column.
import { readFileSync, appendFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { join, dirname, extname } from "node:path";
import { readManifest } from "./fetch.ts";

const here = dirname(new URL(import.meta.url).pathname);
const REPAIR = ["--weld", "0.001", "--floaters", "0.05", "--holes", "64", "--fuse"];

type Look = {
  factors: Record<string, any>;
  maps: Record<string, { w: number; h: number; mips: number } | null>;
  colors: boolean;
};

const MAPS: [string, (m: any) => any][] = [
  ["map", (m) => m.pbrMetallicRoughness?.baseColorTexture],
  ["metalness_map", (m) => m.pbrMetallicRoughness?.metallicRoughnessTexture],
  ["normal_map", (m) => m.normalTexture],
  ["emissive_map", (m) => m.emissiveTexture],
  ["ao_map", (m) => m.occlusionTexture],
];

function size(b: Uint8Array): { w: number; h: number } | null {
  if (b[0] === 0x89) return { w: new DataView(b.buffer, b.byteOffset).getUint32(16), h: new DataView(b.buffer, b.byteOffset).getUint32(20) };
  if (b[0] === 0xff) {
    let i = 2;
    while (i < b.length) {
      while (b[i] === 0xff) i++;
      const m = b[i++];
      const len = (b[i] << 8) | b[i + 1];
      if (m >= 0xc0 && m <= 0xcf && m !== 0xc4 && m !== 0xc8 && m !== 0xcc)
        return { h: (b[i + 3] << 8) | b[i + 4], w: (b[i + 5] << 8) | b[i + 6] };
      i += len;
    }
  }
  return null;
}

function mips(w: number, h: number): number {
  let all = w * h * 4;
  while (w > 1 || h > 1) {
    w = Math.max(1, w >> 1);
    h = Math.max(1, h >> 1);
    all += w * h * 4;
  }
  return all;
}

function glbLook(bytes: Uint8Array): Look {
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const len = dv.getUint32(12, true);
  const j = JSON.parse(new TextDecoder().decode(bytes.subarray(20, 20 + len)));
  const bin = bytes.subarray(20 + len + 8);
  const prim = j.meshes[0].primitives[0];
  const m = prim.material !== undefined ? j.materials[prim.material] : {};
  const pbr = m.pbrMetallicRoughness ?? {};
  const factors = {
    base: pbr.baseColorFactor ?? [1, 1, 1, 1],
    metallic: pbr.metallicFactor ?? 1,
    roughness: pbr.roughnessFactor ?? 1,
    emissive: m.emissiveFactor ?? [0, 0, 0],
    alpha: m.alphaMode ?? "OPAQUE",
    cutoff: (m.alphaMode ?? "OPAQUE") === "MASK" ? m.alphaCutoff ?? 0.5 : null,
    double_sided: m.doubleSided ?? false,
  };
  const maps: Look["maps"] = {};
  for (const [k, f] of MAPS) {
    const info = f(m);
    if (!info) continue;
    const src = j.textures[info.index].source;
    const v = j.bufferViews[j.images[src].bufferView];
    const s = size(bin.subarray(v.byteOffset ?? 0, (v.byteOffset ?? 0) + v.byteLength));
    maps[k] = s ? { ...s, mips: mips(s.w, s.h) } : null;
  }
  const colors = j.meshes.some((me: any) => me.primitives.some((p: any) => "COLOR_0" in p.attributes));
  return { factors, maps, colors };
}

function objLook(path: string): Look {
  const text = readFileSync(path, "utf8");
  const colors = text.split("\n").some((l) => l.startsWith("v ") && l.trim().split(/\s+/).length >= 7);
  const lib = text.split("\n").find((l) => l.startsWith("mtllib "))?.slice(7).trim();
  const lin = (c: number) => (c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4));
  const factors: Record<string, any> = {
    base: [1, 1, 1, 1],
    metallic: 0,
    roughness: 1,
    emissive: [0, 0, 0],
    alpha: "OPAQUE",
    cutoff: null,
    double_sided: false,
  };
  const maps: Look["maps"] = {};
  if (lib && existsSync(join(dirname(path), lib))) {
    for (const l of readFileSync(join(dirname(path), lib), "utf8").split("\n")) {
      const [k, ...v] = l.trim().split(/\s+/);
      const n = v.map(Number);
      if (k === "Kd") factors.base = [...n.slice(0, 3).map(lin), factors.base[3]];
      if (k === "d") factors.base[3] = n[0];
      if (k === "Ke") factors.emissive = n.slice(0, 3).map(lin);
      if (k === "Pr") factors.roughness = n[0];
      if (k === "Pm") factors.metallic = n[0];
      if (k === "map_Kd") {
        const b = new Uint8Array(readFileSync(join(dirname(path), v[v.length - 1])));
        const s = size(b);
        maps.map = s ? { ...s, mips: mips(s.w, s.h) } : null;
      }
    }
    if (factors.base[3] < 1) factors.alpha = "BLEND";
  }
  return { factors, maps, colors };
}

function same(a: any, b: any): boolean {
  if (typeof a === "number" && typeof b === "number") return Math.abs(a - b) <= 1e-6;
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((x, i) => same(x, b[i]));
  return a === b;
}

if (import.meta.main) {
  const [dir, out, shape] = process.argv.slice(2);
  if (!dir || !out || !shape) {
    console.error("usage: bun bench/generated/look.ts DIR OUT SHAPE [--texture-max PX] [--only a,b]");
    process.exit(2);
  }
  const opt = (k: string) => {
    const i = process.argv.indexOf(k);
    return i < 0 ? undefined : process.argv[i + 1];
  };
  const max = opt("--texture-max");
  const only = opt("--only")?.split(",");
  mkdirSync(join(out, "look"), { recursive: true });
  const results = join(out, "look.jsonl");
  writeFileSync(results, "");
  for (const e of readManifest(join(here, "manifest.tsv"))) {
    if (only && !only.includes(e.name)) continue;
    const d = join(dir, e.name);
    const file = require("node:fs").readdirSync(d).find((f: string) => [".glb", ".obj"].includes(extname(f)));
    const path = join(d, file);
    const input = path.endsWith(".glb") ? glbLook(new Uint8Array(readFileSync(path))) : objLook(path);
    const o = join(out, "look", `${e.name}.glb`);
    const argv = [shape, "process", path, "--budget", "10000", "-o", o, ...REPAIR];
    if (max) argv.push("--texture-max", max);
    const t0 = performance.now();
    const r = Bun.spawnSync(argv, { stdout: "pipe", stderr: "pipe" });
    const ms = Math.round(performance.now() - t0);
    const ok = r.exitCode === 0;
    const output = ok ? glbLook(new Uint8Array(readFileSync(o))) : null;
    const report = ok ? JSON.parse(r.stdout.toString()) : null;
    const row = {
      name: e.name,
      generator: e.generator,
      ms,
      ok,
      error: ok ? null : r.stderr.toString().trim(),
      input,
      output,
      factors_kept: output ? Object.keys(input.factors).every((k) => same(input.factors[k], output.factors[k])) : false,
      factors_differ: output ? Object.keys(input.factors).filter((k) => !same(input.factors[k], output.factors[k])) : [],
      maps_kept: output ? Object.keys(input.maps).every((k) => k in output.maps) : false,
      maps_dropped: output ? Object.keys(input.maps).filter((k) => !(k in output.maps)) : [],
      colors_kept: output ? !input.colors || output.colors : false,
      textures: report?.textures ?? null,
      decimate_error: report?.decimate?.error ?? null,
      triangles: report?.measures?.triangles ?? null,
    };
    appendFileSync(results, JSON.stringify(row) + "\n");
    console.error(`${e.name}: ${ok ? "ok" : row.error} ${ms} ms`);
  }
}
