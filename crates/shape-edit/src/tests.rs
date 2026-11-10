//! sand3d's edit cases (`tests/cases.py` at 84c980a), rebuilt: the names
//! must land on the same faces and edges, the walls measure as asked.

use shape_2d::{Rect, Slot, rect, slot};
use shape_core::{DVec3, Frame, trig};
use shape_gen::solids::{BoxDesc, cuboid, sphere};
use shape_mesh::Solid;

use super::*;

fn boxed(w: f64, d: f64, h: f64) -> Solid {
    cuboid(&BoxDesc {
        width: w,
        depth: d,
        height: h,
    })
    .unwrap()
}

fn closed(s: &Solid) -> f64 {
    let m = s.measure();
    assert!(m.closed, "{m:?}");
    m.volume
}

#[test]
fn holes_and_pockets() {
    // plate = box(80, 50, 10); four 6.6 holes; a 5 hole with a 9×4 counterbore.
    let plate = boxed(80.0, 50.0, 10.0);
    let at = [(-30.0, -15.0), (30.0, -15.0), (-30.0, 15.0), (30.0, 15.0)]
        .map(|(x, y)| above(x, y))
        .to_vec();
    let s = hole(
        &plate,
        &Hole {
            diameter: 6.6,
            at,
            ..Default::default()
        },
    )
    .unwrap();
    let s = hole(
        &s,
        &Hole {
            diameter: 5.0,
            at: vec![above(0.0, 15.0)],
            counterbore: (9.0, 4.0),
            ..Default::default()
        },
    )
    .unwrap();
    let v = closed(&s);
    // Every mouth edge is named hole: four through holes have two mouths,
    // the counterbored one a top mouth, its step, and a bottom mouth.
    let mouths = s.edges(&["hole"]).unwrap();
    // Crossing points carry rounding: 6.000000000000001 is on the step.
    let at_z = |p: DVec3, z: f64| (p.z - z).abs() < 1e-9;
    let on = |z: f64| {
        mouths
            .iter()
            .filter(|&&(a, b)| at_z(s.positions[a as usize], z) && at_z(s.positions[b as usize], z))
            .count()
    };
    assert!(
        on(10.0) > 0 && on(0.0) > 0 && on(6.0) > 0,
        "{} {} {}",
        on(10.0),
        on(0.0),
        on(6.0)
    );
    assert!(!s.faces(&["hole.side"]).unwrap().is_empty());
    // The counterbore's floor is the tool's bottom cap.
    assert!(s.faces(&["hole.bottom"]).unwrap().iter().all(|&t| {
        s.triangles[t as usize]
            .iter()
            .all(|&p| at_z(s.positions[p as usize], 6.0))
    }));
    // The plate keeps its own names.
    assert!(!s.faces(&["top"]).unwrap().is_empty() && s.edges(&["side"]).unwrap().len() >= 4);
    let n = |r: f64| shape_2d::curve::segments(r, trig::TAU) as f64;
    let disc = |r: f64| n(r) / 2.0 * r * r * trig::sin(trig::TAU / n(r));
    let want = 80.0 * 50.0 * 10.0 - 4.0 * disc(3.3) * 10.0 - disc(2.5) * 6.0 - disc(4.5) * 4.0;
    assert!((v - want).abs() / want < 1e-9, "{v} {want}");

    // A slot pocket 3 deep from the top.
    let p = pocket(
        &plate,
        &Pocket {
            profile: slot(&Slot {
                length: 20.0,
                width: 8.0,
            })
            .unwrap(),
            depth: 3.0,
            at: vec![],
        },
    )
    .unwrap();
    closed(&p);
    let floor = p.faces(&["pocket.bottom"]).unwrap();
    assert!(floor.iter().all(|&t| {
        p.triangles[t as usize]
            .iter()
            .all(|&q| p.positions[q as usize].z == 7.0)
    }));
    assert!(!p.faces(&["pocket.side"]).unwrap().is_empty());
}

#[test]
fn a_countersink_and_a_blind_hole() {
    let plate = boxed(40.0, 40.0, 10.0);
    let s = hole(
        &plate,
        &Hole {
            diameter: 4.0,
            at: vec![above(0.0, 0.0)],
            depth: 6.0,
            countersink: (8.0, trig::PI / 2.0),
            ..Default::default()
        },
    )
    .unwrap();
    closed(&s);
    let floor = s.faces(&["hole.bottom"]).unwrap();
    assert!(floor.iter().all(|&t| {
        s.triangles[t as usize]
            .iter()
            .all(|&q| s.positions[q as usize].z == 4.0)
    }));
    // The countersink's mouth is a circle of diameter 8 on the top face.
    let r_max = s
        .edges(&["hole"])
        .unwrap()
        .iter()
        .map(|&(a, _)| s.positions[a as usize])
        .filter(|p| p.z == 10.0)
        .map(|p| p.truncate().length())
        .fold(0.0, f64::max);
    assert!((r_max - 4.0).abs() < 1e-6, "{r_max}");
}

/// The countersink's cone is turned half a segment about the axis, so its
/// generators run between the bore's columns and no rim point is made
/// twice an ulp apart: the plate does not cross itself (it did by 4 and
/// 32 pairs with the cone unturned; `docs/research.md` §23).
#[test]
fn a_countersink_rim_does_not_cross_itself() {
    let plate = boxed(80.0, 50.0, 10.0);
    for (x, y, depth) in [(0.0, -5.0, 6.0), (-30.0, -15.0, 6.0), (0.0, 0.0, 0.0)] {
        let s = hole(
            &plate,
            &Hole {
                diameter: 4.0,
                at: vec![above(x, y)],
                depth,
                countersink: (8.0, trig::PI / 2.0),
                ..Default::default()
            },
        )
        .unwrap();
        closed(&s);
        assert_eq!(shape_bool::crossing_pairs(&s), 0, "({x}, {y})");
    }
}

#[test]
fn shells_measure_their_walls() {
    // shell(box(40, 30, 20), 2, open="top")
    let o = shell(
        &boxed(40.0, 30.0, 20.0),
        &Shell {
            thickness: 2.0,
            open: vec!["top".into()],
        },
    )
    .unwrap();
    let v = closed(&o);
    assert!(
        (v - (40.0 * 30.0 * 20.0 - 36.0 * 26.0 * 18.0)).abs() < 1e-9,
        "{v}"
    );
    let inner = o.faces(&["cut"]).unwrap();
    let m = o.measure();
    let lo = inner
        .iter()
        .flat_map(|&t| o.triangles[t as usize])
        .map(|q| o.positions[q as usize])
        .fold(DVec3::INFINITY, DVec3::min);
    assert_eq!(lo, DVec3::new(-18.0, -13.0, 2.0));
    assert_eq!(m.bounds_max.z, 20.0);
    // By direction, the same opening.
    let o2 = shell(
        &boxed(40.0, 30.0, 20.0),
        &Shell {
            thickness: 2.0,
            open: vec!["+z".into()],
        },
    )
    .unwrap();
    assert_eq!(o2.measure().volume, v);
    // shell(sphere(30), 1.5): a closed void, two shells.
    let ball = shell(
        &sphere(15.0).unwrap(),
        &Shell {
            thickness: 1.5,
            open: vec![],
        },
    )
    .unwrap();
    let mb = ball.measure();
    assert!(mb.closed && mb.components == 2, "{mb:?}");
    let r_in = ball
        .faces(&["cut"])
        .unwrap()
        .iter()
        .flat_map(|&t| ball.triangles[t as usize])
        .map(|q| ball.positions[q as usize].length())
        .fold(f64::INFINITY, f64::min);
    // Inner vertices sit 1.5 inside the facets they touch: on the sphere's
    // flats, 15·cos(half a facet) − 1.5 at least.
    assert!(r_in > 13.0 && r_in < 13.6, "{r_in}");
    // A curved face cannot be opened.
    assert!(matches!(
        shell(
            &sphere(15.0).unwrap(),
            &Shell {
                thickness: 1.0,
                open: vec!["side".into()]
            }
        ),
        Err(EditError::NotFlat(_))
    ));
}

#[test]
fn split_seat_merge_dent() {
    let b = boxed(20.0, 20.0, 20.0);
    let f = Frame {
        origin: DVec3::new(0.0, 0.0, 5.0),
        ..Frame::IDENTITY
    };
    let above_part = split(&b, &f, Keep::Above).unwrap();
    let below_part = split(&b, &f, Keep::Below).unwrap();
    assert!((closed(&above_part) - 20.0 * 20.0 * 15.0).abs() < 1e-9);
    assert!((closed(&below_part) - 20.0 * 20.0 * 5.0).abs() < 1e-9);

    // A knob seated half into a console.
    let console = boxed(60.0, 40.0, 10.0);
    let mut knob = boxed(10.0, 10.0, 10.0);
    knob.map_positions(|p| p + DVec3::new(0.0, 0.0, 5.0), false);
    let seated = seat(&console, &knob, "knob").unwrap();
    assert!((closed(&seated) - (60.0 * 40.0 * 10.0 - 10.0 * 10.0 * 5.0)).abs() < 1e-9);
    assert!(seated.edges(&["knob"]).is_ok());
    let mut away = knob.clone();
    away.map_positions(|p| p + DVec3::new(100.0, 0.0, 0.0), false);
    assert_eq!(seat(&console, &away, "knob"), Err(EditError::NoOverlap));

    // merge keeps two shells; a boolean on it fuses them first.
    let mut other = boxed(20.0, 20.0, 20.0);
    other.map_positions(|p| p + DVec3::new(10.0, 0.0, 0.0), false);
    let m = merge(&[&b, &other]);
    assert_eq!(m.measure().components, 2);
    let r = rect(&Rect {
        width: 4.0,
        height: 4.0,
        radius: 0.0,
    })
    .unwrap();
    let cut = pocket(
        &m,
        &Pocket {
            profile: r,
            depth: 2.0,
            at: vec![],
        },
    )
    .unwrap();
    let mc = cut.measure();
    assert!(mc.closed && mc.components == 1);
    assert!(
        (mc.volume - (30.0 * 20.0 * 20.0 - 4.0 * 4.0 * 2.0)).abs() < 1e-9,
        "{}",
        mc.volume
    );

    // dent(pad, spots, 15, 5): three hollows in a cushion's top.
    let pad = boxed(100.0, 100.0, 30.0);
    let spots: Vec<DVec3> = [-25.0, 0.0, 25.0]
        .iter()
        .map(|&x| DVec3::new(x, 0.0, 30.0))
        .collect();
    let d = dent(
        &pad,
        &Dent {
            at: spots,
            radius: 15.0,
            depth: 5.0,
        },
    )
    .unwrap();
    let v = closed(&d);
    assert!(v < 100.0 * 100.0 * 30.0);
    assert_eq!(d.measure().bounds_min.z, 0.0);
    let lowest_top = d
        .faces(&["dent"])
        .unwrap()
        .iter()
        .flat_map(|&t| d.triangles[t as usize])
        .map(|q| d.positions[q as usize].z)
        .fold(f64::INFINITY, f64::min);
    assert!((lowest_top - 25.0).abs() < 1e-9, "{lowest_top}");
}

/// `docs/research.md` §13.6, item 13: an axis through an edge shared by
/// two triangles meets the solid. On a turned box, a hole whose axis runs
/// along a face's normal through a point of the face's diagonal missed
/// both triangles by floating point `u, v` (`Misses`); it is decided by
/// `orient3d` now.
#[test]
fn an_axis_through_a_face_diagonal_meets_the_solid() {
    let (s, c) = trig::sin_cos(21.0 * 0.0137);
    let (s2, c2) = trig::sin_cos(21.0 * 0.0137 * 1.7);
    let z = DVec3::new(s * c2, s * s2, c).normalize();
    let turned = Frame::from_normal(DVec3::new(0.3, -0.2, 0.1), z, DVec3::X).unwrap();
    let mut b = boxed(10.0, 10.0, 10.0);
    b.map_positions(|p| turned.point_to_world(p), false);
    // 3/8 of the way along the top face's diagonal, looking down its normal.
    let at = Frame::from_normal(
        DVec3::new(4.014963521695149, 2.3280884594923625, 9.206745564330646),
        DVec3::new(0.2504811681798565, 0.13331104117759812, 0.9588990304966416),
        DVec3::X,
    )
    .unwrap();
    let before = closed(&b);
    let s = hole(
        &b,
        &Hole {
            diameter: 1.0,
            depth: 3.0,
            at: vec![at],
            ..Default::default()
        },
    )
    .unwrap();
    let v = closed(&s);
    assert!(before - v > 2.0 && before - v < 2.4, "{before} {v}");
}

/// `docs/research.md` §13.6, item 13 as phase 6 met it: a through hole
/// down a boss, then a pocket from the bottom face's centre. The pocket's
/// axis runs inside the hole and meets no face; the error says so, with
/// the frame's origin, the direction and the hole's clearance.
#[test]
fn an_axis_inside_a_through_hole_is_reported_with_its_clearance() {
    let b = boxed(80.0, 80.0, 80.0);
    let s = hole(
        &b,
        &Hole {
            diameter: 6.0,
            at: vec![above(0.0, 0.0)],
            ..Default::default()
        },
    )
    .unwrap();
    let bottom = Frame::from_normal(DVec3::ZERO, -DVec3::Z, DVec3::X).unwrap();
    let e = pocket(
        &s,
        &Pocket {
            profile: rect(&Rect {
                width: 56.0,
                height: 56.0,
                ..Default::default()
            })
            .unwrap(),
            depth: 20.0,
            at: vec![bottom],
        },
    )
    .unwrap_err();
    let EditError::Misses {
        origin,
        direction,
        clearance,
    } = e
    else {
        panic!("{e}");
    };
    assert_eq!((origin, direction), (DVec3::ZERO, DVec3::Z));
    // The bore is a polygon inscribed in the 3 mm circle.
    assert!(clearance > 2.9 && clearance <= 3.0, "{clearance}");
    // A pocket clear of the hole's axis cuts as asked.
    let off = Frame {
        origin: DVec3::new(10.0, 0.0, 0.0),
        ..bottom
    };
    let p = pocket(
        &s,
        &Pocket {
            profile: rect(&Rect {
                width: 10.0,
                height: 10.0,
                ..Default::default()
            })
            .unwrap(),
            depth: 20.0,
            at: vec![off],
        },
    )
    .unwrap();
    closed(&p);
}
