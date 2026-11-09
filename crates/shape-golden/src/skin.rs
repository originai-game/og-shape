//! Golden cases for skins (shape-mesh's skin, shape-skin, and the GLB
//! skin in shape-tools): weights after welding, weights from a rig with
//! refined blend bands, a posed mesh with its measures, and the GLB.

use shape_bool::{Boolean, boolean};
use shape_core::{DVec3, Frame, HashError, OutputHash, trig};
use shape_gen::solids::{CylinderDesc, cylinder};
use shape_mesh::{Bone, Skin, Solid, Weights};
use shape_skin::{Pose, Rig, RigBone, Turn, measure_pose, pose, skin};
use shape_tools::{Part, Scene, glb};

use crate::cases::Case;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "skin.weld",
            run: weld,
        },
        Case {
            name: "skin.weights",
            run: |h| skinned_arm()?.hash_into(h),
        },
        Case {
            name: "skin.pose",
            run: posed,
        },
        Case {
            name: "skin.glb",
            run: |h| {
                let s = skinned_arm()?;
                let bytes = glb(&Scene {
                    name: "arm".into(),
                    parts: vec![Part::new("arm", &s)],
                })
                .map_err(|_| HashError::NaN)?;
                h.bytes(&bytes);
                Ok(())
            },
        },
    ]
}

/// Two cylinders, each weighted to its own bone, appended so their
/// touching cap rings weld: the welded vertices take the mean.
fn weld(h: &mut OutputHash) -> Result<(), HashError> {
    let bones = vec![
        Bone::new("a", None, Frame::IDENTITY),
        Bone::new(
            "b",
            Some(0),
            Frame {
                origin: DVec3::Z * 10.0,
                ..Frame::IDENTITY
            },
        ),
    ];
    let part = |z: f64, bone: u32| -> Result<Solid, HashError> {
        let mut c = cylinder(&CylinderDesc {
            radius: 4.0,
            height: 10.0,
            ..Default::default()
        })
        .map_err(|_| HashError::NaN)?;
        c.map_positions(|p| p + DVec3::Z * z, false);
        c.skin = Some(Skin {
            bones: bones.clone(),
            weights: vec![Weights::single(bone); c.positions.len()],
        });
        Ok(c)
    };
    let mut s = part(0.0, 0)?;
    s.append(&part(10.0, 1)?);
    s.canonicalize()?;
    s.hash_into(h)
}

fn arm() -> Result<Solid, HashError> {
    let part = |r: f64, z0: f64, z1: f64, name: &str| -> Result<Solid, HashError> {
        let mut c = cylinder(&CylinderDesc {
            radius: r,
            height: z1 - z0,
            ..Default::default()
        })
        .map_err(|_| HashError::NaN)?;
        c.map_positions(|p| p + DVec3::Z * z0, false);
        c.tag_all_faces(name);
        Ok(c)
    };
    boolean(
        &part(10.0, 0.0, 60.0, "upper_arm")?,
        &part(9.0, 55.0, 120.0, "forearm")?,
        &Boolean::default(),
    )
    .map_err(|_| HashError::NaN)
}

fn skinned_arm() -> Result<Solid, HashError> {
    let rig = Rig {
        bones: vec![
            RigBone {
                name: "upper_arm".into(),
                regions: vec!["upper_arm".into()],
                ..Default::default()
            },
            RigBone {
                name: "forearm".into(),
                parent: "upper_arm".into(),
                pivot: Frame {
                    origin: DVec3::new(0.0, 0.0, 57.5),
                    ..Frame::IDENTITY
                },
                regions: vec!["forearm".into()],
            },
        ],
        blend: 40.0,
        ..Default::default()
    };
    skin(&arm()?, &rig).map_err(|_| HashError::NaN)
}

fn posed(h: &mut OutputHash) -> Result<(), HashError> {
    let s = skinned_arm()?;
    let p = Pose {
        turns: vec![
            Turn {
                bone: "forearm".into(),
                axis: DVec3::X,
                angle: trig::to_radians(90.0),
            },
            Turn {
                bone: "upper_arm".into(),
                axis: DVec3::new(0.0, 1.0, 1.0),
                angle: 0.3,
            },
        ],
    };
    let posed = pose(&s, &p).map_err(|_| HashError::NaN)?;
    posed.hash_into(h)?;
    let m = measure_pose(&s, &p).map_err(|_| HashError::NaN)?;
    h.u64(m.inverted as u64);
    h.u64(m.collapsed as u64);
    h.u64(u64::from(m.closed));
    h.f64s(&[
        m.max_turn,
        m.min_area_ratio,
        m.max_area_ratio,
        m.volume_ratio,
    ])
}
