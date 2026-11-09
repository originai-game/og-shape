/**
 * Every code sample in `guides/` builds: each ```ts block is a whole
 * program (`export default function (og) { …; return shape; }`) run
 * through the binding, and each ```json block a program replayed. Every
 * part they make must be closed. The phase 6 evaluation's committed
 * programs (`eval/phase6/<run>/<object>/program.json`) must replay too.
 *
 * Run by the gate's `binding` step (it needs bun and the built binding;
 * without them the step says `binding: NOT RUN`).
 */

import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { load } from "../src/index.ts";

const og = await load();
const root = fileURLToPath(new URL("../../../../", import.meta.url));
const scratch = mkdtempSync(join(tmpdir(), "og-shape-guides-"));

/** The fenced blocks of `lang` in a markdown text. */
function blocks(text: string, lang: string): string[] {
  const out: string[] = [];
  const re = new RegExp("```" + lang + "\\n([\\s\\S]*?)```", "g");
  for (const m of text.matchAll(re)) out.push(m[1]);
  return out;
}

function allClosed(shape: any): void {
  const m = shape.measure();
  expect(m.parts.length).toBeGreaterThan(0);
  for (const p of m.parts) expect({ part: p.name, closed: p.closed }).toEqual({ part: p.name, closed: true });
}

const guides = readdirSync(join(root, "guides")).filter((f) => f.endsWith(".md"));

describe("guide samples build", () => {
  for (const guide of guides) {
    const text = readFileSync(join(root, "guides", guide), "utf8");
    blocks(text, "ts").forEach((source, i) => {
      test(`${guide} ts #${i + 1}`, async () => {
        const file = join(scratch, `${guide.replace(/\W/g, "_")}_${i}.ts`);
        writeFileSync(file, source);
        const mod = await import(file);
        expect(typeof mod.default).toBe("function");
        const shape = await mod.default(og);
        allClosed(shape);
      });
    });
    blocks(text, "json").forEach((source, i) => {
      const program = JSON.parse(source);
      if (!Array.isArray(program.ops)) return; // a config, not a program
      test(`${guide} json #${i + 1}`, () => {
        allClosed(og.replay(source));
      });
    });
  }
});

describe("the phase 6 evaluation's programs replay", () => {
  const evalDir = join(root, "eval/phase6");
  const programs: string[] = [];
  if (existsSync(evalDir)) {
    for (const run of readdirSync(evalDir, { withFileTypes: true })) {
      if (!run.isDirectory()) continue;
      for (const obj of readdirSync(join(evalDir, run.name), { withFileTypes: true })) {
        const p = join(evalDir, run.name, obj.name, "program.json");
        if (obj.isDirectory() && existsSync(p)) programs.push(p);
      }
    }
  }
  for (const p of programs) {
    test(p.slice(evalDir.length + 1), () => {
      const shape = og.replay(readFileSync(p, "utf8"));
      expect(shape.measure().parts.length).toBeGreaterThan(0);
    });
  }
});
