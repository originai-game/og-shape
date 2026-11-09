//! Ten sand3d programs rebuilt with existing generators only: the
//! gallery's articulated objects that need no boolean and no bevel, and
//! `docs/examples/gate.py`. Each follows its sand3d source line by line
//! (Sandflow-ai/sand3d at 84c980a, Apache-2.0; the gallery programs
//! rebuild Articraft-10K records, CC BY 4.0, as their docstrings say).

// The sources' rounded constants (1.5708, 6.28319) are kept as written.
#![allow(clippy::approx_constant)]

use shape_core::trig;
use shape_tools::JointKind::{Continuous, Prismatic, Revolute};

use super::sand3d::*;

fn deg(rad: f64) -> f64 {
    trig::to_degrees(rad)
}

fn rgb(r: f64, g: f64, b: f64) -> Option<[f64; 3]> {
    Some([r, g, b])
}

/// A box of `w × d × h` centred on `(x, y, zc)`.
fn centred(w: f64, d: f64, h: f64, x: f64, y: f64, zc: f64) -> shape_mesh::Solid {
    at_xyz(box_(w, d, h), x, y, zc - h / 2.0)
}

/// Every program, by its sand3d name.
pub fn all() -> Vec<Model> {
    vec![
        xyz_stage(),
        task_lamp(),
        garden_gate(),
        refrigerator(),
        skateboard(),
        binoculars(),
        tailgate(),
        keyboard(),
        bevel_transfer(),
        gate(),
    ]
}

/// Gallery A7, `xyz_cartesian_stage`.
pub fn xyz_stage() -> Model {
    let mut m = Model::new("xyz_cartesian_stage");
    let (dark, milled, rail, stop, blue) = (
        hex("#14171a"),
        hex("#b8bdc2"),
        hex("#5c6166"),
        hex("#050505"),
        hex("#0d33d9"),
    );
    #[allow(clippy::too_many_arguments)]
    fn block(
        m: &mut Model,
        name: &str,
        w: f64,
        d: f64,
        h: f64,
        x: f64,
        y: f64,
        zc: f64,
        kind: &str,
        color: [f64; 3],
    ) -> Id {
        let id = m.add(name, centred(w, d, h, x, y, zc));
        m.material(&[id], &material(kind, Some(color)));
        id
    }
    let _plate = block(
        &mut m,
        "base_plate",
        500.0,
        320.0,
        35.0,
        0.0,
        0.0,
        17.5,
        "aluminium",
        dark,
    );
    let x_rail_neg = block(
        &mut m,
        "x_rail_neg",
        400.0,
        26.0,
        22.0,
        0.0,
        -90.0,
        46.0,
        "steel",
        rail,
    );
    block(
        &mut m,
        "x_rail_pos",
        400.0,
        26.0,
        22.0,
        0.0,
        90.0,
        46.0,
        "steel",
        rail,
    );
    for (s, x) in [("neg", -225.0), ("pos", 225.0)] {
        block(
            &mut m,
            &format!("x_stop_{s}"),
            25.0,
            240.0,
            30.0,
            x,
            0.0,
            50.0,
            "plastic",
            stop,
        );
    }
    // x_carriage, link at z = 57
    let z = 57.0;
    let cross = block(
        &mut m,
        "x_cross_plate",
        200.0,
        220.0,
        25.0,
        0.0,
        0.0,
        z + 38.5,
        "aluminium",
        milled,
    );
    let mut x_parts = vec![cross];
    for (sx, x) in [("neg", -65.0), ("pos", 65.0)] {
        for (sy, y) in [("neg", -90.0), ("pos", 90.0)] {
            let name = format!("x_bearing_{sx}_{sy}");
            x_parts.push(block(
                &mut m,
                &name,
                70.0,
                46.0,
                26.0,
                x,
                y,
                z + 13.0,
                "plastic",
                stop,
            ));
        }
    }
    x_parts.push(block(
        &mut m,
        "x_axis_cover",
        70.0,
        25.0,
        10.0,
        0.0,
        0.0,
        z + 56.0,
        "painted_metal",
        blue,
    ));
    // y_stage, link at z = 108
    let z = 108.0;
    let mut y_parts = vec![block(
        &mut m,
        "y_carriage_plate",
        220.0,
        180.0,
        25.0,
        0.0,
        0.0,
        z + 38.5,
        "aluminium",
        milled,
    )];
    for (s, x) in [("neg", -70.0), ("pos", 70.0)] {
        y_parts.push(block(
            &mut m,
            &format!("y_rail_{s}"),
            20.0,
            205.0,
            15.0,
            x,
            0.0,
            z + 58.5,
            "steel",
            rail,
        ));
    }
    for (sx, x) in [("neg", -65.0), ("pos", 65.0)] {
        for (sy, y) in [("neg", -50.0), ("pos", 50.0)] {
            let name = format!("y_bearing_{sx}_{sy}");
            y_parts.push(block(
                &mut m,
                &name,
                46.0,
                70.0,
                26.0,
                x,
                y,
                z + 13.0,
                "plastic",
                stop,
            ));
        }
    }
    y_parts.push(block(
        &mut m,
        "y_axis_cover",
        160.0,
        20.0,
        10.0,
        0.0,
        -92.5,
        z + 71.0,
        "painted_metal",
        blue,
    ));
    // z_column, link at z = 174, welded into the Y stage's group
    let z = 174.0;
    let foot = block(
        &mut m,
        "column_foot",
        160.0,
        160.0,
        20.0,
        0.0,
        0.0,
        z + 10.0,
        "aluminium",
        dark,
    );
    y_parts.push(foot);
    y_parts.push(block(
        &mut m,
        "vertical_backbone",
        120.0,
        30.0,
        220.0,
        0.0,
        65.0,
        z + 110.0,
        "aluminium",
        milled,
    ));
    y_parts.push(block(
        &mut m,
        "z_rail_neg",
        18.0,
        12.0,
        180.0,
        -45.0,
        44.0,
        z + 115.0,
        "steel",
        rail,
    ));
    y_parts.push(block(
        &mut m,
        "z_rail_pos",
        18.0,
        12.0,
        180.0,
        45.0,
        44.0,
        z + 115.0,
        "steel",
        rail,
    ));
    y_parts.push(block(
        &mut m,
        "z_top_stop",
        140.0,
        35.0,
        25.0,
        0.0,
        55.0,
        z + 232.5,
        "plastic",
        stop,
    ));
    // top_table, link at z = 199
    let z = 199.0;
    let table = vec![
        block(
            &mut m,
            "z_slide_plate",
            130.0,
            12.0,
            100.0,
            0.0,
            24.0,
            z + 50.0,
            "plastic",
            stop,
        ),
        block(
            &mut m,
            "z_bearing_neg",
            26.0,
            8.0,
            45.0,
            -45.0,
            34.0,
            z + 60.0,
            "steel",
            rail,
        ),
        block(
            &mut m,
            "z_bearing_pos",
            26.0,
            8.0,
            45.0,
            45.0,
            34.0,
            z + 60.0,
            "steel",
            rail,
        ),
        block(
            &mut m,
            "top_table_plate",
            160.0,
            120.0,
            18.0,
            0.0,
            -30.0,
            z + 109.0,
            "aluminium",
            milled,
        ),
        block(
            &mut m,
            "table_neck",
            90.0,
            35.0,
            20.0,
            0.0,
            -2.0,
            z + 100.0,
            "aluminium",
            milled,
        ),
    ];
    let on = m.face(x_rail_neg, "+Z");
    m.joint(
        "base_to_x",
        &x_parts,
        on,
        x_rail_neg,
        Prismatic,
        PX,
        Some((-75.0, 75.0)),
    );
    let on = m.face(cross, "+Z");
    m.joint(
        "x_to_y",
        &y_parts,
        on,
        cross,
        Prismatic,
        PY,
        Some((-75.0, 75.0)),
    );
    let on = m.face(foot, "+Z");
    m.joint(
        "column_to_table",
        &table,
        on,
        foot,
        Prismatic,
        PZ,
        Some((0.0, 120.0)),
    );
    m
}

/// Gallery A6, `floor_task_lamp`.
pub fn task_lamp() -> Model {
    let mut m = Model::new("floor_task_lamp");
    let dark = material("steel", rgb(0.28, 0.30, 0.32));
    let bright = material("brushed_steel", rgb(0.67, 0.69, 0.72));
    let base = m.add(
        "weighted_base",
        revolve(&[
            [0.0, 0.0],
            [106.0, 0.0],
            [132.0, 4.0],
            [145.0, 14.0],
            [143.0, 26.0],
            [118.0, 34.0],
            [50.0, 38.0],
            [0.0, 38.0],
        ]),
    );
    m.material(&[base], &material("painted_metal", rgb(0.12, 0.12, 0.13)));
    let collar = m.add("base_collar", at_xyz(cylinder(54.0, 14.0), 0.0, 0.0, 38.0));
    let column = m.add("column", at_xyz(cylinder(32.0, 1130.0), 0.0, 0.0, 38.0));
    let top = m.add("top_collar", at_xyz(cylinder(48.0, 12.0), 0.0, 0.0, 1156.0));
    m.material(&[collar, top], &dark);
    m.material(&[column], &bright);
    let stand = [base, collar, column, top];
    // swivel head, on the top collar (z = 1168)
    let z0 = 1168.0;
    let disk = m.add("swivel_disk", at_xyz(cylinder(80.0, 14.0), 0.0, 0.0, z0));
    let stem = m.add(
        "swivel_stem",
        at_xyz(cylinder(36.0, 32.0), 0.0, 0.0, z0 + 10.0),
    );
    let bridge = m.add(
        "shoulder_bridge",
        at_xyz(box_(30.0, 28.0, 18.0), 12.0, 0.0, z0 + 21.0),
    );
    let left = m.add(
        "left_cheek",
        at_xyz(box_(40.0, 6.0, 28.0), 18.0, 14.0, z0 + 26.0),
    );
    let right = m.add(
        "right_cheek",
        at_xyz(box_(40.0, 6.0, 28.0), 18.0, -14.0, z0 + 26.0),
    );
    m.material(&[disk, left, right], &dark);
    m.material(&[stem, bridge], &bright);
    let swivel = [disk, stem, bridge, left, right];
    // arm, from the shoulder axis (38, 0, 1208) along +X
    let (sx, sz) = (38.0, z0 + 40.0);
    let barrel = m.add(
        "shoulder_barrel",
        at(cylinder(21.0, 22.0), [sx, 11.0, sz], [90.0, 0.0, 0.0]),
    );
    let neck = m.add(
        "shoulder_neck",
        at_xyz(box_(24.0, 16.0, 16.0), sx + 12.0, 0.0, sz - 8.0),
    );
    let tube = m.add(
        "arm_tube",
        at(
            cylinder(22.0, 386.0),
            [sx + 24.0, 0.0, sz],
            [0.0, 90.0, 0.0],
        ),
    );
    let tip = m.add(
        "tip_bridge",
        at_xyz(box_(28.0, 18.0, 16.0), sx + 414.0, 0.0, sz - 8.0),
    );
    let tip_l = m.add(
        "tip_left_cheek",
        at_xyz(box_(24.0, 5.0, 30.0), sx + 418.0, 11.5, sz - 15.0),
    );
    let tip_r = m.add(
        "tip_right_cheek",
        at_xyz(box_(24.0, 5.0, 30.0), sx + 418.0, -11.5, sz - 15.0),
    );
    m.material(&[barrel, neck, tip, tip_l, tip_r], &dark);
    m.material(&[tube], &bright);
    let arm = [barrel, neck, tube, tip, tip_l, tip_r];
    // shade, from the tilt axis (477, 0, 1208)
    let tx = sx + 439.0;
    let trunnion = m.add(
        "shade_trunnion",
        at(cylinder(18.0, 18.0), [tx, 9.0, sz], [90.0, 0.0, 0.0]),
    );
    let mount = m.add(
        "shade_mount",
        at_xyz(box_(36.0, 16.0, 48.0), tx + 18.0, 0.0, sz - 30.0),
    );
    m.material(&[trunnion, mount], &dark);
    let shell = revolve(&[
        [42.0, 0.0],
        [52.0, 16.0],
        [56.0, 50.0],
        [56.0, 170.0],
        [50.0, 200.0],
        [46.0, 200.0],
        [46.0, 50.0],
        [37.0, 26.0],
        [0.0, 10.0],
    ]);
    let shell = m.add(
        "shade_shell",
        at(shell, [tx + 4.0, 0.0, sz - 38.0], [0.0, 90.0, 0.0]),
    );
    m.material(&[shell], &material("plastic", rgb(0.88, 0.87, 0.82)));
    let shade = [trunnion, mount, shell];

    let on = m.bbox(&stand, "+Z");
    let r = deg(1.91986);
    m.joint(
        "stand_to_swivel",
        &swivel,
        on,
        base,
        Revolute,
        PZ,
        Some((-r, r)),
    );
    let on = m.bbox(&[left], "+X");
    m.joint(
        "swivel_to_arm",
        &arm,
        on,
        left,
        Revolute,
        NY,
        Some((deg(-0.698132), deg(1.22173))),
    );
    let on = m.bbox(&[tip_l], "+X").offset(9.0);
    m.joint(
        "arm_to_shade",
        &shade,
        on,
        tip_l,
        Revolute,
        NY,
        Some((deg(-1.13446), deg(1.22173))),
    );
    m
}

/// Gallery A2, `double_leaf_garden_gate`.
pub fn garden_gate() -> Model {
    let mut m = Model::new("double_leaf_garden_gate");
    let green = material("painted_metal", rgb(0.18, 0.24, 0.17));
    let steel = material("steel", rgb(0.70, 0.72, 0.74));
    let rod = |r: f64, len: f64, x: f64, y: f64, zc: f64| {
        at_xyz(cylinder(2.0 * r, len), x, y, zc - len / 2.0)
    };
    let beam = m.add(
        "foundation_beam",
        centred(1960.0, 240.0, 200.0, 0.0, 0.0, -240.0),
    );
    m.material(&[beam], &material("concrete", rgb(0.55, 0.55, 0.53)));
    let socket = m.add("drop_socket", rod(14.0, 220.0, 42.0, -5.5, -60.0));
    m.material(&[socket], &steel);
    let mut pins = Vec::new();
    for (prefix, x, side) in [("left_post", -800.0, 1.0), ("right_post", 800.0, -1.0)] {
        let body = m.add(
            &format!("{prefix}_body"),
            centred(100.0, 120.0, 1600.0, x, 0.0, 660.0),
        );
        let cap = m.add(
            &format!("{prefix}_cap"),
            centred(120.0, 140.0, 25.0, x, 0.0, 1472.5),
        );
        let lower = m.add(
            &format!("{prefix}_lower_pin"),
            centred(40.0, 24.0, 120.0, x + side * 70.0, 0.0, 320.0),
        );
        let upper = m.add(
            &format!("{prefix}_upper_pin"),
            centred(40.0, 24.0, 120.0, x + side * 70.0, 0.0, 920.0),
        );
        m.material(&[body, cap], &green);
        m.material(&[lower, upper], &steel);
        pins.push(lower);
    }
    // The left leaf, its hinge line at x0; the right leaf is its mirror
    // image across x = 0.
    let leaf = |m: &mut Model, prefix: &str, mirror: bool| -> Vec<Id> {
        let (x0, y, t) = (-730.0, -35.0, 40.0);
        let place = |s| if mirror { mirror_x(s) } else { s };
        let mut frame = Vec::new();
        let mut hardware = Vec::new();
        let piece = |m: &mut Model, list: &mut Vec<Id>, name: &str, s| {
            list.push(m.add(&format!("{prefix}_{name}"), place(s)));
        };
        piece(
            m,
            &mut frame,
            "outer_stile",
            centred(t, 35.0, 1100.0, x0 + 24.0, y, 630.0),
        );
        piece(
            m,
            &mut frame,
            "inner_stile",
            centred(t, 35.0, 1100.0, x0 + 690.0, y, 630.0),
        );
        piece(
            m,
            &mut frame,
            "bottom_rail",
            centred(665.0, 35.0, t, x0 + 357.5, y, 100.0),
        );
        piece(
            m,
            &mut frame,
            "top_rail",
            centred(665.0, 35.0, t, x0 + 357.5, y, 1160.0),
        );
        for (i, bx) in [200.0, 360.0, 520.0].into_iter().enumerate() {
            piece(
                m,
                &mut frame,
                &format!("bar_{}", i + 1),
                centred(18.0, 18.0, 1020.0, x0 + bx, y, 630.0),
            );
        }
        piece(
            m,
            &mut hardware,
            "latch_plate",
            centred(12.0, 20.0, 180.0, x0 + 696.0, y - 20.0, 680.0),
        );
        for (name, z) in [("lower_sleeve", 320.0), ("upper_sleeve", 920.0)] {
            piece(
                m,
                &mut frame,
                &format!("{name}_strap"),
                centred(24.0, 28.0, 120.0, x0 + 32.0, -28.0, z),
            );
            piece(
                m,
                &mut hardware,
                name,
                centred(40.0, 22.0, 120.0, x0, -3.0, z),
            );
        }
        m.material(&frame, &green);
        m.material(&hardware, &steel);
        frame.into_iter().chain(hardware).collect()
    };
    let left_leaf = leaf(&mut m, "left_leaf", false);
    let mut right_leaf = leaf(&mut m, "right_leaf", true);
    let mut guides = Vec::new();
    for (name, z) in [("upper", 780.0), ("lower", 270.0)] {
        guides.push(m.add(
            &format!("right_leaf_{name}_guide"),
            rod(12.0, 80.0, 40.0, -5.5, z),
        ));
    }
    m.material(&guides, &steel);
    right_leaf.extend(&guides);
    let bolt = m.add("bolt_rod", rod(8.0, 750.0, 40.0, -5.5, 465.0));
    m.material(&[bolt], &steel);

    let quarter = deg(1.5708);
    let on = m.bbox(&[pins[0]], "-Z");
    m.joint(
        "left_leaf_hinge",
        &left_leaf,
        on,
        pins[0],
        Revolute,
        PZ,
        Some((0.0, quarter)),
    );
    let on = m.bbox(&[pins[1]], "-Z");
    m.joint(
        "right_leaf_hinge",
        &right_leaf,
        on,
        pins[1],
        Revolute,
        PZ,
        Some((-quarter, 0.0)),
    );
    let on = m.bbox(&[guides[0]], "+Z");
    m.joint(
        "drop_bolt_slide",
        &[bolt],
        on,
        guides[0],
        Prismatic,
        NZ,
        Some((0.0, 170.0)),
    );
    m
}

/// Gallery A3, `compact_apartment_refrigerator`.
pub fn refrigerator() -> Model {
    let mut m = Model::new("compact_apartment_refrigerator");
    let steel = material("steel", rgb(0.67, 0.69, 0.72));
    let bx = |w: f64, d: f64, h: f64, c: [f64; 3]| centred(w, d, h, c[0], c[1], c[2]);
    let pin =
        |d: f64, len: f64, c: [f64; 3]| at_xyz(cylinder(d, len), c[0], c[1], c[2] - len / 2.0);
    let (d, h) = (530.0, 1420.0);
    let (hinge_x, hinge_y) = (283.0, 271.0);
    let walls = vec![
        m.add("left_wall", bx(28.0, d, h, [-261.0, 0.0, 710.0])),
        m.add("right_wall", bx(28.0, d, h, [261.0, 0.0, 710.0])),
        m.add("top_shell", bx(494.0, 518.0, 30.0, [0.0, 6.0, 1405.0])),
        m.add("bottom_shell", bx(494.0, 518.0, 40.0, [0.0, 6.0, 20.0])),
        m.add("back_panel", bx(494.0, 12.0, h, [0.0, -259.0, 710.0])),
        m.add(
            "center_face_rail",
            bx(494.0, 18.0, 24.0, [0.0, 256.0, 1080.0]),
        ),
    ];
    m.material(&walls, &material("painted_metal", rgb(0.92, 0.93, 0.94)));
    let shelf = m.add("divider_shelf", bx(494.0, 518.0, 32.0, [0.0, 6.0, 1081.0]));
    m.material(&[shelf], &material("plastic", rgb(0.84, 0.86, 0.88)));
    let mut pins = Vec::new();
    let mut hinge_parts = Vec::new();
    for (name, z) in [
        ("upper_top", 1385.0),
        ("upper_center", 1100.0),
        ("lower_center", 1058.0),
        ("lower_bottom", 35.0),
    ] {
        hinge_parts.push(m.add(
            &format!("{name}_hinge_pin_bracket"),
            bx(22.0, 18.0, 30.0, [275.5, 264.5, z]),
        ));
        let p = m.add(
            &format!("{name}_hinge_pin"),
            pin(14.0, 30.0, [hinge_x, hinge_y, z]),
        );
        pins.push(p);
        hinge_parts.push(p);
    }
    m.material(&hinge_parts, &steel);
    let housing = m.add("control_box", bx(160.0, 100.0, 50.0, [90.0, 100.0, 1030.0]));
    let tab = m.add("mount_tab", bx(80.0, 60.0, 10.0, [90.0, 100.0, 1060.0]));
    m.material(&[housing, tab], &material("plastic", rgb(0.74, 0.76, 0.79)));
    let front = m.bbox(&[housing], "+Y");
    let dial_axis = front.mv(-35.0, -4.0, 0.0).offset(6.0);
    let bezel = m.add(
        "dial_bezel",
        on(cylinder(44.0, 6.0), &front.mv(-35.0, -4.0, 0.0)),
    );
    m.material(&[bezel], &material("plastic", rgb(0.28, 0.30, 0.33)));

    let door = |m: &mut Model, name: &str, door_h: f64, z: f64, handle_h: f64, handle_z: f64| {
        let panel = m.add(
            &format!("{name}_outer_panel"),
            bx(575.0, 55.0, door_h, [hinge_x - 273.5, hinge_y + 34.5, z]),
        );
        let flange = m.add(
            &format!("{name}_hinge_flange"),
            bx(
                14.0,
                20.0,
                door_h - 100.0,
                [hinge_x + 11.0, hinge_y + 20.0, z],
            ),
        );
        m.material(
            &[panel, flange],
            &material("glossy_plastic", rgb(0.97, 0.97, 0.96)),
        );
        let gasket = m.add(
            &format!("{name}_door_gasket"),
            bx(
                464.0,
                10.0,
                door_h - 35.0,
                [hinge_x - 283.0, hinge_y + 1.0, z],
            ),
        );
        m.material(&[gasket], &material("rubber", rgb(0.76, 0.78, 0.80)));
        let liner = m.add(
            &format!("{name}_inner_liner"),
            bx(
                449.0,
                28.0,
                door_h - 75.0,
                [hinge_x - 283.0, hinge_y + 19.0, z],
            ),
        );
        m.material(&[liner], &material("plastic", rgb(0.84, 0.86, 0.88)));
        let handle = m.add(
            &format!("{name}_handle"),
            bx(
                22.0,
                20.0,
                handle_h,
                [hinge_x - 515.0, hinge_y + 67.0, z + handle_z],
            ),
        );
        m.material(&[handle], &material("plastic", rgb(0.28, 0.30, 0.33)));
        let k = door_h / 2.0 - 52.5;
        let knuckles = vec![
            m.add(
                &format!("{name}_top_knuckle"),
                pin(14.0, 45.0, [hinge_x, hinge_y, z + k]),
            ),
            m.add(
                &format!("{name}_bottom_knuckle"),
                pin(14.0, 45.0, [hinge_x, hinge_y, z - k]),
            ),
        ];
        m.material(&knuckles, &steel);
        let mut g = vec![panel, gasket, liner, flange, handle];
        g.extend(knuckles);
        g
    };
    let upper = door(&mut m, "upper_freezer_door", 315.0, 1242.5, 140.0, -55.0);
    let lower = door(&mut m, "lower_main_door", 1053.0, 546.5, 240.0, 220.0);
    let knob = m.add("dial_body", on(cylinder(36.0, 12.0), &dial_axis));
    m.material(&[knob], &material("plastic", rgb(0.20, 0.22, 0.24)));
    let mark = m.add("dial_indicator", bx(4.0, 3.0, 12.0, [125.0, 168.5, 1039.0]));
    m.material(&[mark], &material("plastic", rgb(0.93, 0.31, 0.18)));

    let opening = Some((deg(-1.95), 0.0));
    let on_pin = m.bbox(&[pins[0]], "+Z");
    m.joint(
        "upper_door_hinge",
        &upper,
        on_pin,
        pins[0],
        Revolute,
        PZ,
        opening,
    );
    let on_pin = m.bbox(&[pins[2]], "+Z");
    m.joint(
        "lower_door_hinge",
        &lower,
        on_pin,
        pins[2],
        Revolute,
        PZ,
        opening,
    );
    m.joint(
        "thermostat_dial_joint",
        &[knob, mark],
        dial_axis,
        housing,
        Revolute,
        PY,
        Some((deg(-2.2), deg(0.6))),
    );
    m
}

/// Gallery A5, `skateboard`.
pub fn skateboard() -> Model {
    let mut m = Model::new("skateboard");
    let board = m.add(
        "board",
        at_xyz(extrude(&rect_r(800.0, 200.0, 90.0), 15.0), 0.0, 0.0, 92.5),
    );
    m.material(&[board], &material("oak", None));
    let front_plate = m.add(
        "front_baseplate",
        at_xyz(box_(80.0, 60.0, 10.0), 250.0, 0.0, 82.5),
    );
    let rear_plate = m.add(
        "rear_baseplate",
        at_xyz(box_(80.0, 60.0, 10.0), -250.0, 0.0, 82.5),
    );
    m.material(&[front_plate, rear_plate], &material("aluminium", None));
    let truck = |m: &mut Model, side: &str, x: f64| {
        let axle = m.add(
            &format!("{side}_axle"),
            at(cylinder(8.0, 360.0), [x, 180.0, 27.5], [90.0, 0.0, 0.0]),
        );
        let strut = m.add(
            &format!("{side}_strut"),
            at_xyz(box_(30.0, 40.0, 56.0), x, 0.0, 27.5),
        );
        m.material(&[axle, strut], &material("steel", rgb(0.7, 0.7, 0.7)));
        (axle, [axle, strut])
    };
    let (front_axle, front_truck) = truck(&mut m, "front", 250.0);
    let (rear_axle, rear_truck) = truck(&mut m, "rear", -250.0);
    let steer = Some((deg(-0.3), deg(0.3)));
    let on = m
        .bbox(&[front_plate], "+Z")
        .offset(-65.0)
        .turn(0.0, -45.0, 0.0);
    m.joint(
        "front_truck_steer",
        &front_truck,
        on,
        front_plate,
        Revolute,
        Axis::Normal,
        steer,
    );
    let on = m
        .bbox(&[rear_plate], "+Z")
        .offset(-65.0)
        .turn(0.0, 45.0, 0.0);
    m.joint(
        "rear_truck_steer",
        &rear_truck,
        on,
        rear_plate,
        Revolute,
        Axis::Normal,
        steer,
    );
    for (side, axle, x) in [("front", front_axle, 250.0), ("rear", rear_axle, -250.0)] {
        let mut wheel = |name: &str, y: f64| {
            let w = m.add(
                name,
                at(cylinder(55.0, 35.0), [x, y + 17.5, 27.5], [90.0, 0.0, 0.0]),
            );
            m.material(&[w], &material("rubber", rgb(0.95, 0.9, 0.7)));
            w
        };
        let left = wheel(&format!("{side}_left_wheel"), 160.0);
        let right = wheel(&format!("{side}_right_wheel"), -160.0);
        let on = m.bbox(&[axle], "+Y").offset(-20.0);
        m.joint(
            &format!("{side}_left_spin"),
            &[left],
            on,
            axle,
            Continuous,
            PY,
            None,
        );
        let on = m.bbox(&[axle], "-Y").offset(-20.0);
        m.joint(
            &format!("{side}_right_spin"),
            &[right],
            on,
            axle,
            Continuous,
            PY,
            None,
        );
    }
    m
}

/// Gallery A9, `compact_theatre_binocular`.
pub fn binoculars() -> Model {
    let mut m = Model::new("compact_theatre_binocular");
    const BODY_X: f64 = 33.0;
    const HINGE_Y: f64 = -11.0;
    let black = material("plastic", rgb(0.10, 0.10, 0.11));
    let armor = material("plastic", rgb(0.16, 0.16, 0.17));
    let gray = material("plastic", rgb(0.29, 0.30, 0.33));
    let glass = material("plastic", rgb(0.12, 0.17, 0.22));
    let along_y = |d: f64, len: f64, x: f64, y: f64| {
        on(
            cylinder(d, len),
            &frame([x, y + len / 2.0, 0.0], [0.0, -1.0, 0.0]),
        )
    };
    let block = |w: f64, d: f64, h: f64, x: f64, y: f64, z: f64| centred(w, d, h, x, y, z);
    let housing = |m: &mut Model, side: f64| {
        let cx = side * BODY_X;
        let sections = [
            (-3.0, 31.0, 34.0, 6.0),
            (-13.0, 32.0, 35.0, 6.0),
            (-23.0, 28.0, 31.0, 5.0),
            (-33.0, 23.0, 24.0, 4.0),
        ];
        // Sections on frames looking −Y: stacked along that normal.
        let shell = loft(
            sections
                .iter()
                .map(|&(y, w, h, r)| (rect_r(w, h, r), 0.0, 0.0, -3.0 - y))
                .collect(),
        );
        let shell = m.add(
            "body_shell",
            on(shell, &frame([cx, -3.0, 0.0], [0.0, -1.0, 0.0])),
        );
        let barrel = m.add("objective_barrel", along_y(27.0, 44.0, cx, 18.0));
        let eyepiece = m.add("eyepiece_barrel", along_y(22.4, 28.0, cx, -31.0));
        let arm = m.add(
            "bridge_arm",
            block(26.4, 14.0, 12.0, side * 17.7, HINGE_Y, 0.0),
        );
        let trim = vec![
            m.add("objective_armor", along_y(28.4, 13.0, cx, 12.0)),
            m.add("objective_bezel", along_y(29.0, 4.0, cx, 40.0)),
            m.add("eyepiece_shoulder", along_y(25.2, 8.0, cx, -19.0)),
        ];
        let lens = m.add("objective_lens", along_y(23.0, 1.2, cx, 42.5));
        m.material(&[shell, barrel, eyepiece, arm], &black);
        m.material(&trim, &armor);
        m.material(&[lens], &glass);
        let mut parts = vec![shell, barrel, eyepiece, arm];
        parts.extend(trim);
        parts.push(lens);
        (parts, eyepiece)
    };
    let eyecup = |m: &mut Model, end: &shape_core::Frame| {
        let outer = [
            [11.4, 0.0],
            [12.3, 2.0],
            [14.4, 7.0],
            [15.2, 14.0],
            [14.2, 17.0],
        ];
        let inner = [[10.8, 1.0], [11.0, 3.0], [12.6, 10.0], [13.0, 16.0]];
        let profile: Vec<[f64; 2]> = outer.iter().chain(inner.iter().rev()).copied().collect();
        let shell = m.add("eyecup_shell", on(revolve(&profile), end));
        let collar = m.add("eyecup_mount_collar", on(cylinder(22.0, 2.0), end));
        m.material(&[shell, collar], &armor);
        vec![shell, collar]
    };
    let (mut left, left_eyepiece) = housing(&mut m, -1.0);
    let upper = m.add(
        "hinge_knuckle_upper",
        at_xyz(cylinder(9.4, 6.0), 0.0, HINGE_Y, 6.0),
    );
    let lower = m.add(
        "hinge_knuckle_lower",
        at_xyz(cylinder(9.4, 6.0), 0.0, HINGE_Y, -12.0),
    );
    m.material(&[upper, lower], &armor);
    let post_r = m.add(
        "focus_post_right",
        block(4.0, 10.0, 18.0, 11.0, -12.0, 22.0),
    );
    let focus = vec![
        m.add("focus_stem", block(6.0, 8.0, 24.0, -15.0, -12.0, 18.0)),
        m.add("focus_arch", block(32.0, 6.0, 4.0, 0.0, -12.0, 31.0)),
        m.add(
            "focus_post_left",
            block(4.0, 10.0, 18.0, -11.0, -12.0, 22.0),
        ),
        post_r,
    ];
    m.material(&focus, &black);
    left.extend([upper, lower]);
    left.extend(&focus);
    let (mut right, right_eyepiece) = housing(&mut m, 1.0);
    let center = m.add(
        "hinge_knuckle_center",
        at_xyz(cylinder(9.4, 12.0), 0.0, HINGE_Y, -6.0),
    );
    m.material(&[center], &armor);
    right.push(center);
    let axle = |d: f64, len: f64| {
        on(
            cylinder(d, len),
            &frame([-len / 2.0, -12.0, 22.0], [1.0, 0.0, 0.0]),
        )
    };
    let core = m.add("focus_wheel_core", axle(17.0, 18.0));
    let tread = m.add("focus_wheel_tread", axle(20.4, 12.0));
    let hub = m.add("focus_axle_hub", axle(7.0, 20.0));
    m.material(&[core, hub], &gray);
    m.material(&[tread], &armor);
    let knob = [core, tread, hub];
    let left_end = m.face(left_eyepiece, "-Y").mv(-BODY_X, 0.0, 0.0);
    let right_end = m.face(right_eyepiece, "-Y").mv(BODY_X, 0.0, 0.0);
    let left_cup = eyecup(&mut m, &left_end);
    let right_cup = eyecup(&mut m, &right_end);

    let on_hinge = m.face(upper, "+Z").mv(0.0, HINGE_Y, 0.0);
    m.joint(
        "central_hinge",
        &right,
        on_hinge,
        upper,
        Revolute,
        PZ,
        Some((deg(-0.55), deg(0.12))),
    );
    let on_post = m.face(post_r, "+X").mv(-12.0, 22.0, 0.0);
    let turn = deg(6.28319);
    m.joint(
        "focus_knob_spin",
        &knob,
        on_post,
        post_r,
        Revolute,
        PX,
        Some((-turn, turn)),
    );
    m.joint(
        "left_eyecup_twist",
        &left_cup,
        left_end,
        left_eyepiece,
        Revolute,
        PY,
        Some((0.0, deg(4.2))),
    );
    m.joint(
        "right_eyecup_twist",
        &right_cup,
        right_end,
        right_eyepiece,
        Revolute,
        PY,
        Some((0.0, deg(4.2))),
    );
    m
}

/// Gallery A10, `compact_pickup_tailgate`.
pub fn tailgate() -> Model {
    let mut m = Model::new("compact_pickup_tailgate");
    let (paint, liner, hardware, metal) = (
        [0.16, 0.24, 0.34],
        [0.17, 0.17, 0.18],
        [0.10, 0.10, 0.11],
        [0.65, 0.66, 0.68],
    );
    let block =
        |m: &mut Model, name: &str, size: [f64; 3], c: [f64; 3], at: [f64; 3], color: [f64; 3]| {
            let id = m.add(
                name,
                centred(
                    size[0],
                    size[1],
                    size[2],
                    c[0] + at[0],
                    c[1] + at[1],
                    c[2] + at[2],
                ),
            );
            m.material(&[id], &material("painted_metal", Some(color)));
            id
        };
    let rod = |m: &mut Model,
               name: &str,
               d: f64,
               len: f64,
               c: [f64; 3],
               along_y: bool,
               at_: [f64; 3],
               color: [f64; 3]| {
        let (x, y, z) = (c[0] + at_[0], c[1] + at_[1], c[2] + at_[2]);
        let s = if along_y {
            at(cylinder(d, len), [x, y + len / 2.0, z], [90.0, 0.0, 0.0])
        } else {
            at(cylinder(d, len), [x - len / 2.0, y, z], [0.0, 90.0, 0.0])
        };
        let id = m.add(name, s);
        m.material(&[id], &material("steel", Some(color)));
        id
    };
    let o = [0.0; 3];
    block(
        &mut m,
        "bed_floor",
        [610.0, 1280.0, 28.0],
        [322.0, 0.0, 86.0],
        o,
        liner,
    );
    let sill = block(
        &mut m,
        "left_sill_pad",
        [60.0, 160.0, 28.0],
        [-12.0, 180.0, 86.0],
        o,
        liner,
    );
    block(
        &mut m,
        "right_sill_pad",
        [60.0, 160.0, 28.0],
        [-12.0, -180.0, 86.0],
        o,
        liner,
    );
    block(
        &mut m,
        "left_bedside",
        [610.0, 60.0, 390.0],
        [322.0, 670.0, 295.0],
        o,
        paint,
    );
    block(
        &mut m,
        "right_bedside",
        [610.0, 60.0, 390.0],
        [322.0, -670.0, 295.0],
        o,
        paint,
    );
    block(
        &mut m,
        "left_rail_cap",
        [610.0, 88.0, 36.0],
        [322.0, 656.0, 492.0],
        o,
        liner,
    );
    block(
        &mut m,
        "right_rail_cap",
        [610.0, 88.0, 36.0],
        [322.0, -656.0, 492.0],
        o,
        liner,
    );
    block(
        &mut m,
        "left_hinge_support",
        [82.0, 40.0, 70.0],
        [-10.0, 655.0, 115.0],
        o,
        paint,
    );
    block(
        &mut m,
        "right_hinge_support",
        [82.0, 40.0, 70.0],
        [-10.0, -655.0, 115.0],
        o,
        paint,
    );
    block(
        &mut m,
        "front_bulkhead",
        [28.0, 1400.0, 420.0],
        [641.0, 0.0, 310.0],
        o,
        paint,
    );
    let t = [-36.0, 0.0, 100.0];
    let mut gate = vec![
        block(
            &mut m,
            "outer_skin",
            [18.0, 1240.0, 332.0],
            [15.0, 0.0, 206.0],
            t,
            paint,
        ),
        block(
            &mut m,
            "bottom_rail",
            [45.0, 1240.0, 40.0],
            [23.0, 0.0, 20.0],
            t,
            paint,
        ),
        block(
            &mut m,
            "top_cap",
            [34.0, 1240.0, 30.0],
            [20.0, 0.0, 347.0],
            t,
            paint,
        ),
        block(
            &mut m,
            "left_side_frame",
            [45.0, 26.0, 338.0],
            [23.0, 607.0, 209.0],
            t,
            paint,
        ),
        block(
            &mut m,
            "right_side_frame",
            [45.0, 26.0, 338.0],
            [23.0, -607.0, 209.0],
            t,
            paint,
        ),
        rod(
            &mut m,
            "left_hinge_barrel",
            26.0,
            160.0,
            [10.0, 360.0, 0.0],
            true,
            t,
            metal,
        ),
        rod(
            &mut m,
            "center_hinge_barrel",
            26.0,
            120.0,
            [10.0, 0.0, 0.0],
            true,
            t,
            metal,
        ),
        rod(
            &mut m,
            "right_hinge_barrel",
            26.0,
            160.0,
            [10.0, -360.0, 0.0],
            true,
            t,
            metal,
        ),
        block(
            &mut m,
            "inner_panel",
            [6.0, 1130.0, 232.0],
            [41.0, 0.0, 192.0],
            t,
            liner,
        ),
        block(
            &mut m,
            "left_inner_flange",
            [10.0, 22.0, 290.0],
            [37.0, 594.0, 190.0],
            t,
            liner,
        ),
        block(
            &mut m,
            "right_inner_flange",
            [10.0, 22.0, 290.0],
            [37.0, -594.0, 190.0],
            t,
            liner,
        ),
        block(
            &mut m,
            "top_inner_flange",
            [10.0, 1160.0, 22.0],
            [37.0, 0.0, 323.0],
            t,
            liner,
        ),
        block(
            &mut m,
            "bottom_inner_flange",
            [10.0, 1160.0, 24.0],
            [37.0, 0.0, 50.0],
            t,
            liner,
        ),
        block(
            &mut m,
            "upper_inner_rib",
            [12.0, 940.0, 56.0],
            [35.0, 0.0, 262.0],
            t,
            liner,
        ),
        block(
            &mut m,
            "lower_inner_rib",
            [12.0, 940.0, 60.0],
            [35.0, 0.0, 122.0],
            t,
            liner,
        ),
        block(
            &mut m,
            "center_inner_rib",
            [10.0, 660.0, 104.0],
            [34.0, 0.0, 192.0],
            t,
            liner,
        ),
    ];
    let bezels: Vec<Id> = [("left", 435.0), ("right", -435.0)]
        .iter()
        .map(|&(s, y)| {
            block(
                &mut m,
                &format!("{s}_latch_bezel"),
                [6.0, 56.0, 32.0],
                [37.0, y, 304.0],
                t,
                hardware,
            )
        })
        .collect();
    let pads: Vec<Id> = [("left", 340.0), ("right", -340.0)]
        .iter()
        .map(|&(s, y)| {
            block(
                &mut m,
                &format!("{s}_work_stop_pad"),
                [6.0, 45.0, 18.0],
                [35.0, y, 349.0],
                t,
                hardware,
            )
        })
        .collect();
    gate.extend(&bezels);
    gate.extend(&pads);
    let mut knobs = Vec::new();
    for (s, y) in [("left", 435.0), ("right", -435.0)] {
        let origin = [10.0, y, 404.0];
        knobs.push(vec![
            rod(
                &mut m,
                &format!("{s}_shaft"),
                10.0,
                12.0,
                o,
                false,
                origin,
                metal,
            ),
            rod(
                &mut m,
                &format!("{s}_hub"),
                24.0,
                10.0,
                [10.0, 0.0, 0.0],
                false,
                origin,
                hardware,
            ),
            block(
                &mut m,
                &format!("{s}_paddle"),
                [8.0, 40.0, 14.0],
                [15.0, 0.0, 0.0],
                origin,
                hardware,
            ),
            block(
                &mut m,
                &format!("{s}_pointer"),
                [6.0, 12.0, 10.0],
                [19.0, 0.0, 12.0],
                origin,
                hardware,
            ),
        ]);
    }
    let w = [9.0, 0.0, 449.0];
    let stop = vec![
        rod(
            &mut m,
            "left_sleeve",
            14.0,
            40.0,
            [0.0, 340.0, 0.0],
            true,
            w,
            metal,
        ),
        rod(
            &mut m,
            "right_sleeve",
            14.0,
            40.0,
            [0.0, -340.0, 0.0],
            true,
            w,
            metal,
        ),
        block(
            &mut m,
            "left_arm",
            [10.0, 28.0, 84.0],
            [6.0, 340.0, -42.0],
            w,
            metal,
        ),
        block(
            &mut m,
            "right_arm",
            [10.0, 28.0, 84.0],
            [6.0, -340.0, -42.0],
            w,
            metal,
        ),
        block(
            &mut m,
            "stop_bar",
            [12.0, 1000.0, 18.0],
            [10.0, 0.0, -78.0],
            w,
            metal,
        ),
    ];
    let on_sill = m.face(sill, "+Z").mv(-36.0, 0.0, 0.0);
    m.joint(
        "bed_to_tailgate",
        &gate,
        on_sill,
        sill,
        Revolute,
        NY,
        Some((0.0, deg(1.50098))),
    );
    for ((side, y), (bezel, k)) in [("left", 435.0), ("right", -435.0)]
        .into_iter()
        .zip(bezels.iter().zip(&knobs))
    {
        let on_bezel = m.face(*bezel, "+X").mv(y, 404.0, 0.0).offset(6.0);
        let r = deg(1.2);
        m.joint(
            &format!("tailgate_to_{side}_latch_knob"),
            k,
            on_bezel,
            *bezel,
            Revolute,
            Axis::Normal,
            Some((-r, r)),
        );
    }
    let on_pad = m.face(pads[0], "+X").mv(0.0, 449.0, 0.0).offset(7.0);
    m.joint(
        "tailgate_to_work_stop",
        &stop,
        on_pad,
        pads[0],
        Revolute,
        NY,
        Some((0.0, deg(1.36136))),
    );
    m
}

/// Gallery A4, `compact_trackpoint_keyboard`.
pub fn keyboard() -> Model {
    let mut m = Model::new("compact_trackpoint_keyboard");
    const COLUMNS: [f64; 10] = [
        -89.5, -70.5, -51.5, -32.5, -13.5, 13.5, 32.5, 51.5, 70.5, 89.5,
    ];
    const ROWS: [(f64, f64, f64, f64); 4] = [
        (-28.0, 14.6, 1.3, 6.0),
        (-9.0, 15.8, 0.5, 0.0),
        (10.0, 17.1, -0.4, -4.0),
        (29.0, 18.3, -1.2, -8.0),
    ];
    let case_c = [0.18, 0.19, 0.21];
    let deck_c = [0.11, 0.12, 0.13];
    #[allow(clippy::too_many_arguments)]
    fn slab(
        m: &mut Model,
        name: &str,
        w: f64,
        d: f64,
        h: f64,
        x: f64,
        y: f64,
        zc: f64,
        c: [f64; 3],
    ) -> Id {
        let id = m.add(name, centred(w, d, h, x, y, zc));
        m.material(&[id], &material("plastic", Some(c)));
        id
    }
    let keycap = |h: f64, shift: f64| {
        let w = 16.2;
        loft(vec![
            (rect_r(w, w, w * 0.15), 0.0, 0.0, 0.0),
            (
                rect_r(w * 0.95, w * 0.92, w * 0.13),
                0.0,
                shift * 0.45,
                h * 0.55,
            ),
            (rect_r(w * 0.86, w * 0.79, w * 0.10), 0.0, shift, h),
        ])
    };
    let deck = slab(
        &mut m, "key_deck", 238.0, 90.0, 4.0, 0.0, -2.0, 10.0, deck_c,
    );
    let top = m.face(deck, "+Z");
    slab(
        &mut m,
        "bottom_shell",
        278.0,
        118.0,
        4.0,
        0.0,
        0.0,
        2.0,
        case_c,
    );
    slab(
        &mut m,
        "front_lip",
        278.0,
        10.0,
        14.0,
        0.0,
        -54.0,
        7.0,
        case_c,
    );
    slab(
        &mut m,
        "rear_wall",
        278.0,
        8.0,
        31.0,
        0.0,
        55.0,
        15.5,
        case_c,
    );
    slab(
        &mut m,
        "left_wall",
        8.0,
        100.0,
        25.0,
        -135.0,
        0.0,
        12.5,
        case_c,
    );
    slab(
        &mut m,
        "right_wall",
        8.0,
        100.0,
        25.0,
        135.0,
        0.0,
        12.5,
        case_c,
    );
    slab(
        &mut m,
        "rear_control_shelf",
        238.0,
        14.0,
        6.0,
        0.0,
        45.0,
        13.0,
        deck_c,
    );
    slab(
        &mut m,
        "media_pod_base",
        36.0,
        28.0,
        6.0,
        108.0,
        45.0,
        9.0,
        deck_c,
    );
    slab(
        &mut m,
        "media_pod_front_cheek",
        22.0,
        4.0,
        20.0,
        110.0,
        33.0,
        22.0,
        case_c,
    );
    slab(
        &mut m,
        "media_pod_rear_cheek",
        22.0,
        4.0,
        20.0,
        110.0,
        57.0,
        22.0,
        case_c,
    );
    slab(
        &mut m,
        "media_pod_outer_wall",
        6.0,
        24.0,
        18.0,
        126.0,
        45.0,
        21.0,
        case_c,
    );
    slab(
        &mut m,
        "media_pod_inner_boss",
        4.0,
        18.0,
        12.0,
        94.0,
        45.0,
        18.0,
        case_c,
    );
    let along_y = |d: f64, h: f64, yc: f64| {
        on(
            cylinder(d, h),
            &frame([110.0, yc - h / 2.0, 24.0], [0.0, 1.0, 0.0]),
        )
    };
    let spindle = m.add("spindle", along_y(5.0, 20.0, 45.0));
    let shoulder = m.add("front_shoulder", along_y(8.4, 2.0, 36.0));
    let clip = m.add("rear_clip", along_y(9.6, 1.6, 54.2));
    m.material(
        &[spindle, shoulder, clip],
        &material("steel", rgb(0.60, 0.61, 0.63)),
    );
    for (row, &(y, z, shift, offset)) in ROWS.iter().enumerate() {
        for (col, &x) in COLUMNS.iter().enumerate() {
            let key = keycap(9.0 - row as f64 * 0.2, shift);
            let key = m.add(&format!("key_r{row}_c{col}"), at_xyz(key, x + offset, y, z));
            let dark = row == 3 || col == 0 || col == 9;
            let c = if dark {
                [0.36, 0.37, 0.39]
            } else {
                [0.82, 0.82, 0.80]
            };
            m.material(&[key], &material("plastic", Some(c)));
            let on_top = top.mv(x + offset, y, z - 12.0);
            m.joint(
                &format!("case_to_key_r{row}_c{col}"),
                &[key],
                on_top,
                deck,
                Prismatic,
                NZ,
                Some((0.0, 2.2)),
            );
        }
    }
    let mount = m.add(
        "trackpoint_mount",
        at_xyz(cylinder(9.2, 4.0), 0.0, -9.0, 12.0),
    );
    m.material(&[mount], &material("plastic", rgb(0.12, 0.12, 0.12)));
    let tilt = deg(0.18);
    m.joint(
        "case_to_trackpoint_mount",
        &[mount],
        top.mv(0.0, -9.0, 0.0),
        deck,
        Revolute,
        PX,
        Some((-tilt, tilt)),
    );
    let stem = m.add("nub_stem", at_xyz(cylinder(6.2, 8.0), 0.0, -9.0, 16.0));
    let tip = m.add("nub_tip", at_xyz(sphere(7.2), 0.0, -9.0, 16.0 + 10.5 - 3.6));
    m.material(&[stem, tip], &material("rubber", rgb(0.74, 0.10, 0.10)));
    let on_mount = m.face(mount, "top").mv(0.0, -9.0, 0.0);
    m.joint(
        "trackpoint_mount_to_nub",
        &[stem, tip],
        on_mount,
        mount,
        Revolute,
        PY,
        Some((-tilt, tilt)),
    );
    let ring = |r: f64, t: f64, dy: f64| {
        on(
            torus(2.0 * r, 2.0 * t),
            &frame([110.0, 45.0 + dy - t, 24.0], [0.0, 1.0, 0.0]),
        )
    };
    let wheel = merge(vec![
        ring(7.2, 1.8, 0.0),
        ring(7.4, 1.9, 0.0),
        ring(7.6, 1.7, -3.2),
        ring(7.6, 1.7, 3.2),
    ]);
    let wheel = m.add("media_wheel", wheel);
    m.material(&[wheel], &material("rubber", rgb(0.15, 0.16, 0.17)));
    let on_spindle = m.bbox(&[spindle], "+Y");
    m.joint(
        "wheel_shaft_to_media_wheel",
        &[wheel],
        on_spindle,
        spindle,
        Continuous,
        PY,
        None,
    );
    m
}

/// Gallery A12, `bevel_to_spur_transfer`. The bevel gears' 14 mm bores
/// are left out: sand3d cuts them with a boolean, and a loft takes
/// single-contour sections only (both bores sit inside a hub).
pub fn bevel_transfer() -> Model {
    let mut m = Model::new("bevel_to_spur_transfer");
    const APEX: f64 = 59.02;
    const BEVEL_H: f64 = 31.5;
    const FLANK: [(f64, f64); 7] = [
        (48.6, 8.0),
        (50.0, 6.6),
        (52.5, 5.45),
        (57.5, 4.55),
        (62.1, 3.4),
        (65.2, 2.45),
        (69.08, 1.35),
    ];
    let outline = |scale: f64| {
        let mut pts = Vec::new();
        for k in 0..18 {
            let c = 10.0 + 20.0 * f64::from(k);
            let side: Vec<(f64, f64)> = FLANK
                .iter()
                .map(|&(r, a)| (r, c - a))
                .chain(FLANK.iter().rev().map(|&(r, a)| (r, c + a)))
                .chain([(48.5, c + 10.0)])
                .collect();
            for (r, t) in side {
                let (s, co) = trig::sin_cos(trig::to_radians(t));
                pts.push([scale * r * co, scale * r * s]);
            }
        }
        poly(&pts)
    };
    let bevel = |heel: &shape_core::Frame| {
        let g = loft(vec![
            (outline(1.0), 0.0, 0.0, 0.0),
            (outline((APEX - BEVEL_H) / APEX), 0.0, 0.0, BEVEL_H),
        ]);
        on(g, heel)
    };
    let along_x = |s, x: f64, y: f64, z: f64, length: f64| {
        on(
            s,
            &frame_up([x - length / 2.0, y, z], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        )
    };
    let (shaft_z, out_y) = (263.5, 127.0);
    let base_c = material("painted_metal", rgb(0.12, 0.14, 0.15));
    let bearing_c = material("painted_metal", rgb(0.025, 0.027, 0.03));
    let steel = material("brushed_steel", None);
    let brass = material("brass", None);
    let plate = m.add(
        "base_plate",
        at_xyz(box_(860.0, 380.0, 40.0), 360.0, 70.0, 0.0),
    );
    let lower = m.add(
        "lower_vertical_bearing",
        at_xyz(tube(72.0, 28.0, 52.0), 160.0, 0.0, 40.0),
    );
    m.material(&[plate], &base_c);
    m.material(&[lower], &bearing_c);
    let mut bearings = Vec::new();
    for (x, y, ped, brg) in [
        (280.0, 0.0, "transfer_pedestal_0", "transfer_bearing_0"),
        (660.0, 0.0, "transfer_pedestal_1", "transfer_bearing_1"),
        (370.0, out_y, "output_pedestal_0", "output_bearing_0"),
        (640.0, out_y, "output_pedestal_1", "output_bearing_1"),
    ] {
        let b = m.add(ped, at_xyz(box_(52.0, 66.0, shaft_z - 78.0), x, y, 40.0));
        m.material(&[b], &base_c);
        let id = m.add(brg, along_x(tube(78.0, 31.0, 40.0), x, y, shaft_z, 40.0));
        m.material(&[id], &bearing_c);
        bearings.push(id);
    }
    // input shaft, origin (160, 0, 205), vertical
    let z0 = 205.0;
    let bar = m.add(
        "input_shaft_bar",
        at_xyz(cylinder(20.0, 194.0), 160.0, 0.0, z0 - 67.0 - 97.0),
    );
    let in_bevel = m.add(
        "input_bevel",
        bevel(&frame([160.0, 0.0, z0], [0.0, 0.0, 1.0])),
    );
    let hub = m.add(
        "input_hub",
        at_xyz(cylinder(42.0, 34.0), 160.0, 0.0, z0 + 32.0 - 17.0),
    );
    let collar = m.add(
        "input_bearing_collar",
        at_xyz(cylinder(40.0, 8.0), 160.0, 0.0, z0 - 109.0 - 4.0),
    );
    m.material(&[bar, hub, collar], &steel);
    m.material(&[in_bevel], &brass);
    let input = [bar, in_bevel, hub, collar];
    // transfer shaft, origin (160, 0, 263.5), along +X
    let x0 = 160.0;
    let t_bar = m.add(
        "transfer_shaft_bar",
        along_x(cylinder(20.0, 620.0), x0 + 230.0, 0.0, shaft_z, 620.0),
    );
    let heel = frame_up(
        [x0 - APEX, 0.0, shaft_z + 0.52],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
    );
    let t_bevel = m.add("transfer_bevel", bevel(&heel));
    let t_hub = m.add(
        "bevel_hub",
        along_x(cylinder(40.0, 30.0), x0 - 41.0, 0.0, shaft_z, 30.0),
    );
    let mut transfer = vec![t_bar, t_bevel, t_hub];
    let mut steel_parts = vec![t_bar, t_hub];
    for (i, x) in [96.0, 144.0, 476.0, 524.0].into_iter().enumerate() {
        let c = m.add(
            &format!("transfer_collar_{i}"),
            along_x(cylinder(36.0, 8.0), x0 + x, 0.0, shaft_z, 8.0),
        );
        transfer.push(c);
        steel_parts.push(c);
    }
    let t_spur = m.add(
        "transfer_spur",
        along_x(spur(18, 6.0, 36.0, 14.0), x0 + 340.0, 0.0, shaft_z, 36.0),
    );
    let t_spur_hub = m.add(
        "transfer_spur_hub",
        along_x(cylinder(42.0, 44.0), x0 + 340.0, 0.0, shaft_z, 44.0),
    );
    transfer.extend([t_spur, t_spur_hub]);
    steel_parts.push(t_spur_hub);
    m.material(&steel_parts, &steel);
    m.material(&[t_bevel, t_spur], &brass);
    // output shaft, origin (160, 127, 263.5), along +X
    let o_bar = m.add(
        "output_shaft_bar",
        along_x(cylinder(20.0, 430.0), x0 + 300.0, out_y, shaft_z, 430.0),
    );
    let mut output = vec![o_bar];
    let mut o_steel = vec![o_bar];
    for (i, x) in [186.0, 234.0, 456.0, 504.0].into_iter().enumerate() {
        let c = m.add(
            &format!("output_collar_{i}"),
            along_x(cylinder(36.0, 8.0), x0 + x, out_y, shaft_z, 8.0),
        );
        output.push(c);
        o_steel.push(c);
    }
    let o_spur = m.add(
        "output_spur",
        along_x(spur(24, 6.0, 40.0, 14.0), x0 + 340.0, out_y, shaft_z, 40.0),
    );
    let o_spur_hub = m.add(
        "output_spur_hub",
        along_x(cylinder(48.0, 50.0), x0 + 340.0, out_y, shaft_z, 50.0),
    );
    output.extend([o_spur, o_spur_hub]);
    o_steel.push(o_spur_hub);
    m.material(&o_steel, &steel);
    m.material(&[o_spur], &material("painted_metal", rgb(0.13, 0.25, 0.46)));

    let on_lower = m.bbox(&[lower], "+Z");
    m.joint("input_axis", &input, on_lower, lower, Continuous, PZ, None);
    let on_t = m.bbox(&[bearings[0]], "+X");
    m.joint(
        "transfer_axis",
        &transfer,
        on_t,
        bearings[0],
        Continuous,
        PX,
        None,
    );
    m.follows("transfer_axis", "input_axis", -1.0);
    let on_o = m.bbox(&[bearings[2]], "+X");
    m.joint(
        "output_axis",
        &output,
        on_o,
        bearings[2],
        Continuous,
        PX,
        None,
    );
    m.follows("output_axis", "transfer_axis", -18.0 / 24.0);
    m
}

/// `docs/examples/gate.py`: a gate whose leaf opens 60°, and its mirror
/// image. sand3d's `copy` of the jointed leaf also copies its hinge; here
/// the image's hinge is declared on the image's post, about −Z, so that it
/// too opens toward +Y.
pub fn gate() -> Model {
    let mut m = Model::new("gate");
    let (w, h, post) = (750.0, 1200.0, 100.0);
    let l_post = m.add(
        "post",
        at_xyz(box_(post, post, h + 300.0), -w - post / 2.0, 0.0, 0.0),
    );
    let r_post = m.add(
        "post",
        mirror_x(at_xyz(
            box_(post, post, h + 300.0),
            -w - post / 2.0,
            0.0,
            0.0,
        )),
    );
    let board = at_xyz(box_(w, 40.0, h), -w / 2.0, 0.0, 150.0);
    let leaf_board = m.add("board", board.clone());
    let f = m.bbox(&[leaf_board], "+Y").mv(60.0 - w / 2.0, 0.0, 0.0);
    let handle = on(cylinder(20.0, 60.0), &f);
    let leaf_handle = m.add("handle", handle.clone());
    let leaf = [leaf_board, leaf_handle];
    let twin = [
        m.add("board", mirror_x(board)),
        m.add("handle", mirror_x(handle)),
    ];
    let on_post = m.bbox(&[l_post], "+X+Y");
    m.joint(
        "hinge",
        &leaf,
        on_post,
        l_post,
        Revolute,
        PZ,
        Some((0.0, 90.0)),
    );
    let on_twin = m.bbox(&[r_post], "-X+Y");
    m.joint(
        "hinge.001",
        &twin,
        on_twin,
        r_post,
        Revolute,
        NZ,
        Some((0.0, 90.0)),
    );
    m.pose("hinge", 60.0);
    m
}
