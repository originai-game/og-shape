//! Bevels carry UVs (`docs/research.md` §21.2): the same geometry as the
//! bevel of the solid without UVs; faces the bevel does not touch keep
//! their UVs exactly; every face, new or trimmed, reads the way its
//! chart does (no triangle turned over or flattened in UV); and every new
//! face is continuous with the chart it takes along their shared edges.

use shape_2d::{Rect, rect};
use shape_bevel::{Chamfer, Fillet, Soften, chamfer, fillet, soften};
use shape_core::DVec2;
use shape_core::predicates::orient2d;
use shape_edit::{Hole, above, hole};
use shape_gen::solids::{BoxDesc, CylinderDesc, Extrude, Generator, textured};
use shape_mesh::Solid;

fn bare(s: &Solid) -> Solid {
    let mut b = s.clone();
    b.uvs = None;
    b
}

/// Checks `out` (the bevel of `input` with UVs) against `plain` (the same
/// bevel without), and returns the share of edges between a new face
/// and an old one along which UVs are continuous.
fn check(what: &str, input: &Solid, out: &Solid, plain: &Solid, kind: &str) -> f64 {
    assert_eq!(&bare(out), plain, "{what}: geometry");
    let uvs = out.uvs.as_ref().expect(what);
    let kind_tag = out.names.get(kind);
    let new = |i: usize| out.face_tags[i].len() == 1 && Some(out.face_tags[i][0]) == kind_tag;
    for (i, u) in uvs.iter().enumerate() {
        let [a, b, c] = out.triangles[i].map(|v| out.positions[v as usize]);
        let area3 = (b - a).cross(c - a).length();
        if area3 < 1e-9 {
            continue;
        }
        assert_eq!(
            orient2d(u[0], u[1], u[2]),
            core::cmp::Ordering::Greater,
            "{what}: triangle {i} ({}) turned over or flat in UV: {u:?}",
            if new(i) { "new" } else { "old" }
        );
        if new(i) {
            let area2 = (u[1] - u[0]).perp_dot(u[2] - u[0]);
            let ratio = area2 / area3;
            assert!(
                (0.05..=2.0).contains(&ratio),
                "{what}: new triangle {i} stretched {ratio}"
            );
        }
    }
    // Untouched faces: a triangle of the input whose corners are all in
    // the output, as a triangle, keeps its UVs.
    let ins = input.uvs.as_ref().unwrap();
    let mut kept = 0;
    for (j, t) in input.triangles.iter().enumerate() {
        let p = t.map(|v| input.positions[v as usize]);
        for (i, u) in out.triangles.iter().enumerate() {
            let q = u.map(|v| out.positions[v as usize]);
            if let Some(r) = (0..3).find(|&r| q[r] == p[0] && q[(r + 1) % 3] == p[1])
                && q[(r + 2) % 3] == p[2]
            {
                let got = [uvs[i][r], uvs[i][(r + 1) % 3], uvs[i][(r + 2) % 3]];
                assert_eq!(got, ins[j], "{what}: untouched triangle {j} moved in UV");
                kept += 1;
            }
        }
    }
    // The box's bottom is untouched by a fillet on its top (on a box
    // softened everywhere, or a plate whose caps all reach a rounded
    // edge, no triangle is left whole).
    assert!(
        kept > 0 || what != "box fillet",
        "{what}: no triangle left untouched"
    );
    // Continuity across new/old edges.
    let mut edges = std::collections::BTreeMap::new();
    for (i, t) in out.triangles.iter().enumerate() {
        for k in 0..3 {
            edges.insert((t[k], t[(k + 1) % 3]), (i, k));
        }
    }
    let (mut across, mut continuous) = (0, 0);
    for (&(a, b), &(i, k)) in &edges {
        let Some(&(o, m)) = edges.get(&(b, a)) else {
            continue;
        };
        if !new(i) || new(o) {
            continue;
        }
        across += 1;
        let (ua, ub) = (uvs[i][k], uvs[i][(k + 1) % 3]);
        let (va, vb) = (uvs[o][(m + 1) % 3], uvs[o][m]);
        let close = |x: DVec2, y: DVec2| (x - y).length() < 1e-9;
        if close(ua, va) && close(ub, vb) {
            continuous += 1;
        }
    }
    continuous as f64 / across.max(1) as f64
}

fn plate() -> Solid {
    let p = rect(&Rect {
        width: 60.0,
        height: 40.0,
        radius: 0.0,
    })
    .unwrap();
    let s = textured(&Generator::Extrude(
        &p,
        Extrude {
            height: 6.0,
            ..Default::default()
        },
    ))
    .unwrap();
    hole(
        &s,
        &Hole {
            diameter: 10.0,
            at: vec![above(10.0, 0.0)],
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn fillets_chamfers_and_soften_carry_uvs() {
    let b = textured(&Generator::Box(BoxDesc {
        width: 20.0,
        depth: 20.0,
        height: 10.0,
    }))
    .unwrap();
    let top = vec!["top".to_string()];
    let fd = Fillet {
        radius: 2.0,
        edges: top.clone(),
        ..Default::default()
    };
    let share = check(
        "box fillet",
        &b,
        &fillet(&b, &fd).unwrap(),
        &fillet(&bare(&b), &fd).unwrap(),
        "fillet",
    );
    // A new triangle on an old face's edge faces that face most, so it
    // takes its chart: continuous there (the seam runs inside the strip).
    assert_eq!(share, 1.0, "box fillet continuity");
    let cd = Chamfer {
        distance: 1.5,
        edges: top,
        ..Default::default()
    };
    check(
        "box chamfer",
        &b,
        &chamfer(&b, &cd).unwrap(),
        &chamfer(&bare(&b), &cd).unwrap(),
        "chamfer",
    );
    let sd = Soften {
        radius: 1.0,
        ..Default::default()
    };
    check(
        "box soften",
        &b,
        &soften(&b, &sd).unwrap(),
        &soften(&bare(&b), &sd).unwrap(),
        "fillet",
    );
    // A curved wall (its seam at angle 0) and a planar cap.
    let c = textured(&Generator::Cylinder(CylinderDesc {
        radius: 10.0,
        height: 12.0,
        segments: 48,
        ..Default::default()
    }))
    .unwrap();
    let fd = Fillet {
        radius: 2.0,
        edges: vec!["top".into()],
        ..Default::default()
    };
    check(
        "cylinder fillet",
        &c,
        &fillet(&c, &fd).unwrap(),
        &fillet(&bare(&c), &fd).unwrap(),
        "fillet",
    );
    // A plate with a hole (UVs through the boolean), its top edges and
    // the hole's mouth rounded.
    let p = plate();
    let fd = Fillet {
        radius: 1.5,
        edges: vec!["top".into(), "hole".into()],
        ..Default::default()
    };
    check(
        "plate fillet",
        &p,
        &fillet(&p, &fd).unwrap(),
        &fillet(&bare(&p), &fd).unwrap(),
        "fillet",
    );
}
