//! Atlas packing on an integer texel grid (`docs/research.md` §34).
//!
//! Entry: [`pack`] places charts (triangles in surface units, grouped by
//! chart) in the unit square; [`unwrap_atlas`] makes a solid's charts as
//! [`crate::unwrap()`] does and packs them so; [`repack`] takes the UV
//! islands a solid (or several parts) already has as charts. Each returns
//! an [`AtlasReport`]: where every chart went and the measures (coverage,
//! overlap, texel density).
//!
//! The rule:
//! 1. Each chart is turned to its smallest box (rotating calipers over its
//!    exact hull) and laid wide, from the origin.
//! 2. One scale `s` (texels per surface unit) holds for every chart; a
//!    chart with weight `w` takes `s √w`, so its texel density is that
//!    much finer. A chart's rectangle is its box at that scale rounded up
//!    to whole texels, plus `padding` texels.
//! 3. Rectangles are taken tallest first, then widest, then by chart id,
//!    and each goes where a bottom-left skyline on the integer grid puts
//!    it lowest, then leftmost; with turns allowed the chart is also tried
//!    a quarter turn round and goes where it lies lower (laid wide on a
//!    tie).
//! 4. The largest `s` at which every rectangle fits is found by bisection
//!    (40 halvings).
//!
//! A chart's triangles sit `⌊padding/2⌋` texels inside its rectangle, so
//! two charts are at least `padding` texels apart and every chart at least
//! `⌊padding/2⌋` from the edge. Positions are f64 `+ − × /` only and the
//! grid is integer, so the placement is the same on every host.

use std::collections::BTreeMap;
use std::fmt;

use shape_core::DVec2;
use shape_mesh::Solid;

use crate::json::Json;
use crate::unwrap::{Unwrap, UnwrapError};

/// How to pack. The zero value is a 1024-texel square, 2 texels between
/// charts, quarter turns allowed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Atlas {
    /// Texels across the square; 0 is 1024.
    pub size: u32,
    /// Texels between charts (half that, rounded down, to the edge); 0
    /// is 2.
    pub padding: u32,
    /// Keep every chart laid wide: no quarter turns while packing.
    pub no_turns: bool,
}

impl Atlas {
    /// The size and padding with their defaults.
    pub fn resolved(&self) -> (u32, u32) {
        (
            if self.size == 0 { 1024 } else { self.size },
            if self.padding == 0 { 2 } else { self.padding },
        )
    }
}

/// Where one chart went.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChartPlace {
    /// The rectangle's corner, texels from the left and from the bottom
    /// (v up).
    pub x: u32,
    /// See `x`.
    pub y: u32,
    /// The rectangle's size in texels, padding included.
    pub width: u32,
    /// See `width`.
    pub height: u32,
    /// Turned a quarter turn from wide.
    pub turned: bool,
    /// Its triangles.
    pub triangles: usize,
}

/// What a packing did, and the measures of the result.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AtlasReport {
    /// Texels across.
    pub size: u32,
    /// Texels between charts.
    pub padding: u32,
    /// Each chart's place, by chart id (the order charts were given or
    /// found in).
    pub places: Vec<ChartPlace>,
    /// Texels per surface unit at weight 1 (millimetres for a solid).
    pub scale: f64,
    /// The measures of the packed UVs.
    pub measures: AtlasMeasures,
}

/// Measures of UVs on a texel grid.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AtlasMeasures {
    /// Texels across.
    pub size: u32,
    /// Charts.
    pub charts: usize,
    /// Share of the texels whose centre a triangle covers (exact, top-left
    /// rule).
    pub coverage: f64,
    /// Share of the texels whose centre two triangles or more cover.
    pub overlap: f64,
    /// Pairs of UV triangles whose interiors overlap (exact).
    pub overlapping_pairs: usize,
    /// Triangles with a corner outside the unit square.
    pub outside: usize,
    /// Texels per millimetre of surface, per triangle (√ of UV area in
    /// texels over surface area): the least.
    pub density_min: f64,
    /// The median, weighted by surface area.
    pub density_median: f64,
    /// The most.
    pub density_max: f64,
}

/// Why charts could not be packed.
#[derive(Clone, Debug, PartialEq)]
pub enum AtlasError {
    /// A size, padding or weight out of range (which).
    BadDesc(String),
    /// Nothing to pack.
    Empty,
    /// The solid has no UVs to repack.
    NoUvs,
    /// The charts' padding alone does not fit in the square.
    TooManyCharts(usize),
    /// The unwrap refused (why).
    Unwrap(UnwrapError),
}

impl fmt::Display for AtlasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AtlasError::BadDesc(w) => write!(f, "atlas: {w}"),
            AtlasError::Empty => f.write_str("atlas: no triangles"),
            AtlasError::NoUvs => f.write_str("atlas: the solid has no UVs to repack"),
            AtlasError::TooManyCharts(n) => write!(
                f,
                "atlas: {n} charts do not fit in the square with their padding"
            ),
            AtlasError::Unwrap(e) => write!(f, "atlas: {e}"),
        }
    }
}

impl std::error::Error for AtlasError {}

/// Packed charts: what [`pack`] returns.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Packing {
    /// Each triangle's UVs in the unit square.
    pub uvs: Vec<[DVec2; 3]>,
    /// Each chart's place, in increasing chart id.
    pub places: Vec<ChartPlace>,
    /// Texels per surface unit at weight 1.
    pub scale: f64,
}

/// Packs charts in the unit square. `corners[i]` is triangle `i` in its
/// chart's surface coordinates (any rigid placement), `charts[i]` its
/// chart (ids need not be contiguous; they are taken in increasing
/// order), and `weights[c]` the density weight of the `c`-th chart in that
/// order (empty: all 1). Returns each triangle's UVs, each chart's place
/// (in the same order) and the scale, texels per surface unit at weight 1.
pub fn pack(
    corners: &[[DVec2; 3]],
    charts: &[u32],
    weights: &[f64],
    d: &Atlas,
) -> Result<Packing, AtlasError> {
    let (size, padding) = d.resolved();
    if size > 1 << 15 {
        return Err(AtlasError::BadDesc("size must be at most 32768".into()));
    }
    if padding >= size {
        return Err(AtlasError::BadDesc(
            "padding must be less than the size".into(),
        ));
    }
    if corners.is_empty() {
        return Err(AtlasError::Empty);
    }
    if charts.len() != corners.len() {
        return Err(AtlasError::BadDesc(
            "one chart id per triangle is needed".into(),
        ));
    }
    if corners.iter().flatten().any(|q| !q.is_finite()) {
        return Err(AtlasError::BadDesc("a coordinate is not finite".into()));
    }
    let mut ids: Vec<u32> = charts.to_vec();
    ids.sort_unstable();
    ids.dedup();
    if !weights.is_empty() && weights.len() != ids.len() {
        return Err(AtlasError::BadDesc("one weight per chart is needed".into()));
    }
    if weights.iter().any(|w| !(*w > 0.0 && w.is_finite())) {
        return Err(AtlasError::BadDesc("weights must be positive".into()));
    }
    let index: BTreeMap<u32, usize> = ids.iter().enumerate().map(|(k, &c)| (c, k)).collect();
    let mut members: Vec<Vec<u32>> = vec![Vec::new(); ids.len()];
    for (i, c) in charts.iter().enumerate() {
        members[index[c]].push(i as u32);
    }
    let mut local = vec![[DVec2::ZERO; 3]; corners.len()];
    let sizes: Vec<DVec2> = members
        .iter()
        .map(|m| crate::unwrap::lay(m, corners, &mut local))
        .collect();
    // √weight per chart: the factor on its scale.
    let root: Vec<f64> = (0..ids.len())
        .map(|c| weights.get(c).map_or(1.0, |w| w.sqrt()))
        .collect();
    let (rects, scale) = fit(&sizes, &root, size, padding, !d.no_turns)
        .ok_or(AtlasError::TooManyCharts(ids.len()))?;
    let half = f64::from(padding / 2);
    let n = f64::from(size);
    let mut out = vec![[DVec2::ZERO; 3]; corners.len()];
    let mut places = Vec::with_capacity(ids.len());
    for (c, m) in members.iter().enumerate() {
        let r = rects[c];
        let k = scale * root[c];
        let at = DVec2::new(f64::from(r.x) + half, f64::from(r.y) + half);
        // The chart's height at this scale, for the quarter turn.
        let h = sizes[c].y * k;
        for &i in m {
            out[i as usize] = local[i as usize].map(|q| {
                let q = q * k;
                // A quarter turn: (x, y) ↦ (h − y, x), winding kept.
                let q = if r.turned {
                    DVec2::new(h - q.y, q.x)
                } else {
                    q
                };
                (at + q) / n
            });
        }
        places.push(ChartPlace {
            x: r.x,
            y: r.y,
            width: r.w,
            height: r.h,
            turned: r.turned,
            triangles: m.len(),
        });
    }
    Ok(Packing {
        uvs: out,
        places,
        scale,
    })
}

#[derive(Clone, Copy, Debug, Default)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    turned: bool,
}

/// The rectangles at the largest scale that fits (bisection), or `None`
/// when even the padding alone does not fit.
fn fit(
    sizes: &[DVec2],
    root: &[f64],
    size: u32,
    padding: u32,
    turns: bool,
) -> Option<(Vec<Rect>, f64)> {
    let total: f64 = sizes.iter().zip(root).map(|(s, r)| s.x * s.y * r * r).sum();
    let n = f64::from(size);
    let at = |scale: f64| place_all(sizes, root, scale, size, padding, turns);
    let mut best = (at(0.0)?, 0.0);
    // No scale above the one at which the charts' area alone fills the
    // square fits.
    let mut hi = if total > 0.0 {
        n / total.sqrt()
    } else {
        let longest = sizes
            .iter()
            .zip(root)
            .map(|(s, r)| s.x.max(s.y) * r)
            .fold(0.0, f64::max);
        if longest > 0.0 { n / longest } else { 1.0 }
    };
    if let Some(p) = at(hi) {
        return Some((p, hi));
    }
    let mut lo = 0.0;
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        match at(mid) {
            Some(p) => {
                best = (p, mid);
                lo = mid;
            }
            None => hi = mid,
        }
    }
    Some(best)
}

/// Every chart's rectangle at `scale`, by the module's rule; `None` if one
/// does not fit.
fn place_all(
    sizes: &[DVec2],
    root: &[f64],
    scale: f64,
    size: u32,
    padding: u32,
    turns: bool,
) -> Option<Vec<Rect>> {
    let texels = |v: f64| -> Option<u32> {
        let t = v.ceil();
        if t > f64::from(size) {
            None
        } else {
            Some(t as u32 + padding)
        }
    };
    let mut dims = Vec::with_capacity(sizes.len());
    for (s, r) in sizes.iter().zip(root) {
        let k = scale * r;
        dims.push((texels(s.x * k)?, texels(s.y * k)?));
    }
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|&a, &b| {
        dims[b]
            .1
            .cmp(&dims[a].1)
            .then(dims[b].0.cmp(&dims[a].0))
            .then(a.cmp(&b))
    });
    let mut sky = Skyline::new(size);
    let mut out = vec![Rect::default(); sizes.len()];
    for c in order {
        let (w, h) = dims[c];
        let wide = sky.find(w, h);
        let tall = if turns && w != h {
            sky.find(h, w)
        } else {
            None
        };
        let (x, y, turned) = match (wide, tall) {
            (Some(a), Some(b)) if (b.1, b.0) < (a.1, a.0) => (b.0, b.1, true),
            (Some(a), _) => (a.0, a.1, false),
            (None, Some(b)) => (b.0, b.1, true),
            (None, None) => return None,
        };
        let (rw, rh) = if turned { (h, w) } else { (w, h) };
        sky.raise(x, rw, y + rh);
        out[c] = Rect {
            x,
            y,
            w: rw,
            h: rh,
            turned,
        };
    }
    Some(out)
}

/// A bottom-left skyline on the integer grid: steps `(x, height)`, each
/// running to the next one's x (the last to the size).
struct Skyline {
    size: u32,
    steps: Vec<(u32, u32)>,
}

impl Skyline {
    fn new(size: u32) -> Skyline {
        Skyline {
            size,
            steps: vec![(0, 0)],
        }
    }

    /// The lowest, then leftmost, corner where a `w` × `h` rectangle fits.
    fn find(&self, w: u32, h: u32) -> Option<(u32, u32)> {
        let mut best: Option<(u32, u32)> = None;
        for i in 0..self.steps.len() {
            let x = self.steps[i].0;
            if x + w > self.size {
                break;
            }
            let mut y = 0;
            let mut j = i;
            while j < self.steps.len() && self.steps[j].0 < x + w {
                y = y.max(self.steps[j].1);
                j += 1;
            }
            if y + h > self.size {
                continue;
            }
            if best.is_none_or(|(bx, by)| (y, x) < (by, bx)) {
                best = Some((x, y));
            }
        }
        best
    }

    /// Raises `[x, x + w)` to `top`.
    fn raise(&mut self, x: u32, w: u32, top: u32) {
        let end = x + w;
        // The height the skyline had just right of the rectangle.
        let under = self
            .steps
            .iter()
            .rev()
            .find(|s| s.0 <= end)
            .map_or(0, |s| s.1);
        let mut next: Vec<(u32, u32)> = self.steps.iter().copied().filter(|s| s.0 < x).collect();
        next.push((x, top));
        if end < self.size {
            next.push((end, under));
        }
        next.extend(self.steps.iter().copied().filter(|s| s.0 > end));
        // Equal neighbours merge.
        next.dedup_by(|b, a| a.1 == b.1);
        self.steps = next;
    }
}

/// UVs replaced by charts made as [`crate::unwrap()`] makes them (`u`'s
/// angle; its resolution and padding are not used) and packed by
/// [`pack`] on `d`'s grid at one density. As in the unwrap, a triangle
/// that rounding pushes into an overlap with one of its chart leaves for
/// a chart of its own and the charts are packed again. On an error the
/// solid is unchanged.
pub fn unwrap_atlas(s: &mut Solid, u: &Unwrap, d: &Atlas) -> Result<AtlasReport, AtlasError> {
    let probe = Unwrap {
        resolution: 0,
        padding: 0.0,
        ..*u
    };
    crate::unwrap::check(s, &probe).map_err(AtlasError::Unwrap)?;
    let (mut charts, _, flat) = crate::unwrap::form(s, &probe);
    for round in 0..16 {
        let mut chart_of = vec![0u32; s.triangles.len()];
        for (c, m) in charts.iter().enumerate() {
            for &i in m {
                chart_of[i as usize] = c as u32;
            }
        }
        let Packing {
            uvs: uv,
            places,
            scale,
        } = pack(&flat, &chart_of, &[], d)?;
        let mut leaving: Vec<u32> = Vec::new();
        for members in &charts {
            for (i, j) in crate::unwrap::overlapping(members, &uv) {
                leaving.push(i.max(j));
                if round >= 3 {
                    leaving.push(i.min(j));
                }
            }
        }
        if leaving.is_empty() {
            s.uvs = Some(uv);
            let measures = atlas_measures(&[&*s], d);
            return Ok(AtlasReport {
                size: d.resolved().0,
                padding: d.resolved().1,
                places,
                scale,
                measures,
            });
        }
        leaving.sort_unstable();
        leaving.dedup();
        for members in &mut charts {
            members.retain(|i| leaving.binary_search(i).is_err());
        }
        charts.extend(leaving.iter().map(|&i| vec![i]));
        charts.retain(|m| !m.is_empty());
    }
    Err(AtlasError::BadDesc(
        "charts still overlap after rounding; this is a bug".into(),
    ))
}

/// The UV islands of a solid: triangles joined across an edge whose two
/// triangles give both its ends the same UVs. Ids in order of each
/// island's first triangle.
pub(crate) fn islands(s: &Solid, uv: &[[DVec2; 3]]) -> Vec<u32> {
    let n = s.triangles.len();
    let mut parent: Vec<u32> = (0..n as u32).collect();
    fn find(p: &mut [u32], mut i: u32) -> u32 {
        while p[i as usize] != i {
            p[i as usize] = p[p[i as usize] as usize];
            i = p[i as usize];
        }
        i
    }
    let key = |q: DVec2| (q.x.to_bits(), q.y.to_bits());
    type Ends = [(u64, u64); 2];
    let mut by_edge: BTreeMap<(u32, u32), Vec<(u32, Ends)>> = BTreeMap::new();
    for (i, t) in s.triangles.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let (qa, qb) = (key(uv[i][k]), key(uv[i][(k + 1) % 3]));
            let (e, q) = if a < b {
                ((a, b), [qa, qb])
            } else {
                ((b, a), [qb, qa])
            };
            by_edge.entry(e).or_default().push((i as u32, q));
        }
    }
    for users in by_edge.values() {
        for x in 0..users.len() {
            for y in x + 1..users.len() {
                if users[x].1 == users[y].1 {
                    let (a, b) = (find(&mut parent, users[x].0), find(&mut parent, users[y].0));
                    if a != b {
                        parent[a.max(b) as usize] = a.min(b);
                    }
                }
            }
        }
    }
    let mut id: BTreeMap<u32, u32> = BTreeMap::new();
    (0..n as u32)
        .map(|i| {
            let r = find(&mut parent, i);
            let next = id.len() as u32;
            *id.entry(r).or_insert(next)
        })
        .collect()
}

/// The UV islands of every part, repacked into one atlas at one texel
/// density: each island is scaled to millimetres (√ of its surface area
/// over its UV area) and packed by [`pack`]; an island of no UV area is
/// split into its triangles, each laid flat on its own plane. Parts
/// without UVs are left as they are and not counted. A file's maps no
/// longer fit the new UVs: the caller drops or re-bakes them.
pub fn repack(parts: &mut [Solid], d: &Atlas) -> Result<AtlasReport, AtlasError> {
    let mut corners: Vec<[DVec2; 3]> = Vec::new();
    let mut chart_of: Vec<u32> = Vec::new();
    let mut next = 0u32;
    for s in parts.iter() {
        let Some(uv) = &s.uvs else { continue };
        if uv.len() != s.triangles.len() {
            return Err(AtlasError::BadDesc(
                "the UVs are for another number of triangles".into(),
            ));
        }
        let own = islands(s, uv);
        let count = own.iter().copied().max().map_or(0, |m| m as usize + 1);
        let (mut a3, mut a2) = (vec![0.0f64; count], vec![0.0f64; count]);
        for (i, t) in s.triangles.iter().enumerate() {
            let [a, b, c] = t.map(|v| s.positions[v as usize]);
            a3[own[i] as usize] += (b - a).cross(c - a).length() / 2.0;
            let q = uv[i];
            a2[own[i] as usize] += ((q[1] - q[0]).perp_dot(q[2] - q[0]) / 2.0).abs();
        }
        let mut flat_ids: BTreeMap<usize, u32> = BTreeMap::new();
        for (i, t) in s.triangles.iter().enumerate() {
            let c = own[i] as usize;
            if a2[c] > 0.0 {
                let k = (a3[c] / a2[c]).sqrt();
                corners.push(uv[i].map(|q| q * k));
                chart_of.push(next + c as u32);
            } else {
                // No UV area: each triangle on its own plane.
                let [a, b, c3] = t.map(|v| s.positions[v as usize]);
                let e = b - a;
                let len = e.length();
                let x = if len > 0.0 {
                    e / len
                } else {
                    shape_core::DVec3::X
                };
                let nrm = e.cross(c3 - a);
                let y = nrm.cross(x);
                let ly = y.length();
                let y = if ly > 0.0 {
                    y / ly
                } else {
                    shape_core::DVec3::Y
                };
                let f = |p: shape_core::DVec3| DVec2::new((p - a).dot(x), (p - a).dot(y));
                corners.push([f(a), f(b), f(c3)]);
                let id = flat_ids.len() as u32;
                flat_ids.insert(i, id);
                chart_of.push(u32::MAX - id);
            }
        }
        next += count as u32;
    }
    if corners.is_empty() {
        return Err(AtlasError::NoUvs);
    }
    let Packing {
        uvs: uv,
        places,
        scale,
    } = pack(&corners, &chart_of, &[], d)?;
    let mut k = 0;
    for s in parts.iter_mut() {
        if let Some(own) = &mut s.uvs {
            let n = own.len();
            own.copy_from_slice(&uv[k..k + n]);
            k += n;
        }
    }
    let refs: Vec<&Solid> = parts.iter().filter(|s| s.uvs.is_some()).collect();
    let measures = atlas_measures(&refs, d);
    Ok(AtlasReport {
        size: d.resolved().0,
        padding: d.resolved().1,
        places,
        scale,
        measures,
    })
}

/// The measures of the UVs of `parts` together on `d`'s grid: coverage
/// and overlap of texel centres (exact, a centre on an edge counted for
/// one side only), exact overlapping pairs, triangles outside the square,
/// charts (UV islands) and texel density per millimetre of surface.
pub fn atlas_measures(parts: &[&Solid], d: &Atlas) -> AtlasMeasures {
    let (size, _) = d.resolved();
    let mut all: Vec<[DVec2; 3]> = Vec::new();
    let mut dens: Vec<(f64, f64)> = Vec::new();
    let mut charts = 0;
    let n2 = f64::from(size) * f64::from(size);
    for s in parts {
        let Some(uv) = &s.uvs else { continue };
        charts += islands(s, uv)
            .iter()
            .copied()
            .max()
            .map_or(0, |m| m as usize + 1);
        for (i, t) in s.triangles.iter().enumerate() {
            let [a, b, c] = t.map(|v| s.positions[v as usize]);
            let area = (b - a).cross(c - a).length() / 2.0;
            let q = uv[i];
            let uva = ((q[1] - q[0]).perp_dot(q[2] - q[0]) / 2.0).abs();
            if area > 0.0 && uva > 0.0 {
                dens.push(((uva * n2 / area).sqrt(), area));
            }
        }
        all.extend_from_slice(uv);
    }
    let uvm = crate::measure::uv_measures(&all, size);
    dens.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let total: f64 = dens.iter().map(|d| d.1).sum();
    let mut median = 0.0;
    let mut acc = 0.0;
    for &(v, a) in &dens {
        acc += a;
        median = v;
        if acc * 2.0 >= total {
            break;
        }
    }
    AtlasMeasures {
        size,
        charts,
        coverage: uvm.coverage,
        overlap: uvm.overlap,
        overlapping_pairs: uvm.overlapping_pairs,
        outside: uvm.outside,
        density_min: dens.first().map_or(0.0, |d| d.0),
        density_median: median,
        density_max: dens.last().map_or(0.0, |d| d.0),
    }
}

impl AtlasMeasures {
    pub(crate) fn json(&self) -> Json {
        Json::Obj(vec![
            ("size", Json::Int(u64::from(self.size))),
            ("charts", Json::Int(self.charts as u64)),
            ("coverage", Json::Num(self.coverage)),
            ("overlap", Json::Num(self.overlap)),
            (
                "overlapping_pairs",
                Json::Int(self.overlapping_pairs as u64),
            ),
            ("outside", Json::Int(self.outside as u64)),
            (
                "density_texels_per_mm",
                Json::Obj(vec![
                    ("min", Json::Num(self.density_min)),
                    ("median", Json::Num(self.density_median)),
                    ("max", Json::Num(self.density_max)),
                ]),
            ),
        ])
    }
}

impl AtlasReport {
    /// The report as JSON with keys in a fixed order (each chart's place
    /// left out; [`AtlasReport::places`] has them).
    pub fn to_json(&self) -> String {
        let turned = self.places.iter().filter(|p| p.turned).count();
        let mut s = String::new();
        Json::Obj(vec![
            ("size", Json::Int(u64::from(self.size))),
            ("padding", Json::Int(u64::from(self.padding))),
            ("charts", Json::Int(self.places.len() as u64)),
            ("turned", Json::Int(turned as u64)),
            ("texels_per_mm", Json::Num(self.scale)),
            ("measures", self.measures.json()),
        ])
        .write(&mut s);
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cmp::Ordering;
    use shape_core::predicates::orient2d;
    use shape_gen::solids::{CylinderDesc, cylinder, sphere};

    #[test]
    fn skyline_places_bottom_left_and_raises() {
        let mut s = Skyline::new(10);
        assert_eq!(s.find(4, 3), Some((0, 0)));
        s.raise(0, 4, 3);
        assert_eq!(s.steps, vec![(0, 3), (4, 0)]);
        assert_eq!(s.find(4, 5), Some((4, 0)));
        s.raise(4, 4, 5);
        assert_eq!(s.steps, vec![(0, 3), (4, 5), (8, 0)]);
        // A 4-wide one: over [0, 4) at 3, not over [8, 10) (too narrow).
        assert_eq!(s.find(4, 2), Some((0, 3)));
        s.raise(0, 4, 5);
        assert_eq!(s.steps, vec![(0, 5), (8, 0)]);
        assert_eq!(s.find(3, 1), Some((0, 5)));
        assert_eq!(s.find(2, 1), Some((8, 0)));
    }

    fn ccw(t: [DVec2; 3]) -> bool {
        orient2d(t[0], t[1], t[2]) == Ordering::Greater
    }

    fn check_unwrap(s: &Solid, d: &Atlas) -> AtlasReport {
        let mut s = s.clone();
        let r = unwrap_atlas(&mut s, &Unwrap::default(), d).unwrap();
        let uv = s.uvs.as_ref().unwrap();
        let (size, pad) = d.resolved();
        for t in uv {
            assert!(ccw(*t));
            for q in t {
                // At least ⌊padding/2⌋ texels from the edge.
                let m = f64::from(pad / 2) / f64::from(size);
                assert!(
                    q.x >= m && q.x <= 1.0 - m && q.y >= m && q.y <= 1.0 - m,
                    "{q}"
                );
            }
        }
        // Rectangles never overlap and stay inside.
        for (a, p) in r.places.iter().enumerate() {
            assert!(p.x + p.width <= size && p.y + p.height <= size);
            for q in &r.places[a + 1..] {
                let apart = p.x + p.width <= q.x
                    || q.x + q.width <= p.x
                    || p.y + p.height <= q.y
                    || q.y + q.height <= p.y;
                assert!(apart, "{p:?} {q:?}");
            }
        }
        assert_eq!(r.measures.overlapping_pairs, 0);
        assert_eq!(r.measures.overlap, 0.0);
        r
    }

    #[test]
    fn packs_on_the_grid_without_overlap_at_one_density() {
        let s = sphere(10.0).unwrap();
        let r = check_unwrap(&s, &Atlas::default());
        // Planar charts at one scale: density varies only by the
        // projection's shrinking (within the 66° chart angle).
        let m = r.measures;
        assert!(m.density_max <= r.scale * (1.0 + 1e-9), "{m:?} {}", r.scale);
        assert!(m.density_min >= r.scale * 0.35, "{m:?}");
        assert!(m.coverage > 0.3, "{m:?}");
        let c = cylinder(&CylinderDesc {
            radius: 5.0,
            height: 40.0,
            ..Default::default()
        })
        .unwrap();
        let d = Atlas {
            size: 256,
            padding: 3,
            no_turns: false,
        };
        let turned = check_unwrap(&c, &d);
        let wide = check_unwrap(
            &c,
            &Atlas {
                no_turns: true,
                ..d
            },
        );
        assert!(wide.places.iter().all(|p| !p.turned));
        // A tall cylinder's wall is turned to fit beside the others.
        assert!(turned.places.iter().any(|p| p.turned));
        let _ = wide;
    }

    #[test]
    fn weights_scale_density() {
        // Two equal squares, the second with weight 4: twice the texels
        // per unit.
        let sq = |o: f64| {
            let w = DVec2::new;
            [
                [w(o, 0.0), w(o + 1.0, 0.0), w(o + 1.0, 1.0)],
                [w(o, 0.0), w(o + 1.0, 1.0), w(o, 1.0)],
            ]
        };
        let mut corners = sq(0.0).to_vec();
        corners.extend(sq(5.0));
        let Packing {
            uvs: uv, places, ..
        } = pack(&corners, &[0, 0, 1, 1], &[1.0, 4.0], &Atlas::default()).unwrap();
        let area = |t: [DVec2; 3]| (t[1] - t[0]).perp_dot(t[2] - t[0]);
        let ratio = area(uv[2]) / area(uv[0]);
        assert!((ratio - 4.0).abs() < 1e-9, "{ratio}");
        assert_eq!(places.len(), 2);
    }

    #[test]
    fn repack_keeps_islands_rigid_and_separates_them() {
        // A sphere unwrapped, its islands all moved onto one spot: the
        // repack puts them apart, each moved rigidly and scaled.
        let mut s = sphere(10.0).unwrap();
        crate::unwrap(&mut s, &crate::Unwrap::default()).unwrap();
        let before = s.uvs.clone().unwrap();
        let ids = islands(&s, &before);
        let mut parts = vec![s];
        let r = repack(&mut parts, &Atlas::default()).unwrap();
        assert_eq!(
            r.places.len(),
            ids.iter().copied().max().unwrap() as usize + 1
        );
        assert_eq!(r.measures.overlapping_pairs, 0);
        let after = parts[0].uvs.as_ref().unwrap();
        // Within an island, every edge's length ratio is the same.
        let mut ratio: BTreeMap<u32, f64> = BTreeMap::new();
        for (i, t) in before.iter().enumerate() {
            let (a, b) = ((t[1] - t[0]).length(), (after[i][1] - after[i][0]).length());
            if a > 1e-9 {
                let k = *ratio.entry(ids[i]).or_insert(b / a);
                assert!((b / a - k).abs() < 1e-6 * k, "{} {}", b / a, k);
            }
        }
    }
}
