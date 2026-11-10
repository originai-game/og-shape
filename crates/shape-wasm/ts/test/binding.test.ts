/**
 * The binding over wasm32-unknown-unknown: errors as thrown values with
 * the kernel's message, frames turned in the kernel, and the washing
 * machine giving the very bytes the native and wasm32-wasip1 golden runs
 * hash (`golden/hashes.txt`), from the program committed in
 * `golden/programs/`.
 *
 * Run: `bun test test/` after `../build.sh`.
 */

import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { load } from "../src/index.ts";
import { washerOpen } from "../examples/washing_machine.ts";

const og = await load();
const root = new URL("../../../../", import.meta.url);

/** The gate's framing: SHA-256 over the length (u64, little-endian) and the bytes. */
function goldenHash(bytes: Uint8Array): string {
  const len = Buffer.alloc(8);
  len.writeBigUInt64LE(BigInt(bytes.length));
  return createHash("sha256").update(len).update(bytes).digest("hex");
}

function golden(name: string): string {
  const line = readFileSync(new URL("golden/hashes.txt", root), "utf8")
    .split("\n")
    .find((l) => l.startsWith(`${name} `));
  if (!line) throw new Error(`no golden line ${name}`);
  return line.split(" ")[1];
}

describe("frames", () => {
  test("face frames, moves and quarter turns", () => {
    const s = og.model({ name: "f" });
    const b = s.box({ name: "b", width: 600, depth: 400, height: 850 });
    const front = s.face(b, "+X");
    expect(front.origin).toEqual([300, 0, 425]);
    expect(front.x).toEqual([0, 1, 0]);
    expect(front.move(0, -425).origin).toEqual([300, 0, 0]);
    expect(front.offset(10).origin).toEqual([310, 0, 425]);
    const t = front.turn(0, 0, og.deg(90));
    expect(t.x).toEqual([0, 0, 1]);
    expect(t.y).toEqual([0, -1, 0]);
  });

  test("distributors", () => {
    const s = og.model({ name: "d" });
    expect(s.grid({ nx: 2, px: 50 }).map((f) => f.origin[0])).toEqual([-25, 25]);
    expect(s.ring({ n: 6, radius: 10 })).toHaveLength(6);
  });
});

describe("errors are thrown with the kernel's message", () => {
  test("a face name that does not exist lists the names", () => {
    const s = og.model({ name: "e" });
    const b = s.box({ name: "b", width: 1, depth: 1, height: 1 });
    expect(() => s.face(b, "lid")).toThrow(/face names: .*top/);
  });
  test("a used-up tool", () => {
    const s = og.model({ name: "e" });
    const a = s.box({ name: "a", width: 10, depth: 10, height: 10 });
    const t = s.cylinder({ name: "t", radius: 2, height: 30, on: og.world.move(0, 0, -5) });
    s.cut(a, t);
    expect(() => s.cut(a, t)).toThrow(/used up/);
  });
  test("a pose out of range", () => {
    const s = washerOpen(og);
    expect(() => s.pose("door_hinge", og.deg(150))).toThrow(/outside its range/);
  });
  test("a frame read off a posed part", () => {
    const s = og.model({ name: "p" });
    const base = s.box({ name: "base", width: 40, depth: 40, height: 10 });
    const lid = s.box({ name: "lid", width: 40, depth: 40, height: 2, on: s.face(base, "top") });
    s.joint(lid, { name: "flip", parent: base, on: s.bbox(lid, "-Y-Z"), axis: "-X", range: [0, 2] });
    s.pose("flip", 1);
    expect(() => s.face(lid, "top")).toThrow(/at rest.*"flip"/);
    expect(s.face(base, "top").origin).toEqual([0, 0, 10]);
  });
});

describe("the pose", () => {
  test("measure({ posed: true }) measures at the pose, with the joints", () => {
    const s = washerOpen(og);
    const rest = s.measure();
    const posed = s.measure({ posed: true });
    expect(posed.posed).toBe(true);
    expect(posed.pose.drawer_slide).toBe(120);
    expect(posed.joints.map((j: any) => j.name)).toEqual(["door_hinge", "drum_spin", "drawer_slide", "dial_turn"]);
    // The open door reaches beyond the rest's bounds; the volume is the rest's.
    expect(posed.boundsMax[0]).toBeGreaterThan(rest.boundsMax[0]);
    expect(Math.abs(posed.volume - rest.volume)).toBeLessThan(1e-3);
    expect(() => s.measure({ pose: true } as any)).toThrow(/posed/);
  });
  test("OBJ at the pose", () => {
    expect(washerOpen(og).obj().split("\n")[0]).toEndWith(", posed");
  });
});

describe("errors say where", () => {
  test("the op, its index, the part and the author's line", () => {
    const s = og.model({ name: "e" });
    s.box({ name: "plate", width: 10, depth: 10, height: 2 });
    let message = "";
    try {
      s.cylinder({ name: "rod", radius: 1, height: 2, segmentz: 8 } as never);
    } catch (e) {
      message = (e as Error).message;
    }
    expect(message).toStartWith('op 1 cylinder "rod": cylinder has no field "segmentz"; it takes: ');
    expect(message).toMatch(/\(at \/.*binding\.test\.ts:\d+:\d+\)$/);
  });
  test("descs the binding reads itself refuse unknown fields too", () => {
    expect(() => og.frame({ origin: [0, 0, 0], norm: [0, 0, 1] } as never)).toThrow('frame has no field "norm"');
    const s = og.model({ name: "p" });
    s.box({ name: "b", width: 1, depth: 1, height: 1 });
    expect(() => s.preview({ widht: 64 } as never)).toThrow('preview has no field "widht"');
  });
});

describe("profile helpers", () => {
  test("translate, rotate, offset and scale are the JSON forms", () => {
    const r = og.rect({ width: 4, height: 2 });
    expect(og.translate({ profile: r, x: 1 })).toEqual({ translate: { profile: r, x: 1 } });
    const s = og.model({ name: "p" });
    const p = s.extrude({
      name: "p",
      height: 1,
      profile: og.offset({ profile: og.rotate({ profile: og.scale({ profile: r, x: 2 }), angle: og.deg(90) }), distance: 0.5 }),
    });
    const m = s.measure().parts[0];
    // 8 × 2, turned a quarter, grown 0.5: 3 wide in x, 9 in y.
    expect(m.boundsMax[0] - m.boundsMin[0]).toBeCloseTo(3, 9);
    expect(m.boundsMax[1] - m.boundsMin[1]).toBeCloseTo(9, 9);
    expect(p.name).toBe("p");
  });
});

describe("the rest of the surface (stream R)", () => {
  test("profiles from paths, until, helices, merge, delete and boolean reports", () => {
    const s = og.model({ name: "r" });
    const d = og.path([[0, -5], { arc: { through: [5, 0], to: [0, 5] } }, [-3, 5], { tangentArc: { to: [-3, -1] } }]);
    expect(d).toEqual({ path: [[0, -5], { arc: { through: [5, 0], to: [0, 5] } }, [-3, 5], { tangentArc: { to: [-3, -1] } }] });
    s.extrude({ name: "d", profile: d, height: 1 });
    s.extrude({ name: "h", profile: og.hull(og.circle({ radius: 2 }), og.mirror({ profile: og.superellipse({ rx: 1, ry: 1, exponent: 4 }) })), height: 1 });
    s.extrude({ name: "k", profile: og.stroke({ path: [[0, 0], [5, 0]], width: 1 }), height: 1 });
    s.extrude({ name: "p", profile: og.spline({ points: [[0, 0], [3, 0], [2, 2]] }), height: 1 });
    const shelf = s.box({ name: "shelf", width: 4, depth: 4, height: 1, on: og.world.move(10, 0, 5) });
    const leg = s.extrude({ name: "leg", profile: og.rect({ width: 1, height: 1 }), until: { part: shelf, face: "bottom" }, on: og.world.move(10, 0, 0) });
    const spring = s.wire({ name: "spring", diameter: 0.5, path: og.helix({ radius: 2, pitch: 1, turns: 2 }) });
    s.merge(leg, spring);
    const block = s.box({ name: "block", width: 6, depth: 6, height: 6, on: og.world.move(0, 20, 0) });
    const hole = s.box({ name: "hole", width: 2, depth: 2, height: 2, on: og.world.move(0, 20, 2) });
    expect(s.cut(block, hole, { check: true })).toEqual({ voids: 1, touchingEdges: 0, touching: false });
    const scrap = s.box({ name: "scrap", width: 1, depth: 1, height: 1 });
    s.delete(scrap);
    const legM = s.measure().parts.find((p: any) => p.name === "leg");
    expect(legM.boundsMax[2]).toBe(5);
    expect(legM.components).toBe(2);
    expect(s.measure().parts.map((p: any) => p.name)).not.toContain("scrap");
    expect(og.replay(s.program()).glb()).toEqual(s.glb());
  });
  test("text takes the caller's font by name, and replay is given it again", () => {
    const fonts = { "fonts/x.ttf": new TextEncoder().encode("not a font") };
    const s = og.model({ name: "t", fonts });
    expect(() => s.extrude({ name: "t", profile: og.text({ text: "A", font: "fonts/x.ttf", size: 5 }), height: 1 })).toThrow("not a TrueType");
    expect(() => s.extrude({ name: "t", profile: og.text({ text: "A", font: "other.ttf", size: 5 }), height: 1 })).toThrow('"other.ttf" was not');
    const program = JSON.stringify({ name: "t", ops: [{ op: "extrude", name: "t", height: 1, profile: { text: { text: "A", font: "fonts/x.ttf", size: 5 } } }] });
    expect(() => og.replay(program)).toThrow("none");
    expect(() => og.replay(program, { fonts })).toThrow("not a TrueType");
  });
});

describe("the washing machine", () => {
  const s = washerOpen(og);
  test("records the committed program", () => {
    const committed = readFileSync(new URL("golden/programs/washing_machine.json", root), "utf8");
    expect(s.program()).toBe(committed.trim());
  });
  test("its GLB is the golden bytes", () => {
    expect(goldenHash(s.glb())).toBe(golden("model.washing_machine.glb"));
  });
  test("its preview is the golden PNG", () => {
    const png = s.preview({ width: 320, views: 4, columns: 4, elevation: og.deg(20) });
    expect(goldenHash(png)).toBe(golden("model.washing_machine.png"));
  });
  test("a replay gives the same GLB", () => {
    expect(og.replay(s.program()).glb()).toEqual(s.glb());
  });
  test("every part is closed", () => {
    const m = s.measure();
    expect(m.parts.map((p: any) => p.name)).toEqual(["cabinet", "drum", "ring", "glass", "handle", "drawer", "dial"]);
    expect(m.parts.every((p: any) => p.closed)).toBe(true);
    expect(m.joints).toEqual(["door_hinge", "drum_spin", "drawer_slide", "dial_turn"]);
  });
});
