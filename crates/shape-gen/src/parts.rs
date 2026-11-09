//! Parts that are tedious by hand (sand3d's `parts`): [`gear_solid`], a
//! spur gear with a bore and a hub, and [`knurl`], a knurled grip.
//!
//! | generator | faces | edges |
//! |---|---|---|
//! | [`gear_solid`] | `bottom`, `top`, `side`; the hub's wall `side` + `hub`, its top `top` + `hub`; the bore `side` + `inner` + `bore` | `bottom`, `top` (cap rims; `hub` too on the hub's), `side` (along tooth corners) |
//! | [`knurl`] | `bottom`, `top`, `side` | `bottom`, `top` |
//!
//! Neither needs a boolean: every cap is a region whose holes are given
//! directly, so the walls and the caps share their points exactly.

use shape_2d::curve::segments;
use shape_2d::{Contour, Gear, Profile, Region, gear};
use shape_core::{DVec2, DVec3, trig};
use shape_mesh::Solid;

use crate::solids::{GenError, finish, positive, runs};

/// A spur gear solid.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GearDesc {
    /// Number of teeth, at least 6.
    pub teeth: u32,
    /// Module: pitch diameter / teeth, in millimetres.
    pub module: f64,
    /// Pressure angle, radians; 0 for 20°.
    pub pressure_angle: f64,
    /// Thickness of the toothed part along +z, from z = 0.
    pub width: f64,
    /// Diameter of a round hole through gear and hub; 0 for none.
    pub bore: f64,
    /// Diameter of a round hub on the top face; 0 for none. Above the
    /// bore and below the root circle.
    pub hub: f64,
    /// Height of the hub above the top face; 0 for none.
    pub hub_height: f64,
}

/// A ring of `n` points on a circle of radius `r`, counter-clockwise from +x.
fn ring(r: f64) -> Contour {
    let n = segments(r, trig::TAU);
    Contour::smooth(
        (0..n)
            .map(|k| {
                let (s, c) = trig::sin_cos(trig::TAU * (k as f64 / n as f64));
                DVec2::new(r * c, r * s)
            })
            .collect(),
    )
}

/// Fills `outer` minus `hole` at height `z`, facing +z (`up`) or −z.
fn cap(
    s: &mut Solid,
    outer: &Contour,
    hole: Option<&Contour>,
    z: f64,
    up: bool,
    tags: &[&str],
) -> Result<(), GenError> {
    let region = Region {
        outer: outer.clone(),
        holes: hole.map(|h| h.reversed()).into_iter().collect(),
    };
    let fill = Profile {
        regions: vec![region],
    }
    .triangulate()?;
    let ids: Vec<u32> = fill
        .points
        .iter()
        .map(|p| s.push_vertex(p.extend(z)))
        .collect();
    let surface = s.surfaces.iter().max().map_or(0, |m| m + 1);
    for t in &fill.triangles {
        let t = if up { *t } else { [t[0], t[2], t[1]] };
        s.push_triangle(t.map(|i| ids[i as usize]), surface, tags);
    }
    Ok(())
}

/// The wall of a contour from `z0` to `z1`, one surface per run between
/// corners, the rims named `rim0` and `rim1`, sharp verticals `side`.
/// A counter-clockwise contour faces out, a clockwise one in.
fn wall(s: &mut Solid, c: &Contour, z0: f64, z1: f64, tags: &[&str], rims: [&[&str]; 2]) {
    let n = c.points.len();
    let lo: Vec<u32> = c
        .points
        .iter()
        .map(|p| s.push_vertex(p.extend(z0)))
        .collect();
    let hi: Vec<u32> = c
        .points
        .iter()
        .map(|p| s.push_vertex(p.extend(z1)))
        .collect();
    let first = s.surfaces.iter().max().map_or(0, |m| m + 1);
    for (surface, (a, b)) in (first..).zip(runs(c)) {
        for i in a..b {
            let (i0, i1) = (i % n, (i + 1) % n);
            s.push_triangle([lo[i0], lo[i1], hi[i1]], surface, tags);
            s.push_triangle([lo[i0], hi[i1], hi[i0]], surface, tags);
        }
    }
    for i in 0..n {
        let i1 = (i + 1) % n;
        s.tag_edge(lo[i], lo[i1], rims[0]);
        s.tag_edge(hi[i], hi[i1], rims[1]);
        if c.sharp[i] {
            s.tag_edge(lo[i], hi[i], &["side"]);
        }
    }
}

/// An involute spur gear along +z from z = 0, a tooth centred on +x (the
/// [`shape_2d::gear`] outline), with an optional bore through it and an
/// optional hub on its top face (sand3d's `gear(teeth, module, width,
/// bore)`, plus the hub).
pub fn gear_solid(d: &GearDesc) -> Result<Solid, GenError> {
    let w = positive(d.width, "width")?;
    let outline = gear(&Gear {
        teeth: d.teeth,
        module: d.module,
        pressure_angle: d.pressure_angle,
    })?;
    let outer = &outline.regions[0].outer;
    let root = d.module * d.teeth as f64 / 2.0 - 1.25 * d.module;
    let bore = if d.bore == 0.0 {
        None
    } else {
        Some(positive(d.bore, "bore")? / 2.0)
    };
    let hub = match (d.hub, d.hub_height) {
        (0.0, 0.0) => None,
        (hub, hh) => Some((positive(hub, "hub")? / 2.0, positive(hh, "hub_height")?)),
    };
    let inside = hub.map_or(root, |(r, _)| r);
    if let Some(b) = bore
        && b >= inside
    {
        return Err(GenError::Invalid(format!(
            "gear: the bore ({}) must be narrower than the {} ({})",
            2.0 * b,
            if hub.is_some() { "hub" } else { "root circle" },
            2.0 * inside
        )));
    }
    if let Some((r, _)) = hub
        && r >= root
    {
        return Err(GenError::Invalid(format!(
            "gear: the hub ({}) must be narrower than the root circle ({})",
            2.0 * r,
            2.0 * root
        )));
    }
    let bore_ring = bore.map(ring);
    let mut s = Solid::default();
    cap(&mut s, outer, bore_ring.as_ref(), 0.0, false, &["bottom"])?;
    wall(&mut s, outer, 0.0, w, &["side"], [&["bottom"], &["top"]]);
    let top = match hub {
        None => w,
        Some((r, hh)) => {
            let hub_ring = ring(r);
            cap(&mut s, outer, Some(&hub_ring), w, true, &["top"])?;
            wall(
                &mut s,
                &hub_ring,
                w,
                w + hh,
                &["side", "hub"],
                [&["top", "hub"], &["top", "hub"]],
            );
            cap(
                &mut s,
                &hub_ring,
                bore_ring.as_ref(),
                w + hh,
                true,
                &["top", "hub"],
            )?;
            w + hh
        }
    };
    if hub.is_none() {
        cap(&mut s, outer, bore_ring.as_ref(), w, true, &["top"])?;
    }
    if let Some(b) = &bore_ring {
        wall(
            &mut s,
            &b.reversed(),
            0.0,
            top,
            &["side", "inner", "bore"],
            [&["bottom"], &["top"]],
        );
    }
    finish(s)
}

/// A knurled grip.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Knurl {
    /// Diameter over the teeth.
    pub diameter: f64,
    /// Length along +z, from z = 0.
    pub height: f64,
    /// Distance between teeth; 0 for 1 mm.
    pub pitch: f64,
    /// Straight ridges along z instead of diamonds.
    pub straight: bool,
}

/// 1 at whole numbers, 0 half-way between: a V wave.
fn vee(f: f64) -> f64 {
    (2.0 * (f - f.floor()) - 1.0).abs()
}

/// sand3d's `knurl(d, h, pitch, straight)`: a surface of revolution
/// whose radius dips `pitch / 2` in a V between teeth. Diamonds are two
/// 30° helices crossing, the deeper of the two Vs at each point; straight
/// ridges are one V round. `π d / pitch` teeth (at least 6), four grid
/// points per tooth round; along z, four rows per helix lead (one row per
/// pitch for straight ridges). Each grid quad is split along the diagonal
/// nearer the surface at its centre (sand3d splits all one way, which
/// leaves one helix's creases across the triangles). Flat `bottom` and
/// `top`.
pub fn knurl(d: &Knurl) -> Result<Solid, GenError> {
    let dia = positive(d.diameter, "diameter")?;
    let h = positive(d.height, "height")?;
    let pitch = if d.pitch == 0.0 {
        1.0
    } else {
        positive(d.pitch, "pitch")?
    };
    if pitch >= dia {
        return Err(GenError::Invalid(format!(
            "knurl: the pitch ({pitch}) must be smaller than the diameter ({dia})"
        )));
    }
    let teeth = ((trig::PI * dia / pitch).round() as u32).max(6);
    let depth = 0.5 * pitch;
    let r = dia / 2.0;
    let tf = teeth as f64;
    let around = 4 * teeth;
    // Axial advance per tooth along a 30° helix.
    let lead = trig::PI * dia / trig::tan(trig::PI / 6.0) / tf;
    let along = if d.straight {
        ((h / pitch).round() as u32).max(1)
    } else {
        ((4.0 * h / lead).ceil() as u32).max(4)
    };
    // The radius at `t` teeth round and height z.
    let radius = |t: f64, z: f64| {
        if d.straight {
            r - depth * vee(t)
        } else {
            r - depth * vee(t + z / lead).max(vee(t - z / lead))
        }
    };
    let mut s = Solid::default();
    let rings: Vec<Vec<u32>> = (0..=along)
        .map(|j| {
            let z = h * (j as f64 / along as f64);
            (0..around)
                .map(|i| {
                    let (sn, cs) = trig::sin_cos(trig::TAU * (i as f64 / around as f64));
                    let rho = radius(i as f64 / 4.0, z);
                    s.push_vertex(DVec3::new(rho * cs, rho * sn, z))
                })
                .collect()
        })
        .collect();
    let n = around as usize;
    let z_of = |j: usize| h * (j as f64 / along as f64);
    for (j, w) in rings.windows(2).enumerate() {
        for i in 0..n {
            let i1 = (i + 1) % n;
            // Split each quad along the diagonal whose middle lies nearer
            // the surface at the quad's centre, so the creases of both
            // helices fall on edges.
            let (z0, z1) = (z_of(j), z_of(j + 1));
            let (t0, t1) = (i as f64 / 4.0, (i + 1) as f64 / 4.0);
            let mid = radius((t0 + t1) / 2.0, (z0 + z1) / 2.0);
            let rising = (radius(t0, z0) + radius(t1, z1)) / 2.0;
            let falling = (radius(t1, z0) + radius(t0, z1)) / 2.0;
            if (falling - mid).abs() < (rising - mid).abs() {
                s.push_triangle([w[0][i], w[0][i1], w[1][i]], 2, &["side"]);
                s.push_triangle([w[0][i1], w[1][i1], w[1][i]], 2, &["side"]);
            } else {
                s.push_triangle([w[0][i], w[0][i1], w[1][i1]], 2, &["side"]);
                s.push_triangle([w[0][i], w[1][i1], w[1][i]], 2, &["side"]);
            }
        }
    }
    for (ring, name, surface, up) in [
        (&rings[0], "bottom", 0, false),
        (&rings[along as usize], "top", 1, true),
    ] {
        let flat = Contour::smooth(
            ring.iter()
                .map(|&v| s.positions[v as usize].truncate())
                .collect(),
        );
        let fill = Profile::from_contour(flat).triangulate()?;
        let z = s.positions[ring[0] as usize].z;
        let ids: Vec<u32> = fill
            .points
            .iter()
            .map(|p| s.push_vertex(p.extend(z)))
            .collect();
        for t in &fill.triangles {
            let t = if up { *t } else { [t[0], t[2], t[1]] };
            s.push_triangle(t.map(|i| ids[i as usize]), surface, &[name]);
        }
        for i in 0..n {
            s.tag_edge(ring[i], ring[(i + 1) % n], &[name]);
        }
    }
    finish(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn closed(s: &Solid) -> f64 {
        let m = s.measure();
        assert!(m.closed, "open: {m:?}");
        assert_eq!(m.degenerate_triangles, 0);
        assert_eq!(m.components, 1);
        m.volume
    }

    fn area(c: &Contour) -> f64 {
        c.signed_area2() / 2.0
    }

    #[test]
    fn a_gear_with_a_bore_and_a_hub_has_its_prisms_volume() {
        let d = GearDesc {
            teeth: 20,
            module: 2.0,
            width: 8.0,
            bore: 6.0,
            hub: 20.0,
            hub_height: 5.0,
            ..Default::default()
        };
        let s = gear_solid(&d).unwrap();
        let outline = gear(&Gear {
            teeth: 20,
            module: 2.0,
            pressure_angle: 0.0,
        })
        .unwrap();
        let (g, b, hub) = (
            area(&outline.regions[0].outer),
            area(&ring(3.0)),
            area(&ring(10.0)),
        );
        let want = (g - b) * 8.0 + (hub - b) * 5.0;
        assert!(
            (closed(&s) - want).abs() < 1e-9 * want,
            "{} {want}",
            s.measure().volume
        );
        assert!(!s.faces(&["bore"]).unwrap().is_empty());
        assert!(!s.faces(&["hub"]).unwrap().is_empty());
        // The hub's two rims, and the bore's at both ends.
        let hub_rims = s.edges(&["hub"]).unwrap().len();
        assert_eq!(hub_rims, 2 * segments(10.0, trig::TAU) as usize);
        assert_eq!(s.measure().bounds_max.z, 13.0);
        // No hub, no bore: the extruded outline.
        let plain = gear_solid(&GearDesc {
            hub: 0.0,
            hub_height: 0.0,
            bore: 0.0,
            ..d
        })
        .unwrap();
        assert!((closed(&plain) - g * 8.0).abs() < 1e-9 * g * 8.0);
        assert!(plain.faces(&["inner"]).is_err());
    }

    #[test]
    fn a_gear_refuses_a_bore_wider_than_its_hub() {
        let d = GearDesc {
            teeth: 12,
            module: 1.0,
            width: 4.0,
            bore: 5.0,
            hub: 4.0,
            hub_height: 2.0,
            ..Default::default()
        };
        assert!(matches!(gear_solid(&d), Err(GenError::Invalid(_))));
        let d = GearDesc { hub: 20.0, ..d };
        assert!(matches!(gear_solid(&d), Err(GenError::Invalid(_))));
        let d = GearDesc {
            hub: 8.0,
            hub_height: 0.0,
            ..d
        };
        assert_eq!(gear_solid(&d), Err(GenError::NotPositive("hub_height")));
    }

    #[test]
    fn a_straight_knurl_is_its_star_prism() {
        let s = knurl(&Knurl {
            diameter: 10.0,
            height: 6.0,
            pitch: 1.0,
            straight: true,
        })
        .unwrap();
        // 31 teeth, radii r, r - d/2, r - d, r - d/2 round each tooth.
        let teeth = 31;
        let n = 4 * teeth;
        let rho = |i: usize| 5.0 - 0.5 * [1.0, 0.5, 0.0, 0.5][i % 4];
        let (sn, _) = trig::sin_cos(trig::TAU / n as f64);
        let a: f64 = (0..n).map(|i| 0.5 * rho(i) * rho((i + 1) % n) * sn).sum();
        assert!(
            (closed(&s) - 6.0 * a).abs() < 1e-9 * a,
            "{}",
            s.measure().volume
        );
        assert_eq!(s.edges(&["top"]).unwrap().len(), n);
    }

    #[test]
    fn a_diamond_knurl_lies_between_its_root_and_crest() {
        let s = knurl(&Knurl {
            diameter: 12.0,
            height: 10.0,
            ..Default::default()
        })
        .unwrap();
        let v = closed(&s);
        let (lo, hi) = (trig::PI * 5.5 * 5.5 * 10.0, trig::PI * 36.0 * 10.0);
        assert!(lo < v && v < hi, "{v}");
        let m = s.measure();
        assert_eq!((m.bounds_min.z, m.bounds_max.z), (0.0, 10.0));
        assert!(!s.faces(&["bottom"]).unwrap().is_empty());
        assert_eq!(
            knurl(&Knurl {
                diameter: 1.0,
                height: 1.0,
                ..Default::default()
            })
            .map(|_| ()),
            Err(GenError::Invalid(
                "knurl: the pitch (1) must be smaller than the diameter (1)".into()
            ))
        );
    }
}
