//! Corner patches where three or more bevelled edges meet
//! (`docs/research.md` §7.3).
//!
//! The hole is bounded by the `m` edge chains. With an even segment count
//! `n = 2h` it gets the ADJ quad pattern: one `h × h` grid per boundary
//! point (sector), sectors meeting along spokes from each chain's middle to
//! one centre vertex. Positions:
//! - `m = 3`: the octant patch, the affine image of the superellipsoid
//!   octant `|x|^p + |y|^p + |z|^p = 1` whose boundary arcs are the three
//!   chains (for a fillet on a box corner, a sphere octant);
//! - `m > 3`: Catmull-Clark subdivision from a single centre vertex, with
//!   the boundary held on the profiles at every level (after Levin's
//!   boundary-constrained schemes), to the next power of two, then
//!   resampled to `h`.
//!
//! One segment gives a flat polygon fan; an odd count above one gives the
//! cutoff fallback, a fan from the hole's centroid.

use std::collections::BTreeMap;

use shape_core::{DVec3, trig};

use crate::BevelError;
use crate::profile;

/// A corner: the original vertex, the octant centre (for `m = 3`) and,
/// per bevelled edge `j` around it, the profile ends `a = B_j`,
/// `b = B_{j-1}`, the profile corner and the chain's vertex ids
/// (`B_j → B_{j-1}`).
pub(crate) struct Corner {
    pub v: DVec3,
    pub o: DVec3,
    pub ends: Vec<(DVec3, DVec3, DVec3, Vec<u32>)>,
    pub n: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    /// On edge `j`'s chain, at this index from `B_j`.
    Chain(usize, u32),
    /// On the spoke between sectors `j - 1` and `j`, from the chain middle.
    Spoke(usize, u32),
    Center,
    In(usize, u32, u32),
}

/// The vertex at `(a, b)` of sector `j` (an `h × h` grid; `a` runs along
/// edge `j + 1` toward `B_{j+1}`, `b` along edge `j` toward `B_{j-1}`),
/// shared vertices named once.
fn key(m: usize, h: u32, j: usize, a: u32, b: u32) -> Key {
    let n = 2 * h;
    let next = (j + 1) % m;
    if a == h && b == h {
        Key::Center
    } else if a == h {
        if b == 0 {
            Key::Chain(next, h)
        } else {
            Key::Spoke(next, b)
        }
    } else if b == h {
        if a == 0 {
            Key::Chain(j, h)
        } else {
            Key::Spoke(j, a)
        }
    } else if b == 0 {
        Key::Chain(next, n - a)
    } else if a == 0 {
        Key::Chain(j, b)
    } else {
        Key::In(j, a, b)
    }
}

type Samples<'a> = &'a mut dyn FnMut(u32) -> Vec<(f64, f64)>;

/// Triangles filling the corner; new vertices are pushed onto `pos`.
pub(crate) fn fill(
    c: &Corner,
    exponent: f64,
    samples: Samples<'_>,
    pos: &mut Vec<DVec3>,
) -> Result<Vec<[u32; 3]>, BevelError> {
    let m = c.ends.len();
    let n = c.n;
    let first = |j: usize| c.ends[j].3[0];
    if n == 1 {
        return Ok((1..m - 1)
            .map(|j| [first(0), first(j), first(j + 1)])
            .collect());
    }
    if n % 2 == 1 {
        return Ok(cutoff(c, pos));
    }
    let h = n / 2;
    let inner = if m == 3 {
        octant(c, exponent, samples).unwrap_or_else(|| adj(c, samples))
    } else {
        adj(c, samples)
    };
    let mut ids: BTreeMap<Key, u32> = BTreeMap::new();
    let mut id = |k: Key, pos: &mut Vec<DVec3>| -> Result<u32, BevelError> {
        if let Key::Chain(j, i) = k {
            return Ok(c.ends[j].3[i as usize]);
        }
        if let Some(&i) = ids.get(&k) {
            return Ok(i);
        }
        let p = *inner
            .get(&k)
            .ok_or(BevelError::Unsupported(c.v, "corner patch"))?;
        if !p.is_finite() {
            return Err(BevelError::TooLarge(c.v));
        }
        pos.push(p);
        let i = pos.len() as u32 - 1;
        ids.insert(k, i);
        Ok(i)
    };
    let mut out = Vec::with_capacity(2 * m * (h * h) as usize);
    for j in 0..m {
        for a in 0..h {
            for b in 0..h {
                let k00 = id(key(m, h, j, a, b), pos)?;
                let k10 = id(key(m, h, j, a + 1, b), pos)?;
                let k11 = id(key(m, h, j, a + 1, b + 1), pos)?;
                let k01 = id(key(m, h, j, a, b + 1), pos)?;
                out.push([k00, k10, k11]);
                out.push([k00, k11, k01]);
            }
        }
    }
    Ok(out)
}

/// The hole's boundary loop: `B_0 → B_1 → …`, each edge's chain read
/// backwards.
fn ring(c: &Corner) -> Vec<u32> {
    let mut out = Vec::new();
    for end in &c.ends {
        let chain = &end.3;
        out.extend(chain.iter().rev().take(chain.len() - 1));
    }
    out
}

fn cutoff(c: &Corner, pos: &mut Vec<DVec3>) -> Vec<[u32; 3]> {
    let loop_ = ring(c);
    let z = loop_.iter().fold(DVec3::ZERO, |a, &i| a + pos[i as usize]) / loop_.len() as f64;
    pos.push(z);
    let zi = pos.len() as u32 - 1;
    (0..loop_.len())
        .map(|i| [loop_[i], loop_[(i + 1) % loop_.len()], zi])
        .collect()
}

/// The superellipsoid octant over the three boundary points; `None` when
/// they are in a plane with the centre.
fn octant(c: &Corner, p: f64, samples: Samples<'_>) -> Option<BTreeMap<Key, DVec3>> {
    let h = c.n / 2;
    let cols: Vec<DVec3> = c.ends.iter().map(|e| e.0 - c.o).collect();
    let det = cols[0].dot(cols[1].cross(cols[2]));
    let scale = cols[0].length() * cols[1].length() * cols[2].length();
    if det.is_nan() || det.abs() <= 1e-6 * scale {
        return None;
    }
    let s = samples(c.n);
    let ratio: Vec<f64> = (0..=h as usize)
        .map(|t| {
            if t == h as usize {
                1.0
            } else {
                s[t].1 / s[t].0
            }
        })
        .collect();
    let at = |u: [f64; 3]| {
        let norm = trig::pow(
            trig::pow(u[0], p) + trig::pow(u[1], p) + trig::pow(u[2], p),
            1.0 / p,
        );
        c.o + (cols[0] * u[0] + cols[1] * u[1] + cols[2] * u[2]) / norm
    };
    let mut out = BTreeMap::new();
    for j in 0..3 {
        for a in 0..=h {
            for b in 0..=h {
                let k = key(3, h, j, a, b);
                if matches!(k, Key::Chain(..)) || out.contains_key(&k) {
                    continue;
                }
                let mut u = [0.0; 3];
                u[j] = 1.0;
                u[(j + 1) % 3] = ratio[a as usize];
                u[(j + 2) % 3] = ratio[b as usize];
                out.insert(k, at(u));
            }
        }
    }
    Some(out)
}

fn chain_points(c: &Corner, s: &[(f64, f64)]) -> Vec<Vec<DVec3>> {
    c.ends
        .iter()
        .map(|e| profile::chain(e.0, e.1, e.2, s))
        .collect()
}

/// The ADJ pattern by boundary-constrained Catmull-Clark.
fn adj(c: &Corner, samples: Samples<'_>) -> BTreeMap<Key, DVec3> {
    let m = c.ends.len();
    let h = c.n / 2;
    // Level 1: the boundary points, the chain middles and a centre.
    let mut hl = 1u32;
    let mut pos: BTreeMap<Key, DVec3> = BTreeMap::new();
    let chains = chain_points(c, &samples(2));
    for (j, ch) in chains.iter().enumerate() {
        for (i, &p) in ch.iter().enumerate() {
            pos.insert(Key::Chain(j, i as u32), p);
        }
    }
    let mid = chains.iter().fold(DVec3::ZERO, |a, ch| a + ch[1]) / m as f64;
    let base = chains.iter().fold(DVec3::ZERO, |a, ch| a + ch[0]) / m as f64;
    // Pushed out past the middles' centroid, as far again as they sit from
    // the boundary points', so the subdivided centre lands near the
    // rounded surface.
    pos.insert(Key::Center, mid * 2.0 - base);
    while hl < h {
        pos = refine(m, hl, &pos, &chain_points(c, &samples(4 * hl)));
        hl *= 2;
    }
    if hl == h {
        return pos;
    }
    // Resample the 2^k grid to h per sector.
    let mut out = BTreeMap::new();
    let get = |j: usize, a: u32, b: u32| pos[&key(m, hl, j, a, b)];
    for j in 0..m {
        for a in 0..=h {
            for b in 0..=h {
                let k = key(m, h, j, a, b);
                if matches!(k, Key::Chain(..)) || out.contains_key(&k) {
                    continue;
                }
                let split = |x: u32| {
                    let f = x as f64 * hl as f64 / h as f64;
                    let i = (f as u32).min(hl - 1);
                    (i, f - i as f64)
                };
                let ((ia, ta), (ib, tb)) = (split(a), split(b));
                let p = get(j, ia, ib) * ((1.0 - ta) * (1.0 - tb))
                    + get(j, ia + 1, ib) * (ta * (1.0 - tb))
                    + get(j, ia + 1, ib + 1) * (ta * tb)
                    + get(j, ia, ib + 1) * ((1.0 - ta) * tb);
                out.insert(k, p);
            }
        }
    }
    out
}

/// One Catmull-Clark step from `h` to `2h` per sector; boundary vertices
/// are taken from the profiles at the new resolution instead of the
/// subdivision rules.
fn refine(
    m: usize,
    h: u32,
    pos: &BTreeMap<Key, DVec3>,
    chains: &[Vec<DVec3>],
) -> BTreeMap<Key, DVec3> {
    let h2 = 2 * h;
    let mut new: BTreeMap<Key, DVec3> = BTreeMap::new();
    for (j, ch) in chains.iter().enumerate() {
        for (i, &p) in ch.iter().enumerate() {
            new.insert(Key::Chain(j, i as u32), p);
        }
    }
    type Pair = (Key, Key);
    let mut edges: BTreeMap<Pair, (Vec<DVec3>, Key)> = BTreeMap::new();
    let mut vfaces: BTreeMap<Key, (Vec<DVec3>, Key)> = BTreeMap::new();
    let mut vedges: BTreeMap<Key, Vec<Pair>> = BTreeMap::new();
    let corner = [(0, 0), (1, 0), (1, 1), (0, 1)];
    for j in 0..m {
        for a in 0..h {
            for b in 0..h {
                let ks = corner.map(|(da, db)| key(m, h, j, a + da, b + db));
                let f = ks.iter().fold(DVec3::ZERO, |s, k| s + pos[k]) / 4.0;
                new.insert(key(m, h2, j, 2 * a + 1, 2 * b + 1), f);
                let mids = [
                    (2 * a + 1, 2 * b),
                    (2 * a + 2, 2 * b + 1),
                    (2 * a + 1, 2 * b + 2),
                    (2 * a, 2 * b + 1),
                ];
                for i in 0..4 {
                    let (x, y) = (ks[i], ks[(i + 1) % 4]);
                    let pair = if x < y { (x, y) } else { (y, x) };
                    let nk = key(m, h2, j, mids[i].0, mids[i].1);
                    edges.entry(pair).or_insert((Vec::new(), nk)).0.push(f);
                    let (da, db) = corner[i];
                    let vk = key(m, h2, j, 2 * (a + da), 2 * (b + db));
                    vfaces.entry(ks[i]).or_insert((Vec::new(), vk)).0.push(f);
                    for z in [x, y] {
                        let list = vedges.entry(z).or_default();
                        if !list.contains(&pair) {
                            list.push(pair);
                        }
                    }
                }
            }
        }
    }
    for (&(x, y), (fs, nk)) in &edges {
        if fs.len() == 2 && !matches!(nk, Key::Chain(..)) {
            new.insert(*nk, (pos[&x] + pos[&y] + fs[0] + fs[1]) / 4.0);
        }
    }
    for (k, (fs, nk)) in &vfaces {
        if matches!(k, Key::Chain(..)) {
            continue;
        }
        let kf = fs.len() as f64;
        let f = fs.iter().fold(DVec3::ZERO, |s, &p| s + p) / kf;
        let es = &vedges[k];
        let r = es
            .iter()
            .fold(DVec3::ZERO, |s, (x, y)| s + (pos[x] + pos[y]) / 2.0)
            / es.len() as f64;
        new.insert(*nk, (f + r * 2.0 + pos[k] * (kf - 3.0)) / kf);
    }
    new
}
