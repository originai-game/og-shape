//! Golden cases for decimation that does not fold and non-manifold repair
//! (stream AK, `docs/research.md` §32): a thin shell decimated past the
//! point where it would fold, UV islands merged into one chart, two
//! cubes on one edge split apart, a hole over the fill limit, a fill
//! across a UV seam, and the pockets an untangling leaves.

use shape_core::{DVec2, DVec3, HashError, OutputHash};
use shape_gen::solids::{Generator, SphereDesc, sphere, textured};
use shape_mesh::Solid;
use shape_tools::{Decimate, DecimateReport, Floaters, Repair, close_holes, decimate, repair};

use crate::cases::Case;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "meshtools.decimate.fold",
            run: fold_case,
        },
        Case {
            name: "meshtools.decimate.charts",
            run: charts_case,
        },
        Case {
            name: "meshtools.repair.non_manifold",
            run: non_manifold_case,
        },
        Case {
            name: "meshtools.repair.large_hole",
            run: large_hole_case,
        },
        Case {
            name: "meshtools.repair.seam_fill",
            run: seam_fill_case,
        },
        Case {
            name: "meshtools.repair.pockets",
            run: pockets_case,
        },
    ]
}

fn fail(e: impl std::fmt::Display) -> HashError {
    eprintln!("meshrepair: {e}");
    HashError::NaN
}

fn hash_report(h: &mut OutputHash, r: &DecimateReport) -> Result<(), HashError> {
    h.u64(r.triangles as u64);
    h.f64(r.error)?;
    h.u64(r.folds_refused as u64);
    let b = &r.blocked;
    for v in [
        b.fixed,
        b.error,
        b.link,
        b.turn,
        b.uv,
        b.fold,
        b.not_retried,
        r.uv_islands,
        r.uv_charts,
    ] {
        h.u64(v as u64);
    }
    Ok(())
}

/// A hollow ball, its wall 1 % of the radius, decimated to 8 triangles:
/// the outer sphere's chords would cut through the inner one, so it stops
/// short and says why.
fn fold_case(h: &mut OutputHash) -> Result<(), HashError> {
    let mut shell = sphere(10.0).map_err(fail)?;
    let mut inner = sphere(9.9).map_err(fail)?;
    for i in 0..inner.triangles.len() {
        inner.reverse_triangle(i);
    }
    shell.append(&inner);
    shell.canonicalize()?;
    let (low, r) = decimate(
        &shell,
        &Decimate {
            target: 8,
            ..Default::default()
        },
    )
    .map_err(fail)?;
    hash_report(h, &r)?;
    low.hash_into(h)
}

/// A flat 6 × 6 grid whose right half's UVs are its left half's turned a
/// quarter and moved: one chart with `merge_charts`.
fn charts_case(h: &mut OutputHash) -> Result<(), HashError> {
    let mut g = Solid {
        uvs: Some(Vec::new()),
        ..Default::default()
    };
    for y in 0..=6 {
        for x in 0..=6 {
            g.push_vertex(DVec3::new(f64::from(x), f64::from(y), 0.0));
        }
    }
    let uv = |x: u32, y: u32, right: bool| {
        let (x, y) = (f64::from(x) * 0.125, f64::from(y) * 0.125);
        if right {
            DVec2::new(2.0 - y, x)
        } else {
            DVec2::new(x, y)
        }
    };
    for y in 0..6u32 {
        for x in 0..6u32 {
            let right = x >= 3;
            let v = |x: u32, y: u32| y * 7 + x;
            for t in [
                [(x, y), (x + 1, y), (x + 1, y + 1)],
                [(x, y), (x + 1, y + 1), (x, y + 1)],
            ] {
                g.push_triangle_uv(
                    t.map(|(x, y)| v(x, y)),
                    0,
                    &["grid"],
                    t.map(|(x, y)| uv(x, y, right)),
                );
            }
        }
    }
    g.canonicalize()?;
    for merge_charts in [0.0, 1e-9] {
        let (low, r) = decimate(
            &g,
            &Decimate {
                target: 2,
                max_error: 1e-6,
                merge_charts,
                ..Default::default()
            },
        )
        .map_err(fail)?;
        hash_report(h, &r)?;
        low.hash_into(h)?;
    }
    Ok(())
}

/// A unit cube's corners, `size` across, at `offset`, two triangles a
/// face.
fn cube(offset: DVec3, size: f64) -> Solid {
    let mut s = Solid::default();
    for i in 0..8 {
        let p = DVec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64);
        s.push_vertex(offset + p * size);
    }
    let quads = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    for (k, q) in quads.iter().enumerate() {
        s.push_triangle([q[0], q[1], q[2]], k as u32, &["cube"]);
        s.push_triangle([q[0], q[2], q[3]], k as u32, &["cube"]);
    }
    s
}

fn hash_repair(h: &mut OutputHash, s: &Solid, d: &Repair) -> Result<(), HashError> {
    let (out, r) = repair(s, d).map_err(fail)?;
    h.str(&format!("{r:?}"));
    out.hash_into(h)
}

/// Two cubes touching along one edge: split into two closed cubes.
fn non_manifold_case(h: &mut OutputHash) -> Result<(), HashError> {
    let mut s = cube(DVec3::ZERO, 10.0);
    s.append(&cube(DVec3::new(10.0, 10.0, 0.0), 10.0));
    s.canonicalize()?;
    hash_repair(
        h,
        &s,
        &Repair {
            non_manifold: true,
            check_intersections: true,
            ..Default::default()
        },
    )
}

/// A cube without one face, closed with a limit of three edges: the hole
/// is flat and clear.
fn large_hole_case(h: &mut OutputHash) -> Result<(), HashError> {
    let mut s = cube(DVec3::ZERO, 10.0);
    for _ in 0..2 {
        s.triangles.pop();
        s.surfaces.pop();
        s.face_tags.pop();
    }
    let (out, holes) = close_holes(&s, 3).map_err(fail)?;
    h.str(&format!("{holes:?}"));
    out.hash_into(h)
}

/// A textured sphere without the fan of one seam vertex: the fill takes
/// one UV.
fn seam_fill_case(h: &mut OutputHash) -> Result<(), HashError> {
    let ball = textured(&Generator::Sphere(SphereDesc {
        radius: 7.3,
        ..Default::default()
    }))
    .map_err(fail)?;
    let uvs = ball.uvs.as_ref().ok_or(HashError::NaN)?;
    let mut sides: Vec<Vec<DVec2>> = vec![Vec::new(); ball.positions.len()];
    for (t, tri) in ball.triangles.iter().enumerate() {
        for k in 0..3 {
            let u = uvs[t][k];
            if !sides[tri[k] as usize].contains(&u) {
                sides[tri[k] as usize].push(u);
            }
        }
    }
    let v = (0..ball.positions.len())
        .find(|&v| sides[v].len() == 2 && ball.positions[v].z.abs() < 3.0)
        .ok_or(HashError::NaN)? as u32;
    let mut open = ball.clone();
    let keep: Vec<usize> = (0..open.triangles.len())
        .filter(|&t| !open.triangles[t].contains(&v))
        .collect();
    open.triangles = keep.iter().map(|&t| ball.triangles[t]).collect();
    open.surfaces = keep.iter().map(|&t| ball.surfaces[t]).collect();
    open.face_tags = keep.iter().map(|&t| ball.face_tags[t].clone()).collect();
    open.uvs = Some(keep.iter().map(|&t| uvs[t]).collect());
    open.canonicalize()?;
    let (out, holes) = close_holes(&open, 64).map_err(fail)?;
    h.str(&format!("{holes:?}"));
    out.hash_into(h)
}

/// A ball folded by noise (1.5 per axis, splitmix64 seed 5): fused, its
/// three pockets dropped as floaters.
fn pockets_case(h: &mut OutputHash) -> Result<(), HashError> {
    let mut ball = sphere(10.0).map_err(fail)?;
    let mut state = 5u64;
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 52) as f64 - 1.0
    };
    for p in &mut ball.positions {
        *p += DVec3::new(next(), next(), next()) * 1.5;
    }
    hash_repair(
        h,
        &ball,
        &Repair {
            fuse: true,
            floaters: Floaters {
                fraction: 0.05,
                ..Default::default()
            },
            check_intersections: true,
            ..Default::default()
        },
    )
}
