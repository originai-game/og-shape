//! Writes a scene as URDF: a link a part, a joint where a part hangs,
//! each link's mesh as binary STL in the link's own frame
//! (`docs/research.md` §52).
//!
//! Entry: [`urdf`]. URDF is metres, radians and Z up, as the kernel is
//! but for the unit. A link's frame sits at its joint's pivot (a welded
//! part shares its parent's), the joint's axis is the pivot's, and the
//! parts stand as built, every joint at 0. What URDF asks for and a
//! scene does not hold is not invented: no `inertial`, and a limit's
//! `effort` and `velocity` are written 0 for the author to set.

use std::collections::BTreeMap;

use shape_core::{DVec3, trig};

use crate::export::{ExportError, Rigid, Scene, check_joints};
use crate::surface::METRES;

/// A scene as URDF.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Urdf {
    /// The robot description.
    pub xml: String,
    /// Each link's mesh: the file name the description gives it, and
    /// the STL's bytes. They go beside the description.
    pub meshes: Vec<(String, Vec<u8>)>,
}

fn escaped(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A number to nine decimals, without the noise past them.
fn n(v: f64) -> String {
    let r = (v * 1e9).round() / 1e9;
    format!("{}", if r == 0.0 { 0.0 } else { r })
}

fn xyz(v: DVec3) -> String {
    format!("{} {} {}", n(v.x), n(v.y), n(v.z))
}

/// A rotation as URDF's roll, pitch and yaw (about fixed x, y, z).
fn rpy(r: &Rigid) -> DVec3 {
    let flat = (r.x.x * r.x.x + r.x.y * r.x.y).sqrt();
    if flat < 1e-12 {
        // Pitch at a quarter turn: yaw is taken as 0.
        return DVec3::new(trig::atan2(-r.z.y, r.y.y), trig::atan2(-r.x.z, flat), 0.0);
    }
    DVec3::new(
        trig::atan2(r.y.z, r.z.z),
        trig::atan2(-r.x.z, flat),
        trig::atan2(r.x.y, r.x.x),
    )
}

fn origin(r: &Rigid) -> String {
    format!(
        "<origin xyz=\"{}\" rpy=\"{}\"/>",
        xyz(r.t * METRES),
        xyz(rpy(r))
    )
}

/// A file name's worth of a part's name.
fn file_name(prefix: &str, part: &str) -> String {
    let safe: String = part
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{prefix}_{safe}.stl")
}

fn stl(name: &str, points: &[DVec3], triangles: &[[u32; 3]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(84 + 50 * triangles.len());
    let mut header = format!("{name} - written by og-shape, metres, the link's frame").into_bytes();
    header.truncate(80);
    header.resize(80, b' ');
    out.extend_from_slice(&header);
    out.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
    for t in triangles {
        let [a, b, c] = t.map(|v| points[v as usize]);
        let normal = (b - a).cross(c - a);
        let length = normal.length();
        let normal = if length > 0.0 {
            normal / length
        } else {
            DVec3::ZERO
        };
        for v in [normal, a, b, c] {
            for x in [v.x, v.y, v.z] {
                out.extend_from_slice(&(x as f32).to_le_bytes());
            }
        }
        out.extend_from_slice(&0u16.to_le_bytes());
    }
    out
}

/// `scene` as URDF, its meshes named `<prefix>_<part>.stl`.
pub fn urdf(scene: &Scene, prefix: &str) -> Result<Urdf, ExportError> {
    let checked = check_joints(scene)?;
    // Each part's link frame, in the world.
    let mut frames: BTreeMap<&str, Rigid> = BTreeMap::new();
    let roots: Vec<&str> = scene
        .parts
        .iter()
        .filter(|p| p.parent.is_empty())
        .map(|p| p.name.as_str())
        .collect();
    // URDF has one root: several unparented parts hang on a link of
    // the scene's name.
    let base = (roots.len() != 1).then(|| {
        let mut name = if scene.name.is_empty() {
            "base".to_string()
        } else {
            scene.name.clone()
        };
        while scene.parts.iter().any(|p| p.name == name) {
            name.push('_');
        }
        name
    });
    let mut links = String::new();
    let mut joints = String::new();
    let mut meshes = Vec::new();
    if let Some(b) = &base {
        links.push_str(&format!("  <link name=\"{}\"/>\n", escaped(b)));
    }
    for (p, (c, _)) in scene.parts.iter().zip(&checked) {
        let above = frames
            .get(p.parent.as_str())
            .copied()
            .unwrap_or(Rigid::IDENTITY);
        let frame = c.as_ref().map_or(above, |c| c.pivot);
        frames.insert(p.name.as_str(), frame);
        let back = frame.inverse();
        let points: Vec<DVec3> = p
            .solid
            .positions
            .iter()
            .map(|&v| back.apply(v) * METRES)
            .collect();
        if p.solid.triangles.is_empty() {
            // A group: a link with nothing to draw.
            links.push_str(&format!("  <link name=\"{}\"/>\n", escaped(&p.name)));
        } else {
            let file = file_name(prefix, &p.name);
            let geometry = format!(
                "<geometry><mesh filename=\"{}\"/></geometry>",
                escaped(&file)
            );
            links.push_str(&format!(
                "  <link name=\"{0}\">\n    <visual>{1}</visual>\n    <collision>{1}</collision>\n  </link>\n",
                escaped(&p.name),
                geometry
            ));
            meshes.push((file, stl(&p.name, &points, &p.solid.triangles)));
        }
        let parent = if p.parent.is_empty() {
            base.as_deref()
        } else {
            Some(p.parent.as_str())
        };
        let Some(parent) = parent else {
            continue;
        };
        let place = above.inverse().then(&frame);
        let (name, kind) = match c {
            Some(c) => (c.name.clone(), c.kind),
            None => (format!("{}_weld", p.name), "fixed"),
        };
        let mut body = format!(
            "    <parent link=\"{}\"/>\n    <child link=\"{}\"/>\n    {}\n",
            escaped(parent),
            escaped(&p.name),
            origin(&place)
        );
        if let Some(c) = c {
            if let Some(axis) = c.local {
                body.push_str(&format!("    <axis xyz=\"{}\"/>\n", xyz(axis)));
            }
            match (c.kind, c.span) {
                ("revolute" | "prismatic", span) => {
                    let (lo, hi) = span.unwrap_or((0.0, 0.0));
                    body.push_str(&format!(
                        "    <limit lower=\"{}\" upper=\"{}\" effort=\"0\" velocity=\"0\"/>\n",
                        n(lo),
                        n(hi)
                    ));
                }
                ("continuous", _) => {
                    body.push_str("    <limit effort=\"0\" velocity=\"0\"/>\n");
                }
                _ => {}
            }
            if let Some((leader, multiplier)) = &c.mimic {
                body.push_str(&format!(
                    "    <mimic joint=\"{}\" multiplier=\"{}\"/>\n",
                    escaped(leader),
                    n(*multiplier)
                ));
            }
        }
        joints.push_str(&format!(
            "  <joint name=\"{}\" type=\"{kind}\">\n{body}  </joint>\n",
            escaped(&name)
        ));
    }
    let robot = if scene.name.is_empty() {
        "robot"
    } else {
        &scene.name
    };
    Ok(Urdf {
        xml: format!(
            "<?xml version=\"1.0\"?>\n<!-- written by og-shape: metres, radians, Z up -->\n<robot name=\"{}\">\n{links}{joints}</robot>\n",
            escaped(robot)
        ),
        meshes,
    })
}
