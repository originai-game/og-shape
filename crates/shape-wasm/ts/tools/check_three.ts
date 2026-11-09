/**
 * Reads the washing machine's GLB with three.js's own GLTFLoader, headless,
 * and checks that its joints work as three.js sees them:
 *
 * 1. every joint node carries `userData.s3d_joint` with a kind, an axis in
 *    its own frame, a value and limits in SI units;
 * 2. re-posing in three.js: each joint node's rotation (or position)
 *    undone by its exported `value` and redone at another value gives
 *    the same world vertices as the kernel's own export at that pose:
 *    the posed file re-posed to rest matches the rest file, and the rest
 *    file posed to `washer_open()` matches the posed file;
 * 3. at rest the door is closed on the front and the drawer flush; open,
 *    the door swings out past the front and the drawer slides out 120 mm.
 *
 * Run: `bun tools/check_three.ts` after `../build.sh` and `bun install`.
 * Exits 1 on any failure.
 */

import * as THREE from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { load } from "../src/index.ts";
import { washer, washerOpen } from "../examples/washing_machine.ts";

type Joint = { name: string; type: string; value: number; axis?: number[]; lower?: number; upper?: number };

function parse(bytes: Uint8Array): Promise<THREE.Group> {
  const buf = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
  return new Promise((resolve, reject) =>
    new GLTFLoader().parse(buf, "", (g) => resolve(g.scene), reject),
  );
}

function joints(scene: THREE.Object3D): Map<string, THREE.Object3D> {
  const out = new Map<string, THREE.Object3D>();
  scene.traverse((o) => {
    if (o.userData?.s3d_joint) out.set(o.userData.s3d_joint.name, o);
  });
  return out;
}

/** Moves a joint node from its exported value to `value`, as a viewer would. */
function setJoint(node: THREE.Object3D, value: number) {
  const j: Joint = node.userData.s3d_joint;
  const current = node.userData.current ?? j.value;
  if (j.type === "fixed" || j.type === "floating") return;
  const axis = new THREE.Vector3(...(j.axis as [number, number, number]));
  if (j.type === "prismatic") {
    // The slide runs along the axis in the node's own frame.
    const d = axis.clone().applyQuaternion(node.quaternion).multiplyScalar(value - current);
    node.position.add(d);
  } else {
    node.quaternion.multiply(new THREE.Quaternion().setFromAxisAngle(axis, value - current));
  }
  node.userData.current = value;
}

/** World positions of every mesh vertex, by mesh name. */
function vertices(scene: THREE.Object3D): Map<string, THREE.Vector3[]> {
  scene.updateMatrixWorld(true);
  const out = new Map<string, THREE.Vector3[]>();
  scene.traverse((o) => {
    const mesh = o as THREE.Mesh;
    if (!mesh.isMesh) return;
    const pos = mesh.geometry.getAttribute("position");
    const list: THREE.Vector3[] = [];
    for (let i = 0; i < pos.count; i++) list.push(new THREE.Vector3().fromBufferAttribute(pos, i).applyMatrix4(mesh.matrixWorld));
    out.set(mesh.name || o.parent?.name || "?", list);
  });
  return out;
}

function box(scene: THREE.Object3D, name: string): THREE.Box3 {
  scene.updateMatrixWorld(true);
  const o = scene.getObjectByName(name);
  if (!o) throw new Error(`no node ${name}`);
  return new THREE.Box3().setFromObject(o);
}

let failures = 0;
function check(what: string, ok: boolean, detail = "") {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}${detail ? `: ${detail}` : ""}`);
  if (!ok) failures++;
}

function maxDistance(a: Map<string, THREE.Vector3[]>, b: Map<string, THREE.Vector3[]>): number {
  let worst = 0;
  for (const [name, va] of a) {
    const vb = b.get(name);
    if (!vb || vb.length !== va.length) return Infinity;
    for (let i = 0; i < va.length; i++) worst = Math.max(worst, va[i].distanceTo(vb[i]));
  }
  return worst;
}

const og = await load();
const rest = await parse(washer(og).glb());
const open = await parse(washerOpen(og).glb());
const pose: Record<string, number> = { door_hinge: og.deg(92), drawer_slide: 0.12, dial_turn: og.deg(30), drum_spin: 0 };

// 1. The extras.
const restJoints = joints(rest);
check("four joints", restJoints.size === 4, [...restJoints.keys()].join(", "));
const expect: Record<string, string> = {
  door_hinge: "revolute",
  drum_spin: "continuous",
  drawer_slide: "prismatic",
  dial_turn: "continuous",
};
for (const [name, kind] of Object.entries(expect)) {
  const j: Joint | undefined = restJoints.get(name)?.userData.s3d_joint;
  check(`${name} is ${kind}`, j?.type === kind, JSON.stringify(j));
}
const hinge: Joint = restJoints.get("door_hinge")!.userData.s3d_joint;
check("door limits in radians", Math.abs((hinge.upper ?? 0) - og.deg(143)) < 1e-9 && hinge.lower === 0);
const slide: Joint = restJoints.get("drawer_slide")!.userData.s3d_joint;
check("drawer limits in metres", Math.abs((slide.upper ?? 0) - 0.2) < 1e-12);
const openJoints = joints(open);
check(
  "posed values written",
  Math.abs(openJoints.get("door_hinge")!.userData.s3d_joint.value - og.deg(92)) < 1e-12 &&
    Math.abs(openJoints.get("drawer_slide")!.userData.s3d_joint.value - 0.12) < 1e-12,
);

// 2. Re-posing in three.js matches the kernel's export at that pose.
const restV = vertices(rest);
const openV = vertices(open);
for (const node of openJoints.values()) setJoint(node, 0);
const d1 = maxDistance(vertices(open), restV);
check("open file posed to rest in three.js = rest file", d1 < 1e-6, `${d1.toExponential(2)} m`);
for (const [name, node] of restJoints) setJoint(node, pose[name]);
const d2 = maxDistance(vertices(rest), openV);
check("rest file posed open in three.js = open file", d2 < 1e-6, `${d2.toExponential(2)} m`);
for (const node of restJoints.values()) setJoint(node, 0);

// 3. What the joints do (metres, Y up: kernel +X stays +X).
const doorRest = box(rest, "door");
check("door closed at rest: on the front (x 0.300–0.375)", Math.abs(doorRest.min.x - 0.3) < 1e-4 && Math.abs(doorRest.max.x - 0.375) < 1e-4, `${doorRest.min.x.toFixed(4)}–${doorRest.max.x.toFixed(4)}`);
setJoint(restJoints.get("door_hinge")!, og.deg(143));
const doorWide = box(rest, "door");
check("door at 143° swings out past the front", doorWide.max.x > 0.6, `max x ${doorWide.max.x.toFixed(3)}`);
setJoint(restJoints.get("door_hinge")!, 0);
const drawerRest = box(rest, "drawer");
check("drawer flush at rest (front at x 0.315)", Math.abs(drawerRest.max.x - 0.315) < 1e-4, drawerRest.max.x.toFixed(4));
setJoint(restJoints.get("drawer_slide")!, 0.12);
const drawerOut = box(rest, "drawer");
check("drawer slides out 0.120 m", Math.abs(drawerOut.max.x - drawerRest.max.x - 0.12) < 1e-6, (drawerOut.max.x - drawerRest.max.x).toFixed(6));
const dialBefore = vertices(rest).get("dial")!;
setJoint(restJoints.get("dial_turn")!, og.deg(30));
const dialAfter = vertices(rest).get("dial")!;
const dialMoved = Math.max(...dialBefore.map((v, i) => v.distanceTo(dialAfter[i])));
const dialBox = [box(rest, "dial")];
check("dial turns in place", dialMoved > 0.005 && dialBox[0].max.x < 0.321, `moved ${dialMoved.toFixed(4)} m`);

console.log(failures === 0 ? "three.js: all checks pass" : `three.js: ${failures} failed`);
process.exit(failures === 0 ? 0 : 1);
