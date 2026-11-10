//! Golden cases for the placement layer and models (shape-core's frame
//! operations, shape-model): frames read off solids, a jointed model's
//! GLB, and the washing machine's recorded program replayed
//! (`golden/programs/washing_machine.json`, written by the TypeScript
//! binding through wasm). The binding's test hashes the GLB and PNG it
//! makes in wasm32 the same way and compares with these lines.

use shape_core::{DVec3, Frame, HashError, OutputHash, trig};
use shape_gen::solids::{BoxDesc, CylinderDesc, cuboid, cylinder};
use shape_model::{
    JointDesc, JointKind, Material, Mimic, Model, Path, Preview, Unit, place, program,
};

use crate::cases::Case;

const WASHING_MACHINE: &str = include_str!("../../../golden/programs/washing_machine.json");

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "model.frame",
            run: frame_ops,
        },
        Case {
            name: "model.place",
            run: placement,
        },
        Case {
            name: "model.joints.glb",
            run: |h| model_bytes(h, |m| m.glb()),
        },
        Case {
            name: "model.joints.preview",
            run: |h| {
                model_bytes(h, |m| {
                    let d = Preview {
                        width: 96,
                        views: 2,
                        ..Default::default()
                    };
                    m.preview(&d)
                        .map(|v| shape_tools::png(&shape_tools::sheet(&v, 2)))
                })
            },
        },
        Case {
            name: "model.joints.obj",
            run: |h| model_bytes(h, |m| m.obj().map(String::into_bytes)),
        },
        Case {
            name: "model.joints.stl",
            run: |h| model_bytes(h, |m| m.stl()),
        },
        Case {
            name: "model.joints.copies",
            run: |h| model_bytes(h, |m| copies(m).and_then(|m| m.glb())),
        },
        Case {
            name: "model.washing_machine.glb",
            run: |h| washing_machine(h, |m| m.glb()),
        },
        Case {
            name: "model.washing_machine.png",
            run: |h| {
                washing_machine(h, |m| {
                    m.preview(&washing_machine_preview())
                        .map(|v| shape_tools::png(&shape_tools::sheet(&v, 4)))
                })
            },
        },
        Case {
            name: "model.washing_machine.measure",
            run: |h| washing_machine(h, |m| program::measures_json(m).map(String::into_bytes)),
        },
        Case {
            name: "model.washing_machine.posed",
            run: |h| washing_machine(h, |m| program::posed_json(m).map(String::into_bytes)),
        },
    ]
}

/// The preview `examples/washing_machine.ts` renders: four 320² views,
/// 20° up.
pub fn washing_machine_preview() -> Preview {
    Preview {
        width: 320,
        views: 4,
        elevation: Some(20.0 * trig::PI / 180.0),
        ..Default::default()
    }
}

/// A model error, said on stderr; the case fails.
fn fail(e: impl std::fmt::Display) -> HashError {
    eprintln!("{e}");
    HashError::NaN
}

fn frame(h: &mut OutputHash, f: &Frame) -> Result<(), HashError> {
    h.dvec3s(&[f.origin, f.x, f.y, f.z])
}

fn frame_ops(h: &mut OutputHash) -> Result<(), HashError> {
    // A fixed walk of offsets, moves and turns, including quarter turns
    // (snapped exact) and arbitrary ones.
    let mut f = Frame::facing(DVec3::new(300.0, 0.0, 425.0), DVec3::X).ok_or(HashError::NaN)?;
    for k in 0..256 {
        let a = f64::from(k) * 0.37;
        f = match k % 5 {
            0 => f.offset(a - 40.0),
            1 => f.translate(a, -a * 0.5, 3.0),
            2 => f.turn(trig::PI / 2.0, 0.0, -trig::PI),
            3 => f.turn(a * 0.1, -a * 0.03, a * 0.07),
            _ => f.compose(&f.inverse().turn(0.0, 0.2, 0.0)),
        };
        frame(h, &f)?;
        let w = Frame::with_up(f.origin, f.z + DVec3::Z * 0.1, f.y).ok_or(HashError::NaN)?;
        frame(h, &w)?;
    }
    Ok(())
}

fn placement(h: &mut OutputHash) -> Result<(), HashError> {
    let mut m = Model::new("place");
    let cab = m
        .add(
            "cab",
            cuboid(&BoxDesc {
                width: 600.0,
                depth: 400.0,
                height: 850.0,
            })
            .map_err(fail)?,
        )
        .map_err(fail)?;
    let on = Frame::IDENTITY
        .translate(10.0, 20.0, 30.0)
        .turn(0.0, 0.0, trig::PI / 2.0);
    m.at(cab, &on).map_err(fail)?;
    let ok = |r: Result<Frame, shape_model::ModelError>| r.map_err(fail);
    for which in ["+X", "-Y", "top", "bottom", "front"] {
        h.str(which);
        frame(h, &ok(m.face(cab, which))?)?;
    }
    for which in ["+X", "+X-Y", "-X+Y-Z"] {
        frame(h, &ok(m.bbox(&[cab], which))?)?;
    }
    let top = ok(m.face(cab, "top"))?;
    let path = m.edge(cab, "top").map_err(fail)?;
    h.dvec3s(&path.points)?;
    for f in place::grid(3, 2, 50.0, 30.0, true, &top)
        .iter()
        .chain(&place::ring(6, 40.0, 0.25, 0.0, &top))
        .chain(&place::ring(5, 40.0, 0.0, trig::PI, &top))
        .chain(&place::along(&path, 0.0, 9).map_err(fail)?)
        .chain(&place::along(&path, 333.0, 0).map_err(fail)?)
    {
        frame(h, f)?;
    }
    let open = Path {
        points: vec![
            DVec3::ZERO,
            DVec3::new(10.0, 5.0, 0.0),
            DVec3::new(10.0, 5.0, 20.0),
        ],
        closed: false,
    };
    for f in place::along(&open, 2.5, 0).map_err(fail)? {
        frame(h, &f)?;
    }
    let probes = [
        DVec3::new(0.0, 0.0, 2000.0),
        DVec3::new(500.0, 30.0, 400.0),
        DVec3::new(-1000.0, -1000.0, -1000.0),
    ];
    let snapped = m
        .snap(&[cab], &probes, None)
        .map_err(fail)?
        .into_iter()
        .chain(
            m.snap(&[cab], &probes, Some(DVec3::new(0.1, -0.2, -1.0)))
                .map_err(fail)?,
        );
    for f in snapped {
        frame(h, &f)?;
    }
    Ok(())
}

/// A cabinet in centimetres: a door group on a hinge, a flap following
/// it, a knob that slides and turns (two joints), a fixed badge, posed.
fn jointed() -> Result<Model, shape_model::ModelError> {
    let mut m = Model::new("cabinet");
    m.units(Unit::Centimetre)?;
    let block = |w, d, h| {
        cuboid(&BoxDesc {
            width: w,
            depth: d,
            height: h,
        })
    };
    let rod = |r, h| {
        cylinder(&CylinderDesc {
            radius: r,
            height: h,
            ..Default::default()
        })
    };
    let cab = m.add("cab", block(40.0, 30.0, 50.0)?)?;
    let front = m.face(cab, "+Y")?;
    let panel = m.add("panel", block(38.0, 48.0, 1.0)?)?;
    m.at(panel, &front)?;
    let handle = m.add("handle", rod(0.8, 3.0)?)?;
    m.at(handle, &front.translate(15.0, 0.0, 1.0))?;
    let door = m.group("door", &[panel, handle])?;
    m.material(
        &[door],
        &Material {
            color: Some([0.2, 0.3, 0.4]),
            roughness: Some(0.4),
            ..Default::default()
        },
    )?;
    m.joint(
        door,
        cab,
        &JointDesc {
            name: "hinge".into(),
            on: front.translate(-19.0, 0.0, 0.0),
            axis: DVec3::NEG_Z,
            range: Some((0.0, trig::to_radians(110.0))),
            ..Default::default()
        },
    )?;
    let flap = m.add("flap", block(5.0, 5.0, 1.0)?)?;
    m.at(flap, &front.translate(0.0, 20.0, 1.0))?;
    m.joint(
        flap,
        door,
        &JointDesc {
            name: "flap".into(),
            kind: JointKind::Prismatic,
            on: front.translate(0.0, 20.0, 1.0),
            mimic: Some(Mimic {
                joint: "hinge".into(),
                multiplier: 2.0,
            }),
            ..Default::default()
        },
    )?;
    let top = m.face(cab, "top")?;
    let knob = m.add("knob", rod(2.0, 1.5)?)?;
    m.at(knob, &top)?;
    m.joint(
        knob,
        cab,
        &JointDesc {
            name: "lift".into(),
            kind: JointKind::Prismatic,
            on: top,
            range: Some((0.0, 4.0)),
            ..Default::default()
        },
    )?;
    m.joint(
        knob,
        cab,
        &JointDesc {
            name: "spin".into(),
            kind: JointKind::Continuous,
            on: top,
            ..Default::default()
        },
    )?;
    let badge = m.add("badge", block(4.0, 1.0, 0.2)?)?;
    m.at(badge, &top.translate(10.0, 0.0, 0.0))?;
    m.joint(
        badge,
        cab,
        &JointDesc {
            name: "mount".into(),
            kind: JointKind::Fixed,
            on: top.translate(10.0, 0.0, 0.0),
            ..Default::default()
        },
    )?;
    m.pose("hinge", 1.2)?;
    m.pose("lift", 2.5)?;
    m.pose("spin", -0.7)?;
    Ok(m)
}

/// The jointed cabinet with its door copied (moved aside, posed its own
/// way) and mirrored across the cabinet's side, its knob mirrored too:
/// joints through copies (`docs/research.md` §20).
fn copies(m: &Model) -> Result<Model, shape_model::ModelError> {
    let mut m = m.clone();
    let door = m.find("door").ok_or(shape_model::ModelError::EmptyName)?;
    let knob = m.find("knob").ok_or(shape_model::ModelError::EmptyName)?;
    let twin = m.copy(door, "twin")?;
    m.transform(twin, &Frame::IDENTITY.translate(0.0, 0.0, 60.0))?;
    m.pose("hinge.001", 0.4)?;
    let side = Frame::facing(DVec3::new(25.0, 0.0, 0.0), DVec3::X).ok_or(
        shape_model::ModelError::BadValue("the mirror's plane".into()),
    )?;
    m.mirror(door, &side, "door_r")?;
    m.mirror(knob, &side, "knob_r")?;
    m.pose("hinge.002", 0.9)?;
    m.pose("lift.001", 1.5)?;
    m.pose("spin.001", 2.0)?;
    Ok(m)
}

fn model_bytes(
    h: &mut OutputHash,
    f: impl Fn(&Model) -> Result<Vec<u8>, shape_model::ModelError>,
) -> Result<(), HashError> {
    let m = jointed().map_err(fail)?;
    h.bytes(&f(&m).map_err(fail)?);
    Ok(())
}

fn washing_machine(
    h: &mut OutputHash,
    f: impl Fn(&Model) -> Result<Vec<u8>, shape_model::ModelError>,
) -> Result<(), HashError> {
    let m = program::run(WASHING_MACHINE).map_err(fail)?;
    h.bytes(&f(&m).map_err(fail)?);
    Ok(())
}
