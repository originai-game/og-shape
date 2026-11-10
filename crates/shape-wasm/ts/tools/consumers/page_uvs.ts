/**
 * The browser side of `check_uvs.ts`: three.js's GLTFLoader reads the UV
 * plate's GLB (stream Y, `docs/research.md` §21) and renders it with the
 * checker map it carries. `window.check.plate()` answers what three.js
 * read; the driver takes the screenshot.
 */

import * as THREE from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";

const canvas = document.createElement("canvas");
document.body.append(canvas);
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, preserveDrawingBuffer: true });
renderer.setPixelRatio(1);
renderer.setSize(800, 600);
renderer.setClearColor(0xf2f2ee);

async function plate(azimuth: number) {
  const buf = await (await fetch("/out/uv_plate.glb")).arrayBuffer();
  const gltf: any = await new Promise((resolve, reject) => new GLTFLoader().parse(buf, "", resolve, reject));
  const scene: THREE.Object3D = gltf.scene;
  const meshes: any[] = [];
  scene.traverse((o) => {
    const m = o as THREE.Mesh;
    if (!m.isMesh) return;
    const g = m.geometry;
    const mat = m.material as THREE.MeshStandardMaterial;
    const uv = g.getAttribute("uv");
    let finite = true;
    for (let i = 0; uv && i < uv.count * 2; i++) finite &&= Number.isFinite((uv.array as Float32Array)[i]);
    if (mat.map) {
      // Crisp squares, so a stretch or a seam shows.
      mat.map.magFilter = THREE.NearestFilter;
      mat.map.needsUpdate = true;
    }
    meshes.push({
      name: m.name,
      vertices: g.getAttribute("position").count,
      uvs: uv ? uv.count : 0,
      finite,
      map: mat.map ? [mat.map.image.width, mat.map.image.height] : null,
      wrap: mat.map ? [mat.map.wrapS === THREE.RepeatWrapping, mat.map.wrapT === THREE.RepeatWrapping] : null,
    });
  });
  const s = new THREE.Scene();
  s.add(new THREE.HemisphereLight(0xffffff, 0x666655, 1.8));
  const sun = new THREE.DirectionalLight(0xffffff, 1.8);
  sun.position.set(2, 4, 3);
  s.add(sun);
  s.add(scene);
  scene.updateMatrixWorld(true);
  const box = new THREE.Box3().setFromObject(scene);
  const c = box.getCenter(new THREE.Vector3());
  const r = box.getSize(new THREE.Vector3()).length() / 2;
  const cam = new THREE.PerspectiveCamera(30, 800 / 600, r / 100, r * 100);
  const d = r / Math.sin((14 * Math.PI) / 180);
  const el = 0.6;
  cam.position.set(c.x + d * Math.cos(el) * Math.cos(azimuth), c.y + d * Math.sin(el), c.z - d * Math.cos(el) * Math.sin(azimuth));
  cam.lookAt(c);
  renderer.render(s, cam);
  return meshes;
}

(window as any).check = { plate };
(window as any).ready = true;
