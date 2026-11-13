//! Writes a scene as URDF: a link a part, a joint where a part hangs,
//! each link's mesh as binary STL in the link's own frame
//! (`docs/research.md` §52).
//!
//! Entry: [`urdf`]. URDF is metres, radians and Z up, as the kernel is
//! but for the unit. A link's frame sits at its joint's pivot (a welded
//! part shares its parent's), the joint's axis is the pivot's, and the
//! parts stand as built, every joint at 0. What URDF asks for and a
//! scene does not hold comes from [`UrdfDesc`]: with a density, each
//! closed link gets the `inertial` of that much of an even solid
//! (mass, centre and inertia from its mesh); a limit's `effort` and
//! `velocity` are the desc's. The zero value writes no `inertial` and
//! 0 for both, for the author to set.

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
    /// Links a density was given for that got no `inertial`: they are
    /// not closed, so they have no volume to weigh.
    pub not_weighed: Vec<String>,
}

/// What a scene does not hold and URDF asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UrdfDesc {
    /// Kilograms a cubic metre every link is taken to be made of
    /// evenly (water 1 000, ABS 1 050, aluminium 2 700, steel 7 850);
    /// 0 writes no `inertial`.
    pub density: f64,
    /// Every moving joint's limit of effort: newtons for a slider,
    /// newton metres for a hinge.
    pub effort: f64,
    /// Every moving joint's limit of velocity: metres a second for a
    /// slider, radians a second for a hinge.
    pub velocity: f64,
}

/// A closed mesh's volume (m³), centre of mass (m) and inertia about
/// that centre for a density of 1 (xx, xy, xz, yy, yz, zz), by summing
/// over the tetrahedra its triangles make with the origin.
fn mass_properties(points: &[DVec3], triangles: &[[u32; 3]]) -> (f64, DVec3, [f64; 6]) {
    let mut volume = 0.0;
    let mut first = DVec3::ZERO;
    // ∫x², ∫y², ∫z², ∫xy, ∫xz, ∫yz.
    let mut second = [0.0f64; 6];
    for t in triangles {
        let [a, b, c] = t.map(|v| points[v as usize]);
        let det = a.dot(b.cross(c));
        volume += det / 6.0;
        first += (a + b + c) * (det / 24.0);
        let sq = |f: fn(DVec3) -> f64| {
            let (x, y, z) = (f(a), f(b), f(c));
            det / 60.0 * (x * x + y * y + z * z + x * y + x * z + y * z)
        };
        let mixed = |f: fn(DVec3) -> f64, g: fn(DVec3) -> f64| {
            let (x, y, z) = (f(a), f(b), f(c));
            let (u, v, w) = (g(a), g(b), g(c));
            det / 120.0
                * (2.0 * (x * u + y * v + z * w) + x * v + y * u + x * w + z * u + y * w + z * v)
        };
        second[0] += sq(|p| p.x);
        second[1] += sq(|p| p.y);
        second[2] += sq(|p| p.z);
        second[3] += mixed(|p| p.x, |p| p.y);
        second[4] += mixed(|p| p.x, |p| p.z);
        second[5] += mixed(|p| p.y, |p| p.z);
    }
    if volume <= 0.0 {
        return (0.0, DVec3::ZERO, [0.0; 6]);
    }
    let c = first / volume;
    // About the origin, then moved to the centre (parallel axes).
    let [xx, yy, zz, xy, xz, yz] = second;
    (
        volume,
        c,
        [
            yy + zz - volume * (c.y * c.y + c.z * c.z),
            -(xy - volume * c.x * c.y),
            -(xz - volume * c.x * c.z),
            xx + zz - volume * (c.x * c.x + c.z * c.z),
            -(yz - volume * c.y * c.z),
            xx + yy - volume * (c.x * c.x + c.y * c.y),
        ],
    )
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

/// A number to nine significant figures (masses and inertias span
/// many decades; nine decimals would lose a small link's).
fn g(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return "0".into();
    }
    let s = format!("{v:.8e}");
    // Back through a parse: the shortest text of that rounded value.
    s.parse::<f64>().map_or(s, |r| format!("{r}"))
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

/// `scene` as URDF, its meshes named `<prefix>_<part>.stl`; no
/// `inertial`, limits of effort and velocity 0 ([`urdf_with`] with the
/// zero desc).
pub fn urdf(scene: &Scene, prefix: &str) -> Result<Urdf, ExportError> {
    urdf_with(scene, prefix, &UrdfDesc::default())
}

/// `scene` as URDF, its meshes named `<prefix>_<part>.stl`, with what
/// `d` adds. A link that is not closed gets no `inertial` (it has no
/// volume to weigh), and [`Urdf::not_weighed`] names it.
pub fn urdf_with(scene: &Scene, prefix: &str, d: &UrdfDesc) -> Result<Urdf, ExportError> {
    for (v, what) in [
        (d.density, "density"),
        (d.effort, "effort"),
        (d.velocity, "velocity"),
    ] {
        if !(v >= 0.0 && v.is_finite()) {
            return Err(ExportError::NotFinite(format!("the URDF's {what}")));
        }
    }
    let checked = check_joints(scene)?;
    let mut not_weighed = Vec::new();
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
            let mut inertial = String::new();
            if d.density > 0.0 {
                let (volume, centre, i) = mass_properties(&points, &p.solid.triangles);
                if volume > 0.0 && p.solid.measure().closed {
                    let k = d.density;
                    inertial = format!(
                        "    <inertial>\n      <origin xyz=\"{}\" rpy=\"0 0 0\"/>\n      <mass value=\"{}\"/>\n      <inertia ixx=\"{}\" ixy=\"{}\" ixz=\"{}\" iyy=\"{}\" iyz=\"{}\" izz=\"{}\"/>\n    </inertial>\n",
                        xyz(centre),
                        g(volume * k),
                        g(i[0] * k),
                        g(i[1] * k),
                        g(i[2] * k),
                        g(i[3] * k),
                        g(i[4] * k),
                        g(i[5] * k),
                    );
                } else {
                    not_weighed.push(p.name.clone());
                }
            }
            links.push_str(&format!(
                "  <link name=\"{0}\">\n{2}    <visual>{1}</visual>\n    <collision>{1}</collision>\n  </link>\n",
                escaped(&p.name),
                geometry,
                inertial
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
                        "    <limit lower=\"{}\" upper=\"{}\" effort=\"{}\" velocity=\"{}\"/>\n",
                        n(lo),
                        n(hi),
                        n(d.effort),
                        n(d.velocity)
                    ));
                }
                ("continuous", _) => {
                    body.push_str(&format!(
                        "    <limit effort=\"{}\" velocity=\"{}\"/>\n",
                        n(d.effort),
                        n(d.velocity)
                    ));
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
        not_weighed,
    })
}
