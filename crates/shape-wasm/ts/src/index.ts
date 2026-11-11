/**
 * og-shape: the private TypeScript binding over the kernel's wasm build
 * (`docs/design.md` §3), in the engine's API style: zero-value desc
 * objects with camelCase fields, errors thrown as `Error` with the
 * kernel's message, three.js names where they fit.
 *
 * Entry: {@link load}. It gives an {@link Og}: `og.model({ name })` starts
 * a {@link Shape} (one model), whose methods add, place, edit, group and
 * joint parts and read frames back. Profiles are plain objects built by
 * `og.rect`, `og.circle` and the others. A font for `og.text` is the
 * caller's: its bytes go to `og.model({ fonts })` under the name the
 * profile gives (for `shape build` to replay it, the font's path relative
 * to the program). Textures likewise: a material's `map` names an image
 * whose bytes go to `og.model({ textures })`. UVs are asked for with
 * `og.model({ uvs: true })` (every part) or `uvs: true` on a part.
 *
 * A desc field the kernel does not take is an error that names it and
 * lists the ones it takes. Every error thrown names the operation, its
 * index in the program, the part, and the call site in the author's code
 * (found from the stack only when a call fails, so it costs nothing on
 * success).
 *
 * Units: the model's (millimetres by default); angles in radians
 * (`og.deg(90)`). Every frame is in the world at rest. Frames are turned
 * in the kernel (wasm), never with JavaScript's `Math.sin`, so a program
 * gives the same bytes on every host. `shape.program()` is the recorded
 * program: `shape build/export/measure/preview` replay it natively.
 */

import { Model as WasmModel, initSync } from "../pkg/shape_wasm.js";

export type Vec3 = [number, number, number];

/** A frame as the kernel writes it. */
export interface FrameData {
  origin: Vec3;
  x: Vec3;
  y: Vec3;
  z: Vec3;
}

/** A polyline in the world. */
export interface Path {
  points: Vec3[];
  closed: boolean;
}

/** A profile: an object with one key (`{ rect: {...} }`), as `og.rect` gives. */
export type Profile = Record<string, unknown>;

/** The axis a turned solid turns about: `"+Y"` (the default) or `"+Z"`. */
export type TurnAxis = "+Y" | "+Z";

/** A piece of a path after its start: a point (a line to it), a three-point arc, an arc leaving tangent, or a spline through points. */
export type PathPiece<P> = P | { arc: { through: P; to: P } } | { tangentArc: { to: P } } | { spline: { points: P[] } };

/** A 2D path: a start point, then pieces (`og.path`, `og.stroke`). */
export type Path2 = [[number, number], ...PathPiece<[number, number]>[]];

/** A 3D path for `sweep` and `wire`: a start point and pieces, or a helix about +z (`og.helix`). */
export type Route = [Vec3, ...PathPiece<Vec3>[]] | { helix: { radius: number; pitch: number; turns: number } };

/** What `seat`, `hole`, `pocket`, `shell` or `dent` did that its result alone does not say. */
export interface EditReport {
  /** Pairs of the result's triangles that still cross after the repair of rounding errors: 0 unless the result passes through itself. */
  crossingPairs: number;
}

/** What a boolean did that its result alone does not say. */
export interface BoolReport {
  /** Closed shells facing in: a cut whose tool lies wholly inside the target leaves one. */
  voids: number;
  /** Edges where the operands touch (four faces, two of each): the result is not closed there. */
  touchingEdges: number;
  /** Whether faces of one operand lie on or meet faces of the other. */
  touching: boolean;
  /** Faces left without UVs: from the operand that had none, when the other had some. */
  facesWithoutUvs: number;
  /** Pairs of the result's triangles that still cross after the repair of rounding errors: 0 unless the result passes through itself. */
  crossingPairs: number;
}

/** A view of a `body`: one value, `[y, value]` points, or points on a spline. */
export type View = number | [number, number][] | { points: [number, number][]; smooth?: boolean };

/** A part or a group of a model. */
export class Part {
  readonly id: number;
  readonly name: string;
  constructor(id: number, name: string) {
    this.id = id;
    this.name = name;
  }
  toJSON(): number {
    return this.id;
  }
}

type Parts = Part | Part[];

function list(p: Parts): Part[] {
  return Array.isArray(p) ? p : [p];
}

/** A wasm call: the JSON answer, or the kernel's error thrown. */
type Call = (op: string, fields: Record<string, unknown>) => any;

/** This file, to skip its frames when looking for the author's call site. */
const SELF = decodeURIComponent(new URL(import.meta.url).pathname);

/**
 * The first stack frame outside this binding: the author's call. Read
 * only when a call has failed, while that call is still on the stack.
 */
function callSite(): string | undefined {
  const stack = new Error().stack ?? "";
  for (const line of stack.split("\n").slice(1)) {
    if (line.includes(SELF) || line.includes(import.meta.url)) continue;
    const m = line.match(/((?:file:\/\/)?\/[^\s()]*:\d+:\d+)/);
    if (m) return m[1].replace(/^file:\/\//, "");
  }
  return undefined;
}

/** Throws `e` with the author's call site appended to its message. */
function located(e: unknown): never {
  const message = e instanceof Error ? e.message : String(e);
  const at = callSite();
  throw new Error(at === undefined ? message : `${message} (at ${at})`, { cause: e });
}

/** Refuses a desc field the binding does not take (for descs that never reach the kernel whole). */
function only(what: string, d: object, accepted: string[]): void {
  for (const k of Object.keys(d)) {
    if (!accepted.includes(k)) {
      located(new Error(`${what} has no field ${JSON.stringify(k)}; it takes: ${accepted.join(", ")}`));
    }
  }
}

/** A place to build on: an origin and right-handed axes, in the world. */
export class Frame implements FrameData {
  readonly origin: Vec3;
  readonly x: Vec3;
  readonly y: Vec3;
  readonly z: Vec3;
  private readonly call: Call;

  /** @internal */
  constructor(d: FrameData, call: Call) {
    this.call = call;
    this.origin = d.origin;
    this.x = d.x;
    this.y = d.y;
    this.z = d.z;
  }

  private wrap(op: string, fields: Record<string, unknown>): Frame {
    return new Frame(this.call(op, { frame: this, ...fields }), this.call);
  }

  /** The same frame moved `distance` along its normal (+z). */
  offset(distance: number): Frame {
    return this.wrap("offset", { distance });
  }

  /** The same frame moved along its own axes. */
  move(x = 0, y = 0, z = 0): Frame {
    return this.wrap("translate", { x, y, z });
  }

  /** The same frame turned about its own X, then Y, then Z (radians). */
  turn(rx = 0, ry = 0, rz = 0): Frame {
    return this.wrap("turn", { rx, ry, rz });
  }

  /** A frame given in this frame's coordinates, in the world. */
  compose(local: FrameData): Frame {
    return this.wrap("compose", { local });
  }

  toJSON(): FrameData {
    return { origin: this.origin, x: this.x, y: this.y, z: this.z };
  }
}

/** Where a generator puts its part: on a frame (its own space's origin there). */
interface Placed {
  name: string;
  on?: FrameData;
  /** UVs on this part (in the model's unit on the surface), or none; the model's `uvs` by default. */
  uvs?: boolean;
}

export interface MaterialDesc {
  /** The material's name in the file; empty takes the part's. */
  name?: string;
  /** sRGB `"#rrggbb"` or `[r, g, b]` in 0..1; white by default. */
  color?: string | Vec3;
  metalness?: number;
  /** 1 by default (matte). */
  roughness?: number;
  emissive?: string | Vec3;
  /** 1 by default; below 1 the material blends. */
  opacity?: number;
  /** A base-colour image (PNG or JPEG) by the name its bytes were given to `og.model({ textures })`; the parts need UVs. */
  map?: string;
  /** Units of the parts' UVs (the model's unit) the map spans each way; 1 by default. */
  mapSize?: number;
}

export type JointKind = "revolute" | "continuous" | "prismatic" | "fixed" | "floating";

export interface JointDesc {
  name: string;
  /** The part (or group) the child moves on. */
  parent: Part;
  kind?: JointKind;
  /** The pivot, in the world at rest. */
  on: FrameData;
  /** A world direction, `"+X"`…`"-Z"` or `[x, y, z]`; `on`'s normal by default. */
  axis?: string | Vec3;
  /** Radians for turns, the model's unit for slides; revolute and prismatic need one. */
  range?: [number, number];
  /** This joint's value is `multiplier` × another's. */
  mimic?: { joint: string; multiplier?: number };
}

export interface PreviewDesc {
  /** Pixels across a view; 256 by default. */
  width?: number;
  height?: number;
  /** Views round the turntable; 4 by default. */
  views?: number;
  /** Views a row; all in one row by default. */
  columns?: number;
  /** Radians above the horizon; 25° by default. */
  elevation?: number;
  /** Radians from the front (the kernel's −Y side). */
  azimuth?: number;
  noEdges?: boolean;
  noGround?: boolean;
  /** Perspective: vertical field of view in radians, up to π/3; orthographic by default. */
  fov?: number;
  /** Colour only: no light. */
  unlit?: boolean;
}

/** One model. Every method is one kernel operation. */
export class Shape {
  private readonly m: WasmModel;
  private readonly call: Call;

  /** @internal */
  constructor(m: WasmModel, call: Call) {
    this.m = m;
    this.call = call;
  }

  private part(op: string, fields: Record<string, unknown>): Part {
    const r = this.call(op, fields);
    return new Part(r.id, String(fields.name));
  }

  private frame(op: string, fields: Record<string, unknown>): Frame {
    return new Frame(this.call(op, fields), this.call);
  }

  private frames(op: string, fields: Record<string, unknown>): Frame[] {
    return this.call(op, fields).frames.map((f: FrameData) => new Frame(f, this.call));
  }

  // ── generators: each makes a named part, in its own space or `on` a frame ──

  /** A box centred in x and y, from z = 0 up. */
  box(d: Placed & { width: number; depth: number; height: number }): Part {
    return this.part("box", d);
  }
  /** A cylinder on z = 0 about z (`radiusTop` for a frustum); `segments` round, 0 for the chord rule. */
  cylinder(d: Placed & { radius: number; height: number; radiusTop?: number; segments?: number }): Part {
    return this.part("cylinder", d);
  }
  cone(d: Placed & { radius: number; height: number; segments?: number }): Part {
    return this.part("cone", d);
  }
  /** A tube on z = 0: `outer` and `inner` radii. */
  tube(d: Placed & { outer: number; inner: number; height: number; segments?: number }): Part {
    return this.part("tube", d);
  }
  /** Centred (about +y), or resting on z = 0 with `axis: "+Z"`; `segments` round, half as many pole to pole. */
  sphere(d: Placed & { radius: number; axis?: TurnAxis; segments?: number }): Part {
    return this.part("sphere", d);
  }
  /** A spherical cap `height` high (a half sphere by default) on a flat `bottom`. */
  dome(d: Placed & { radius: number; height?: number; axis?: TurnAxis; segments?: number }): Part {
    return this.part("dome", d);
  }
  /** `angle` for a part ring (with `start` and `end` caps); `segments` round the axis, `tubeSegments` round the tube. */
  torus(
    d: Placed & {
      radius: number;
      tube: number;
      angle?: number;
      axis?: TurnAxis;
      segments?: number;
      tubeSegments?: number;
    },
  ): Part {
    return this.part("torus", d);
  }
  wedge(d: Placed & { width: number; height: number; depth: number }): Part {
    return this.part("wedge", d);
  }
  /** A profile extruded along +z: `height`, or `until` a face's plane (a tilted face cuts the top). */
  extrude(
    d: Placed & {
      profile: Profile;
      height?: number;
      until?: { part: Part; face: string };
      twist?: number;
      taper?: number;
      bothSides?: boolean;
    },
  ): Part {
    return this.part("extrude", d);
  }
  /** A profile (x the radius) turned about +y (or +z, its y the height). */
  revolve(d: Placed & { profile: Profile; angle?: number; axis?: TurnAxis; segments?: number }): Part {
    return this.part("revolve", d);
  }
  /** The profile along a path (points, arcs, splines, or a helix), with rotation-minimizing frames. */
  sweep(d: Placed & { profile: Profile; path: Route; up?: Vec3; twist?: number; scaleEnd?: number }): Part {
    return this.part("sweep", d);
  }
  loft(
    d: Placed & {
      sections: { profile: Profile; z: number }[];
      samples?: number;
      steps?: number;
      curve?: "auto" | "straight" | "catmullRom" | "spline";
    },
  ): Part {
    return this.part("loft", d);
  }
  thread(d: Placed & { radius: number; pitch: number; length: number; depth: number }): Part {
    return this.part("thread", d);
  }
  /** A round rod along a path; corners bent round `fillet` (or `fillets[i]`, points-only paths) on the centre line, else mitred. */
  wire(
    d: Placed & {
      path: Route;
      diameter: number;
      closed?: boolean;
      fillet?: number;
      fillets?: number[];
      segments?: number;
    },
  ): Part {
    return this.part("wire", d);
  }
  /** A solid along +y from its views: `plan` the width, `top` and `bottom` the z lines, `n` the squareness. */
  body(d: Placed & { plan: View; top: View; bottom: View; n?: View }): Part {
    return this.part("body", d);
  }
  /** A knurled grip about +z from z = 0: diamonds, or `straight` ridges. */
  knurl(d: Placed & { diameter: number; height: number; pitch?: number; straight?: boolean }): Part {
    return this.part("knurl", d);
  }
  /** A spur gear from z = 0 up `width`, with a round `bore` and a `hub` on top (diameters). */
  gear(
    d: Placed & {
      teeth: number;
      module: number;
      width: number;
      pressureAngle?: number;
      bore?: number;
      hub?: number;
      hubHeight?: number;
    },
  ): Part {
    return this.part("gear", d);
  }

  // ── placement ──

  /** Puts the part's own space on `frame`; what is under it moves along. */
  at(part: Part, frame: FrameData): void {
    this.call("at", { id: part, frame });
  }
  /** Moves the part by a rigid motion of the world. */
  transform(part: Part, frame: FrameData): void {
    this.call("transform", { id: part, frame });
  }
  /** The frame of a named face (or the face furthest along `"+X"`…): origin at its centroid, +z its normal. */
  face(part: Part, name: string): Frame {
    return this.frame("face", { id: part, name });
  }
  /** The named edges as a path in the world. */
  edge(part: Part, name: string): Path {
    return this.call("edge", { id: part, name });
  }
  /** A frame on the bounding box: `"+X"`, `"+X-Y"`, `"+X-Y+Z"`. */
  bbox(parts: Parts, sides: string): Frame {
    return this.frame("bbox", { ids: list(parts), sides });
  }
  /** A frame of the part's own space, in the world. */
  local(part: Part, frame: FrameData): Frame {
    return this.frame("local", { id: part, frame });
  }
  placement(part: Part): Frame {
    return this.frame("placement", { id: part });
  }
  anchor(assembly: Part, name: string): Frame {
    return this.frame("anchor", { id: assembly, name });
  }
  grid(d: { nx: number; ny?: number; px?: number; py?: number; stagger?: boolean; on?: FrameData }): Frame[] {
    return this.frames("grid", d);
  }
  ring(d: { n: number; radius: number; start?: number; angle?: number; on?: FrameData }): Frame[] {
    return this.frames("ring", d);
  }
  along(path: Path, d: { pitch?: number; count?: number }): Frame[] {
    return this.frames("along", { path, ...d });
  }
  snap(parts: Parts, d: { points: Vec3[]; direction?: Vec3 }): Frame[] {
    return this.frames("snap", { ids: list(parts), ...d });
  }
  /** The face and edge names a part carries. */
  names(part: Part): { faces: string[]; edges: string[] } {
    return this.call("names", { id: part });
  }

  // ── structure ──

  group(d: { name: string; children: Part[] }): Part {
    return this.part("group", d);
  }
  /** A new part: the convex hull of `parts` as they stand in the world (the parts stay). Faces and edges are named `hull`. */
  wrap(d: { name: string; parts: Part[] }): Part {
    return this.part("wrap", d);
  }
  assembly(d: { name: string; children: Part[]; anchors?: Record<string, FrameData> }): Part {
    return this.part("assembly", d);
  }
  attach(assembly: Part, d: { name: string; children: Part[] }): Part {
    return this.part("attach", { to: assembly, ...d });
  }
  copy(part: Part, name: string): Part {
    return this.part("copy", { id: part, name });
  }
  /** A mirror image across the plane's XY (its normal is +z). */
  mirror(part: Part, d: { plane: FrameData; name: string }): Part {
    return this.part("mirror", { id: part, ...d });
  }

  // ── edits ──

  /** `target` minus `tool`; the tool is used up. `check` refuses an operand that passes through itself first. */
  cut(target: Part, tool: Part, d: { check?: boolean } = {}): BoolReport {
    return this.call("cut", { target, tool, ...d });
  }
  join(target: Part, tool: Part, d: { check?: boolean } = {}): BoolReport {
    return this.call("join", { target, tool, ...d });
  }
  intersect(target: Part, tool: Part, d: { check?: boolean } = {}): BoolReport {
    return this.call("intersect", { target, tool, ...d });
  }
  /** Adds `tool`'s solid to `target`'s as it is, no boolean; the tool is used up. */
  merge(target: Part, tool: Part): void {
    this.call("merge", { target, tool });
  }
  /** Removes a part or group and everything under it (and the joints hanging them). */
  delete(part: Part): void {
    this.call("delete", { id: part });
  }
  /** Cuts the volume `part` fills out of `host`; `part` stays. */
  seat(host: Part, part: Part): EditReport {
    return this.call("seat", { host, part });
  }
  /** Scales a part along its own axes about its own origin (0 or missing keeps 1). */
  scale(part: Part, d: { x?: number; y?: number; z?: number }): void {
    this.call("scale", { id: part, ...d });
  }
  hole(
    part: Part,
    d: { diameter: number; at: FrameData | FrameData[]; depth?: number; counterbore?: [number, number]; countersink?: [number, number] },
  ): EditReport {
    return this.call("hole", { id: part, ...d });
  }
  pocket(part: Part, d: { profile: Profile; depth?: number; at?: FrameData | FrameData[] }): EditReport {
    return this.call("pocket", { id: part, ...d });
  }
  split(part: Part, d: { frame: FrameData; keep?: "above" | "below" }): void {
    this.call("split", { id: part, ...d });
  }
  shell(part: Part, d: { thickness: number; open?: string | string[] }): EditReport {
    return this.call("shell", { id: part, ...d });
  }
  dent(part: Part, d: { at: Vec3[]; radius: number; depth: number }): EditReport {
    return this.call("dent", { id: part, ...d });
  }
  /** Rounds a part by subdivision (Catmull and Clark): its flat faces are the cage, each level about four times the faces; edges named in `sharp` stay sharp. Ask for all levels at once. */
  subdivide(part: Part, d: { levels?: number; sharp?: string | string[] } = {}): void {
    this.call("subdivide", { id: part, ...d });
  }
  fillet(part: Part, d: { radius: number; edges?: string | string[]; segments?: number; profile?: number }): void {
    this.call("fillet", { id: part, ...d });
  }
  chamfer(
    part: Part,
    d: { distance: number; distance2?: number; angle?: number; edges?: string | string[]; segments?: number },
  ): void {
    this.call("chamfer", { id: part, ...d });
  }
  soften(part: Part, d: { radius: number; angle?: number; edges?: string | string[]; segments?: number }): void {
    this.call("soften", { id: part, ...d });
  }

  // ── materials, joints, pose ──

  material(parts: Parts, d: MaterialDesc): void {
    this.call("material", { ids: list(parts), ...d });
  }
  /** How `child` moves on `d.parent`. Several on one child compose in order. */
  joint(child: Part, d: JointDesc): void {
    this.call("joint", { child, ...d });
  }
  /** Poses a joint (radians or the unit), within its range. */
  pose(joint: string, value: number): void {
    this.call("pose", { joint, value });
  }
  /**
   * A clip: a named motion of joints over time, written to the GLB as an animation.
   * Each key is a time in seconds (rising from 0 or more) and the values of the joints it moves,
   * as `pose` takes them. A joint a key leaves out keeps its own track.
   */
  clip(d: { name: string; keys: { time: number; pose: Record<string, number> }[] }): void {
    this.call("clip", d);
  }
  /** Poses the model as clip `name` has it at `time` seconds (to preview or measure a moment of it). */
  poseAt(clip: string, time: number): void {
    this.call("pose", { clip, time });
  }

  // ── output ──

  /**
   * Measures of every part and of the whole model, at rest. With
   * `{ posed: true }`: at the pose, with `pose`, `posed`, how each joint
   * holds (`joints`) and the interference at the pose.
   */
  measure(d: { posed?: boolean } = {}): any {
    only("measure", d, ["posed"]);
    if (d.posed) return this.call("measure", { posed: true });
    return JSON.parse(this.m.measure());
  }
  /** The posed model as GLB: metres, Y up, joints as `s3d_joint` extras. */
  glb(): Uint8Array {
    return this.m.glb();
  }
  /** OBJ text: at the pose when a joint is posed (header ends `posed`), else at rest. */
  obj(): string {
    return this.m.obj();
  }
  /** Binary STL: at the pose when a joint is posed, else at rest. */
  stl(): Uint8Array {
    return this.m.stl();
  }
  /** A turntable of the posed model as one PNG, from the CPU rasterizer. */
  preview(d: PreviewDesc = {}): Uint8Array {
    only("preview", d, [
      "width", "height", "views", "columns", "elevation", "azimuth", "noEdges", "noGround", "fov", "unlit",
    ]);
    return this.m.preview(
      d.width ?? 0,
      d.height ?? 0,
      d.views ?? 0,
      d.columns ?? 0,
      d.elevation ?? -1,
      d.azimuth ?? 0,
      d.noEdges ?? false,
      d.noGround ?? false,
      d.fov ?? 0,
      d.unlit ?? false,
    );
  }
  /** The recorded program (JSON), which `shape` replays to the same bytes. */
  program(): string {
    return this.m.program();
  }
  /** Frees the model's wasm memory. */
  free(): void {
    this.m.free();
  }
}

function caller(m: WasmModel): Call {
  return (op, fields) => {
    let answer: string;
    try {
      answer = m.apply(JSON.stringify({ op, ...fields }));
    } catch (e) {
      located(e);
    }
    return JSON.parse(answer);
  };
}

/** The loaded kernel. */
export interface Og {
  /**
   * A new, empty model; `fonts` are font files' bytes under the names `og.text` gives, `textures`
   * images' bytes under the names materials' `map`s give; `uvs: true` gives every part UVs.
   */
  model(d: {
    name: string;
    units?: "mm" | "cm" | "m";
    uvs?: boolean;
    fonts?: Record<string, Uint8Array>;
    textures?: Record<string, Uint8Array>;
  }): Shape;
  /** A model replayed from a recorded program, with the fonts and textures it names. */
  replay(program: string, d?: { fonts?: Record<string, Uint8Array>; textures?: Record<string, Uint8Array> }): Shape;
  /** A frame at `origin` looking along `normal` (face axes), or with Y toward `up`. */
  frame(d: { origin?: Vec3; normal?: Vec3; up?: Vec3 }): Frame;
  /** The world frame. */
  readonly world: Frame;
  /** Degrees to radians. */
  deg(degrees: number): number;
  rect(d: { width: number; height: number; radius?: number }): Profile;
  circle(d: { radius: number; segments?: number }): Profile;
  ellipse(d: { rx: number; ry: number; segments?: number }): Profile;
  slot(d: { length: number; width: number }): Profile;
  /** `radius` to the corners, or `flats` across the flats (a nut's size). */
  polygon(d: { sides: number; radius?: number; flats?: number }): Profile;
  /** `radius` rounds a corner; `bulge` bows the edge to the next point into an arc (its height; out of the outline when positive). */
  poly(d: { points: ([number, number] | { x: number; y: number; radius?: number; bulge?: number })[] }): Profile;
  gear(d: { teeth: number; module: number; pressureAngle?: number }): Profile;
  union(...profiles: Profile[]): Profile;
  subtract(...profiles: Profile[]): Profile;
  intersect(...profiles: Profile[]): Profile;
  translate(d: { profile: Profile; x?: number; y?: number }): Profile;
  /** About the profile's origin, counter-clockwise (radians). */
  rotate(d: { profile: Profile; angle: number }): Profile;
  /** Grown outward (negative: shrunk), corners rounded. */
  offset(d: { profile: Profile; distance: number }): Profile;
  /** Factors along x and y about the origin; 0 or missing keeps 1. */
  scale(d: { profile: Profile; x?: number; y?: number }): Profile;
  /** |x/rx|ⁿ + |y/ry|ⁿ = 1: 2 an ellipse, larger squarer. */
  superellipse(d: { rx: number; ry: number; exponent: number; segments?: number }): Profile;
  /** A closed smooth curve through the points. */
  spline(d: { points: [number, number][] }): Profile;
  /** An outline from a start point through pieces, closed back to the start. */
  path(path: Path2): Profile;
  /** The area within `width / 2` of an open path (or a closed one). */
  stroke(d: { path: Path2; width: number; join?: "round" | "miter" | "bevel"; roundCaps?: boolean; closed?: boolean }): Profile;
  /** The convex hull of the profiles. */
  hull(...profiles: Profile[]): Profile;
  /** Mirrored across the y axis (x → −x), or the x axis with `axis: "x"`. */
  mirror(d: { profile: Profile; axis?: "x" | "y" }): Profile;
  /** A line of text from a font supplied to the model (`fonts`), baseline on y = 0 from x = 0. */
  text(d: { text: string; font: string; size: number; spacing?: number }): Profile;
  /** A helix about +z from (radius, 0, 0), a path for `sweep` and `wire`. */
  helix(d: { radius: number; pitch: number; turns: number }): Route;
}

/**
 * Loads the kernel. `wasm` is the module's bytes (or a compiled module);
 * by default it is read from `../pkg/shape_wasm_bg.wasm` with `node:fs`
 * (bun and node).
 */
export async function load(wasm?: BufferSource | WebAssembly.Module): Promise<Og> {
  let module = wasm;
  if (module === undefined) {
    const fs = await import("node:fs");
    module = fs.readFileSync(new URL("../pkg/shape_wasm_bg.wasm", import.meta.url));
  }
  initSync({ module });
  // Frames are pure: they are turned by a model that records nothing.
  const frames = new WasmModel("frames");
  const call = caller(frames);
  const og: Og = {
    model(d) {
      only("model", d, ["name", "units", "uvs", "fonts", "textures"]);
      const m = new WasmModel(d.name);
      for (const [name, bytes] of Object.entries(d.fonts ?? {})) m.font(name, bytes);
      for (const [name, bytes] of Object.entries(d.textures ?? {})) m.texture(name, bytes);
      const shape = new Shape(m, caller(m));
      if (d.units !== undefined) caller(m)("units", { unit: d.units });
      if (d.uvs === true) caller(m)("uvs", {});
      else if (d.uvs !== undefined && d.uvs !== false) located(new Error("model: uvs is true or false"));
      return shape;
    },
    replay(program, d = {}) {
      only("replay", d, ["fonts", "textures"]);
      const m = new WasmModel("replay");
      for (const [name, bytes] of Object.entries(d.fonts ?? {})) m.font(name, bytes);
      for (const [name, bytes] of Object.entries(d.textures ?? {})) m.texture(name, bytes);
      try {
        m.replay_into(program);
      } catch (e) {
        m.free();
        located(e);
      }
      return new Shape(m, caller(m));
    },
    frame(d) {
      only("frame", d, ["origin", "normal", "up"]);
      return new Frame(
        call("frame", { origin: d.origin ?? [0, 0, 0], normal: d.normal ?? [0, 0, 1], up: d.up }),
        call,
      );
    },
    world: new Frame({ origin: [0, 0, 0], x: [1, 0, 0], y: [0, 1, 0], z: [0, 0, 1] }, call),
    deg: (degrees) => (degrees * Math.PI) / 180,
    rect: (d) => ({ rect: d }),
    circle: (d) => ({ circle: d }),
    ellipse: (d) => ({ ellipse: d }),
    slot: (d) => ({ slot: d }),
    polygon: (d) => ({ polygon: d }),
    poly: (d) => ({ poly: d }),
    gear: (d) => ({ gear: d }),
    union: (...p) => ({ union: p }),
    subtract: (...p) => ({ subtract: p }),
    intersect: (...p) => ({ intersect: p }),
    translate: (d) => ({ translate: d }),
    rotate: (d) => ({ rotate: d }),
    offset: (d) => ({ offset: d }),
    scale: (d) => ({ scale: d }),
    superellipse: (d) => ({ superellipse: d }),
    spline: (d) => ({ spline: d }),
    path: (path) => ({ path }),
    stroke: (d) => ({ stroke: d }),
    hull: (...p) => ({ hull: p }),
    mirror: (d) => ({ mirror: d }),
    text: (d) => ({ text: d }),
    helix: (d) => ({ helix: d }),
  };
  return og;
}
