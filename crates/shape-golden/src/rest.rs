//! Golden cases for the rest of the authoring surface (`docs/research.md`
//! §16): a program that uses every operation and field stream R added to
//! the program JSON, replayed, its GLB, measures and boolean reports
//! hashed. The profiles `superellipse`, `spline`, `path` (three-point and
//! tangent arcs, a spline piece), `stroke`, `hull`, `mirror` and `text`
//! (from the hand-made font of [`crate::font`]); `extrude` `until` a
//! parallel face and a tilted one; `sweep` along a path with a spline,
//! `wire` along a helix and along a tangent arc; `merge`, `delete`; and a
//! checked `cut` that leaves a void.

use shape_core::{HashError, OutputHash};
use shape_model::{Model, ModelError, program};

use crate::cases::Case;

const PROGRAM: &str = r#"{"name": "rest", "ops": [
  {"op": "extrude", "name": "squircle", "height": 2,
   "profile": {"superellipse": {"rx": 5, "ry": 3, "exponent": 4, "segments": 32}}},
  {"op": "extrude", "name": "pebble", "height": 2,
   "profile": {"spline": {"points": [[0, 0], [6, 1], [5, 6], [-1, 4]]}},
   "on": {"origin": [15, 0, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "extrude", "name": "cam", "height": 2,
   "profile": {"path": [[0, -5], {"arc": {"through": [5, 0], "to": [0, 5]}}, [-3, 5],
     {"tangentArc": {"to": [-3, -1]}}, {"spline": {"points": [[-2, -4]]}}]},
   "on": {"origin": [30, 0, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "extrude", "name": "hook", "height": 1,
   "profile": {"stroke": {"path": [[0, 0], [8, 0], {"arc": {"through": [12, 4], "to": [8, 8]}}],
     "width": 2, "join": "miter", "roundCaps": true}},
   "on": {"origin": [45, 0, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "extrude", "name": "belt", "height": 1,
   "profile": {"hull": [{"circle": {"radius": 3, "segments": 16}},
     {"translate": {"profile": {"circle": {"radius": 1, "segments": 8}}, "x": 10}}]},
   "on": {"origin": [0, 15, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "extrude", "name": "flag", "height": 1,
   "profile": {"mirror": {"profile": {"poly": {"points": [[0, 0], [5, 0], [0, 3]]}}, "axis": "x"}},
   "on": {"origin": [20, 15, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "extrude", "name": "label", "height": 0.5,
   "profile": {"text": {"text": "DIO", "font": "test.ttf", "size": 5, "spacing": 0.5}},
   "on": {"origin": [30, 15, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "box", "name": "shelf", "width": 10, "depth": 10, "height": 1,
   "on": {"origin": [0, 30, 8], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "extrude", "name": "leg", "profile": {"rect": {"width": 2, "height": 2}},
   "until": {"part": 7, "face": "bottom"},
   "on": {"origin": [0, 30, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "box", "name": "roof", "width": 20, "depth": 20, "height": 2,
   "on": {"origin": [20, 30, 10], "x": [0.9805806756909202, 0, -0.19611613513818404],
          "y": [0, 1, 0], "z": [0.19611613513818404, 0, 0.9805806756909202]}},
  {"op": "extrude", "name": "post", "profile": {"circle": {"radius": 2, "segments": 12}},
   "until": {"part": 9, "face": "bottom"},
   "on": {"origin": [20, 30, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "sweep", "name": "rail", "profile": {"rect": {"width": 1, "height": 2}},
   "path": [[0, 0, 0], [6, 0, 0], {"spline": {"points": [[10, 4, 2], [14, 0, 4]]}}],
   "on": {"origin": [0, 45, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "wire", "name": "spring", "diameter": 1, "segments": 8,
   "path": {"helix": {"radius": 3, "pitch": 2, "turns": 2.5}},
   "on": {"origin": [25, 45, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "wire", "name": "crook", "diameter": 1, "fillet": 1, "segments": 8,
   "path": [[0, 0, 0], [0, 0, 8], {"tangentArc": {"to": [4, 0, 8]}}, [4, 0, 6],
     {"arc": {"through": [5, 0, 5], "to": [6, 0, 6]}}],
   "on": {"origin": [35, 45, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "box", "name": "left", "width": 2, "depth": 2, "height": 2,
   "on": {"origin": [0, 60, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "box", "name": "right", "width": 2, "depth": 2, "height": 2,
   "on": {"origin": [4, 60, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "merge", "target": 14, "tool": 15},
  {"op": "box", "name": "scrap", "width": 1, "depth": 1, "height": 1},
  {"op": "delete", "id": 16},
  {"op": "box", "name": "block", "width": 10, "depth": 10, "height": 10,
   "on": {"origin": [20, 60, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "box", "name": "void", "width": 4, "depth": 4, "height": 4,
   "on": {"origin": [20, 60, 3], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "cut", "target": 17, "tool": 18, "check": true}
]}"#;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "rest.program.glb",
            run: |h| rest(h, |m| m.glb()),
        },
        Case {
            name: "rest.program.measure",
            run: |h| rest(h, |m| program::measures_json(m).map(String::into_bytes)),
        },
        Case {
            name: "rest.program.booleans",
            run: |h| rest(h, |m| Ok(format!("{:?}", m.booleans()).into_bytes())),
        },
    ]
}

fn rest(
    h: &mut OutputHash,
    out: impl Fn(&Model) -> Result<Vec<u8>, ModelError>,
) -> Result<(), HashError> {
    let fail = |e: ModelError| {
        eprintln!("{e}");
        HashError::NaN
    };
    let mut fonts = program::Fonts::new();
    fonts.insert("test.ttf".into(), crate::font::font());
    let m = program::run_with(PROGRAM, &fonts).map_err(fail)?;
    for p in m.measure().map_err(fail)? {
        if !p.measures.closed || p.measures.degenerate_triangles > 0 {
            eprintln!("rest: {} is open or has degenerate triangles", p.name);
            return Err(HashError::NaN);
        }
    }
    let voids: usize = m.booleans().iter().map(|b| b.report.voids).sum();
    if voids != 1 {
        eprintln!("rest: the cut left {voids} voids, not 1");
        return Err(HashError::NaN);
    }
    h.bytes(&out(&m).map_err(fail)?);
    Ok(())
}
