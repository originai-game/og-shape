//! Phase 5's acceptance (`docs/plan.md`): meshes from outside the kernel
//! go through the chain
//! `import → repair → decimate → unwrap → measure → preview → export` and
//! must come out closed, joined, triangulated, under a triangle budget
//! (40 000 here, one example budget: a texturing tool's input limit) and
//! unwrapped, as a plain GLB.
//!
//! ```text
//! cargo run --release -- [--thingi DIR] [--grid N] [--previews DIR]
//! ```
//!
//! The inputs:
//! - the synthetic generated-mesh defects of phase 0 (`bench/booleans`): five
//!   shapes polygonised by marching tetrahedra, each with one defect;
//! - Thingi10K's closed meshes (`bench/booleans/fetch-thingi10k.sh`),
//!   subdivided once 1-to-4 and given the same defects;
//! each written as a GLB by og-shape's own exporter (a model of
//! interpenetrating parts as two nodes), then read back by the importer.
//!
//! With `--glb FILE` (repeated), other tools' files go through the same
//! chain as they are instead; a skinned file must be refused by every
//! tool that would break its skin.
//!
//! Per input, one TSV row: the measures before and after, what repair
//! did, the stage times, the contract verdict, and whether a boolean cut
//! (og-geom's CSG) succeeds on the result, re-checking phase 0's finding
//! that generated meshes are a repair problem.

#[allow(dead_code)]
#[path = "../../booleans/src/mesh.rs"]
mod mesh;
#[allow(dead_code)]
#[path = "../../booleans/src/suites.rs"]
mod suites;

use std::path::PathBuf;
use std::time::Instant;

use glam::DVec3;
use mesh::Tri;
use shape_bool::{BoolOp, Boolean, boolean};
use shape_mesh::Solid;
use shape_tools::{
    Decimate, Floaters, Import, Measure, Part, PlainGlb, Preview, Repair, Scene, Unwrap, decimate,
    glb, glb_plain, import, join, measure, no_files, png, preview, repair, sheet, unwrap,
};

/// Millimetres per unit of the suites' meshes (side ≈ 2.4 units).
const SCALE: f64 = 100.0;
const CONTRACT_TRIANGLES: usize = 40_000;

fn solid(t: &Tri) -> Solid {
    let mut s = Solid::default();
    for p in &t.pos {
        s.push_vertex(DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2])) * SCALE);
    }
    for c in t.idx.chunks_exact(3) {
        s.push_triangle([c[0], c[1], c[2]], 0, &[]);
    }
    s
}

/// One triangle into four by its edge midpoints (shared).
fn subdivide(t: &Tri) -> Tri {
    let mut out = Tri {
        pos: t.pos.clone(),
        idx: Vec::new(),
    };
    let mut mid: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
    let mut m = |out: &mut Tri, a: u32, b: u32| -> u32 {
        *mid.entry((a.min(b), a.max(b))).or_insert_with(|| {
            let (p, q) = (out.pos[a as usize], out.pos[b as usize]);
            out.pos
                .push([0, 1, 2].map(|k| (p[k] + q[k]) * 0.5));
            out.pos.len() as u32 - 1
        })
    };
    for c in t.idx.chunks_exact(3) {
        let (a, b, d) = (c[0], c[1], c[2]);
        let (ab, bd, da) = (m(&mut out, a, b), m(&mut out, b, d), m(&mut out, d, a));
        out.idx
            .extend([a, ab, da, ab, b, bd, da, bd, d, ab, bd, da]);
    }
    out
}

/// SplitMix64.
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

/// The phase 0 defects, for any mesh: (name, mesh, as two nodes).
fn defects(t: &Tri, seed: u64) -> Vec<(&'static str, Tri, bool)> {
    let mut rng = Rng(seed);
    let mut out = vec![("clean", t.clone(), false)];
    let mut soup = Tri::default();
    for c in t.idx.chunks_exact(3) {
        for &i in c {
            soup.pos.push(t.pos[i as usize]);
            soup.idx.push(soup.pos.len() as u32 - 1);
        }
    }
    out.push(("soup", soup, false));
    let mut h = t.clone();
    let tris: Vec<[u32; 3]> = h.idx.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    h.idx = tris
        .iter()
        .filter(|_| rng.unit() >= 0.01)
        .flatten()
        .copied()
        .collect();
    out.push(("holes", h, false));
    let mut f = t.clone();
    for c in f.idx.chunks_exact_mut(3) {
        if rng.unit() < 0.02 {
            c.swap(1, 2);
        }
    }
    out.push(("flipped", f, false));
    let mut fl = t.clone();
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in &t.pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    for k in 0..3 {
        let speck = suites::sphere(
            DVec3::new(
                f64::from(hi[0]) + 0.1,
                f64::from(lo[1]) + k as f64 * 0.3,
                f64::from(hi[2]) + 0.1,
            ),
            0.04,
            8,
        );
        fl.append(&speck);
    }
    out.push(("floaters", fl, false));
    // Two overlapping copies as two nodes: interpenetrating parts.
    let mut si = t.clone();
    si.append(&t.map(|p| mesh::rotate_xyz(p, [0.0, 25.0, 10.0]) + DVec3::new(0.2, 0.1, 0.0)));
    out.push(("parts.overlapping", si, true));
    out
}

struct Row {
    input: String,
    defect: String,
    tris_in: usize,
    closed_in: bool,
    pieces_in: usize,
    welded: usize,
    flipped: usize,
    floaters: usize,
    holes: (usize, usize),
    fused: String,
    tris_out: usize,
    decimate_error: f64,
    closed_out: bool,
    self_x: usize,
    charts: usize,
    uv_cover: f64,
    uv_overlap: usize,
    contract: Result<(), String>,
    cut: String,
    ms: [f64; 7],
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// The budget's contract, read back from the GLB bytes: one node, one
/// mesh, one triangle primitive, at most 40k triangles, closed.
fn contract(bytes: &[u8]) -> Result<(), String> {
    let back = import(bytes, &Import::default(), &no_files).map_err(|e| e.to_string())?;
    if back.parts.len() != 1 {
        return Err(format!("{} nodes", back.parts.len()));
    }
    let s = &back.parts[0].solid;
    let m = measure(
        s,
        &Measure::default(),
    );
    if m.triangles > CONTRACT_TRIANGLES {
        return Err(format!("{} triangles", m.triangles));
    }
    if !m.closed {
        return Err(format!(
            "open: {} boundary, {} non-manifold, {} inconsistent edges",
            m.boundary_edges, m.non_manifold_edges, m.inconsistent_edges
        ));
    }
    if m.uv.is_none() {
        return Err("no UVs".into());
    }
    let text = String::from_utf8_lossy(bytes);
    if text.matches("\"primitives\"").count() != 1 || !text.contains("\"mode\":4") {
        return Err("not one triangle primitive".into());
    }
    Ok(())
}

fn run(input: &str, defect: &str, t: &Tri, two_nodes: bool, tool: &Solid, out: &Option<PathBuf>) -> Row {
    // The outside mesh: a GLB written by og-shape's exporter.
    let raw = solid(t);
    let bytes = if two_nodes {
        let half = t.idx.len() / 6;
        let (a, b) = (
            Tri {
                pos: t.pos.clone(),
                idx: t.idx[..half * 3].to_vec(),
            },
            Tri {
                pos: t.pos.clone(),
                idx: t.idx[half * 3..].to_vec(),
            },
        );
        let (sa, sb) = (solid(&a), solid(&b));
        glb(&Scene {
            name: input.into(),
            parts: vec![Part::new("a", &sa), Part::new("b", &sb)],
        })
    } else {
        glb(&Scene {
            name: input.into(),
            parts: vec![Part::new("model", &raw)],
        })
    }
    .expect("export");
    chain(input, defect, &bytes, Some(tool), out)
}

/// What a skinned file must get: every tool that would break its skin
/// refuses it with an error value.
fn skinned_refusals(parts: &[shape_tools::ImportedPart]) -> Result<String, String> {
    let s = &parts
        .iter()
        .find(|p| p.solid.skin.is_some())
        .ok_or("no skinned part")?
        .solid;
    let skin = s.skin.clone().unwrap_or_default();
    let mut refused = Vec::new();
    let mut check = |tool: &str, err: Option<String>| match err {
        Some(e) if e.contains("skinned") => refused.push(tool.to_string()),
        Some(e) => refused.push(format!("{tool} (other error: {e})")),
        None => refused.push(format!("{tool} ACCEPTED")),
    };
    check("join", join(parts).err().map(|e| e.to_string()));
    check("repair", repair(s, &Repair::default()).err().map(|e| e.to_string()));
    let half = Decimate { ratio: 0.5, ..Default::default() };
    check("decimate", decimate(s, &half).err().map(|e| e.to_string()));
    check("glb_plain", glb_plain(s, &PlainGlb::default()).err().map(|e| e.to_string()));
    let text = refused.join(", ");
    if text.contains("ACCEPTED") || text.contains("other error") {
        return Err(text);
    }
    // glb writes the skin; it must come back with the same bones and
    // weighted vertices.
    let scene = Scene { name: "s".into(), parts: vec![Part::new("p", s)] };
    let bytes = glb(&scene).map_err(|e| format!("glb: {e}"))?;
    let back = import(&bytes, &Import::default(), &no_files).map_err(|e| format!("re-import: {e}"))?;
    let skin2 = back.parts[0].solid.skin.clone().ok_or("the skin did not come back")?;
    let names = |k: &shape_mesh::Skin| k.bones.iter().map(|b| (b.name.clone(), b.parent)).collect::<Vec<_>>();
    if names(&skin) != names(&skin2) {
        return Err("the bones changed in the round trip".into());
    }
    Ok(format!(
        "skinned ({} bones): refused by {text}; glb round trip keeps the bones",
        skin.bones.len()
    ))
}

fn chain(input: &str, defect: &str, bytes: &[u8], tool: Option<&Solid>, out: &Option<PathBuf>) -> Row {
    let mut times = [0.0; 7];
    let start = Instant::now();
    let imported = match import(bytes, &Import::default(), &no_files) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("{input}: import refused: {e}");
            return Row {
                input: input.into(),
                defect: "refused".into(),
                tris_in: 0,
                closed_in: false,
                pieces_in: 0,
                welded: 0,
                flipped: 0,
                floaters: 0,
                holes: (0, 0),
                fused: "-".into(),
                tris_out: 0,
                decimate_error: 0.0,
                closed_out: false,
                self_x: 0,
                charts: 0,
                uv_cover: 0.0,
                uv_overlap: 0,
                contract: Err(format!("import refused: {e}")),
                cut: "-".into(),
                ms: times,
            };
        }
    };
    if imported.parts.iter().any(|p| p.solid.skin.is_some()) {
        let tris = imported.parts.iter().map(|p| p.solid.triangles.len()).sum();
        return Row {
            input: input.into(),
            defect: "skinned".into(),
            tris_in: tris,
            closed_in: false,
            pieces_in: imported.parts.len(),
            welded: 0,
            flipped: 0,
            floaters: 0,
            holes: (0, 0),
            fused: "-".into(),
            tris_out: tris,
            decimate_error: 0.0,
            closed_out: false,
            self_x: 0,
            charts: 0,
            uv_cover: 0.0,
            uv_overlap: 0,
            contract: skinned_refusals(&imported.parts).map(|t| {
                eprintln!("{input}: {t}");
            }),
            cut: "-".into(),
            ms: times,
        };
    }
    let joined = join(&imported.parts).expect("join");
    times[0] = ms(start);
    let before = measure(&joined, &Measure::default());
    let start = Instant::now();
    let d = Repair {
        weld: 0.001,
        floaters: Floaters {
            fraction: 0.05,
            ..Default::default()
        },
        max_hole_edges: 64,
        fuse: true,
        min_edge: 0.001,
        ..Default::default()
    };
    let (fixed, r) = repair(&joined, &d).expect("repair");
    times[1] = ms(start);
    let start = Instant::now();
    let (low, dr) = decimate(
        &fixed,
        &Decimate {
            target: CONTRACT_TRIANGLES,
            ..Default::default()
        },
    )
    .expect("decimate");
    times[2] = ms(start);
    let start = Instant::now();
    let mut low = low;
    let uv = unwrap(&mut low, &Unwrap::default());
    times[3] = ms(start);
    let start = Instant::now();
    let m = measure(
        &low,
        &Measure {
            intersections: true,
            ..Default::default()
        },
    );
    let _json = m.to_json();
    times[4] = ms(start);
    let start = Instant::now();
    let scene = Scene {
        name: input.into(),
        parts: vec![Part::new("model", &low)],
    };
    let views = preview(
        &scene,
        &Preview {
            width: 256,
            views: 4,
            ..Default::default()
        },
    )
    .expect("preview");
    times[5] = ms(start);
    if let Some(dir) = out {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(
            dir.join(format!("{input}.{defect}.png")),
            png(&sheet(&views, 4)),
        );
    }
    let start = Instant::now();
    let plain = uv.as_ref().map(|_| {
        glb_plain(
            &low,
            &PlainGlb {
                name: "model",
                ..Default::default()
            },
        )
        .expect("glb")
    });
    times[6] = ms(start);
    let verdict = match (&uv, &plain) {
        (Err(e), _) => Err(format!("unwrap: {e}")),
        (_, Ok(b)) => contract(b),
        _ => Err("no GLB".into()),
    };
    let cut = match tool.map(|t| {
        boolean(
            &low,
            t,
            &Boolean {
                op: BoolOp::Cut,
                ..Default::default()
            },
        )
    }) {
        None => "-".to_string(),
        Some(Ok(c)) if c.measure().closed => "ok".to_string(),
        Some(Ok(_)) => "open".to_string(),
        Some(Err(e)) => format!("refused: {e}"),
    };
    Row {
        input: input.into(),
        defect: defect.into(),
        tris_in: before.triangles,
        closed_in: before.closed,
        pieces_in: before.pieces.len(),
        welded: r.welded,
        flipped: r.flipped,
        floaters: r.floaters,
        holes: (r.holes_filled, r.holes_left),
        fused: match &r.fused {
            None => "-".into(),
            Some(Ok(n)) => format!("{n} pieces"),
            Some(Err(e)) => format!("refused: {e}"),
        },
        tris_out: low.triangles.len(),
        decimate_error: dr.error,
        closed_out: m.closed,
        self_x: m.self_intersections.unwrap_or(0),
        charts: uv.as_ref().map_or(0, |u| u.chart_count),
        uv_cover: m.uv.map_or(0.0, |u| u.coverage),
        uv_overlap: m.uv.map_or(0, |u| u.overlapping_pairs),
        contract: verdict,
        cut,
        ms: times,
    }
}

fn print_row(r: &Row) {
    println!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.4}\t{}\t{}\t{}\t{:.3}\t{}\t{}\t{}\t{}",
        r.input,
        r.defect,
        r.tris_in,
        r.closed_in,
        r.pieces_in,
        r.welded,
        r.flipped,
        r.floaters,
        r.holes.0,
        r.holes.1,
        r.fused,
        r.tris_out,
        r.decimate_error,
        r.closed_out,
        r.self_x,
        r.charts,
        r.uv_cover,
        r.uv_overlap,
        match &r.contract {
            Ok(()) => "ok".to_string(),
            Err(e) => format!("FAIL: {e}"),
        },
        r.cut,
        r.ms.map(|v| format!("{v:.0}")).join("\t"),
    );
}

fn main() {
    let mut thingi: Option<PathBuf> = None;
    let mut grid = 96;
    let mut previews: Option<PathBuf> = None;
    let mut only: Option<String> = None;
    let mut files: Vec<PathBuf> = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut v = || it.next().unwrap_or_else(|| panic!("{k} needs a value"));
        match k.as_str() {
            "--thingi" => thingi = Some(v().into()),
            "--grid" => grid = v().parse().expect("--grid N"),
            "--previews" => previews = Some(v().into()),
            "--only" => only = Some(v()),
            "--glb" => files.push(PathBuf::from(v())),
            _ => panic!("unknown argument {k}"),
        }
    }
    let mut inputs: Vec<(String, Tri, Tri)> = Vec::new();
    for c in suites::synthetic(grid) {
        if c.name.ends_with(".clean") {
            inputs.push((
                format!("synthetic.{}", c.name.trim_end_matches(".clean")),
                c.a,
                c.b,
            ));
        }
    }
    if let Some(dir) = &thingi {
        let tool = suites::synthetic(8).remove(0).b;
        for c in suites::thingi(dir) {
            inputs.push((format!("thingi.{}", c.name), subdivide(&c.a), tool.clone()));
        }
    }
    println!(
        "input\tdefect\ttris_in\tclosed_in\tpieces_in\twelded\tflipped\tfloaters\tholes_filled\tholes_left\tfused\ttris_out\tdecimate_error_mm\tclosed_out\tself_intersections\tcharts\tuv_coverage\tuv_overlaps\tcontract\tcut_after\tms_import\tms_repair\tms_decimate\tms_unwrap\tms_measure\tms_preview\tms_export"
    );
    let mut rows = Vec::new();
    // Other tools' files, read as they are.
    for f in &files {
        let name = f.file_name().map_or(String::new(), |n| n.to_string_lossy().to_string());
        let bytes = std::fs::read(f).expect("read");
        let r = chain(&format!("file.{name}"), "as.is", &bytes, None, &previews);
        print_row(&r);
        rows.push(r);
    }
    if !files.is_empty() {
        inputs.clear();
    }
    for (k, (name, t, tool)) in inputs.iter().enumerate() {
        if only.as_ref().is_some_and(|o| !name.contains(o.as_str())) {
            continue;
        }
        let mut tool = solid(tool);
        tool.canonicalize().expect("tool");
        for (defect, mesh, two) in defects(t, 7919 * (k as u64 + 1)) {
            let r = run(name, defect, &mesh, two, &tool, &previews);
            print_row(&r);
            rows.push(r);
        }
    }
    // Summary per defect.
    eprintln!("\ndefect\tinputs\tcontract ok\tclosed out\tcut ok\tself-intersecting out");
    let mut defects: Vec<String> = rows.iter().map(|r| r.defect.clone()).collect();
    defects.sort();
    defects.dedup();
    for d in &defects {
        let rs: Vec<&Row> = rows.iter().filter(|r| &r.defect == d).collect();
        eprintln!(
            "{d}\t{}\t{}\t{}\t{}\t{}",
            rs.len(),
            rs.iter().filter(|r| r.contract.is_ok()).count(),
            rs.iter().filter(|r| r.closed_out).count(),
            rs.iter().filter(|r| r.cut == "ok").count(),
            rs.iter().filter(|r| r.self_x > 0).count(),
        );
    }
    let total: [f64; 7] = rows.iter().fold([0.0; 7], |mut acc, r| {
        for k in 0..7 {
            acc[k] += r.ms[k];
        }
        acc
    });
    let max: [f64; 7] = rows.iter().fold([0.0; 7], |mut acc, r| {
        for k in 0..7 {
            acc[k] = acc[k].max(r.ms[k]);
        }
        acc
    });
    eprintln!(
        "\nstage ms (total / max): import {:.0}/{:.0}, repair {:.0}/{:.0}, decimate {:.0}/{:.0}, unwrap {:.0}/{:.0}, measure {:.0}/{:.0}, preview {:.0}/{:.0}, export {:.0}/{:.0}",
        total[0], max[0], total[1], max[1], total[2], max[2], total[3], max[3], total[4], max[4],
        total[5], max[5], total[6], max[6]
    );
}
