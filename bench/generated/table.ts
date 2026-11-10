// Tables for docs/research.md §28 from run.ts's results:
//
//   bun bench/generated/table.ts OUT/results.jsonl
//
// Markdown on stdout: what comes in, per generator; the chain per budget
// and mode; failures by cause; the per-file numbers the gaps are ranked
// by. Counts and measures only.
import { readFileSync } from "node:fs";

const rows: any[] = readFileSync(process.argv[2], "utf8")
  .trim()
  .split("\n")
  .map((l) => JSON.parse(l));

const med = (xs: number[]) => {
  if (!xs.length) return NaN;
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
};
const f = (x: number, d = 0) => (Number.isFinite(x) ? x.toFixed(d) : "–");
const k = (x: number) => (x >= 1000 ? `${f(x / 1000, x >= 10000 ? 0 : 1)}k` : f(x));
const mib = (b: number) => f(b / 1048576, 1);
const count = (xs: any[], p: (x: any) => boolean) => xs.filter(p).length;
const gens = [...new Set(rows.map((r) => r.generator))];
const by = (g: string) => rows.filter((r) => r.generator === g);
const diag = (m: any) => Math.hypot(...m.bounds.size);

console.log("### What comes in: geometry\n");
console.log(
  "| generator | files | format | triangles (median, range) | closed | pieces (median, max) | non-manifold edges (files) | degenerate (files) | self-intersecting (files; median pairs) | longest side (median, m) |",
);
console.log("|" + "---|".repeat(10));
for (const g of gens) {
  const ms = by(g).map((r) => r.measure).filter((m) => m && !m.error);
  const tri = ms.map((m) => m.triangles);
  const si = ms.map((m) => m.self_intersections ?? 0);
  console.log(
    `| ${g} | ${by(g).length} | ${[...new Set(by(g).map((r) => r.file.format))].join(", ")} | ${k(med(tri))} (${k(Math.min(...tri))}–${k(Math.max(...tri))}) | ${count(ms, (m) => m.closed)} | ${med(ms.map((m) => m.pieces))}, ${Math.max(...ms.map((m) => m.pieces))} | ${count(ms, (m) => m.non_manifold_edges > 0)} | ${count(ms, (m) => m.degenerate_triangles > 0)} | ${count(si, (x) => x > 0)}; ${med(si.filter((x) => x > 0)) || 0} | ${f(med(ms.map((m) => Math.max(...m.bounds.size) / 1000)), 2)} |`,
  );
}
console.log("\n### What comes in: appearance\n");
console.log(
  "| generator | UVs (files) | UV islands (median) | UV coverage (mean) | UV overlapping pairs (files) | textures (median per file; sizes) | decoded with mips (median MiB) | maps: base / MR / normal / emissive / occlusion | vertex colours | compression |",
);
console.log("|" + "---|".repeat(10));
for (const g of gens) {
  const rs = by(g);
  const uv = rs.filter((r) => r.file.uv_sets > 0);
  const tex = rs.map((r) => r.import.images?.length ?? 0);
  const sizes = [...new Set(rs.flatMap((r) => (r.import.images ?? []).map((i: any) => `${i.width}`)))].sort();
  const mapCount = (key: string) => count(rs, (r) => (r.file.maps[key] ?? 0) > 0);
  console.log(
    `| ${g} | ${uv.length} | ${uv.length ? f(med(uv.map((r) => r.file.uv_islands ?? NaN))) : "–"} | ${uv.length ? f(uv.reduce((a, r) => a + (r.measure.uv?.coverage ?? 0), 0) / uv.length, 2) : "–"} | ${count(uv, (r) => (r.measure.uv?.overlapping_pairs ?? 0) > 0)} | ${med(tex)}; ${sizes.join(", ") || "–"} | ${mib(med(rs.map((r) => r.measure.texture?.decoded_bytes_with_mips ?? 0)))} | ${["base_colour", "metallic_roughness", "normal", "emissive", "occlusion"].map(mapCount).join(" / ")} | ${count(rs, (r) => r.file.vertex_colours)} | ${count(rs, (r) => r.file.compression.length > 0)} |`,
  );
}

console.log("\n### The chain (`shape process`, repair as the guide's, `--intersections`)\n");
console.log(
  "| mode | budget | ran | within budget | closed | one piece | error (median mm; median % of diagonal; max %) | UV overlapping pairs (files) | UV flipped (files) | coverage (mean) | base colour kept | maps dropped (files) | texture out (median MiB) | time (median, max s) | peak memory (median, max MiB) |",
);
console.log("|" + "---|".repeat(15));
for (const mode of ["keep", "fresh"]) {
  for (const b of [2000, 10000, 50000]) {
    const runs = rows.map((r) => ({ r, x: r.runs.find((x: any) => x.mode === mode && x.budget === b) })).filter((p) => p.x);
    const ok = runs.filter((p) => p.x.ok);
    const m = ok.map((p) => p.x.measures);
    const err = ok.filter((p) => p.x.decimate).map((p) => p.x.decimate.error);
    const errPct = ok.filter((p) => p.x.decimate).map((p) => (100 * p.x.decimate.error) / diag(p.r.measure));
    const dropped = count(ok, (p) => Object.keys(p.r.file.maps).some((key) => key !== "base_colour"));
    console.log(
      `| ${mode} | ${k(b)} | ${ok.length}/${runs.length} | ${count(m, (x) => x.triangles <= b)} | ${count(m, (x) => x.closed)} | ${count(m, (x) => x.pieces === 1)} | ${f(med(err), 1)}; ${f(med(errPct), 2)}; ${f(Math.max(...errPct), 2)} | ${count(m, (x) => (x.uv?.overlapping_pairs ?? 0) > 0)} | ${count(m, (x) => (x.uv?.flipped ?? 0) > 0)} | ${f(m.reduce((a, x) => a + (x.uv?.coverage ?? 0), 0) / Math.max(1, m.length), 2)} | ${count(ok, (p) => (p.x.texture_out?.images ?? 0) > 0)} | ${dropped} | ${mib(med(ok.map((p) => p.x.texture_out?.decoded_bytes_with_mips ?? 0)))} | ${f(med(runs.map((p) => p.x.ms)) / 1000, 1)}, ${f(Math.max(...runs.map((p) => p.x.ms)) / 1000, 1)} | ${med(runs.map((p) => p.x.peak_mib))}, ${Math.max(...runs.map((p) => p.x.peak_mib))} |`,
    );
  }
}

console.log("\n### Per generator, keep, within budget / closed\n");
console.log("| generator | 2k | 10k | 50k | fresh 2k | holes left after repair (files) | fuse refused (files) | floaters dropped (files) |");
console.log("|---|---|---|---|---|---|---|---|");
for (const g of gens) {
  const rs = by(g);
  const cell = (mode: string, b: number) => {
    const xs = rs.map((r) => r.runs.find((x: any) => x.mode === mode && x.budget === b)).filter((x) => x?.ok);
    return `${count(xs, (x) => x.measures.triangles <= b)} / ${count(xs, (x) => x.measures.closed)}`;
  };
  const rep = rs.map((r) => r.runs[0]?.repair).filter(Boolean);
  console.log(
    `| ${g} | ${cell("keep", 2000)} | ${cell("keep", 10000)} | ${cell("keep", 50000)} | ${cell("fresh", 2000)} | ${count(rep, (x) => x.holes_left > 0)} | ${count(rep, (x) => x.fused?.refused !== undefined)} | ${count(rep, (x) => x.floaters > 0)} |`,
  );
}

console.log("\n### Failures by cause\n");
const causes = new Map<string, string[]>();
for (const r of rows) {
  for (const x of r.runs) {
    const why = !x.ok
      ? `error: ${x.error.split("\n")[0].slice(0, 120)}`
      : x.measures.triangles > x.budget
        ? `over budget (${x.mode})`
        : !x.measures.closed
          ? `open result (${x.mode})`
          : null;
    if (why) causes.set(why, [...(causes.get(why) ?? []), `${r.name}@${k(x.budget)}`]);
  }
}
for (const [why, list] of causes) console.log(`- ${why}: ${list.length} runs (${list.slice(0, 12).join(", ")}${list.length > 12 ? ", ..." : ""})`);

console.log("\n### LODs (`decimate --ratio 0.5 --lods 3` of the keep 50k output)\n");
console.log("| generator | levels made | each level ≤ the one before | all closed | last level triangles (median) | last level error (median % of diagonal) |");
console.log("|---|---|---|---|---|---|");
for (const g of gens) {
  const ls = by(g).filter((r) => Array.isArray(r.lods));
  console.log(
    `| ${g} | ${ls.length} | ${count(ls, (r) => r.lods.every((l: any, i: number) => i === 0 || l.triangles < r.lods[i - 1].triangles))} | ${count(ls, (r) => r.lods.every((l: any) => l.closed))} | ${k(med(ls.map((r) => r.lods[r.lods.length - 1].triangles)))} | ${f(med(ls.map((r) => (100 * r.lods[r.lods.length - 1].error) / diag(r.measure))), 2)} |`,
  );
}

console.log("\n### Per file (for the gaps)\n");
console.log("| file | category | triangles in | closed in | holes left | maps beyond base colour | vertex colours | texture in (MiB with mips) | keep 10k: triangles, closed | fresh 10k: triangles, closed |");
console.log("|---|---|---|---|---|---|---|---|---|---|");
for (const r of rows) {
  const at = (mode: string) => r.runs.find((x: any) => x.mode === mode && x.budget === 10000);
  const cell = (x: any) => (x?.ok ? `${k(x.measures.triangles)}, ${x.measures.closed ? "yes" : "no"}` : "error");
  const extra = Object.keys(r.file.maps).filter((key) => key !== "base_colour");
  console.log(
    `| ${r.name} | ${r.category} | ${k(r.measure.triangles)} | ${r.measure.closed ? "yes" : "no"} | ${r.runs[0]?.repair?.holes_left ?? "–"} | ${extra.join(", ") || "–"} | ${r.file.vertex_colours ? "yes" : "–"} | ${mib(r.measure.texture?.decoded_bytes_with_mips ?? 0)} | ${cell(at("keep"))} | ${cell(at("fresh"))} |`,
  );
}
