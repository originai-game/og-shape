//! The engines under test, behind one call.

use std::time::Instant;

use manifold_rust::manifold::Manifold;
use manifold_rust::types::{BooleanEngine, Error, MeshGL, OpType};
use shape_bool::{CsgOp, csg};
use shape_gen::Placement;

use crate::mesh::Tri;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    /// og-geom's exact-predicate CSG (shape-bool).
    OgGeom,
    /// manifold-rust, its default exact pipeline (manifold input only).
    ManifoldExact,
    /// manifold-rust's robust engine (exact rationals, accepts soup).
    ManifoldRobust,
}

impl Engine {
    pub const ALL: [Engine; 3] = [
        Engine::OgGeom,
        Engine::ManifoldExact,
        Engine::ManifoldRobust,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Engine::OgGeom => "og-geom",
            Engine::ManifoldExact => "manifold-exact",
            Engine::ManifoldRobust => "manifold-robust",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Union,
    Intersect,
    Subtract,
}

impl Op {
    pub const ALL: [Op; 3] = [Op::Union, Op::Intersect, Op::Subtract];
}

/// One boolean's result: the mesh, or the error value the engine gave.
pub struct Run {
    pub result: Result<Tri, String>,
    pub ms: f64,
}

fn to_manifold(t: &Tri, robust: bool) -> Result<Manifold, String> {
    let mut gl = MeshGL {
        num_prop: 3,
        vert_properties: t.pos.iter().flatten().copied().collect(),
        tri_verts: t.idx.clone(),
        ..Default::default()
    };
    gl.merge();
    let m = if robust {
        Manifold::from_mesh_gl_robust(&gl)
    } else {
        Manifold::from_mesh_gl(&gl)
    };
    match m.status() {
        Error::NoError => Ok(m),
        e => Err(format!("import: {e:?}")),
    }
}

fn from_manifold(m: &Manifold) -> Result<Tri, String> {
    match m.status() {
        Error::NoError => {}
        e => return Err(format!("{e:?}")),
    }
    let gl = m.get_mesh_gl(-1);
    let n = gl.num_prop as usize;
    let pos = gl
        .vert_properties
        .chunks_exact(n)
        .map(|v| [v[0], v[1], v[2]])
        .collect();
    Ok(Tri {
        pos,
        idx: gl.tri_verts.clone(),
    })
}

pub fn run(engine: Engine, a: &Tri, b: &Tri, op: Op) -> Run {
    let start = Instant::now();
    let result = match engine {
        Engine::OgGeom => {
            let op = match op {
                Op::Union => CsgOp::Union,
                Op::Intersect => CsgOp::Intersect,
                Op::Subtract => CsgOp::Subtract,
            };
            csg(
                &a.to_gen(),
                Placement::default(),
                &b.to_gen(),
                Placement::default(),
                op,
            )
            .map(|m| Tri::from_gen(&m))
        }
        Engine::ManifoldExact | Engine::ManifoldRobust => {
            let robust = engine == Engine::ManifoldRobust;
            let kind = if robust {
                BooleanEngine::Robust
            } else {
                BooleanEngine::Exact
            };
            let op = match op {
                Op::Union => OpType::Add,
                Op::Intersect => OpType::Intersect,
                Op::Subtract => OpType::Subtract,
            };
            to_manifold(a, robust)
                .and_then(|ma| to_manifold(b, robust).map(|mb| (ma, mb)))
                .and_then(|(ma, mb)| from_manifold(&ma.boolean_with_engine(&mb, op, kind)))
        }
    };
    Run {
        result,
        ms: start.elapsed().as_secs_f64() * 1000.0,
    }
}
