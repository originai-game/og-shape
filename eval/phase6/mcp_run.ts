/**
 * Run `U` (research §19.7): run `M`'s five objects again, through the MCP
 * server instead of `run.ts`, with the guides as they are now. Each
 * object is one headless Claude Code agent (`claude -p`) whose only tools
 * are the og-shape server's (`shape-mcp`, its workspace a fresh directory
 * holding the brief and the reference render) and Read; it reads the
 * guides as MCP resources.
 *
 * Run from the repository root, after `cargo build --release -p
 * shape-mcp` and `crates/shape-wasm/build.sh`:
 *
 *   node eval/phase6/mcp_run.ts <scratch dir> [model] [object …] [--summary]
 *       [--run=<name>] [--jobs=<n>]
 *
 * `--summary` runs no agent: it only redoes the previews, measures and
 * the summary table from the programs already in the run's directory.
 * `--run` names the run's directory (`U` by default; run `Z`, research
 * §22, used `Z/Z1` and `Z/Z2`); without objects, run `U` takes its five
 * and any other run all twenty of `references.json`. `--jobs` caps the
 * agents running at once (all by default).
 * Writes `eval/phase6/<run>/<object>/`: `program.ts` (the last source the
 * agent ran), `program.json`, `preview.png`, `measure.json`, `runs.log`
 * (one line per `run_typescript` or `build`), `errors.log` (each failed
 * build's whole error text), `report.md` (the agent's
 * answer) and `usage.json` (turns and tokens from Claude Code's result);
 * `preview.png` and `measure.json` come from `run.ts` on `program.ts`,
 * as in the other runs. Then prints a table: builds (failed), turns,
 * tokens, triangles and the largest per-axis size deviation from
 * `references.json`.
 * The transcripts stay in the scratch directory.
 */

import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const repo = fileURLToPath(new URL("../../", import.meta.url));
const argv = process.argv.slice(2);
const summaryOnly = argv.includes("--summary");
const flag = (name: string) => argv.find((a) => a.startsWith(`--${name}=`))?.slice(name.length + 3);
const run = flag("run") ?? "U";
const jobs = Number(flag("jobs") ?? 0);
const [scratch, model = "claude-opus-5-5", ...only] = argv.filter((a) => !a.startsWith("--"));
if (!scratch) throw new Error("usage: mcp_run.ts <scratch dir> [model] [object …] [--summary] [--run=<name>] [--jobs=<n>]");
const references = JSON.parse(readFileSync(`${repo}eval/phase6/references.json`, "utf8"));
const objects = only.length
  ? only
  : run === "U"
    ? ["auto1_curved_tubes", "auto2_lever_knob", "sim2_mouse_and_pad", "obj2_djembe", "obj4_spectacles"]
    : Object.keys(references);

const PROMPT = `You are building a 3D model with og-shape, through its MCP server (the og-shape tools).

1. Read the server's resources og-shape://guides/shape.md and og-shape://guides/typescript.md. Read no other guide, file or source, except brief.md and reference.png in the current directory.
2. Read brief.md and look at reference.png (both in the current directory): the object to build.
3. Write the object as a TypeScript program and run it with the run_typescript tool, always with program_out: "program.json". Look at it with the preview tool (path "program.json"; use pose-free previews) and check it with measure. Fix the program and run it again. Use at most 8 run_typescript calls.
4. Finish with the program saved (program_out "program.json") and reply with:
   - ITERATIONS: how many run_typescript calls you made, and how many failed;
   - PROBLEMS: a list of what in the guides was missing, wrong or misleading for this object (or "none").`;

type Usage = Record<string, unknown>;

function runOne(object: string): Promise<void> {
  const ws = `${scratch}/${object}`;
  rmSync(ws, { recursive: true, force: true });
  mkdirSync(ws, { recursive: true });
  copyFileSync(`${repo}eval/phase6/briefs/${object}/brief.md`, `${ws}/brief.md`);
  copyFileSync(`${repo}eval/phase6/briefs/${object}/reference.png`, `${ws}/reference.png`);
  const config = {
    mcpServers: {
      "og-shape": {
        command: `${repo}target/release/shape-mcp`,
        args: ["--root", ws, "--binding", `${repo}crates/shape-wasm/ts`],
      },
    },
  };
  writeFileSync(`${scratch}/${object}.mcp.json`, JSON.stringify(config));
  const args = [
    "-p",
    PROMPT,
    "--model",
    model,
    "--output-format",
    "stream-json",
    "--verbose",
    "--mcp-config",
    `${scratch}/${object}.mcp.json`,
    "--strict-mcp-config",
    "--allowedTools",
    "mcp__og-shape__run_typescript,mcp__og-shape__build,mcp__og-shape__preview,mcp__og-shape__measure,mcp__og-shape__export,ListMcpResourcesTool,ReadMcpResourceTool,Read",
    "--disallowedTools",
    "Bash,Edit,Write,NotebookEdit,WebFetch,WebSearch,Task,Agent,Glob,Grep",
    "--max-turns",
    "80",
  ];
  const t0 = Date.now();
  return new Promise((resolve) => {
    const child = spawn("claude", args, { cwd: ws, stdio: ["ignore", "pipe", "pipe"] });
    let out = "";
    child.stdout.on("data", (d) => (out += d));
    child.stderr.on("data", (d) => process.stderr.write(`[${object}] ${d}`));
    child.on("close", (code) => {
      writeFileSync(`${scratch}/${object}.jsonl`, out);
      collect(object, ws, out, Date.now() - t0, code ?? -1);
      resolve();
    });
  });
}

function collect(object: string, ws: string, transcript: string, ms: number, code: number) {
  const dir = `${repo}eval/phase6/${run}/${object}`;
  mkdirSync(dir, { recursive: true });
  const errors: string[] = [];
  const events = transcript
    .split("\n")
    .filter(Boolean)
    .map((l) => {
      try {
        return JSON.parse(l);
      } catch {
        return {};
      }
    });
  // Tool calls by id, and their results.
  const calls = new Map<string, { name: string; input: any }>();
  const runs: string[] = [];
  let lastSource = "";
  for (const e of events) {
    for (const c of e.message?.content ?? []) {
      if (c.type === "tool_use") calls.set(c.id, { name: c.name, input: c.input });
      if (c.type === "tool_result") {
        const call = calls.get(c.tool_use_id);
        if (!call || !/run_typescript|__build$/.test(call.name)) continue;
        const text = Array.isArray(c.content) ? c.content.map((x: any) => x.text ?? "").join("") : String(c.content ?? "");
        const failed = c.is_error === true || text.startsWith("error:");
        if (call.name.endsWith("run_typescript") && call.input?.source) {
          if (!failed) lastSource = call.input.source;
        }
        let summary = "";
        if (!failed) {
          try {
            const r = JSON.parse(text);
            const parts = r.measures?.parts ?? [];
            summary = `parts ${parts.length} open ${parts.filter((p: any) => !p.closed).length} joints ${(r.measures?.joints ?? []).length} triangles ${r.measures?.triangles ?? "?"}`;
          } catch {
            summary = "";
          }
        }
        if (failed) errors.push(`--- build ${runs.length + 1} (${call.name.replace("mcp__og-shape__", "")})\n${text}`);
        runs.push(`${call.name.replace("mcp__og-shape__", "")} ${failed ? `error ${text.replace(/\s+/g, " ").slice(0, 160)}` : `ok ${summary}`}`);
      }
    }
  }
  const result = events.find((e) => e.type === "result") ?? {};
  writeFileSync(`${dir}/runs.log`, `${runs.join("\n")}\n`);
  writeFileSync(`${dir}/errors.log`, errors.length ? `${errors.join("\n")}\n` : "");
  writeFileSync(`${dir}/report.md`, `${result.result ?? "(no result)"}\n`);
  if (lastSource) writeFileSync(`${dir}/program.ts`, lastSource.endsWith("\n") ? lastSource : `${lastSource}\n`);
  const usage: Usage = {
    model,
    exit: code,
    seconds: Math.round(ms / 1000),
    turns: result.num_turns,
    usage: result.usage,
    builds: runs.length,
    failed: runs.filter((r) => r.includes(" error ")).length,
  };
  writeFileSync(`${dir}/usage.json`, `${JSON.stringify(usage, null, 1)}\n`);
  try {
    copyFileSync(`${ws}/program.json`, `${dir}/program.json`);
  } catch {
    // No program saved.
  }
  console.log(`${object}: exit ${code}, ${usage.seconds} s, ${usage.turns} turns, ${usage.builds} builds (${usage.failed} failed)`);
}

/** The preview and measures, by `run.ts` as for the other runs, and one table row. */
function summarize(object: string): string {
  const dir = `${repo}eval/phase6/${run}/${object}`;
  const tmp = `${scratch}/${object}.run`;
  rmSync(tmp, { recursive: true, force: true });
  const r = spawnSync("bun", [`${repo}eval/phase6/run.ts`, `${dir}/program.ts`, tmp], { encoding: "utf8" });
  if (r.status !== 0) return `| ${object} | run.ts failed: ${r.stderr.split("\n")[0]} |`;
  copyFileSync(`${tmp}/preview.png`, `${dir}/preview.png`);
  copyFileSync(`${tmp}/measure.json`, `${dir}/measure.json`);
  const m = JSON.parse(readFileSync(`${tmp}/measure.json`, "utf8"));
  const ref = references[object];
  const dev = Math.max(...m.size.map((v: number, k: number) => Math.abs(v - ref.reference_size_mm[k]) / ref.reference_size_mm[k]));
  const u = JSON.parse(readFileSync(`${dir}/usage.json`, "utf8"));
  const t = u.usage ?? {};
  const tokens = (t.input_tokens ?? 0) + (t.cache_creation_input_tokens ?? 0) + (t.cache_read_input_tokens ?? 0) + (t.output_tokens ?? 0);
  const open = m.parts.filter((p: any) => !p.closed).length;
  return `| ${object} | ${u.builds} (${u.failed}) | ${u.turns} | ${tokens} | ${t.output_tokens} | ${m.triangles} | ${(dev * 100).toFixed(1)} % | ${m.joints.length}/${ref.n_joints} | ${open} |`;
}

if (!summaryOnly) {
  const queue = [...objects];
  const worker = async () => {
    for (let o = queue.shift(); o; o = queue.shift()) await runOne(o);
  };
  await Promise.all(Array.from({ length: jobs > 0 ? jobs : objects.length }, worker));
}
console.log("| object | builds (failed) | turns | tokens (all) | output | triangles | size dev. | joints | open parts |");
console.log("|---|---|---|---|---|---|---|---|---|");
for (const o of objects) console.log(summarize(o));
