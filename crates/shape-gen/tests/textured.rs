//! `textured` on the generators beyond extrude, revolve and the
//! primitives: the same solid as the generator alone, plus UVs on every
//! corner, every face with area reading counter-clockwise from outside,
//! and walls in millimetres of arc length (`docs/research.md` §21).

use shape_2d::{Circle, Rect, circle, rect};
use shape_core::predicates::orient2d;
use shape_core::{DVec2, DVec3};
use shape_gen::body::{Body, View, body};
use shape_gen::parts::{GearDesc, Knurl, gear_solid, knurl};
use shape_gen::paths::{Loft, Sweep, Thread, loft, sweep, thread};
use shape_gen::solids::{Generator, textured, wedge};
use shape_gen::wire::{Wire, wire};
use shape_mesh::Solid;

fn check(name: &str, t: &Solid, plain: &Solid) {
    let mut bare = t.clone();
    let uvs = bare.uvs.take().expect(name);
    assert_eq!(&bare, plain, "{name}: geometry changed");
    assert_eq!(uvs.len(), t.triangles.len());
    for (i, u) in uvs.iter().enumerate() {
        let [a, b, c] = t.triangles[i].map(|v| t.positions[v as usize]);
        if (b - a).cross(c - a).length() > 1e-9 {
            assert_eq!(
                orient2d(u[0], u[1], u[2]),
                core::cmp::Ordering::Greater,
                "{name}: triangle {i} reads mirrored or flat: {u:?}"
            );
        }
    }
}

#[test]
fn every_generator_carries_uvs_counter_clockwise() {
    let square = rect(&Rect {
        width: 20.0,
        height: 20.0,
        radius: 0.0,
    })
    .unwrap();
    let round = circle(&Circle {
        radius: 8.0,
        segments: 24,
    })
    .unwrap();
    let ld = Loft {
        sections: vec![
            (square.clone(), 0.0),
            (round.clone(), 15.0),
            (square.clone(), 30.0),
        ],
        ..Default::default()
    };
    check(
        "loft",
        &textured(&Generator::Loft(&ld)).unwrap(),
        &loft(&ld).unwrap(),
    );
    let sd = Sweep {
        path: vec![
            DVec3::ZERO,
            DVec3::new(0.0, 0.0, 20.0),
            DVec3::new(10.0, 0.0, 35.0),
            DVec3::new(30.0, 0.0, 40.0),
        ],
        twist: 0.5,
        scale_end: 0.5,
        ..Default::default()
    };
    check(
        "sweep",
        &textured(&Generator::Sweep(&round, &sd)).unwrap(),
        &sweep(&round, &sd).unwrap(),
    );
    let th = Thread {
        radius: 5.0,
        pitch: 1.5,
        length: 6.0,
        depth: 0.0,
    };
    check(
        "thread",
        &textured(&Generator::Thread(th)).unwrap(),
        &thread(&th).unwrap(),
    );
    for closed in [false, true] {
        let wd = Wire {
            path: vec![
                DVec3::ZERO,
                DVec3::new(40.0, 0.0, 0.0),
                DVec3::new(40.0, 30.0, 0.0),
                DVec3::new(0.0, 30.0, 10.0),
            ],
            diameter: 4.0,
            closed,
            fillet: if closed { 0.0 } else { 6.0 },
            ..Default::default()
        };
        check(
            "wire",
            &textured(&Generator::Wire(&wd)).unwrap(),
            &wire(&wd).unwrap(),
        );
    }
    let bd = Body {
        plan: View::spline(&[[0.0, 0.0], [20.0, 12.0], [60.0, 10.0], [80.0, 4.0]]),
        top: View::points(&[[0.0, 5.0], [80.0, 8.0]]),
        bottom: View::points(&[[0.0, 5.0], [80.0, -2.0]]),
        ..Default::default()
    };
    check(
        "body",
        &textured(&Generator::Body(&bd)).unwrap(),
        &body(&bd).unwrap(),
    );
    for straight in [false, true] {
        let kd = Knurl {
            diameter: 12.0,
            height: 8.0,
            pitch: 1.0,
            straight,
        };
        check(
            "knurl",
            &textured(&Generator::Knurl(kd)).unwrap(),
            &knurl(&kd).unwrap(),
        );
    }
    let gd = GearDesc {
        teeth: 12,
        module: 2.0,
        width: 5.0,
        bore: 5.0,
        hub: 10.0,
        hub_height: 3.0,
        ..Default::default()
    };
    check(
        "gear",
        &textured(&Generator::Gear(gd)).unwrap(),
        &gear_solid(&gd).unwrap(),
    );
    check(
        "wedge",
        &textured(&Generator::Wedge(10.0, 5.0, 3.0)).unwrap(),
        &wedge(10.0, 5.0, 3.0).unwrap(),
    );
}

#[test]
fn a_straight_wire_unrolls_to_its_length_by_its_perimeter() {
    let wd = Wire {
        path: vec![DVec3::ZERO, DVec3::new(0.0, 0.0, 50.0)],
        diameter: 10.0,
        segments: 64,
        ..Default::default()
    };
    let t = textured(&Generator::Wire(&wd)).unwrap();
    let side = t.faces(&["side"]).unwrap();
    let uvs = t.uvs.as_ref().unwrap();
    let (mut lo, mut hi) = (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN));
    for &i in &side {
        for q in uvs[i as usize] {
            lo = lo.min(q);
            hi = hi.max(q);
        }
    }
    // u spans the polygon's perimeter (64 chords of a 5 mm circle), v the
    // length.
    let perimeter = 64.0 * 2.0 * 5.0 * shape_core::trig::sin(shape_core::trig::PI / 64.0);
    assert!(((hi.x - lo.x) - perimeter).abs() < 1e-9, "{lo} {hi}");
    assert!(((hi.y - lo.y) - 50.0).abs() < 1e-9, "{lo} {hi}");
}
