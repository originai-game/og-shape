//! The three suites of `docs/plan.md` phase 0, plus a scaling sweep.

use std::collections::BTreeMap;
use std::path::Path;

use glam::{DVec2, DVec3};
use shape_gen::{Extrude, Lathe, ShapePoints, extrude, lathe, trig};

use crate::mesh::{Tri, rotate_xyz};

/// Two operands and what is known about them.
pub struct Case {
    pub suite: &'static str,
    pub name: String,
    pub a: Tri,
    pub b: Tri,
    /// Reference volumes of A and B for the volume identities; `None`
    /// where the input has no well-defined volume (open, overlapping).
    pub vol_a: Option<f64>,
    pub vol_b: Option<f64>,
}

impl Case {
    fn new(suite: &'static str, name: impl Into<String>, a: Tri, b: Tri) -> Case {
        let (vol_a, vol_b) = (Some(a.volume()), Some(b.volume()));
        Case {
            suite,
            name: name.into(),
            a,
            b,
            vol_a,
            vol_b,
        }
    }
}

// ── builders ────────────────────────────────────────────────────────────────

fn prism(outline: Vec<DVec2>, z0: f64, height: f64) -> Tri {
    let o = Extrude {
        depth: height,
        bevel_enabled: false,
        ..Default::default()
    };
    let m = extrude(
        &[ShapePoints {
            points: outline,
            holes: Vec::new(),
        }],
        &o,
        None,
    );
    Tri::from_gen(&m).translate(DVec3::new(0.0, 0.0, z0))
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<DVec2> {
    vec![
        DVec2::new(x0, y0),
        DVec2::new(x1, y0),
        DVec2::new(x1, y1),
        DVec2::new(x0, y1),
    ]
}

fn boxed(lo: DVec3, hi: DVec3) -> Tri {
    prism(rect(lo.x, lo.y, hi.x, hi.y), lo.z, hi.z - lo.z)
}

fn circle(c: DVec2, r: f64, n: usize) -> Vec<DVec2> {
    (0..n)
        .map(|i| {
            let (s, co) = trig::sincos(i as f64 / n as f64 * trig::TAU);
            c + DVec2::new(r * co, r * s)
        })
        .collect()
}

/// A z-axis cylinder.
fn cylinder(c: DVec2, r: f64, z0: f64, height: f64, n: usize) -> Tri {
    prism(circle(c, r, n), z0, height)
}

pub fn sphere(c: DVec3, r: f64, segments: u32) -> Tri {
    let rows = (segments / 2).max(2);
    let points = (0..=rows)
        .map(|j| {
            let (s, co) = trig::sincos(j as f64 / rows as f64 * trig::PI);
            DVec2::new(if j == 0 || j == rows { 0.0 } else { r * s }, -r * co)
        })
        .collect();
    Tri::from_gen(&lathe(&Lathe {
        points,
        segments,
        ..Default::default()
    }))
    .translate(c)
}

fn slot(c: DVec2, half_length: f64, r: f64, n: usize) -> Vec<DVec2> {
    let mut p = Vec::new();
    for i in 0..=n {
        let a = -trig::PI / 2.0 + i as f64 / n as f64 * trig::PI;
        let (s, co) = trig::sincos(a);
        p.push(c + DVec2::new(half_length + r * co, r * s));
    }
    for i in 0..=n {
        let a = trig::PI / 2.0 + i as f64 / n as f64 * trig::PI;
        let (s, co) = trig::sincos(a);
        p.push(c + DVec2::new(-half_length + r * co, r * s));
    }
    p
}

fn gear(teeth: usize, r: f64) -> Vec<DVec2> {
    let mut points = Vec::new();
    for i in 0..teeth {
        for (f, k) in [(0.0, 1.0), (0.2, 1.2), (0.45, 1.2), (0.65, 1.0)] {
            let (s, c) = trig::sincos((i as f64 + f) / teeth as f64 * trig::TAU);
            points.push(DVec2::new(r * k * c, r * k * s));
        }
    }
    points
}

// ── suite 1: sand3d's booleans, rebuilt by hand ─────────────────────────────

/// The operations sand3d's edit verbs make (cut, join, intersect, split,
/// hole, pocket, seat, shell), in millimetres, including the coplanar and
/// touching cases its tools produce when they do not overshoot.
pub fn sand3d() -> Vec<Case> {
    let s = "sand3d";
    let v = DVec3::new;
    let d2 = DVec2::new;
    let plate = boxed(v(-40.0, -20.0, 0.0), v(40.0, 20.0, 6.0));
    let mut cases = vec![
        Case::new(
            s,
            "hole.through",
            plate.clone(),
            cylinder(d2(0.0, 0.0), 3.3, -1.0, 8.0, 32),
        ),
        Case::new(
            s,
            "hole.blind",
            plate.clone(),
            cylinder(d2(10.0, 0.0), 3.3, 2.0, 5.0, 32),
        ),
        Case::new(
            s,
            "hole.counterbore.flush",
            plate.clone(),
            cylinder(d2(-10.0, 0.0), 5.5, 3.0, 3.0, 32),
        ),
        Case::new(
            s,
            "pocket.flush",
            boxed(v(0.0, 0.0, 0.0), v(40.0, 40.0, 20.0)),
            boxed(v(10.0, 10.0, 10.0), v(30.0, 30.0, 20.0)),
        ),
        Case::new(
            s,
            "pocket.overshoot",
            boxed(v(0.0, 0.0, 0.0), v(40.0, 40.0, 20.0)),
            boxed(v(10.0, 10.0, 10.0), v(30.0, 30.0, 21.0)),
        ),
        Case::new(
            s,
            "seat.boss.touching",
            plate.clone(),
            cylinder(d2(20.0, 0.0), 6.0, 6.0, 10.0, 32),
        ),
        Case::new(
            s,
            "join.shared.face",
            boxed(v(0.0, 0.0, 0.0), v(10.0, 10.0, 10.0)),
            boxed(v(10.0, 0.0, 0.0), v(20.0, 10.0, 10.0)),
        ),
        Case::new(
            s,
            "join.partial.face",
            boxed(v(0.0, 0.0, 0.0), v(10.0, 10.0, 10.0)),
            boxed(v(10.0, 3.0, 2.0), v(18.0, 8.0, 7.0)),
        ),
        Case::new(
            s,
            "join.shared.edge",
            boxed(v(0.0, 0.0, 0.0), v(10.0, 10.0, 10.0)),
            boxed(v(10.0, 10.0, 0.0), v(20.0, 20.0, 10.0)),
        ),
        Case::new(
            s,
            "washer.coplanar.caps",
            cylinder(d2(0.0, 0.0), 10.0, 0.0, 3.0, 48),
            cylinder(d2(0.0, 0.0), 5.0, 0.0, 3.0, 48),
        ),
        Case::new(
            s,
            "shell.inner.box",
            boxed(v(0.0, 0.0, 0.0), v(30.0, 20.0, 15.0)),
            boxed(v(2.0, 2.0, 2.0), v(28.0, 18.0, 13.0)),
        ),
        Case::new(
            s,
            "split.half",
            boxed(v(0.0, 0.0, 0.0), v(30.0, 20.0, 15.0)),
            boxed(v(-1.0, -1.0, -1.0), v(15.0, 21.0, 16.0)),
        ),
        Case::new(
            s,
            "slot.through",
            plate.clone(),
            prism(slot(d2(0.0, 10.0), 8.0, 2.5, 12), -1.0, 8.0),
        ),
        Case::new(
            s,
            "sphere.box",
            sphere(v(0.0, 0.0, 0.0), 10.0, 48),
            boxed(v(-5.0, -5.0, -20.0), v(20.0, 20.0, 20.0)),
        ),
        Case::new(
            s,
            "gear.bore",
            prism(gear(18, 20.0), 0.0, 8.0),
            cylinder(d2(0.0, 0.0), 5.0, -1.0, 10.0, 32),
        ),
    ];
    // Two tubes of one radius crossing at right angles: their surfaces
    // touch along tangent curves.
    let tube_x = cylinder(d2(0.0, 0.0), 4.0, -15.0, 30.0, 32).map(|p| DVec3::new(p.z, p.y, p.x));
    let tube_y = cylinder(d2(0.0, 0.0), 4.0, -15.0, 30.0, 32).map(|p| DVec3::new(p.x, p.z, p.y));
    cases.push(Case::new(s, "tubes.cross.same.radius", tube_x, tube_y));
    // A rotated tool whose vertices land near, not on, the target's faces.
    let tilted = boxed(v(-5.0, -5.0, -5.0), v(5.0, 5.0, 5.0))
        .map(|p| rotate_xyz(p, [0.0, 0.0, 45.0]) + DVec3::new(40.0, 0.0, 6.0));
    cases.push(Case::new(s, "chamfer.cut.rotated.box", plate, tilted));
    cases
}

// ── suite 2: Thingi10K's closed meshes ──────────────────────────────────────

fn read_npy(path: &Path) -> Result<(Vec<u8>, String, Vec<usize>), String> {
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if data.len() < 10 || &data[..6] != b"\x93NUMPY" {
        return Err(format!("{}: not .npy", path.display()));
    }
    let (hlen, start) = if data[6] == 1 {
        (u16::from_le_bytes([data[8], data[9]]) as usize, 10)
    } else {
        (
            u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize,
            12,
        )
    };
    let header = String::from_utf8_lossy(&data[start..start + hlen]).to_string();
    let descr = header
        .split("'descr': '")
        .nth(1)
        .and_then(|s| s.split('\'').next())
        .unwrap_or("")
        .to_string();
    let shape: Vec<usize> = header
        .split("'shape': (")
        .nth(1)
        .and_then(|s| s.split(')').next())
        .unwrap_or("")
        .split(',')
        .filter_map(|x| x.trim().parse().ok())
        .collect();
    if header.contains("'fortran_order': True") {
        return Err("fortran order".into());
    }
    Ok((data[start + hlen..].to_vec(), descr, shape))
}

fn load_thingi(dir: &Path) -> Result<Tri, String> {
    let (vb, vd, vs) = read_npy(&dir.join("vertices.npy"))?;
    let (fb, fd, _) = read_npy(&dir.join("facets.npy"))?;
    let verts: Vec<f64> = match vd.as_str() {
        "<f8" => vb
            .chunks_exact(8)
            .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
            .collect(),
        "<f4" => vb
            .chunks_exact(4)
            .map(|c| f64::from(f32::from_le_bytes(c.try_into().unwrap())))
            .collect(),
        d => return Err(format!("vertex dtype {d}")),
    };
    let idx: Vec<u32> = match fd.as_str() {
        "<i4" => fb
            .chunks_exact(4)
            .map(|c| i32::from_le_bytes(c.try_into().unwrap()) as u32)
            .collect(),
        "<i8" => fb
            .chunks_exact(8)
            .map(|c| i64::from_le_bytes(c.try_into().unwrap()) as u32)
            .collect(),
        d => return Err(format!("face dtype {d}")),
    };
    if vs.get(1) != Some(&3) {
        return Err("vertices are not (n, 3)".into());
    }
    // Centred and scaled to a side of 2, as manifold-rust's own sweep does.
    let pts: Vec<DVec3> = verts
        .chunks_exact(3)
        .map(|c| DVec3::new(c[0], c[1], c[2]))
        .collect();
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for p in &pts {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let c = (lo + hi) / 2.0;
    let side = (hi - lo).max_element();
    let s = if side > 0.0 { 2.0 / side } else { 1.0 };
    let pos = pts
        .iter()
        .map(|p| ((*p - c) * s).as_vec3().to_array())
        .collect();
    Ok(Tri { pos, idx })
}

/// A = the mesh, B = the same mesh rotated (30, 45, 60)° and moved 0.3 in x
/// (manifold-rust's own Thingi10K sweep pairing).
pub fn thingi(root: &Path) -> Vec<Case> {
    let mut ids: Vec<_> = std::fs::read_dir(root)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    let mut cases = Vec::new();
    for dir in ids {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        match load_thingi(&dir) {
            Ok(a) => {
                let b = a.map(|p| rotate_xyz(p, [30.0, 45.0, 60.0]) + DVec3::new(0.3, 0.0, 0.0));
                cases.push(Case::new("thingi10k", name, a, b));
            }
            Err(e) => eprintln!("thingi10k {name}: skipped: {e}"),
        }
    }
    cases
}

// ── suite 3: synthetic AI-mesh defects ──────────────────────────────────────

/// SplitMix64, so every defect is the same on every host.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

type Sdf = fn(DVec3) -> f64;

fn sd_box(p: DVec3, h: DVec3) -> f64 {
    let q = p.abs() - h;
    q.max(DVec3::ZERO).length() + q.max_element().min(0.0)
}

fn sd_torus(p: DVec3, r: f64, t: f64) -> f64 {
    let q = DVec2::new(DVec2::new(p.x, p.z).length() - r, p.y);
    q.length() - t
}

fn smooth_min(a: f64, b: f64, k: f64) -> f64 {
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b * (1.0 - h) + a * h - k * h * (1.0 - h)
}

/// Shapes like the ones providers return: rounded, blobby, a handle.
fn shapes() -> Vec<(&'static str, Sdf)> {
    vec![
        ("sphere", |p| p.length() - 0.9),
        ("rounded.box", |p| {
            sd_box(p, DVec3::new(0.7, 0.5, 0.6)) - 0.15
        }),
        ("torus", |p| sd_torus(p, 0.6, 0.25)),
        ("blob", |p| {
            smooth_min(
                p.length() - 0.6,
                sd_box(p - DVec3::new(0.4, 0.3, 0.0), DVec3::splat(0.35)),
                0.25,
            )
        }),
        ("mug", |p| {
            let body = sd_box(
                DVec3::new(DVec2::new(p.x, p.z).length() - 0.45, p.y, 0.0),
                DVec3::new(0.08, 0.7, 1.0),
            );
            let base = sd_box(p + DVec3::new(0.0, 0.62, 0.0), DVec3::new(0.5, 0.08, 0.5))
                .max(DVec2::new(p.x, p.z).length() - 0.53);
            let handle = sd_torus(DVec3::new(p.x - 0.55, p.z, p.y), 0.28, 0.07);
            body.min(base).min(handle)
        }),
    ]
}

/// Marching tetrahedra over an `n`³ grid on [-1.2, 1.2]³: six tetrahedra
/// per cube along its main diagonal, so neighbours agree and the surface is
/// closed. Vertices are shared through the grid edge they lie on.
fn polygonise(f: Sdf, n: usize, clamp: bool) -> Tri {
    let lo = -1.2;
    let h = 2.4 / n as f64;
    let at = |i: usize, j: usize, k: usize| {
        DVec3::new(lo + i as f64 * h, lo + j as f64 * h, lo + k as f64 * h)
    };
    let id = |i: usize, j: usize, k: usize| ((i * (n + 1) + j) * (n + 1) + k) as u64;
    let mut values = vec![0.0; (n + 1) * (n + 1) * (n + 1)];
    for i in 0..=n {
        for j in 0..=n {
            for k in 0..=n {
                let v = f(at(i, j, k));
                values[id(i, j, k) as usize] = if v == 0.0 { 1e-12 } else { v };
            }
        }
    }
    let mut out = Tri::default();
    let mut on_edge: BTreeMap<(u64, u64), u32> = BTreeMap::new();
    let mut vertex = |out: &mut Tri, a: (u64, DVec3), b: (u64, DVec3)| -> u32 {
        let key = (a.0.min(b.0), a.0.max(b.0));
        *on_edge.entry(key).or_insert_with(|| {
            let (va, vb) = (values[a.0 as usize], values[b.0 as usize]);
            let t = va / (va - vb);
            // Clamped, no two vertices share a position; unclamped, a vertex
            // next to a grid corner rounds onto it, as marching cubes does.
            let t = if clamp { t.clamp(0.01, 0.99) } else { t };
            out.pos.push((a.1 + (b.1 - a.1) * t).as_vec3().to_array());
            out.pos.len() as u32 - 1
        })
    };
    // The six tetrahedra of a cube, as corner numbers (bit 0 = x, 1 = y, 2 = z).
    const TETS: [[usize; 4]; 6] = [
        [0, 1, 3, 7],
        [0, 3, 2, 7],
        [0, 2, 6, 7],
        [0, 6, 4, 7],
        [0, 4, 5, 7],
        [0, 5, 1, 7],
    ];
    for i in 0..n {
        for j in 0..n {
            for k in 0..n {
                let corner = |c: usize| {
                    let (x, y, z) = (i + (c & 1), j + ((c >> 1) & 1), k + ((c >> 2) & 1));
                    (id(x, y, z), at(x, y, z))
                };
                for tet in TETS {
                    let cs = tet.map(corner);
                    let inside: Vec<usize> =
                        (0..4).filter(|&q| values[cs[q].0 as usize] < 0.0).collect();
                    let outside: Vec<usize> = (0..4)
                        .filter(|&q| values[cs[q].0 as usize] >= 0.0)
                        .collect();
                    let polygon: Vec<u32> = match inside.len() {
                        1 => outside
                            .iter()
                            .map(|&o| vertex(&mut out, cs[inside[0]], cs[o]))
                            .collect(),
                        3 => inside
                            .iter()
                            .map(|&q| vertex(&mut out, cs[q], cs[outside[0]]))
                            .collect(),
                        2 => {
                            let (a, b, c, d) = (inside[0], inside[1], outside[0], outside[1]);
                            vec![
                                vertex(&mut out, cs[a], cs[c]),
                                vertex(&mut out, cs[a], cs[d]),
                                vertex(&mut out, cs[b], cs[d]),
                                vertex(&mut out, cs[b], cs[c]),
                            ]
                        }
                        _ => continue,
                    };
                    // Face out: from the inside corners towards the outside ones.
                    let mean =
                        |qs: &[usize]| qs.iter().map(|&q| cs[q].1).sum::<DVec3>() / qs.len() as f64;
                    let outward = mean(&outside) - mean(&inside);
                    let p = |v: u32| DVec3::from(out.pos[v as usize].map(f64::from));
                    for t in 1..polygon.len() - 1 {
                        let (x, y, z) = (polygon[0], polygon[t], polygon[t + 1]);
                        let normal = (p(y) - p(x)).cross(p(z) - p(x));
                        if normal.dot(outward) >= 0.0 {
                            out.idx.extend([x, y, z]);
                        } else {
                            out.idx.extend([x, z, y]);
                        }
                    }
                }
            }
        }
    }
    out
}

fn noisy(t: &Tri, amount: f64, rng: &mut Rng) -> Tri {
    let mut t = t.clone();
    for p in &mut t.pos {
        for x in p.iter_mut() {
            *x += ((rng.unit() - 0.5) * 2.0 * amount) as f32;
        }
    }
    t
}

/// A z-axis cylinder through the middle, tilted a little: the tool every
/// synthetic case is cut with.
fn tool() -> Tri {
    cylinder(DVec2::ZERO, 0.3, -1.6, 3.2, 32)
        .map(|p| rotate_xyz(p, [12.0, 7.0, 0.0]) + DVec3::new(0.05, 0.02, 0.0))
}

/// Each shape, polygonised, then given one defect at a time.
pub fn synthetic(grid: usize) -> Vec<Case> {
    let s = "synthetic";
    let mut cases = Vec::new();
    let b = tool();
    for (name, f) in shapes() {
        let clean = polygonise(f, grid, true);
        let vol = clean.volume();
        let mut rng = Rng(name.len() as u64 * 7919);
        let cell = 2.4 / grid as f64;
        let mut push = |defect: &str, a: Tri, vol_a: Option<f64>| {
            let vol_b = Some(b.volume());
            cases.push(Case {
                suite: s,
                name: format!("{name}.{defect}"),
                a,
                b: b.clone(),
                vol_a,
                vol_b,
            });
        };
        push("clean", clean.clone(), Some(vol));
        let raw = polygonise(f, grid, false);
        let vr = raw.volume();
        push("degenerate", raw, Some(vr));
        let n = noisy(&clean, cell * 0.3, &mut rng);
        let vn = n.volume();
        push("noise", n, Some(vn));
        // Floaters: small closed specks outside the body.
        let mut f = clean.clone();
        let mut fv = vol;
        for k in 0..3 {
            let speck = sphere(DVec3::new(1.05, -1.0 + k as f64 * 0.3, 1.05), 0.04, 8);
            fv += speck.volume();
            f.append(&speck);
        }
        push("floaters", f, Some(fv));
        // Holes: 1% of the triangles removed.
        let mut h = clean.clone();
        let tris: Vec<[u32; 3]> = h.idx.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
        h.idx = tris
            .iter()
            .filter(|_| rng.unit() >= 0.01)
            .flatten()
            .copied()
            .collect();
        push("holes", h, None);
        // Flipped: 2% of the triangles wound backwards.
        let mut fl = clean.clone();
        for t in fl.idx.chunks_exact_mut(3) {
            if rng.unit() < 0.02 {
                t.swap(1, 2);
            }
        }
        push("flipped", fl, None);
        // Triangle soup: every triangle its own three vertices.
        let mut soup = Tri::default();
        for t in clean.idx.chunks_exact(3) {
            for &i in t {
                soup.pos.push(clean.pos[i as usize]);
                soup.idx.push(soup.pos.len() as u32 - 1);
            }
        }
        push("soup", soup, Some(vol));
        // Self-intersecting: two overlapping copies in one mesh, as a
        // provider returns a model made of interpenetrating parts.
        let mut si = clean.clone();
        si.append(&clean.map(|p| rotate_xyz(p, [0.0, 25.0, 10.0]) + DVec3::new(0.2, 0.1, 0.0)));
        push("self.intersecting", si, None);
    }
    cases
}

// ── scaling: two dense spheres ─────────────────────────────────────────────

pub fn scaling() -> Vec<Case> {
    [16u32, 32, 64, 128, 256]
        .iter()
        .map(|&s| {
            let a = sphere(DVec3::ZERO, 1.0, s);
            let b = sphere(DVec3::new(0.5, 0.21, 0.13), 0.8, s)
                .map(|p| rotate_xyz(p, [17.0, 0.0, 9.0]));
            let name = format!("spheres.{}x{}", a.tris(), b.tris());
            Case::new("scaling", name, a, b)
        })
        .collect()
}
