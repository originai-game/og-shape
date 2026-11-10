//! Golden cases for the authoring surface (`docs/research.md` §14): a
//! program that uses every generator and field the program JSON gained in
//! stream M, replayed, its GLB and measures hashed. Segment counts set by
//! `segments`, the part torus, the dome's height, the loft's curve, the
//! turned solids' axis, `wire`, `body`, `knurl`, `gear`, `scale`, and the
//! profiles' arc edge (`bulge`), across-flats size and scale.

use shape_core::{HashError, OutputHash};
use shape_model::{Model, ModelError, program};

use crate::cases::Case;

const PROGRAM: &str = r#"{"name": "surface", "ops": [
  {"op": "cylinder", "name": "cyl", "radius": 4, "height": 6, "segments": 8},
  {"op": "cone", "name": "cone", "radius": 3, "height": 5, "segments": 6,
   "on": {"origin": [12, 0, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "tube", "name": "tube", "outer": 4, "inner": 3, "height": 2, "segments": 10,
   "on": {"origin": [24, 0, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "sphere", "name": "ball", "radius": 4, "segments": 12, "axis": "+Z",
   "on": {"origin": [0, 12, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "dome", "name": "cap", "radius": 5, "height": 2, "axis": "+Z", "segments": 16,
   "on": {"origin": [12, 12, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "torus", "name": "elbow", "radius": 6, "tube": 1.5, "angle": 1.5707963267948966,
   "axis": "+Z", "segments": 24, "tubeSegments": 8,
   "on": {"origin": [24, 12, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "revolve", "name": "vase", "axis": "+Z", "segments": 20,
   "profile": {"poly": {"points": [[0, 0], [4, 0], {"x": 4, "y": 8, "bulge": 1}, [0, 8]]}},
   "on": {"origin": [0, 24, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "loft", "name": "horn", "curve": "spline", "steps": 3, "sections": [
    {"profile": {"circle": {"radius": 3}}, "z": 0},
    {"profile": {"scale": {"profile": {"circle": {"radius": 3}}, "x": 0.5}}, "z": 4},
    {"profile": {"rect": {"width": 3, "height": 2}}, "z": 8}],
   "on": {"origin": [12, 24, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "extrude", "name": "nut", "height": 3,
   "profile": {"subtract": [{"polygon": {"sides": 6, "flats": 8}}, {"circle": {"radius": 2}}]},
   "on": {"origin": [24, 24, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "scale", "id": 8, "z": 1.5},
  {"op": "wire", "name": "rail", "diameter": 1, "fillet": 2,
   "path": [[0, 0, 0], [10, 0, 0], [10, 6, 0], [10, 6, 5]],
   "on": {"origin": [0, 36, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "body", "name": "hull", "plan": [[0, 2], [10, 6], [20, 2]],
   "top": {"points": [[0, 1], [10, 3], [20, 1]], "smooth": true}, "bottom": -1, "n": 3,
   "on": {"origin": [20, 36, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "knurl", "name": "grip", "diameter": 6, "height": 4, "pitch": 1.5,
   "on": {"origin": [0, 60, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "gear", "name": "spur", "teeth": 12, "module": 1, "width": 3, "bore": 3, "hub": 6,
   "hubHeight": 2,
   "on": {"origin": [12, 60, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "extrude", "name": "tab", "height": 1,
   "profile": {"offset": {"profile": {"translate": {"profile": {"rotate": {"profile":
     {"slot": {"length": 6, "width": 2}}, "angle": 0.3}}, "x": 1, "y": 0}}, "distance": 0.5}},
   "on": {"origin": [24, 60, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}}
]}"#;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "surface.program.glb",
            run: |h| surface(h, |m| m.glb()),
        },
        Case {
            name: "surface.program.measure",
            run: |h| surface(h, |m| program::measures_json(m).map(String::into_bytes)),
        },
    ]
}

fn surface(
    h: &mut OutputHash,
    out: impl Fn(&Model) -> Result<Vec<u8>, ModelError>,
) -> Result<(), HashError> {
    let fail = |e: ModelError| {
        eprintln!("{e}");
        HashError::NaN
    };
    let m = program::run(PROGRAM).map_err(fail)?;
    for p in m.measure().map_err(fail)? {
        if !p.measures.closed || p.measures.degenerate_triangles > 0 {
            eprintln!("surface: {} is open or has degenerate triangles", p.name);
            return Err(HashError::NaN);
        }
    }
    h.bytes(&out(&m).map_err(fail)?);
    Ok(())
}
