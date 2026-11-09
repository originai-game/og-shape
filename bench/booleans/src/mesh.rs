//! The benchmark's own triangle mesh, its measures, and builders.

use std::collections::BTreeMap;

use glam::DVec3;
use shape_gen::trig;

/// Positions and triangles, nothing else: what both engines take.
#[derive(Clone, Debug, Default)]
pub struct Tri {
    pub pos: Vec<[f32; 3]>,
    pub idx: Vec<u32>,
}

impl Tri {
    pub fn from_gen(m: &shape_gen::Mesh) -> Tri {
        Tri {
            pos: m.positions.clone(),
            idx: m.indices.clone(),
        }
    }

    pub fn to_gen(&self) -> shape_gen::Mesh {
        shape_gen::Mesh {
            positions: self.pos.clone(),
            normals: vec![[0.0, 1.0, 0.0]; self.pos.len()],
            uvs: vec![[0.0; 2]; self.pos.len()],
            colors: Vec::new(),
            indices: self.idx.clone(),
        }
    }

    pub fn tris(&self) -> usize {
        self.idx.len() / 3
    }

    fn p(&self, i: u32) -> DVec3 {
        DVec3::from(self.pos[i as usize].map(f64::from))
    }

    pub fn map(&self, f: impl Fn(DVec3) -> DVec3) -> Tri {
        Tri {
            pos: self
                .pos
                .iter()
                .map(|p| f(DVec3::from(p.map(f64::from))).as_vec3().to_array())
                .collect(),
            idx: self.idx.clone(),
        }
    }

    pub fn translate(&self, d: DVec3) -> Tri {
        self.map(|p| p + d)
    }

    /// Appends another mesh as more triangles, no boolean.
    pub fn append(&mut self, other: &Tri) {
        let base = self.pos.len() as u32;
        self.pos.extend_from_slice(&other.pos);
        self.idx.extend(other.idx.iter().map(|i| i + base));
    }

    /// The signed volume (positive for a closed mesh facing out).
    pub fn volume(&self) -> f64 {
        self.idx
            .chunks_exact(3)
            .map(|t| self.p(t[0]).dot(self.p(t[1]).cross(self.p(t[2]))) / 6.0)
            .sum()
    }

    /// Welded by exact position, is every edge used as often in one
    /// direction as in the other (closed and oriented, possibly touching
    /// itself along an edge)? Triangles with two corners at one position
    /// are skipped; see [`Tri::degenerate`].
    pub fn closed(&self) -> bool {
        let Some(directed) = self.directed_edges() else {
            return false;
        };
        directed
            .iter()
            .all(|(&(a, b), &n)| directed.get(&(b, a)) == Some(&n))
    }

    /// Triangles with two corners at one position (after exact welding).
    pub fn degenerate(&self) -> usize {
        self.idx
            .chunks_exact(3)
            .filter(|t| {
                let p = |i: u32| self.pos[i as usize].map(|x| (x + 0.0).to_bits());
                p(t[0]) == p(t[1]) || p(t[1]) == p(t[2]) || p(t[0]) == p(t[2])
            })
            .count()
    }

    /// Closed, and every edge used exactly once each way.
    pub fn manifold(&self) -> bool {
        let Some(directed) = self.directed_edges() else {
            return false;
        };
        directed
            .iter()
            .all(|(&(a, b), &n)| n == 1 && directed.get(&(b, a)) == Some(&1))
    }

    fn directed_edges(&self) -> Option<BTreeMap<(u32, u32), u32>> {
        let mut id: BTreeMap<[u32; 3], u32> = BTreeMap::new();
        let w: Vec<u32> = self
            .pos
            .iter()
            .map(|p| {
                let n = id.len() as u32;
                *id.entry(p.map(|x| (x + 0.0).to_bits())).or_insert(n)
            })
            .collect();
        let mut directed: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for t in self.idx.chunks_exact(3) {
            let t = [w[t[0] as usize], w[t[1] as usize], w[t[2] as usize]];
            if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
                continue; // zero area: closes nothing, opens nothing
            }
            for k in 0..3 {
                *directed.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
            }
        }
        Some(directed)
    }

    /// FNV-1a over positions and indices, for comparing hosts.
    pub fn hash(&self) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        let mut eat = |b: u32| {
            for byte in b.to_le_bytes() {
                h ^= u64::from(byte);
                h = h.wrapping_mul(0x100_0000_01b3);
            }
        };
        self.pos.iter().flatten().for_each(|x| eat(x.to_bits()));
        self.idx.iter().for_each(|&i| eat(i));
        h
    }
}

/// A rotation by Euler angles in degrees (X, then Y, then Z).
pub fn rotate_xyz(p: DVec3, deg: [f64; 3]) -> DVec3 {
    let [ax, ay, az] = deg.map(|d| d * trig::PI / 180.0);
    let (sx, cx) = trig::sincos(ax);
    let (sy, cy) = trig::sincos(ay);
    let (sz, cz) = trig::sincos(az);
    let p = DVec3::new(p.x, cx * p.y - sx * p.z, sx * p.y + cx * p.z);
    let p = DVec3::new(cy * p.x + sy * p.z, p.y, -sy * p.x + cy * p.z);
    DVec3::new(cz * p.x - sz * p.y, sz * p.x + cz * p.y, p.z)
}
