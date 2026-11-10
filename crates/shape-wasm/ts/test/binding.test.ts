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
