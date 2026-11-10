/**
 * The browser side of `render_glb.ts`: three.js renders mesh files side
 * by side in one canvas, each in its own square viewport, with every map
 * its material has (GLTFLoader; OBJLoader with its MTL, vertex colours
 * shown when the file has them). `window.view.render(files, azimuth)`
 * draws them and answers what three.js read; the driver takes the
 * screenshot.
 */

import * as THREE from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { MTLLoader } from "three/examples/jsm/loaders/MTLLoader.js";
import { OBJLoader } from "three/examples/jsm/loaders/OBJLoader.js";

const canvas = document.createElement("canvas");
document.body.append(canvas);
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, preserveDrawingBuffer: true });
renderer.setPixelRatio(1);
renderer.setClearColor(0xf2f2ee);
renderer.setScissorTest(true);

async function load(url: string): Promise<THREE.Object3D> {
  if (url.endsWith(".obj")) {
    const dir = url.slice(0, url.lastIndexOf("/") + 1);
    const text = await (await fetch(url)).text();
    const lib = text.match(/^mtllib\s+(.+)$/m)?.[1]?.trim();
    const obj = new OBJLoader();
    if (lib) {
      const r = await fetch(dir + lib);
      if (r.ok) {
        const m = new MTLLoader().setResourcePath(dir).parse(await r.text(), dir);
        m.preload();
        obj.setMaterials(m);
      }
    }
    const group = obj.parse(text);
    group.traverse((o) => {
      const m = o as THREE.Mesh;
      if (m.isMesh && m.geometry.getAttribute("color")) {
        (m.material as THREE.MeshPhongMaterial).vertexColors = true;
      }
    });
    // MTLLoader's textures load after the parse: wait for them.
    const pending: any[] = [];
    group.traverse((o) => {
      const m = o as THREE.Mesh;
      if (m.isMesh) for (const mat of [m.material].flat() as any[]) if (mat.map) pending.push(mat.map);
    });
    for (let t = 0; t < 200 && pending.some((x) => !(x.image?.width > 0)); t++) {
      await new Promise((ok) => setTimeout(ok, 50));
    }
    // OBJ is read with Y up, as glTF is.
    return group;
  }
  const buf = await (await fetch(url)).arrayBuffer();
  const gltf: any = await new Promise((resolve, reject) => new GLTFLoader().parse(buf, "", resolve, reject));
  return gltf.scene;
}

function maps(o: THREE.Object3D) {
  const found = new Set<string>();
  let triangles = 0;
  o.traverse((x) => {
    const m = x as THREE.Mesh;
    if (!m.isMesh) return;
    const g = m.geometry;
    triangles += (g.index ? g.index.count : g.getAttribute("position").count) / 3;
    if (g.getAttribute("color")) found.add("vertex colours");
    for (const mat of [m.material].flat() as any[]) {
      for (const k of ["map", "normalMap", "roughnessMap", "metalnessMap", "emissiveMap", "aoMap"]) {
        if (mat[k]) found.add(k);
      }
    }
  });
  return { triangles, maps: [...found].sort() };
}

async function render(files: string[], azimuth: number, tile: number) {
  renderer.setSize(tile * files.length, tile);
  renderer.setScissorTest(false);
  renderer.clear();
  renderer.setScissorTest(true);
  const read = [];
  for (let i = 0; i < files.length; i++) {
    const object = await load(files[i]);
    read.push(maps(object));
    const s = new THREE.Scene();
    s.add(new THREE.HemisphereLight(0xffffff, 0x666655, 1.6));
    const sun = new THREE.DirectionalLight(0xffffff, 1.6);
    sun.position.set(2, 4, 3);
    s.add(sun);
    s.add(object);
    object.updateMatrixWorld(true);
    const box = new THREE.Box3().setFromObject(object);
    const c = box.getCenter(new THREE.Vector3());
    const r = box.getSize(new THREE.Vector3()).length() / 2;
    const cam = new THREE.PerspectiveCamera(30, 1, r / 100, r * 100);
    const d = r / Math.sin((15 * Math.PI) / 180);
    const el = 0.35;
    cam.position.set(c.x + d * Math.cos(el) * Math.sin(azimuth), c.y + d * Math.sin(el), c.z + d * Math.cos(el) * Math.cos(azimuth));
    cam.lookAt(c);
    renderer.setViewport(i * tile, 0, tile, tile);
    renderer.setScissor(i * tile, 0, tile, tile);
    renderer.render(s, cam);
  }
  return read;
}

(window as any).view = { render };
(window as any).ready = true;
