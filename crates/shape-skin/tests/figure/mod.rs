//! Phase 5b's figure: a stylised body, head, two arms and two legs, built
//! as a program from existing generators (loft, sweep, sphere, cuboid and
//! boolean join), with a named region and a bone per part.
//!
//! Millimetres, Z up, facing −Y. About 180 mm tall.

#![allow(dead_code)]

use shape_2d::{Circle, Ellipse, circle, ellipse};
use shape_bool::{Boolean, boolean};
use shape_core::{DVec3, Frame, trig};
use shape_gen::paths::{Loft, Sweep, loft, sweep};
use shape_gen::solids::{BoxDesc, cuboid, sphere};
use shape_mesh::Solid;
use shape_skin::{Pose, Rig, RigBone, Turn};

/// The width of the blend across each seam: about four limb radii.
pub const BLEND: f64 = 24.0;

fn named(mut s: Solid, region: &str) -> Solid {
    s.tag_all_faces(region);
    s
}

fn limb(from: DVec3, to: DVec3, radius: f64, region: &str) -> Solid {
    let p = circle(&Circle {
        radius,
        segments: 0,
    })
    .unwrap();
    named(
        sweep(
            &p,
            &Sweep {
                path: vec![from, to],
                up: DVec3::Y,
                ..Default::default()
            },
        )
        .unwrap(),
        region,
    )
}

fn ball(at: DVec3, radius: f64, region: &str) -> Solid {
    let mut s = sphere(radius).unwrap();
    s.map_positions(|p| p + at, false);
    named(s, region)
}

fn torso() -> Solid {
    let section = |rx: f64, ry: f64, z: f64| {
        (
            ellipse(&Ellipse {
                rx,
                ry,
                segments: 32,
            })
            .unwrap(),
            z,
        )
    };
    named(
        loft(&Loft {
            sections: vec![
                section(19.0, 12.0, 86.0),
                section(17.0, 11.0, 110.0),
                section(22.0, 12.5, 140.0),
                section(12.0, 9.0, 156.0),
            ],
            samples: 32,
            steps: 4,
            ..Default::default()
        })
        .unwrap(),
        "torso",
    )
}

fn foot(x: f64) -> Solid {
    let mut s = cuboid(&BoxDesc {
        width: 14.0,
        depth: 26.0,
        height: 8.0,
    })
    .unwrap();
    // The box's own placement: centred on x and y, on z = 0 (checked
    // below by its bounds).
    let m = s.measure();
    let c = (m.bounds_min + m.bounds_max) / 2.0;
    s.map_positions(
        |p| p - DVec3::new(c.x, c.y, m.bounds_min.z) + DVec3::new(x, -6.0, 0.0),
        false,
    );
    s
}

/// Which side a limb is on: its name suffix and the sign of x.
pub const SIDES: [(&str, f64); 2] = [("l", 1.0), ("r", -1.0)];

pub const SHOULDER_Z: f64 = 146.0;
pub const ELBOW_Z: f64 = 114.0;
pub const WRIST_Z: f64 = 84.0;
pub const HIP_Z: f64 = 90.0;
pub const KNEE_Z: f64 = 50.0;
pub const ANKLE_Z: f64 = 10.0;
pub const ARM_X: f64 = 26.0;
pub const LEG_X: f64 = 10.0;

/// The figure as one closed solid, every face named after its region.
pub fn body() -> Solid {
    let mut s = torso();
    let join = |a: &Solid, b: &Solid| boolean(a, b, &Boolean::default()).unwrap();
    s = join(&s, &ball(DVec3::new(0.0, 0.0, 170.0), 15.0, "head"));
    for (side, x) in SIDES {
        let ax = ARM_X * x;
        s = join(
            &s,
            &limb(
                DVec3::new(ax * 0.8, 0.0, SHOULDER_Z + 4.0),
                DVec3::new(ax, 0.0, ELBOW_Z - 3.0),
                6.0,
                &format!("upper_arm_{side}"),
            ),
        );
        let mut forearm = limb(
            DVec3::new(ax, 0.0, ELBOW_Z + 3.0),
            DVec3::new(ax, 0.0, WRIST_Z),
            5.0,
            &format!("forearm_{side}"),
        );
        forearm = join(
            &forearm,
            &ball(
                DVec3::new(ax, 0.0, WRIST_Z - 3.0),
                6.5,
                &format!("forearm_{side}"),
            ),
        );
        s = join(&s, &forearm);
        let lx = LEG_X * x;
        s = join(
            &s,
            &limb(
                DVec3::new(lx, 0.0, HIP_Z + 8.0),
                DVec3::new(lx, 0.0, KNEE_Z - 3.0),
                8.0,
                &format!("thigh_{side}"),
            ),
        );
        let shin = join(
            &limb(
                DVec3::new(lx, 0.0, KNEE_Z + 3.0),
                DVec3::new(lx, 0.0, ANKLE_Z - 4.0),
                6.5,
                &format!("shin_{side}"),
            ),
            &named(foot(lx), &format!("shin_{side}")),
        );
        s = join(&s, &shin);
    }
    s
}

fn bone(name: &str, parent: &str, at: DVec3, region: &str) -> RigBone {
    RigBone {
        name: name.into(),
        parent: parent.into(),
        pivot: Frame {
            origin: at,
            ..Frame::IDENTITY
        },
        regions: if region.is_empty() {
            Vec::new()
        } else {
            vec![region.into()]
        },
    }
}

/// The bones: the torso is the root; the head, arms and legs hang on it.
pub fn rig() -> Rig {
    let mut bones = vec![
        bone("torso", "", DVec3::new(0.0, 0.0, HIP_Z), "torso"),
        bone("head", "torso", DVec3::new(0.0, 0.0, 156.0), "head"),
    ];
    for (side, x) in SIDES {
        let ax = ARM_X * x;
        let lx = LEG_X * x;
        let upper = format!("upper_arm_{side}");
        let fore = format!("forearm_{side}");
        let thigh = format!("thigh_{side}");
        let shin = format!("shin_{side}");
        bones.push(bone(
            &upper,
            "torso",
            DVec3::new(ax * 0.8, 0.0, SHOULDER_Z),
            &upper,
        ));
        bones.push(bone(&fore, &upper, DVec3::new(ax, 0.0, ELBOW_Z), &fore));
        bones.push(bone(&thigh, "torso", DVec3::new(lx, 0.0, HIP_Z), &thigh));
        bones.push(bone(&shin, &thigh, DVec3::new(lx, 0.0, KNEE_Z), &shin));
    }
    Rig {
        bones,
        blend: BLEND,
        ..Default::default()
    }
}

fn turn(bone: &str, axis: DVec3, degrees: f64) -> Turn {
    Turn {
        bone: bone.into(),
        axis,
        angle: trig::to_radians(degrees),
    }
}

/// The acceptance pose: the left elbow bent 90° forward (the forearm
/// points −Y) and the right knee bent 90° back (the shin points +Y).
pub fn bend() -> Pose {
    Pose {
        turns: vec![
            turn("forearm_l", DVec3::X, -90.0),
            turn("shin_r", DVec3::X, 90.0),
        ],
    }
}

/// A livelier pose for the preview: the bend, plus a raised arm, a step,
/// and a turned head.
pub fn wave() -> Pose {
    let mut p = bend();
    p.turns.extend([
        turn("upper_arm_r", DVec3::Y, 100.0),
        turn("forearm_r", DVec3::Y, 60.0),
        turn("thigh_l", DVec3::X, -35.0),
        turn("shin_l", DVec3::X, 50.0),
        turn("head", DVec3::Z, 25.0),
    ]);
    p
}
