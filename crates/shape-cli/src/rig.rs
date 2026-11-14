//! `shape rig <mesh> [-o out.glb] [--skeleton joints.json] [--check-clip]`:
//! a skin for a mesh that has none (`shape_skin::bind`). The file's parts
//! are joined; the skeleton is the one given, or a humanoid's found from
//! the mesh (`shape_skin::humanoid`: standing up +Z, facing −Y as read,
//! in a T or A pose; `--up` and `--front` name the file's own axes).
//!
//! The report (JSON) gives the skeleton in the form `--skeleton` reads,
//! so a joint can be moved and the mesh bound again; what the finder
//! measured (`fit`, null for a given skeleton); what the binding measured
//! (`bind`); and how two check poses deform the mesh (`check`). With
//! `--check-clip` the GLB also carries a clip `rig_check` through those
//! poses, for `shape preview out.glb --clip rig_check --frames 4`.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use shape_core::DVec3;
use shape_mesh::{Skin, SkinClip, Solid};
use shape_skin::{
    BindReport, FitReport, Joint, Pose, PoseMeasures, Skeleton, Turn, bind, humanoid, measure_pose,
    skin_matrices,
};
use shape_tools::Import;

use crate::args::{Args, IMPORT_OPTIONS};
use crate::report::object;
use crate::source::{Input, Local, load_mesh};
use crate::{Failure, Files, failed};

/// What to rig. The zero value has no input.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RigDesc {
    /// The mesh file.
    pub input: Input,
    /// How to read it: its unit and axes.
    pub import: Import,
    /// The skeleton; `None` finds a humanoid's.
    pub skeleton: Option<Skeleton>,
    /// Write the clip `rig_check` with the skin.
    pub check_clip: bool,
    /// Where to write the rigged mesh, if anywhere.
    pub out: Option<PathBuf>,
}

/// A skeleton from its JSON: `{"joints": [{"name", "parent", "at":
/// [x, y, z]}]}`, in mm, parents first; `parent` "" or absent for a root.
pub fn skeleton(text: &str) -> Result<Skeleton, Failure> {
    let bad = |why: &str| Failure::Usage(format!("skeleton: {why}"));
    let v: Value = serde_json::from_str(text).map_err(|e| bad(&e.to_string()))?;
    let joints = v
        .get("joints")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("needs \"joints\": a list"))?;
    let mut out = Vec::with_capacity(joints.len());
    for (k, j) in joints.iter().enumerate() {
        let name = j
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| bad(&format!("joint {k} needs a \"name\"")))?;
        let parent = match j.get("parent") {
            None | Some(Value::Null) => "",
            Some(p) => p
                .as_str()
                .ok_or_else(|| bad(&format!("joint {name:?}: \"parent\" is a name")))?,
        };
        let at = j
            .get("at")
            .and_then(Value::as_array)
            .filter(|a| a.len() == 3)
            .and_then(|a| a.iter().map(Value::as_f64).collect::<Option<Vec<f64>>>())
            .ok_or_else(|| bad(&format!("joint {name:?} needs \"at\": [x, y, z] in mm")))?;
        out.push(Joint {
            name: name.into(),
            parent: parent.into(),
            at: DVec3::new(at[0], at[1], at[2]),
        });
    }
    Ok(Skeleton { joints: out })
}

fn point(p: DVec3) -> Value {
    json!([p.x, p.y, p.z])
}

fn skeleton_json(s: &Skeleton) -> Value {
    json!({ "joints": s.joints.iter().map(|j| json!({
        "name": j.name,
        "parent": j.parent,
        "at": point(j.at),
    })).collect::<Vec<_>>() })
}

fn fit_json(r: &FitReport) -> Value {
    json!({
        "voxel": r.voxel,
        "height": r.height,
        "voxels": r.voxels,
        "kept": r.kept,
        "limbs": r.limbs.iter().map(|l| json!({
            "name": l.name,
            "tip": point(l.tip),
            "length": l.length,
            "radius": l.radius,
        })).collect::<Vec<_>>(),
        "outside": r.outside,
        "arm_drop": r.arm_drop,
    })
}

fn bind_json(r: &BindReport, sk: &Skeleton) -> Value {
    let by_bone: serde_json::Map<String, Value> = sk
        .joints
        .iter()
        .zip(&r.vertices_by_bone)
        .filter(|(_, n)| **n > 0)
        .map(|(j, n)| (j.name.clone(), json!(n)))
        .collect();
    json!({
        "vertices": r.vertices,
        "weighted_bones": r.weighted_bones,
        "unseen": r.unseen,
        "pieces": r.pieces,
        "pieces_unseen": r.pieces_unseen,
        "vertices_by_bone": by_bone,
        "iterations": r.iterations,
        "residual": r.residual,
    })
}

fn pose_json(m: &PoseMeasures) -> Value {
    json!({
        "inverted": m.inverted,
        "inverted_area": m.inverted_area,
        "collapsed": m.collapsed,
        "max_turn": m.max_turn,
        "min_area_ratio": m.min_area_ratio,
        "max_area_ratio": m.max_area_ratio,
        "volume_ratio": m.volume_ratio,
    })
}

/// The check poses, for the bones of a humanoid's skeleton that the skin
/// has (the bones' axes are the solid's: +X left, −Y front, +Z up):
/// arms down to 70° below the horizontal with elbows bent forward, the
/// left knee up, the spine turned and the head nodded; then the arms
/// raised to 35° above the horizontal and the right knee up. The arms
/// turn from where they rest, so a T and an A pose reach the same.
fn check_poses(skin: &Skin) -> Vec<(&'static str, Pose)> {
    let turn = |bone: &str, axis: DVec3, angle: f64| Turn {
        bone: bone.into(),
        axis,
        angle,
    };
    // Each arm's angle below the horizontal at rest, shoulder to wrist.
    let drop = |arm: &str, hand: &str| {
        let at = |b: &str| skin.bone(b).map(|i| skin.bones[i as usize].rest.origin);
        match (at(arm), at(hand)) {
            (Some(a), Some(h)) => {
                let d = h - a;
                shape_core::trig::atan2(-d.z, (d.x * d.x + d.y * d.y).sqrt())
            }
            _ => 0.0,
        }
    };
    let (left, right) = (drop("LeftArm", "LeftHand"), drop("RightArm", "RightHand"));
    let (down, up) = (70f64.to_radians(), -(35f64.to_radians()));
    let keep = |turns: Vec<Turn>| Pose {
        turns: turns
            .into_iter()
            .filter(|t| skin.bone(&t.bone).is_some())
            .collect(),
    };
    vec![
        (
            "down",
            keep(vec![
                turn("LeftArm", DVec3::Y, down - left),
                turn("RightArm", DVec3::Y, right - down),
                turn("LeftForeArm", DVec3::Z, -1.0),
                turn("RightForeArm", DVec3::Z, 1.0),
                turn("LeftUpLeg", DVec3::X, -1.2),
                turn("LeftLeg", DVec3::X, 1.5),
                turn("Spine1", DVec3::Z, 0.4),
                turn("Neck", DVec3::X, 0.3),
            ]),
        ),
        (
            "up",
            keep(vec![
                turn("LeftArm", DVec3::Y, up - left),
                turn("RightArm", DVec3::Y, right - up),
                turn("RightUpLeg", DVec3::X, -1.2),
                turn("RightLeg", DVec3::X, 1.5),
                turn("Spine", DVec3::X, -0.3),
            ]),
        ),
    ]
}

/// The clip through the check poses: rest, `down`, rest, `up`, rest, a
/// second each.
fn check_clip(skin: &Skin, poses: &[(&str, Pose)]) -> Result<SkinClip, Failure> {
    let rest = skin_matrices(skin, &Pose::default()).map_err(failed)?;
    let mut keys = vec![rest.clone()];
    for (_, p) in poses {
        keys.push(skin_matrices(skin, p).map_err(failed)?);
        keys.push(rest.clone());
    }
    Ok(SkinClip {
        name: "rig_check".into(),
        times: (0..keys.len()).map(|k| k as f64).collect(),
        poses: keys,
        morphs: Vec::new(),
    })
}

/// The rig report: `skeleton`, `fit`, `bind`, `check`, `output`.
pub fn rig(files: &dyn Files, d: &RigDesc) -> Result<String, Failure> {
    let mut mesh = load_mesh(files, &d.input, &d.import)?;
    let joined: Solid = mesh.take_joined()?;
    if joined.skin.is_some() {
        return Err(Failure::Failed(
            "the mesh already has a skin; rig takes a mesh without one".into(),
        ));
    }
    let (sk, fit) = match &d.skeleton {
        Some(s) => (s.clone(), Value::Null),
        None => {
            let (s, r) = humanoid(&joined).map_err(failed)?;
            let fit = fit_json(&r);
            (s, fit)
        }
    };
    let (rigged, report) = bind(&joined, &sk).map_err(failed)?;
    let skin = rigged.skin.as_ref().expect("bind gives a skin");
    let poses = check_poses(skin);
    let mut check = serde_json::Map::new();
    for (name, p) in &poses {
        if !p.turns.is_empty() {
            let m = measure_pose(&rigged, p).map_err(failed)?;
            check.insert((*name).into(), pose_json(&m));
        }
    }
    let clips = if d.check_clip && check.len() == poses.len() {
        vec![check_clip(skin, &poses)?]
    } else {
        Vec::new()
    };
    let (material, _, _) = mesh.material();
    let output = crate::export::write_solid_clips(
        files,
        &mesh.name,
        &rigged,
        Some(&material),
        &clips,
        d.out.as_deref(),
    )?;
    Ok(object(&[
        ("skeleton", skeleton_json(&sk).to_string()),
        ("fit", fit.to_string()),
        ("bind", bind_json(&report, &sk).to_string()),
        ("check", Value::Object(check).to_string()),
        ("output", output),
    ]))
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    let mut known = vec!["-o", "--out", "--skeleton", "--check-clip"];
    known.extend(IMPORT_OPTIONS);
    a.only(&known)?;
    let skeleton = match a.value(&["--skeleton"])? {
        None => None,
        Some(path) => {
            let bytes = Local.read(Path::new(path))?;
            Some(skeleton(&String::from_utf8_lossy(&bytes))?)
        }
    };
    rig(
        &Local,
        &RigDesc {
            input: a.input()?,
            import: a.import()?,
            skeleton,
            check_clip: a.switch("--check-clip"),
            out: a.out_opt()?,
        },
    )
}
