// The tables of docs/research.md §29.6 from look.ts's results:
//
//   bun bench/generated/look-table.ts BEFORE/look.jsonl AFTER/look.jsonl [BUDGETED/look.jsonl]
//
// BEFORE: an older `shape` (before stream AH); AFTER: this one; BUDGETED:
// this one with --texture-max. Per generator, files whose factors, maps
// and vertex colours reach the output at keep 10k; texture memory of the
// maps in and out; time. Counts and measures only.
import { readFileSync } from "node:fs";

const load = (p: string | undefined): any[] =>
  p
    ? readFileSync(p, "utf8")
        .trim()
        .split("\n")
        .map((l) => JSON.parse(l))
    : [];
const [before, after, budgeted] = process.argv.slice(2).map(load);
const med = (xs: number[]) => {
  if (!xs.length) return NaN;
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
};
const mib = (b: number) => (Number.isFinite(b) ? (b / 1048576).toFixed(1) : "–");
const mem = (look: any) =>
  look ? Object.values(look.maps ?? {}).reduce((a: number, m: any) => a + (m?.mips ?? 0), 0) : NaN;
const gens = [...new Set(after.map((r) => r.generator))];
const by = (rows: any[], g: string) => rows.filter((r) => r.generator === g);
const n = (rows: any[], k: string) => rows.filter((r) => r[k]).length;

console.log("| generator | files | factors kept (before → after) | maps kept | vertex colours kept | maps in (median MiB with mips) | out | out, --texture-max 1024 | time (median s; before → after → budgeted) |");
console.log("|" + "---|".repeat(9));
const rowsFor = (g: string | null) => {
  const pick = (rows: any[]) => (g ? by(rows, g) : rows);
  const [b, a, t] = [pick(before), pick(after), pick(budgeted)];
  const withColours = a.filter((r) => r.input.colors);
  const withMaps = a.filter((r) => Object.keys(r.input.maps).length);
  const kept = (rows: any[], k: string, sub: any[]) =>
    `${rows.filter((r) => r[k] && sub.some((s) => s.name === r.name)).length}`;
  return [
    g ?? "all",
    a.length,
    `${n(b, "factors_kept")} → ${n(a, "factors_kept")}`,
    `${kept(b, "maps_kept", withMaps)} → ${kept(a, "maps_kept", withMaps)} of ${withMaps.length}`,
    `${kept(b, "colors_kept", withColours)} → ${kept(a, "colors_kept", withColours)} of ${withColours.length}`,
    mib(med(a.map((r) => mem(r.input)))),
    mib(med(a.map((r) => mem(r.output)))),
    t.length ? mib(med(t.map((r) => mem(r.output)))) : "–",
    `${(med(b.map((r) => r.ms)) / 1000).toFixed(1)} → ${(med(a.map((r) => r.ms)) / 1000).toFixed(1)}${t.length ? ` → ${(med(t.map((r) => r.ms)) / 1000).toFixed(1)}` : ""}`,
  ];
};
for (const g of [...gens, null]) console.log(`| ${rowsFor(g).join(" | ")} |`);

console.log("\nWhat still differs, per file (after):\n");
for (const r of after) {
  const why = [
    ...(r.factors_differ.length ? [`factors ${r.factors_differ.join(", ")}`] : []),
    ...(r.maps_dropped.length ? [`maps ${r.maps_dropped.join(", ")}`] : []),
    ...(!r.colors_kept ? ["colours"] : []),
    ...(!r.ok ? [`error: ${r.error}`] : []),
  ];
  if (why.length) console.log(`- ${r.name}: ${why.join("; ")}`);
}

if (budgeted.length) {
  console.log("\nTexture budget (--texture-max 1024), per kind of map:\n");
  console.log("| kind | maps | resized | in (MiB with mips, total) | out (total) | file bytes in → out (total MiB) | not decoded |");
  console.log("|" + "---|".repeat(7));
  const kinds = new Map<string, any[]>();
  for (const r of budgeted) for (const t of r.textures ?? []) kinds.set(t.kind, [...(kinds.get(t.kind) ?? []), t]);
  for (const [k, ts] of kinds) {
    const sum = (f: (t: any) => number) => ts.reduce((a, t) => a + f(t), 0);
    const outOf = (t: any) => t.after ?? t.before;
    console.log(
      `| ${k} | ${ts.length} | ${ts.filter((t) => t.after).length} | ${mib(sum((t) => t.before.decoded_bytes_with_mips))} | ${mib(sum((t) => outOf(t).decoded_bytes_with_mips))} | ${mib(sum((t) => t.before.file_bytes))} → ${mib(sum((t) => outOf(t).file_bytes))} | ${ts.filter((t) => t.error).length} |`,
    );
  }
}
