//! Booleans on generated meshes (`docs/research.md` §28, plan phase 5's
//! re-check of §5.3): each file's `keep` output at one budget, as
//! `run.ts` wrote it (repaired and decimated), is cut by a box at one
//! corner and by a cylinder off its centre, and joined with the next
//! file of the same generator, moved to overlap it. Per boolean: closed
//! result, or the refusal's cause, and the time; a cut whose result has
//! no face of the tool missed the mesh, and is counted as such.
//!
//! ```text
//! cargo run --release -- OUT [BUDGET]   # OUT as run.ts's, BUDGET 10000
//! ```
//!
//! Prints a TSV row per boolean (file, op, outcome, ms, triangles out,
//! faces without UVs) and a summary table.

use std::collections::BTreeMap;
use std::time::Instant;

use shape_bool::{BoolError, BoolOp, Boolean, boolean_report};
use shape_core::DVec3;
use shape_gen::solids::{BoxDesc, CylinderDesc, cuboid, cylinder};
use shape_mesh::Solid;

fn load(path: &std::path::Path) -> Option<Solid> {
    let bytes = std::fs::read(path).ok()?;
    let imported = shape_tools::import_gltf(&bytes, &|u: &str| Err(format!("no {u}"))).ok()?;
    let mut s = shape_tools::join(&imported.parts).ok()?;
    // The GLB splits vertices at UV seams; the solid is welded.
    s.canonicalize().ok()?;
    Some(s)
}

fn bounds(s: &Solid) -> (DVec3, DVec3) {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for &p in &s.positions {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    (lo, hi)
}

fn moved(mut s: Solid, by: DVec3) -> Solid {
    for p in &mut s.positions {
        *p += by;
    }
    s
}

/// The cause, without the numbers, for counting.
fn cause(e: &BoolError) -> &'static str {
    match e {
        BoolError::NotClosed(_) => "operand not closed",
        BoolError::NotFinite(_) => "not finite",
        BoolError::Empty => "empty",
        BoolError::Skinned(_) => "skinned",
        BoolError::OpenResult { .. } => "open result",
        BoolError::OpenTouching { .. } => "open where touching",
        BoolError::NotManifold { .. } => "not manifold",
        BoolError::SelfIntersecting { .. } => "operand crosses itself",
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::PathBuf::from(args.first().expect("usage: bench-generated OUT [BUDGET]"));
    let budget = args.get(1).map_or("10000", String::as_str);
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(here.join("manifest.tsv")).expect("manifest.tsv");
    let rows: Vec<(String, String)> = manifest
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f[0].to_string(), f[1].to_string())
        })
        .collect();
    let solids: Vec<Option<Solid>> = rows
        .iter()
        .map(|(name, _)| load(&out.join("glb").join(format!("{name}.keep.{budget}.glb"))))
        .collect();
    // (generator, op) → outcome → count; and times.
    let mut tally: BTreeMap<(String, &str), BTreeMap<String, usize>> = BTreeMap::new();
    let mut times: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    println!("file\top\toutcome\tms\ttriangles\tfaces_without_uvs");
    for (i, (name, generator)) in rows.iter().enumerate() {
        let Some(s) = &solids[i] else {
            println!("{name}\t-\tno output\t-\t-\t-");
            continue;
        };
        let (lo, hi) = bounds(s);
        let size = hi - lo;
        let m = size.min_element();
        // A box over the top middle, 0.4 of each size, at odd offsets so
        // no face lies on one of the mesh's.
        let bx = moved(
            cuboid(&BoxDesc {
                width: size.x * 0.4,
                depth: size.y * 0.4,
                height: size.z * 0.4,
            })
            .unwrap(),
            DVec3::new(
                lo.x + size.x * 0.537,
                lo.y + size.y * 0.541,
                hi.z - size.z * 0.263,
            ),
        );
        // A cylinder along z through the body, off the centre.
        let c = (lo + hi) * 0.5 + DVec3::new(size.x * 0.0731, size.y * 0.0617, 0.0);
        let cyl = moved(
            cylinder(&CylinderDesc {
                radius: m * 0.137,
                height: size.z * 1.6,
                ..Default::default()
            })
            .unwrap(),
            DVec3::new(c.x, c.y, lo.z - size.z * 0.3),
        );
        let mut ops: Vec<(&str, BoolOp, Solid)> = vec![
            ("cut box", BoolOp::Cut, bx),
            ("cut cylinder", BoolOp::Cut, cyl),
        ];
        // The next file of the same generator, moved so a third of it
        // overlaps this one along x.
        let next = (i + 1..rows.len()).find(|&j| rows[j].1 == *generator);
        if let Some(t) = next.and_then(|j| solids[j].as_ref()) {
            let (tlo, _) = bounds(t);
            let by = DVec3::new(
                hi.x - size.x * 0.33 - tlo.x,
                lo.y - tlo.y + size.y * 0.071,
                lo.z - tlo.z,
            );
            ops.push(("join next", BoolOp::Join, moved(t.clone(), by)));
        }
        for (label, op, tool) in ops {
            let t0 = Instant::now();
            let r = boolean_report(
                s,
                &tool,
                &Boolean {
                    op,
                    ..Default::default()
                },
            );
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            let (outcome, tris, bare) = match &r {
                // The target has UVs and the cutting tools none: a cut
                // with no face of the tool's left did not reach the mesh.
                Ok((o, rep)) if op == BoolOp::Cut && rep.faces_without_uvs == 0 => (
                    "tool missed".to_string(),
                    o.triangles.len().to_string(),
                    "0".into(),
                ),
                Ok((o, rep)) if o.measure().closed => (
                    "closed".to_string(),
                    o.triangles.len().to_string(),
                    rep.faces_without_uvs.to_string(),
                ),
                Ok((o, _)) => (
                    "open (returned)".to_string(),
                    o.triangles.len().to_string(),
                    "-".into(),
                ),
                Err(e) => (format!("refused: {}", cause(e)), "-".into(), "-".into()),
            };
            println!("{name}\t{label}\t{outcome}\t{ms:.0}\t{tris}\t{bare}");
            *tally
                .entry((generator.clone(), label))
                .or_default()
                .entry(outcome)
                .or_default() += 1;
            times.entry(label).or_default().push(ms);
        }
    }
    println!();
    println!("| generator | boolean | outcomes |");
    println!("|---|---|---|");
    for ((g, op), o) in &tally {
        let list: Vec<String> = o.iter().map(|(k, n)| format!("{k} {n}")).collect();
        println!("| {g} | {op} | {} |", list.join(", "));
    }
    println!();
    for (op, t) in &times {
        let mut t = t.clone();
        t.sort_by(f64::total_cmp);
        println!(
            "{op}: {} runs, median {:.0} ms, max {:.0} ms",
            t.len(),
            t[t.len() / 2],
            t[t.len() - 1]
        );
    }
}
