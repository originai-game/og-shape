/**
 * The browser side of `check_render.ts`: three.js's GLTFLoader and
 * WebGLRenderer in a real page. Exposes `window.check` with one function
 * per step; each loads, moves or poses, renders into the canvas and
 * returns its measurements. The driver takes the screenshots.
 *
 * Kernel axes to glTF: metres, Y up, (x, y, z) mm → (x, z, −y) / 1000.
 */

import * as THREE from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";

type Joint = { name: string; type: string; value: number; axis?: number[]; lower?: number; upper?: number };
type Turn = { bone: string; axis: [number, number, number]; angle: number };
type Poses = { rest: number[][]; poses: { name: string; turns: Turn[]; positions: number[][] }[] };

const canvas = document.createElement("canvas");
document.body.append(canvas);
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, preserveDrawingBuffer: true });
renderer.setPixelRatio(1);
renderer.setSize(640, 480);
renderer.setClearColor(0xf2f2ee);

async function parse(url: string): Promise<THREE.Group> {
  const buf = await (await fetch(url)).arrayBuffer();
  return new Promise((resolve, reject) => new GLTFLoader().parse(buf, "", (g) => resolve(g.scene), reject));
}

/** Renders `scene` from a fixed three-quarter view framing its bounds. */
function render(scene: THREE.Object3D, azimuth = 0.6, elevation = 0.35): void {
  const s = new THREE.Scene();
  s.add(new THREE.HemisphereLight(0xffffff, 0x666655, 1.6));
  const sun = new THREE.DirectionalLight(0xffffff, 2.2);
  sun.position.set(2, 3, 4);
  s.add(sun);
  s.add(scene);
  scene.updateMatrixWorld(true);
  const box = new THREE.Box3().setFromObject(scene);
  const c = box.getCenter(new THREE.Vector3());
  const r = box.getSize(new THREE.Vector3()).length() / 2;
  const cam = new THREE.PerspectiveCamera(30, 640 / 480, r / 100, r * 100);
  const d = r / Math.sin((15 * Math.PI) / 180);
  // Kernel +X (glTF +X) is the washer's front: look at it from the front right.
  cam.position.set(
    c.x + d * Math.cos(elevation) * Math.cos(azimuth),
    c.y + d * Math.sin(elevation),
    c.z - d * Math.cos(elevation) * Math.sin(azimuth),
  );
  cam.lookAt(c);
  renderer.render(s, cam);
  s.remove(scene);
}

function joints(scene: THREE.Object3D): Map<string, THREE.Object3D> {
  const out = new Map<string, THREE.Object3D>();
  scene.traverse((o) => {
    if (o.userData?.s3d_joint) out.set(o.userData.s3d_joint.name, o);
  });
  return out;
}

/** Moves a joint node from its current value to `value`, as a viewer would. */
function setJoint(node: THREE.Object3D, value: number): void {
  const j: Joint = node.userData.s3d_joint;
  const current = node.userData.current ?? j.value;
  if (j.type === "fixed" || j.type === "floating") return;
  const axis = new THREE.Vector3(...(j.axis as [number, number, number]));
  if (j.type === "prismatic") {
    node.position.add(axis.clone().applyQuaternion(node.quaternion).multiplyScalar(value - current));
  } else {
    node.quaternion.multiply(new THREE.Quaternion().setFromAxisAngle(axis, value - current));
  }
  node.userData.current = value;
}

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

function maxDistance(a: Map<string, THREE.Vector3[]>, b: Map<string, THREE.Vector3[]>): number {
  let worst = 0;
  for (const [name, va] of a) {
    const vb = b.get(name);
    if (!vb || vb.length !== va.length) return Infinity;
    for (let i = 0; i < va.length; i++) worst = Math.max(worst, va[i].distanceTo(vb[i]));
  }
  return worst;
}

function box(scene: THREE.Object3D, name: string): THREE.Box3 {
  scene.updateMatrixWorld(true);
  return new THREE.Box3().setFromObject(scene.getObjectByName(name)!);
}

/** The tree of named nodes, two spaces an indent. */
function tree(o: THREE.Object3D, depth = 0): string[] {
  const kind = (o as THREE.SkinnedMesh).isSkinnedMesh ? "skinned mesh" : (o as THREE.Mesh).isMesh ? "mesh" : (o as THREE.Bone).isBone ? "bone" : "node";
  const j = o.userData?.s3d_joint ? ` joint ${o.userData.s3d_joint.name} (${o.userData.s3d_joint.type})` : "";
  const lines = [`${"  ".repeat(depth)}${o.name || "(unnamed)"}: ${kind}${j}`];
  for (const c of o.children) lines.push(...tree(c, depth + 1));
  return lines;
}

let washerRest: THREE.Group;
let washerOpen: THREE.Group;
let figure: THREE.Group;
let poses: Poses;

const check = {
  async washer() {
    washerRest = await parse("/out/washer_rest.glb");
    washerOpen = await parse("/out/washer_open.glb");
    const rj = joints(washerRest);
    const oj = joints(washerOpen);
    const restV = vertices(washerRest);
    const openV = vertices(washerOpen);
    for (const n of oj.values()) setJoint(n, 0);
    const openToRest = maxDistance(vertices(washerOpen), restV);
    const pose: Record<string, number> = { door_hinge: (92 * Math.PI) / 180, drawer_slide: 0.12, dial_turn: (30 * Math.PI) / 180, drum_spin: 0 };
    for (const [name, n] of rj) setJoint(n, pose[name]);
    const restToOpen = maxDistance(vertices(washerRest), openV);
    for (const n of rj.values()) setJoint(n, 0);
    for (const [name, n] of oj) setJoint(n, pose[name] ?? 0);
    const doorRest = box(washerRest, "door");
    const drawerRest = box(washerRest, "drawer");
    return {
      tree: tree(washerRest),
      joints: [...rj.values()].map((n) => n.userData.s3d_joint),
      openToRest,
      restToOpen,
      doorRest: [doorRest.min.x, doorRest.max.x],
      drawerFront: drawerRest.max.x,
    };
  },
  showWasher(which: "rest" | "open") {
    render(which === "rest" ? washerRest : washerOpen);
  },
  /** Moves the rest file's joints in three.js (radians, metres) and renders. */
  moveWasher(values: Record<string, number>) {
    const rj = joints(washerRest);
    for (const [name, v] of Object.entries(values)) setJoint(rj.get(name)!, v);
    render(washerRest);
    const door = box(washerRest, "door");
    const drawer = box(washerRest, "drawer");
    return { doorMaxX: door.max.x, drawerFront: drawer.max.x };
  },

  async figure() {
    figure = await parse("/out/figure.glb");
    poses = await (await fetch("/out/figure_poses.json")).json();
    let skinned: THREE.SkinnedMesh | undefined;
    figure.traverse((o) => {
      if ((o as THREE.SkinnedMesh).isSkinnedMesh) skinned = o as THREE.SkinnedMesh;
    });
    if (!skinned) throw new Error("no skinned mesh in the figure");
    for (const b of skinned.skeleton.bones) b.userData.rest = b.quaternion.clone();
    return {
      tree: tree(figure),
      bones: skinned.skeleton.bones.map((b) => b.name),
      vertices: skinned.geometry.getAttribute("position").count,
      triangles: (skinned.geometry.index?.count ?? 0) / 3,
      kernelVertices: poses.rest.length,
    };
  },
  /**
   * Poses the figure's bones by the kernel's turns (each bone's rest
   * rotation, then its turns in order about axes in its rest axes),
   * renders, and measures three.js's skinned vertices
   * (`SkinnedMesh.getVertexPosition`, the shader's linear blend on the
   * CPU) against the kernel's `pose`.
   */
  poseFigure(name: string) {
    let skinned!: THREE.SkinnedMesh;
    figure.traverse((o) => {
      if ((o as THREE.SkinnedMesh).isSkinnedMesh) skinned = o as THREE.SkinnedMesh;
    });
    const bones = new Map(skinned.skeleton.bones.map((b) => [b.name, b]));
    for (const b of bones.values()) b.quaternion.copy(b.userData.rest);
    const p = name === "rest" ? { name, turns: [], positions: poses.rest } : poses.poses.find((q) => q.name === name)!;
    for (const t of p.turns) {
      const b = bones.get(t.bone);
      if (!b) throw new Error(`no bone ${t.bone}`);
      // Kernel axes to glTF axes; a bone's rest axes are the world's.
      const axis = new THREE.Vector3(t.axis[0], t.axis[2], -t.axis[1]).normalize();
      b.quaternion.multiply(new THREE.Quaternion().setFromAxisAngle(axis, t.angle));
    }
    figure.updateMatrixWorld(true);
    skinned.skeleton.update();
    render(figure, 0.5, 0.15);

    // The kernel's posed position for each rest position (f32 metres, glTF axes).
    const key = (x: number, y: number, z: number) => `${Math.fround(x)},${Math.fround(y)},${Math.fround(z)}`;
    const kernel = new Map<string, THREE.Vector3>();
    let kernelSplit = 0;
    poses.rest.forEach((r, i) => {
      const q = p.positions[i];
      const k = key(r[0] / 1000, r[2] / 1000, -r[1] / 1000);
      const v = new THREE.Vector3(q[0] / 1000, q[2] / 1000, -q[1] / 1000);
      const had = kernel.get(k);
      if (had && had.distanceTo(v) > 1e-9) kernelSplit++;
      kernel.set(k, v);
    });
    const pos = skinned.geometry.getAttribute("position");
    const seen = new Map<string, THREE.Vector3>();
    let worst = 0;
    let unmatched = 0;
    let tear = 0;
    const v = new THREE.Vector3();
    for (let i = 0; i < pos.count; i++) {
      const k = key(pos.getX(i), pos.getY(i), pos.getZ(i));
      skinned.getVertexPosition(i, v);
      v.applyMatrix4(skinned.matrixWorld);
      const want = kernel.get(k);
      if (!want) unmatched++;
      else worst = Math.max(worst, v.distanceTo(want));
      // Vertices that share a rest position must share a posed one.
      const other = seen.get(k);
      if (other) tear = Math.max(tear, other.distanceTo(v));
      else seen.set(k, v.clone());
    }
    // Triangles' edge lengths stretch, never break: the largest stretch.
    const idx = skinned.geometry.index!;
    let stretch = 0;
    const a = new THREE.Vector3();
    const b = new THREE.Vector3();
    const ra = new THREE.Vector3();
    const rb = new THREE.Vector3();
    for (let t = 0; t < idx.count; t += 3) {
      for (let e = 0; e < 3; e++) {
        const i = idx.getX(t + e);
        const j = idx.getX(t + ((e + 1) % 3));
        skinned.getVertexPosition(i, a);
        skinned.getVertexPosition(j, b);
        ra.fromBufferAttribute(pos, i);
        rb.fromBufferAttribute(pos, j);
        const rest = ra.distanceTo(rb);
        if (rest > 0) stretch = Math.max(stretch, a.distanceTo(b) / rest);
      }
    }
    return { pose: name, vertices: pos.count, unmatched, maxToKernel: worst, sharedRestMaxApart: tear, kernelSplit, maxEdgeStretch: stretch };
  },
};

(window as unknown as { check: typeof check }).check = check;
(window as unknown as { ready: boolean }).ready = true;
