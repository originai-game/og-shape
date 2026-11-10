/**
 * Run `Z`'s shape measure that is not the agent's opinion (research §22):
 * a symmetric surface distance between the mesh a program builds and the
 * dataset's reference mesh.
 *
 *   bun eval/phase6/Z/distance.ts <refs dir> <run dir> [object …]
 *
 * `<refs dir>` holds `<object>.usda` (`fetch_references.ts` downloads
 * them); `<run dir>` holds `<object>/program.json`. Writes
 * `<run dir>/<object>/distance.json` and prints a table.
 *
 * Both meshes at rest, in millimetres. The reference: every `Mesh` prim of
 * the OpenUSD file through its ancestors' `translate`, `orient`, `scale`
 * and `transform` ops, times `metersPerUnit` × 1000. The built mesh: the
 * program replayed by the binding, exported as OBJ (world placement,
 * metres and Y up, turned back).
 * `N` points are sampled on each surface, area-weighted, with a fixed
 * generator; each point's distance to the nearest point of the other
 * surface is exact (point to triangle, through a bounding-volume tree).
 * Reported, in mm and as a fraction of the reference's bounding-box
 * diagonal: the mean of each direction (built → reference, reference →
 * built), their average (`mean`, a symmetric Chamfer distance), the 95th
 * percentile of both directions together and the largest (`hausdorff`).
 * Twice: as placed, and with the built mesh translated so the two
 * bounding-box centres coincide (the briefs give sizes, not origins).
 * No alignment of rotation, no threshold; `centredMeanIf` gives the
 * centred `mean` again with the built mesh mirrored in x, in y, or turned
 * 180° about z, to tell a side chosen differently from a shape built
 * differently. Hidden inner pieces count like visible ones on both sides.
 */

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { load } from "../../../crates/shape-wasm/ts/src/index.ts";

type V = [number, number, number];
type Tri = [V, V, V];
type M = number[]; // 4x4, row-major, column vectors

const N = 20000;

const ident = (): M => [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
function mul(a: M, b: M): M {
  const r = new Array(16).fill(0);
  for (let i = 0; i < 4; i++) for (let j = 0; j < 4; j++) for (let k = 0; k < 4; k++) r[i * 4 + j] += a[i * 4 + k] * b[k * 4 + j];
  return r;
}
const apply = (m: M, p: V): V => [
  m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + m[3],
  m[4] * p[0] + m[5] * p[1] + m[6] * p[2] + m[7],
  m[8] * p[0] + m[9] * p[1] + m[10] * p[2] + m[11],
];
const nums = (s: string) => (s.match(/-?\d+(?:\.\d+)?(?:[eE][-+]?\d+)?/g) ?? []).map(Number);

/** One xform op as a matrix (USD: row vectors; here transposed to column vectors). */
function opMatrix(kind: string, v: number[]): M {
  if (kind === "translate") return [1, 0, 0, v[0], 0, 1, 0, v[1], 0, 0, 1, v[2], 0, 0, 0, 1];
  if (kind === "scale") return [v[0], 0, 0, 0, 0, v[1], 0, 0, 0, 0, v[2], 0, 0, 0, 0, 1];
  if (kind === "orient") {
    const [w, x, y, z] = v;
    return [
      1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w), 0,
      2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w), 0,
      2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y), 0,
      0, 0, 0, 1,
    ];
  }
  if (kind === "transform") {
    // matrix4d rows (a b c 0)(…)(tx ty tz 1), row-vector convention.
    const r = ident();
    for (let i = 0; i < 4; i++) for (let j = 0; j < 4; j++) r[j * 4 + i] = v[i * 4 + j];
    return r;
  }
  if (kind === "rotateX" || kind === "rotateY" || kind === "rotateZ") {
    const a = (v[0] * Math.PI) / 180;
    const c = Math.cos(a), s = Math.sin(a);
    if (kind === "rotateX") return [1, 0, 0, 0, 0, c, -s, 0, 0, s, c, 0, 0, 0, 0, 1];
    if (kind === "rotateY") return [c, 0, s, 0, 0, 1, 0, 0, -s, 0, c, 0, 0, 0, 0, 1];
    return [c, -s, 0, 0, s, c, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
  }
  throw new Error(`xform op ${kind} is not read`);
}

/** Every mesh triangle of a USDA file, in the stage's world, in mm. */
export function readUsda(text: string): Tri[] {
  const lines = text.split("\n");
  const unit = Number(text.match(/metersPerUnit\s*=\s*([\d.eE+-]+)/)?.[1] ?? "0.01") * 1000;
  type Prim = { type: string; ops: Map<string, number[]>; order: string[]; attrs: Map<string, string>; world: M };
  const stack: Prim[] = [];
  let pending: Prim | null = null;
  let depthInMeta = 0;
  const tris: Tri[] = [];
  const top = (): M => (stack.length ? stack[stack.length - 1].world : ident());
  for (let i = 0; i < lines.length; i++) {
    let line = lines[i];
    const t = line.trim();
    const def = t.match(/^(?:def|over)\s+(\w+)?\s*"([^"]+)"/);
    if (def) {
      pending = { type: def[1] ?? "", ops: new Map(), order: [], attrs: new Map(), world: ident() };
      if (t.endsWith("(")) depthInMeta = 1;
      continue;
    }
    if (depthInMeta > 0) {
      for (const ch of t) depthInMeta += ch === "(" ? 1 : ch === ")" ? -1 : 0;
      continue;
    }
    if (t === "{") {
      const prim = pending ?? { type: "", ops: new Map(), order: [], attrs: new Map(), world: ident() };
      prim.world = top(); // until its own ops say otherwise
      stack.push(prim);
      pending = null;
      continue;
    }
    if (t === "}") {
      const p = stack.pop();
      if (p && p.type === "Mesh") emitMesh(p);
      continue;
    }
    const prim = stack[stack.length - 1];
    if (!prim) continue;
    // Gather an attribute whose brackets span lines.
    const attr = t.match(/^(?:uniform\s+)?[\w\[\]]+\s+([\w:]+)\s*=\s*(.*)$/);
    if (!attr) continue;
    let value = attr[2];
    const open = (s: string) => (s.match(/\[/g) ?? []).length - (s.match(/\]/g) ?? []).length;
    while (open(value) > 0 && i + 1 < lines.length) value += lines[++i].trim();
    const name = attr[1];
    if (name === "xformOpOrder") {
      prim.order = (value.match(/"([^"]+)"/g) ?? []).map((s) => s.slice(1, -1));
    } else if (name.startsWith("xformOp:")) {
      prim.ops.set(name, nums(value));
    } else if (name === "points" || name === "faceVertexCounts" || name === "faceVertexIndices") {
      prim.attrs.set(name, value);
    }
    // The world matrix is ready once the op order is known; ops come before it.
    if (name === "xformOpOrder") {
      let local = ident();
      for (const op of prim.order) {
        if (op === "!resetXformStack!") continue;
        const inverse = op.startsWith("!invert!");
        const key = inverse ? op.slice(8) : op;
        if (inverse) throw new Error(`inverted op ${op} is not read`);
        const kind = key.split(":")[1];
        local = mul(local, opMatrix(kind, prim.ops.get(key) ?? []));
      }
      prim.world = mul(stack.length > 1 ? stack[stack.length - 2].world : ident(), local);
    }
  }
  return tris;

  function emitMesh(p: Prim) {
    const world = p.world;
    const pts = nums(p.attrs.get("points") ?? "");
    const counts = nums(p.attrs.get("faceVertexCounts") ?? "");
    const idx = nums(p.attrs.get("faceVertexIndices") ?? "");
    const P: V[] = [];
    for (let k = 0; k + 2 < pts.length; k += 3) {
      const w = apply(world, [pts[k], pts[k + 1], pts[k + 2]]);
      P.push([w[0] * unit, w[1] * unit, w[2] * unit]);
    }
    let at = 0;
    for (const c of counts) {
      for (let k = 1; k + 1 < c; k++) tris.push([P[idx[at]], P[idx[at + k]], P[idx[at + k + 1]]]);
      at += c;
    }
  }
}

/** Triangles of the binding's OBJ text (faces fanned), back to mm and Z up. */
export function readObj(text: string): Tri[] {
  const v: V[] = [];
  const tris: Tri[] = [];
  for (const line of text.split("\n")) {
    if (line.startsWith("v ")) {
      const n = nums(line.slice(2));
      // OBJ is metres, Y up (shape-tools `y_up`: Z becomes Y, Y becomes −Z).
      v.push([n[0] * 1000, -n[2] * 1000, n[1] * 1000]);
    } else if (line.startsWith("f ")) {
      const ids = line
        .slice(2)
        .trim()
        .split(/\s+/)
        .map((s) => {
          const k = Number(s.split("/")[0]);
          return k < 0 ? v.length + k : k - 1;
        });
      for (let k = 1; k + 1 < ids.length; k++) tris.push([v[ids[0]], v[ids[k]], v[ids[k + 1]]]);
    }
  }
  return tris;
}

const sub = (a: V, b: V): V => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const dot = (a: V, b: V) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: V, b: V): V => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const area = (t: Tri) => {
  const c = cross(sub(t[1], t[0]), sub(t[2], t[0]));
  return Math.sqrt(dot(c, c)) / 2;
};

function bounds(tris: Tri[]): [V, V] {
  const lo: V = [Infinity, Infinity, Infinity], hi: V = [-Infinity, -Infinity, -Infinity];
  for (const t of tris) for (const p of t) for (let k = 0; k < 3; k++) (lo[k] = Math.min(lo[k], p[k])), (hi[k] = Math.max(hi[k], p[k]));
  return [lo, hi];
}

/** Area-weighted points, from a fixed 32-bit generator. */
function sample(tris: Tri[], n: number): V[] {
  let seed = 0x9e3779b9;
  const rnd = () => {
    seed ^= seed << 13;
    seed >>>= 0;
    seed ^= seed >>> 17;
    seed ^= seed << 5;
    seed >>>= 0;
    return seed / 4294967296;
  };
  const cum: number[] = [];
  let total = 0;
  for (const t of tris) cum.push((total += area(t)));
  const out: V[] = [];
  for (let i = 0; i < n; i++) {
    const r = ((i + rnd()) / n) * total; // stratified along the cumulative area
    let lo = 0, hi = cum.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (cum[mid] < r) lo = mid + 1;
      else hi = mid;
    }
    const [a, b, c] = tris[lo];
    let u = rnd(), w = rnd();
    if (u + w > 1) (u = 1 - u), (w = 1 - w);
    out.push([a[0] + u * (b[0] - a[0]) + w * (c[0] - a[0]), a[1] + u * (b[1] - a[1]) + w * (c[1] - a[1]), a[2] + u * (b[2] - a[2]) + w * (c[2] - a[2])]);
  }
  return out;
}

/** Squared distance from p to triangle abc (Ericson, Real-Time Collision Detection §5.1.5). */
function pointTri(p: V, a: V, b: V, c: V): number {
  const ab = sub(b, a), ac = sub(c, a), ap = sub(p, a);
  const d1 = dot(ab, ap), d2 = dot(ac, ap);
  let q: V;
  if (d1 <= 0 && d2 <= 0) q = a;
  else {
    const bp = sub(p, b), d3 = dot(ab, bp), d4 = dot(ac, bp);
    if (d3 >= 0 && d4 <= d3) q = b;
    else {
      const vc = d1 * d4 - d3 * d2;
      if (vc <= 0 && d1 >= 0 && d3 <= 0) {
        const v = d1 / (d1 - d3);
        q = [a[0] + v * ab[0], a[1] + v * ab[1], a[2] + v * ab[2]];
      } else {
        const cp = sub(p, c), d5 = dot(ab, cp), d6 = dot(ac, cp);
        if (d6 >= 0 && d5 <= d6) q = c;
        else {
          const vb = d5 * d2 - d1 * d6;
          if (vb <= 0 && d2 >= 0 && d6 <= 0) {
            const w = d2 / (d2 - d6);
            q = [a[0] + w * ac[0], a[1] + w * ac[1], a[2] + w * ac[2]];
          } else {
            const va = d3 * d6 - d5 * d4;
            if (va <= 0 && d4 - d3 >= 0 && d5 - d6 >= 0) {
              const w = (d4 - d3) / (d4 - d3 + (d5 - d6));
              q = [b[0] + w * (c[0] - b[0]), b[1] + w * (c[1] - b[1]), b[2] + w * (c[2] - b[2])];
            } else {
              const den = 1 / (va + vb + vc), v = vb * den, w = vc * den;
              q = [a[0] + ab[0] * v + ac[0] * w, a[1] + ab[1] * v + ac[1] * w, a[2] + ab[2] * v + ac[2] * w];
            }
          }
        }
      }
    }
  }
  const d = sub(p, q);
  return dot(d, d);
}

type Node = { lo: V; hi: V; left?: Node; right?: Node; tris?: Tri[] };
function build(tris: Tri[]): Node {
  const [lo, hi] = bounds(tris);
  if (tris.length <= 8) return { lo, hi, tris };
  const ext = sub(hi, lo);
  const axis = ext[0] >= ext[1] && ext[0] >= ext[2] ? 0 : ext[1] >= ext[2] ? 1 : 2;
  const key = (t: Tri) => t[0][axis] + t[1][axis] + t[2][axis];
  const sorted = [...tris].sort((a, b) => key(a) - key(b));
  const mid = sorted.length >> 1;
  return { lo, hi, left: build(sorted.slice(0, mid)), right: build(sorted.slice(mid)) };
}
function boxDist2(p: V, lo: V, hi: V) {
  let d = 0;
  for (let k = 0; k < 3; k++) {
    const e = p[k] < lo[k] ? lo[k] - p[k] : p[k] > hi[k] ? p[k] - hi[k] : 0;
    d += e * e;
  }
  return d;
}
function nearest(root: Node, p: V): number {
  let best = Infinity;
  const stack: Node[] = [root];
  while (stack.length) {
    const n = stack.pop()!;
    if (boxDist2(p, n.lo, n.hi) >= best) continue;
    if (n.tris) {
      for (const t of n.tris) best = Math.min(best, pointTri(p, t[0], t[1], t[2]));
      continue;
    }
    const dl = boxDist2(p, n.left!.lo, n.left!.hi), dr = boxDist2(p, n.right!.lo, n.right!.hi);
    if (dl < dr) stack.push(n.right!, n.left!);
    else stack.push(n.left!, n.right!);
  }
  return Math.sqrt(best);
}

export function compare(builtAll: Tri[], refAll: Tri[]) {
  // Zero-area triangles carry no surface (and would divide 0 by 0).
  const built = builtAll.filter((t) => area(t) > 1e-9);
  const ref = refAll.filter((t) => area(t) > 1e-9);
  const [rlo, rhi] = bounds(ref);
  const diag = Math.sqrt(dot(sub(rhi, rlo), sub(rhi, rlo)));
  const one = (b: Tri[]) => {
    const tb = build(b), tr = build(ref);
    const ab = sample(b, N).map((p) => nearest(tr, p));
    const ba = sample(ref, N).map((p) => nearest(tb, p));
    const mean = (x: number[]) => x.reduce((s, v) => s + v, 0) / x.length;
    const all = [...ab, ...ba].sort((x, y) => x - y);
    const r = (x: number) => Math.round(x * 1000) / 1000;
    const out = {
      builtToReference: r(mean(ab)),
      referenceToBuilt: r(mean(ba)),
      mean: r((mean(ab) + mean(ba)) / 2),
      p95: r(all[Math.floor(0.95 * (all.length - 1))]),
      hausdorff: r(all[all.length - 1]),
    };
    return { ...out, meanOfDiagonal: Math.round((out.mean / diag) * 1e5) / 1e5, hausdorffOfDiagonal: Math.round((out.hausdorff / diag) * 1e5) / 1e5 };
  };
  const [blo, bhi] = bounds(built);
  const shift: V = [0, 1, 2].map((k) => (rlo[k] + rhi[k]) / 2 - (blo[k] + bhi[k]) / 2) as V;
  const moved = built.map((t) => t.map((p) => [p[0] + shift[0], p[1] + shift[1], p[2] + shift[2]]) as Tri);
  const r3 = (v: V) => v.map((x) => Math.round(x * 100) / 100);
  // The same, centred, with the built mesh mirrored or turned about z:
  // which side an object faces is a choice the briefs leave to the picture.
  const flips: Record<string, (p: V) => V> = {
    mirrorX: (p) => [-p[0], p[1], p[2]],
    mirrorY: (p) => [p[0], -p[1], p[2]],
    turnZ180: (p) => [-p[0], -p[1], p[2]],
  };
  const orientations: Record<string, number> = {};
  for (const [k, f] of Object.entries(flips)) {
    const b = built.map((t) => t.map(f) as Tri);
    const [lo, hi] = bounds(b);
    const d: V = [0, 1, 2].map((i) => (rlo[i] + rhi[i]) / 2 - (lo[i] + hi[i]) / 2) as V;
    orientations[k] = one(b.map((t) => t.map((p) => [p[0] + d[0], p[1] + d[1], p[2] + d[2]]) as Tri)).mean;
  }
  return {
    samples: N,
    referenceTriangles: ref.length,
    builtTriangles: built.length,
    referenceBounds: [r3(rlo), r3(rhi)],
    builtBounds: [r3(blo), r3(bhi)],
    referenceDiagonal: Math.round(diag * 100) / 100,
    asPlaced: one(built),
    centred: { shift: r3(shift), ...one(moved) },
    centredMeanIf: orientations,
  };
}

if (import.meta.main) {
  const [refs, runDir, ...only] = process.argv.slice(2);
  if (!refs || !runDir) throw new Error("usage: bun distance.ts <refs dir> <run dir> [object …]");
  const references = JSON.parse(readFileSync(new URL("../references.json", import.meta.url), "utf8"));
  const objects = only.length ? only : Object.keys(references);
  const og = await load();
  console.log("| object | ref. triangles | mean, mm (centred) | of diagonal | p95, mm | Hausdorff, mm | mean, mm (as placed) | least mean if mirrored or turned |");
  console.log("|---|---|---|---|---|---|---|---|");
  for (const object of objects) {
    let program: string;
    try {
      program = readFileSync(`${runDir}/${object}/program.json`, "utf8");
    } catch {
      continue;
    }
    const ref = readUsda(readFileSync(`${refs}/${object}.usda`, "utf8"));
    const shape = await og.replay(program);
    const built = readObj(shape.obj());
    const d = compare(built, ref);
    mkdirSync(`${runDir}/${object}`, { recursive: true });
    writeFileSync(`${runDir}/${object}/distance.json`, `${JSON.stringify(d, null, 1)}\n`);
    const c = d.centred;
    const [k, v] = Object.entries(d.centredMeanIf).sort((a, b) => a[1] - b[1])[0];
    console.log(`| ${object} | ${d.referenceTriangles} | ${c.mean} | ${(c.meanOfDiagonal * 100).toFixed(2)} % | ${c.p95} | ${c.hausdorff} | ${d.asPlaced.mean} | ${v} (${k}) |`);
  }
}
