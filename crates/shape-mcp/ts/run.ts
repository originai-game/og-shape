/**
 * Runs one og-shape TypeScript program for shape-mcp's `run_typescript`
 * tool and writes the program it recorded.
 *
 *   bun run.ts <binding src/index.ts> <program.ts> <out.json>
 *
 * The program's default export takes the loaded kernel (`og`) and returns
 * the `Shape` it built (it may be async). Its recorded program
 * (`shape.program()`) is written to `out.json`; the server replays it
 * natively, so every measure, preview and file it reports comes from the
 * native kernel, not from this process. What the program logs is passed
 * back as text.
 */

const [bindingPath, programPath, outPath] = process.argv.slice(2);
if (!bindingPath || !programPath || !outPath) {
  throw new Error("usage: bun run.ts <binding index.ts> <program.ts> <out.json>");
}
const { load } = await import(bindingPath);
const mod = await import(programPath);
if (typeof mod.default !== "function") {
  throw new Error("the program must `export default function (og) { …; return shape; }`");
}
const og = await load();
const shape = await mod.default(og);
if (!shape || typeof shape.program !== "function") {
  throw new Error("the default export must return the Shape it built (`og.model({ name })`)");
}
await Bun.write(outPath, shape.program());
