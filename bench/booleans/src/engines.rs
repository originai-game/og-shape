//! The engines under test, behind one call.

use std::time::Instant;

use glam::DVec3;
use manifold_rust::manifold::Manifold;
use manifold_rust::types::{BooleanEngine, Error, MeshGL, OpType};
use shape_bool::{CsgOp, csg, csg_points_traced, self_union_points, selfcheck, uncross_output};
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
    /// The kernel's f64 entry (`csg_points`) with the exact operand check
    /// up front: an operand with a shell crossing itself is refused,
    /// separate shells are joined by self-union, an open result is refused.
    OgShapeChecked,
    /// The kernel's f64 entry after untangling each operand by self-union
    /// (`self_union_points`); an open result is refused.
    OgShapeUntangled,
}

impl Engine {
    pub const ALL: [Engine; 5] = [
        Engine::OgGeom,
        Engine::ManifoldExact,
        Engine::ManifoldRobust,
        Engine::OgShapeChecked,
        Engine::OgShapeUntangled,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Engine::OgGeom => "og-geom",
            Engine::ManifoldExact => "manifold-exact",
            Engine::ManifoldRobust => "manifold-robust",
            Engine::OgShapeChecked => "og-shape-checked",
            Engine::OgShapeUntangled => "og-shape-untangled",
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
    /// Of `ms`, the time spent checking or untangling operands.
    pub prep_ms: f64,
    /// The volumes of the operands the engine used, where it changed them
    /// (untangled): the identities hold for these.
    pub used: Option<(f64, f64)>,
    /// With [`COUNT_CROSSINGS`], the og-shape engines' f64 output's pairs
    /// of triangles crossing within one shell (`docs/research.md` §18).
    pub crossings: Option<usize>,
}

/// Whether the og-shape engines count their f64 outputs' self-crossings
/// (`--crossings`; outside the timing).
pub static COUNT_CROSSINGS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Whether the og-shape engines repair their outputs' crossings by a
/// rounding error (`uncross_output`, inside the timing), as
/// `shape_bool::boolean` does; off with `--no-uncross`
/// (`docs/research.md` §23, §25).
pub static UNCROSS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// A mesh welded by exact position, in f64.
struct Welded {
    pos: Vec<DVec3>,
    tris: Vec<[u32; 3]>,
}

fn welded(t: &Tri) -> Welded {
    let mut id: std::collections::BTreeMap<[u32; 3], u32> = Default::default();
    let mut pos = Vec::new();
    let w: Vec<u32> = t
        .pos
        .iter()
        .map(|p| {
            *id.entry(p.map(|x| (x + 0.0).to_bits())).or_insert_with(|| {
                pos.push(DVec3::from(p.map(f64::from)));
                pos.len() as u32 - 1
            })
        })
        .collect();
    let tris = t
        .idx
        .chunks_exact(3)
        .map(|c| [w[c[0] as usize], w[c[1] as usize], w[c[2] as usize]])
        .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
        .collect();
    Welded { pos, tris }
}

/// Crossing pairs within one shell, and the number of shells.
fn shell_crossings(m: &Welded) -> (usize, usize) {
    let mut parent: Vec<u32> = (0..m.pos.len() as u32).collect();
    fn find(p: &mut [u32], mut i: u32) -> u32 {
        while p[i as usize] != i {
            p[i as usize] = p[p[i as usize] as usize];
            i = p[i as usize];
        }
        i
    }
    for t in &m.tris {
        for k in 1..3 {
            let (a, b) = (find(&mut parent, t[0]), find(&mut parent, t[k]));
            if a != b {
                parent[a.max(b) as usize] = a.min(b);
            }
        }
    }
    let shell: Vec<u32> = m.tris.iter().map(|t| find(&mut parent, t[0])).collect();
    let mut shells = shell.clone();
    shells.sort_unstable();
    shells.dedup();
    let n = selfcheck::self_crossings(&m.pos, &m.tris)
        .into_iter()
        .filter(|&(i, j)| shell[i as usize] == shell[j as usize])
        .count();
    (n, shells.len())
}

/// An f64 mesh welded by exact position, degenerate triangles left out.
fn welded64(points: &[DVec3], tris: &[[u32; 3]]) -> Welded {
    let mut id: std::collections::BTreeMap<[u64; 3], u32> = Default::default();
    let mut pos = Vec::new();
    let w: Vec<u32> = points
        .iter()
        .map(|p| {
            *id.entry(p.to_array().map(|x| (x + 0.0).to_bits()))
                .or_insert_with(|| {
                    pos.push(*p);
                    pos.len() as u32 - 1
                })
        })
        .collect();
    let tris = tris
        .iter()
        .map(|t| t.map(|v| w[v as usize]))
        .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
        .collect();
    Welded { pos, tris }
}

fn from_points(points: &[DVec3], tris: &[[u32; 3]]) -> Tri {
    Tri {
        pos: points.iter().map(|p| p.as_vec3().to_array()).collect(),
        idx: tris.iter().flatten().copied().collect(),
    }
}

/// The two og-shape engines: operands prepared (checked or untangled),
/// then `csg_points`, then the result's closure.
#[allow(clippy::type_complexity)]
fn og_shape(
    a: &Tri,
    b: &Tri,
    op: CsgOp,
    untangle: bool,
) -> (Result<Tri, String>, f64, Option<(f64, f64)>, Option<usize>) {
    let start = Instant::now();
    let mut ops = Vec::new();
    for (name, t) in [("A", a), ("B", b)] {
        let m = welded(t);
        let prepared = if untangle {
            self_union_points((&m.pos, &m.tris))
                .map(|u| Welded {
                    pos: u.points,
                    tris: u.tris,
                })
                .map_err(|e| format!("{name}: {e:?}"))
        } else {
            let (n, shells) = shell_crossings(&m);
            if n > 0 {
                Err(format!("{name} crosses itself ({n} pairs)"))
            } else if shells > 1 {
                self_union_points((&m.pos, &m.tris))
                    .map(|u| Welded {
                        pos: u.points,
                        tris: u.tris,
                    })
                    .map_err(|e| format!("{name}: {e:?}"))
            } else {
                Ok(m)
            }
        };
        match prepared {
            Ok(m) => ops.push(m),
            Err(e) => return (Err(e), start.elapsed().as_secs_f64() * 1000.0, None, None),
        }
    }
    let prep = start.elapsed().as_secs_f64() * 1000.0;
    let vol = |m: &Welded| {
        m.tris
            .iter()
            .map(|t| {
                let [p, q, r] = t.map(|v| m.pos[v as usize]);
                p.dot(q.cross(r)) / 6.0
            })
            .sum::<f64>()
    };
    let used = Some((vol(&ops[0]), vol(&ops[1])));
    let mut crossings = None;
    let r = csg_points_traced((&ops[0].pos, &ops[0].tris), (&ops[1].pos, &ops[1].tris), op)
        .map(|o| {
            let o = if UNCROSS.load(std::sync::atomic::Ordering::Relaxed) {
                uncross_output(&ops[0].pos, &ops[1].pos, o).0
            } else {
                o
            };
            (o.points, o.tris)
        })
        .map_err(|e| format!("{e:?}"))
        .and_then(|(p, t)| {
            let out = from_points(&p, &t);
            if t.is_empty() || out.closed() {
                if COUNT_CROSSINGS.load(std::sync::atomic::Ordering::Relaxed) {
                    crossings = Some(shell_crossings(&welded64(&p, &t)).0);
                }
                Ok(out)
            } else {
                Err("open result".to_string())
            }
        });
    (r, prep, used, crossings)
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
    let mut prep_ms = 0.0;
    let mut used = None;
    let mut crossings = None;
    let csg_op = match op {
        Op::Union => CsgOp::Union,
        Op::Intersect => CsgOp::Intersect,
        Op::Subtract => CsgOp::Subtract,
    };
    let result = match engine {
        Engine::OgShapeChecked | Engine::OgShapeUntangled => {
            let (r, prep, u, x) = og_shape(a, b, csg_op, engine == Engine::OgShapeUntangled);
            prep_ms = prep;
            used = u;
            crossings = x;
            r
        }
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
        prep_ms,
        used,
        crossings,
    }
}
