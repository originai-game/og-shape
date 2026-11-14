//! A clip made for one skeleton moved onto another (retargeting).
//!
//! Bones are matched by name, after a prefix up to the last `:` and a
//! leading `mixamorig` are taken off (`mixamorig:LeftArm` and
//! `mixamorigLeftArm` are `LeftArm`; Tripo's biped names, such as
//! `L_Upperarm`, are read as Mixamo's), and by any pairs given. Each key:
//! - A matched target bone turns in the world as its source bone does
//!   from its rest (the rotation part of the source's skinning matrix),
//!   after its rest direction is first swung onto the source bone's rest
//!   direction (toward the first child that matches), so a T pose's
//!   clip moves an A pose's arms through the same directions.
//! - A target bone without a match turns with its parent.
//! - The roots move as the source's matched root does, scaled by the
//!   ratio of the two roots' rest heights.
//! - A source skeleton that faces another way than the rig (a quarter
//!   turn or more about +Z, judged from the matched bones' bind places)
//!   is first turned by the nearest quarter turn, its clip with it.
//! - Every bone keeps its own length: a child sits where its parent's
//!   posed frame carries its rest offset.
//!
//! Only `+ − × /` and `sqrt`: the same bits on every host.

use std::collections::BTreeMap;

use shape_core::DVec3;
use shape_mesh::skin::{Mat4, affine_inverse};
use shape_mesh::{Skin, SkinClip};

use crate::mat::{apply, mul};
use crate::rig::RigError;

/// How to retarget. The zero value matches bones by name alone.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Retarget {
    /// Extra `(source bone, target bone)` pairs, by their full names;
    /// they win over a match by name.
    pub map: Vec<(String, String)>,
}

/// What [`retarget`] matched.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RetargetReport {
    /// `(source bone, target bone)` pairs used, in the target's order.
    pub matched: Vec<(String, String)>,
    /// Target bones with no source bone, which turn with their parents.
    pub unmatched: Vec<String>,
    /// Source bones no target bone took.
    pub unused: Vec<String>,
    /// The roots' rest heights, target over source: how the root's
    /// motion was scaled.
    pub scale: f64,
    /// The quarter turn about +Z, in degrees counter-clockwise (0, 90,
    /// 180 or 270), the clip was given so its skeleton faces the way the
    /// rig does.
    pub turn: u32,
}

/// The quarter turn about +Z that best carries the source's matched
/// bind origins onto the target's, each about its own centroid, seen
/// from above: in degrees, and its cosine and sine. The least-squares
/// angle has `cos ∝ Σ a·b` and `sin ∝ Σ a×b` (`a` source, `b` target,
/// horizontal parts); the quarter turn nearest it is picked by those
/// two sums alone, ties to the smaller turn, so a skeleton that faces
/// as the rig does is left exactly as it is.
fn facing(pairs: &[(DVec3, DVec3)]) -> (u32, f64, f64) {
    let n = pairs.len() as f64;
    if pairs.is_empty() {
        return (0, 1.0, 0.0);
    }
    let (ca, cb) = pairs
        .iter()
        .fold((DVec3::ZERO, DVec3::ZERO), |(x, y), (a, b)| {
            (x + *a, y + *b)
        });
    let (ca, cb) = (ca / n, cb / n);
    let (mut dot, mut cross) = (0.0, 0.0);
    for (a, b) in pairs {
        let (a, b) = (*a - ca, *b - cb);
        dot += a.x * b.x + a.y * b.y;
        cross += a.x * b.y - a.y * b.x;
    }
    if dot.abs() >= cross.abs() {
        if dot >= 0.0 {
            (0, 1.0, 0.0)
        } else {
            (180, -1.0, 0.0)
        }
    } else if cross > 0.0 {
        (90, 0.0, 1.0)
    } else {
        (270, 0.0, -1.0)
    }
}

/// Tripo's biped skeleton (the origin asset library's motion clips) by
/// Mixamo's names; its twist bones, `Root` and `Pelvis` have none.
const TRIPO: [(&str, &str); 22] = [
    ("Hip", "Hips"),
    ("Waist", "Spine"),
    ("Spine01", "Spine1"),
    ("Spine02", "Spine2"),
    ("NeckTwist01", "Neck"),
    ("L_Thigh", "LeftUpLeg"),
    ("L_Calf", "LeftLeg"),
    ("L_Foot", "LeftFoot"),
    ("L_ToeBase", "LeftToeBase"),
    ("R_Thigh", "RightUpLeg"),
    ("R_Calf", "RightLeg"),
    ("R_Foot", "RightFoot"),
    ("R_ToeBase", "RightToeBase"),
    ("L_Clavicle", "LeftShoulder"),
    ("L_Upperarm", "LeftArm"),
    ("L_Forearm", "LeftForeArm"),
    ("L_Hand", "LeftHand"),
    ("R_Clavicle", "RightShoulder"),
    ("R_Upperarm", "RightArm"),
    ("R_Forearm", "RightForeArm"),
    ("R_Hand", "RightHand"),
    ("Head", "Head"),
];

/// A bone's name without a namespace or Mixamo's prefix, and Tripo's
/// biped names as Mixamo's.
fn plain(name: &str) -> &str {
    let n = name.rsplit(':').next().unwrap_or(name);
    let n = n.strip_prefix("mixamorig").unwrap_or(n);
    TRIPO.iter().find(|(t, _)| *t == n).map_or(n, |(_, m)| m)
}

/// The 3×3 part of `m`, made orthonormal (Gram–Schmidt on its columns),
/// with no translation.
fn rotation(m: &Mat4) -> Mat4 {
    let unit = |v: DVec3| {
        let l = v.length();
        if l > 0.0 { v / l } else { v }
    };
    let x = unit(DVec3::new(m[0], m[1], m[2]));
    let y0 = DVec3::new(m[4], m[5], m[6]);
    let y = unit(y0 - x * x.dot(y0));
    let z = x.cross(y);
    [
        x.x, x.y, x.z, 0.0, //
        y.x, y.y, y.z, 0.0, //
        z.x, z.y, z.z, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]
}

/// The shortest turn taking the direction `a` onto `b` (Rodrigues with
/// the cosine and the cross product: no angle is taken).
fn swing(a: DVec3, b: DVec3) -> Mat4 {
    let (la, lb) = (a.length(), b.length());
    if la == 0.0 || lb == 0.0 {
        return IDENTITY;
    }
    let (a, b) = (a / la, b / lb);
    let v = a.cross(b);
    let c = a.dot(b);
    if c <= -1.0 + 1e-12 {
        // Opposite: a half turn about any axis square to `a`.
        let k = if a.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
        let k = {
            let w = k - a * a.dot(k);
            w / w.length()
        };
        let r = |e: DVec3| k * (2.0 * k.dot(e)) - e;
        let (x, y, z) = (r(DVec3::X), r(DVec3::Y), r(DVec3::Z));
        return [
            x.x, x.y, x.z, 0.0, //
            y.x, y.y, y.z, 0.0, //
            z.x, z.y, z.z, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ];
    }
    let f = 1.0 / (1.0 + c);
    let r = |e: DVec3| e + v.cross(e) + v.cross(v.cross(e)) * f;
    let (x, y, z) = (r(DVec3::X), r(DVec3::Y), r(DVec3::Z));
    [
        x.x, x.y, x.z, 0.0, //
        y.x, y.y, y.z, 0.0, //
        z.x, z.y, z.z, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]
}

const IDENTITY: Mat4 = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn translation(p: DVec3) -> Mat4 {
    let mut m = IDENTITY;
    m[12] = p.x;
    m[13] = p.y;
    m[14] = p.z;
    m
}

/// Each bone's place at bind time: the inverse of its inverse bind
/// matrix's translation applied to the origin.
fn bind_origins(skin: &Skin) -> Vec<DVec3> {
    skin.bones
        .iter()
        .map(|b| {
            affine_inverse(&b.inverse_bind)
                .map(|m| DVec3::new(m[12], m[13], m[14]))
                .unwrap_or(b.rest.origin)
        })
        .collect()
}

/// The clip `clip` of the skin `source`, moved onto the skin `target`
/// (see the module docs), and what was matched.
pub fn retarget(
    source: &Skin,
    clip: &SkinClip,
    target: &Skin,
    d: &Retarget,
) -> Result<(SkinClip, RetargetReport), RigError> {
    if clip.poses.iter().any(|p| p.len() != source.bones.len()) {
        return Err(RigError::NotFinite(format!(
            "clip {:?}: its keys do not hold one matrix a source bone",
            clip.name
        )));
    }
    let nt = target.bones.len();
    // Target bone → source bone.
    let mut by_name: BTreeMap<&str, u32> = BTreeMap::new();
    for (i, b) in source.bones.iter().enumerate() {
        by_name.entry(plain(&b.name)).or_insert(i as u32);
    }
    let mut source_of: Vec<Option<u32>> = target
        .bones
        .iter()
        .map(|b| by_name.get(plain(&b.name)).copied())
        .collect();
    for (s, t) in &d.map {
        let si = source.bone(s).ok_or_else(|| {
            RigError::NoSuchBone(
                s.clone(),
                source.bones.iter().map(|b| b.name.clone()).collect(),
            )
        })?;
        let ti = target.bone(t).ok_or_else(|| {
            RigError::NoSuchBone(
                t.clone(),
                target.bones.iter().map(|b| b.name.clone()).collect(),
            )
        })?;
        source_of[ti as usize] = Some(si);
    }
    if source_of.iter().all(Option::is_none) {
        return Err(RigError::Humanoid(
            "no bone of the clip's skeleton matches the rig's by name; give pairs".into(),
        ));
    }
    let tgt_rest = bind_origins(target);
    let src_bind = bind_origins(source);
    // The source turned to face the rig's way: its bind origins, and
    // (below) each key conjugated by the same turn.
    let (turn_deg, c, s) = facing(
        &(0..nt)
            .filter_map(|i| source_of[i].map(|k| (src_bind[k as usize], tgt_rest[i])))
            .collect::<Vec<_>>(),
    );
    let face: Mat4 = [
        c, s, 0.0, 0.0, //
        -s, c, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ];
    let face_back: Mat4 = [
        c, -s, 0.0, 0.0, //
        s, c, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ];
    let src_rest: Vec<DVec3> = src_bind
        .into_iter()
        .map(|p| if turn_deg == 0 { p } else { apply(&face, p) })
        .collect();
    // Each matched target bone's swing from its rest direction onto its
    // source bone's, toward the first child that matches.
    let mut first_child: Vec<Option<usize>> = vec![None; nt];
    for (i, b) in target.bones.iter().enumerate() {
        if let Some(p) = b.parent
            && first_child[p as usize].is_none()
            && source_of[i].is_some()
        {
            first_child[p as usize] = Some(i);
        }
    }
    let swings: Vec<Mat4> = (0..nt)
        .map(|i| match (source_of[i], first_child[i]) {
            (Some(s), Some(c)) if target.bones[i].parent.is_some() => {
                let sc = source_of[c].expect("a matched child") as usize;
                swing(
                    tgt_rest[c] - tgt_rest[i],
                    src_rest[sc] - src_rest[s as usize],
                )
            }
            _ => IDENTITY,
        })
        .collect();
    // The roots' motion is scaled by their rest heights.
    let root = (0..nt).find(|&i| target.bones[i].parent.is_none());
    // Heights above each skeleton's lowest joint, so a mesh centred on
    // its origin scales as one standing on the ground.
    let floor = |o: &[DVec3]| o.iter().map(|p| p.z).fold(f64::INFINITY, f64::min);
    let (src_floor, tgt_floor) = (floor(&src_rest), floor(&tgt_rest));
    let scale = match root.and_then(|r| source_of[r].map(|s| (r, s as usize))) {
        Some((r, s)) if src_rest[s].z > src_floor && tgt_rest[r].z > tgt_floor => {
            (tgt_rest[r].z - tgt_floor) / (src_rest[s].z - src_floor)
        }
        _ => 1.0,
    };
    let mut poses = Vec::with_capacity(clip.poses.len());
    for key in &clip.poses {
        let turned: Vec<Mat4>;
        let key = if turn_deg == 0 {
            key
        } else {
            turned = key
                .iter()
                .map(|m| mul(&mul(&face, m), &face_back))
                .collect();
            &turned
        };
        let mut turn: Vec<Mat4> = Vec::with_capacity(nt);
        let mut at: Vec<DVec3> = Vec::with_capacity(nt);
        let mut out: Vec<Mat4> = Vec::with_capacity(nt);
        for i in 0..nt {
            let b = &target.bones[i];
            let r = match source_of[i] {
                Some(s) => mul(&rotation(&key[s as usize]), &swings[i]),
                None => b.parent.map_or(IDENTITY, |p| turn[p as usize]),
            };
            let p = match b.parent {
                Some(p) => {
                    let p = p as usize;
                    at[p] + apply(&turn[p], tgt_rest[i] - tgt_rest[p])
                }
                None => match source_of[i] {
                    Some(s) => {
                        let s = s as usize;
                        tgt_rest[i] + (apply(&key[s], src_rest[s]) - src_rest[s]) * scale
                    }
                    None => tgt_rest[i],
                },
            };
            // Skinning: from the bind pose about the rest place, turned,
            // then to where the bone is now.
            out.push(mul(&mul(&translation(p), &r), &translation(-tgt_rest[i])));
            turn.push(r);
            at.push(p);
        }
        poses.push(out);
    }
    let used: Vec<bool> = {
        let mut u = vec![false; source.bones.len()];
        for s in source_of.iter().flatten() {
            u[*s as usize] = true;
        }
        u
    };
    let report = RetargetReport {
        matched: (0..nt)
            .filter_map(|i| {
                source_of[i].map(|s| {
                    (
                        source.bones[s as usize].name.clone(),
                        target.bones[i].name.clone(),
                    )
                })
            })
            .collect(),
        unmatched: (0..nt)
            .filter(|&i| source_of[i].is_none())
            .map(|i| target.bones[i].name.clone())
            .collect(),
        unused: source
            .bones
            .iter()
            .zip(&used)
            .filter(|(_, u)| !**u)
            .map(|(b, _)| b.name.clone())
            .collect(),
        scale,
        turn: turn_deg,
    };
    Ok((
        SkinClip {
            name: clip.name.clone(),
            times: clip.times.clone(),
            poses,
            morphs: Vec::new(),
        },
        report,
    ))
}
