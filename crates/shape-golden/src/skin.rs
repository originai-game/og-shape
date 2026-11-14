//! Golden cases for skins (shape-mesh's skin, shape-skin, and the GLB
//! skin in shape-tools): weights after welding, weights from a rig with
//! refined blend bands, a posed mesh with its measures, the GLB, and a
//! figure rigged from its mesh alone (joints found, bone heat).

use shape_bool::{Boolean, boolean};
use shape_core::{DVec3, Frame, HashError, OutputHash, trig};
use shape_gen::solids::{BoxDesc, CylinderDesc, cuboid, cylinder, sphere};
use shape_mesh::{Bone, Skin, Solid, Weights};
use shape_skin::{
    Pose, Rig, RigBone, Turn, bind, humanoid, measure_pose, pose, retarget, skin, skin_matrices,
};
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
            name: "skin.humanoid",
            run: rigged_figure,
        },
        Case {
            name: "skin.retarget",
            run: retargeted,
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

/// A figure in a T pose from closed pieces that overlap (the one
/// `shape-skin`'s tests rig), its joints found and its weights by bone
/// heat: the joints, then the skinned solid.
fn rigged_figure(h: &mut OutputHash) -> Result<(), HashError> {
    let nan = |_| HashError::NaN;
    let place = |mut s: Solid, f: &dyn Fn(DVec3) -> DVec3| {
        s.map_positions(f, false);
        s
    };
    let cyl = |r: f64, len: f64| {
        cylinder(&CylinderDesc {
            radius: r,
            height: len,
            ..Default::default()
        })
    };
    let block = |w: f64, d: f64, h: f64| {
        cuboid(&BoxDesc {
            width: w,
            depth: d,
            height: h,
        })
    };
    let mut s = place(block(380.0, 220.0, 600.0).map_err(nan)?, &|p| {
        p + DVec3::Z * 850.0
    });
    s.append(&place(cyl(55.0, 140.0).map_err(nan)?, &|p| {
        p + DVec3::Z * 1430.0
    }));
    s.append(&place(sphere(120.0).map_err(nan)?, &|p| {
        p + DVec3::Z * 1650.0
    }));
    for side in [1.0, -1.0] {
        s.append(&place(cyl(70.0, 840.0).map_err(nan)?, &|p| {
            p + DVec3::new(110.0 * side, 0.0, 70.0)
        }));
        s.append(&place(block(130.0, 260.0, 90.0).map_err(nan)?, &|p| {
            p + DVec3::new(110.0 * side, -50.0, 0.0)
        }));
        s.append(&place(cyl(50.0, 680.0).map_err(nan)?, &|p| {
            DVec3::new(side * (p.z + 170.0), p.y, 1390.0 - side * p.x)
        }));
        s.append(&place(block(150.0, 60.0, 110.0).map_err(nan)?, &|p| {
            DVec3::new(side * (p.x + 925.0), p.y, p.z + 1335.0)
        }));
    }
    s.canonicalize()?;
    let (sk, _) = humanoid(&s).map_err(|_| HashError::NaN)?;
    for j in &sk.joints {
        h.str(&j.name);
        h.str(&j.parent);
        h.dvec3s(&[j.at])?;
    }
    let (rigged, _) = bind(&s, &sk).map_err(|_| HashError::NaN)?;
    rigged.hash_into(h)
}

/// A T pose's clip (an elbow bent, a shoulder turned) moved onto an A
/// pose's arm of other lengths: the clip's matrices.
fn retargeted(h: &mut OutputHash) -> Result<(), HashError> {
    let nan = |_| HashError::NaN;
    let chain = |arm: DVec3, length: f64, prefix: &str| Skin {
        bones: ["Hips", "LeftArm", "LeftForeArm", "LeftHand"]
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let s = DVec3::new(0.0, 0.0, 100.0);
                let at = [
                    DVec3::new(0.0, 0.0, 60.0),
                    s,
                    s + arm * length,
                    s + arm * (2.0 * length),
                ][i];
                Bone::new(
                    &format!("{prefix}{n}"),
                    (i > 0).then(|| i as u32 - 1),
                    Frame {
                        origin: at,
                        ..Frame::IDENTITY
                    },
                )
            })
            .collect(),
        weights: Vec::new(),
    };
    let source = chain(DVec3::X, 50.0, "mixamorig:");
    let key = |turns: Vec<Turn>| skin_matrices(&source, &Pose { turns }).map_err(nan);
    let clip = shape_mesh::SkinClip {
        name: "bend".into(),
        times: vec![0.0, 1.0],
        poses: vec![
            key(Vec::new())?,
            key(vec![
                Turn {
                    bone: "mixamorig:LeftForeArm".into(),
                    axis: DVec3::new(0.0, -1.0, 0.0),
                    angle: 1.2,
                },
                Turn {
                    bone: "mixamorig:LeftArm".into(),
                    axis: DVec3::new(0.3, 0.2, 1.0),
                    angle: 0.7,
                },
            ])?,
        ],
        morphs: Vec::new(),
    };
    let target = chain(DVec3::new(0.8, -0.1, -0.6) / 1.005, 37.0, "");
    let (moved, _) = retarget(&source, &clip, &target, &Default::default()).map_err(nan)?;
    for key in &moved.poses {
        for m in key {
            h.f64s(m)?;
        }
    }
    Ok(())
}
