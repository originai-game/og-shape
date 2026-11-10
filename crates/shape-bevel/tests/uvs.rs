//! Bevels do not carry UVs yet (`docs/research.md` §17): a solid with
//! UVs is refused, never returned without them.

use shape_bevel::{BevelError, Chamfer, Fillet, Soften, chamfer, fillet, soften};
use shape_gen::solids::{BoxDesc, Generator, textured};

#[test]
fn a_solid_with_uvs_is_refused_not_stripped() {
    let s = textured(&Generator::Box(BoxDesc {
        width: 20.0,
        depth: 20.0,
        height: 10.0,
    }))
    .unwrap();
    let top = vec!["top".to_string()];
    let f = fillet(
        &s,
        &Fillet {
            radius: 1.0,
            edges: top.clone(),
            ..Default::default()
        },
    );
    let c = chamfer(
        &s,
        &Chamfer {
            distance: 1.0,
            edges: top,
            ..Default::default()
        },
    );
    let o = soften(
        &s,
        &Soften {
            radius: 1.0,
            ..Default::default()
        },
    );
    for r in [f, c, o] {
        assert_eq!(r.unwrap_err(), BevelError::HasUvs);
    }
}
