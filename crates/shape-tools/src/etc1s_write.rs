//! Encodes images as ETC1S blocks under BasisLZ: the small mode of
//! `KHR_texture_basisu` (`docs/research.md` §57).
//!
//! Entry: [`encode`]. Every block of every image is fitted with one
//! base colour and one intensity table; the fitted endpoints are
//! gathered into a codebook by splitting them until it has as many
//! entries as asked; each block's sixteen steps are fitted again to
//! its codebook endpoint and gathered the same way into the selector
//! codebook; and the blocks are written as references into both, with
//! the specification's predictions and Huffman tables. This project's
//! own encoder: Basis Universal's was not read.

use std::collections::BTreeMap;

/// ETC1's intensity tables, least to most.
const TABLES: [[i32; 4]; 8] = [
    [-8, -2, 2, 8],
    [-17, -5, 5, 17],
    [-29, -9, 9, 29],
    [-42, -13, 13, 42],
    [-60, -18, 18, 60],
    [-80, -24, 24, 80],
    [-106, -33, 33, 106],
    [-183, -47, 47, 183],
];

/// One image's worth of blocks to encode: its size and RGB texels, 3
/// bytes each, rows from the top.
pub(crate) struct Plane {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

/// What [`encode`] gives back: the three parts of the global data and
/// each plane's slice.
pub(crate) struct Encoded {
    pub endpoint_count: usize,
    pub selector_count: usize,
    pub endpoints: Vec<u8>,
    pub selectors: Vec<u8>,
    pub tables: Vec<u8>,
    pub slices: Vec<Vec<u8>>,
}

fn wide(c: u8) -> i32 {
    i32::from((c << 3) | (c >> 2))
}

/// Each texel's best step for a base colour and table, and the error.
fn steps(px: &[[u8; 3]; 16], base: [i32; 3], table: usize) -> ([u8; 16], u32) {
    let mut out = [0u8; 16];
    let mut err = 0u32;
    for (i, p) in px.iter().enumerate() {
        let mut least = u32::MAX;
        for (k, &m) in TABLES[table].iter().enumerate() {
            let mut e = 0u32;
            for c in 0..3 {
                let d = (base[c] + m).clamp(0, 255) - i32::from(p[c]);
                e += (d * d) as u32;
            }
            if e < least {
                least = e;
                out[i] = k as u8;
            }
        }
        err += least;
    }
    (out, err)
}

/// A block's best base colour (5 bits a channel) and table.
fn fit(px: &[[u8; 3]; 16]) -> ([u8; 3], u8) {
    let mut mean = [0i32; 3];
    for p in px {
        for c in 0..3 {
            mean[c] += i32::from(p[c]);
        }
    }
    let to5 = |v: i32| (((v.clamp(0, 255) * 31 + 127) / 255) as u8).min(31);
    let start = mean.map(|v| to5((v + 8) / 16));
    let mut best = (u32::MAX, start, 0u8);
    for (table, reach) in TABLES.iter().enumerate() {
        let mut color = start;
        for _ in 0..3 {
            let (chosen, err) = steps(px, color.map(wide), table);
            if err < best.0 {
                best = (err, color, table as u8);
            }
            // The base that suits those steps: the mean of each texel
            // less its step.
            let mut sum = [0i32; 3];
            for (p, &k) in px.iter().zip(&chosen) {
                for c in 0..3 {
                    sum[c] += i32::from(p[c]) - reach[k as usize];
                }
            }
            let next = sum.map(|v| to5((v + 8).div_euclid(16)));
            if next == color {
                break;
            }
            color = next;
        }
    }
    (best.1, best.2)
}

/// Splits `points` (each with a weight) into at most `most` groups:
/// the group whose points spread most along one axis is cut at its
/// mean on that axis, until there are enough or none can be cut. Each
/// point's group.
fn split<const D: usize>(points: &[([f64; D], f64)], most: usize) -> (Vec<u32>, usize) {
    struct Group {
        members: Vec<u32>,
        /// The axis its points spread most along, the mean there, and
        /// that spread (weighted squared distance from the mean).
        axis: usize,
        mean: f64,
        spread: f64,
    }
    let measure = |members: Vec<u32>| -> Group {
        let mut total = 0.0;
        let mut sum = [0.0; D];
        for &i in &members {
            let (p, w) = &points[i as usize];
            total += w;
            for d in 0..D {
                sum[d] += p[d] * w;
            }
        }
        let mut best = (0usize, 0.0f64, 0.0f64);
        for d in 0..D {
            let mean = if total > 0.0 { sum[d] / total } else { 0.0 };
            let spread: f64 = members
                .iter()
                .map(|&i| {
                    let (p, w) = &points[i as usize];
                    (p[d] - mean) * (p[d] - mean) * w
                })
                .sum();
            if spread > best.2 {
                best = (d, mean, spread);
            }
        }
        Group {
            members,
            axis: best.0,
            mean: best.1,
            spread: best.2,
        }
    };
    let mut groups = vec![measure((0..points.len() as u32).collect())];
    while groups.len() < most {
        let mut widest = (0usize, 0.0f64);
        for (g, group) in groups.iter().enumerate() {
            if group.spread > widest.1 {
                widest = (g, group.spread);
            }
        }
        if widest.1 <= 0.0 {
            break;
        }
        let group = groups.swap_remove(widest.0);
        let (low, high): (Vec<u32>, Vec<u32>) = group
            .members
            .iter()
            .partition(|&&i| points[i as usize].0[group.axis] <= group.mean);
        if low.is_empty() || high.is_empty() {
            // Rounding left nothing on one side: it cannot be cut.
            let mut whole = measure(group.members);
            whole.spread = 0.0;
            groups.push(whole);
            continue;
        }
        groups.push(measure(low));
        groups.push(measure(high));
    }
    // In the order of each group's first point, so the result does not
    // depend on how the cuts were stored.
    groups.sort_by_key(|g| g.members.iter().copied().min());
    let mut of = vec![0u32; points.len()];
    for (g, group) in groups.iter().enumerate() {
        for &i in &group.members {
            of[i as usize] = g as u32;
        }
    }
    (of, groups.len())
}

/// Bits written to bytes, the first the lowest of byte 0.
#[derive(Default)]
struct Out {
    bytes: Vec<u8>,
    used: usize,
}

impl Out {
    fn put(&mut self, v: u32, n: u32) {
        for k in 0..n {
            if self.used % 8 == 0 {
                self.bytes.push(0);
            }
            if (v >> k) & 1 == 1 {
                let last = self.bytes.len() - 1;
                self.bytes[last] |= 1 << (self.used % 8);
            }
            self.used += 1;
        }
    }
}

/// Code lengths for symbols used `counts` times each, none over
/// `limit` bits: Huffman's, with the counts halved (never to nothing)
/// until the longest fits.
fn lengths(counts: &[u32], limit: u8) -> Vec<u8> {
    let mut counts: Vec<u64> = counts.iter().map(|&c| u64::from(c)).collect();
    loop {
        let used: Vec<usize> = (0..counts.len()).filter(|&i| counts[i] > 0).collect();
        let mut out = vec![0u8; counts.len()];
        match used.len() {
            0 => return out,
            1 => {
                out[used[0]] = 1;
                return out;
            }
            _ => {}
        }
        // Nodes: (weight, order of making) → children. The two lightest
        // are joined, ties to the earlier made.
        let mut heap: BTreeMap<(u64, u32), (Option<u32>, Option<u32>)> = BTreeMap::new();
        let mut parent: Vec<u32> = vec![u32::MAX; used.len() * 2];
        for (k, &i) in used.iter().enumerate() {
            heap.insert((counts[i], k as u32), (None, None));
        }
        let mut next = used.len() as u32;
        while heap.len() > 1 {
            let (a, _) = heap.pop_first().unwrap_or(((0, 0), (None, None)));
            let (b, _) = heap.pop_first().unwrap_or(((0, 0), (None, None)));
            parent[a.1 as usize] = next;
            parent[b.1 as usize] = next;
            heap.insert((a.0 + b.0, next), (Some(a.1), Some(b.1)));
            next += 1;
        }
        let mut longest = 0;
        for (k, &i) in used.iter().enumerate() {
            let (mut depth, mut at) = (0u32, k as u32);
            while parent[at as usize] != u32::MAX {
                at = parent[at as usize];
                depth += 1;
            }
            longest = longest.max(depth);
            out[i] = depth.min(255) as u8;
        }
        if longest <= u32::from(limit) {
            return out;
        }
        for c in &mut counts {
            if *c > 0 {
                *c = (*c / 2).max(1);
            }
        }
    }
}

/// A Huffman code to write with: each symbol's code and length.
struct Code {
    sizes: Vec<u8>,
    codes: Vec<u32>,
}

impl Code {
    fn new(counts: &[u32], limit: u8) -> Self {
        let sizes = lengths(counts, limit);
        let mut per = [0u32; 18];
        for &s in &sizes {
            per[s as usize] += 1;
        }
        per[0] = 0;
        let mut first = [0u32; 18];
        let mut code = 0u32;
        for len in 1..18 {
            code = (code + per[len - 1]) << 1;
            first[len] = code;
        }
        let codes = sizes
            .iter()
            .map(|&s| {
                let c = first[s as usize];
                first[s as usize] += 1;
                c
            })
            .collect();
        Code { sizes, codes }
    }

    /// A symbol's code, its highest bit first.
    fn put(&self, out: &mut Out, sym: usize) {
        let (code, n) = (self.codes[sym], u32::from(self.sizes[sym]));
        for k in (0..n).rev() {
            out.put((code >> k) & 1, 1);
        }
    }

    /// The table, as the specification stores one: the symbols' code
    /// lengths, runs of none as runs, under a small code of their own.
    fn write(&self, out: &mut Out) {
        const ORDER: [usize; 21] = [
            17, 18, 19, 20, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15, 16,
        ];
        let total = self.sizes.iter().rposition(|&s| s > 0).map_or(0, |i| i + 1);
        out.put(total as u32, 14);
        if total == 0 {
            return;
        }
        // (symbol, extra bits' value, their width).
        let mut items: Vec<(usize, u32, u32)> = Vec::new();
        let mut i = 0;
        while i < total {
            let s = self.sizes[i];
            let mut run = 1;
            while i + run < total && self.sizes[i + run] == s {
                run += 1;
            }
            if s == 0 && run >= 3 {
                let take = run.min(138);
                if take >= 11 {
                    items.push((18, (take - 11) as u32, 7));
                } else {
                    items.push((17, (take - 3) as u32, 3));
                }
                i += take;
            } else {
                items.push((s as usize, 0, 0));
                i += 1;
            }
        }
        let mut counts = [0u32; 21];
        for &(sym, _, _) in &items {
            counts[sym] += 1;
        }
        let small = Code::new(&counts, 7);
        let kept = ORDER
            .iter()
            .rposition(|&s| small.sizes[s] > 0)
            .map_or(1, |i| i + 1);
        out.put(kept as u32, 5);
        for &slot in ORDER.iter().take(kept) {
            out.put(u32::from(small.sizes[slot]), 3);
        }
        for &(sym, extra, width) in &items {
            small.put(out, sym);
            out.put(extra, width);
        }
    }
}

/// A recently used list, as the decoder keeps it.
struct Recent {
    values: Vec<u32>,
    rover: usize,
}

impl Recent {
    fn add(&mut self, v: u32) {
        self.values[self.rover] = v;
        self.rover += 1;
        if self.rover == self.values.len() {
            self.rover = self.values.len() / 2;
        }
    }
}

/// How many entries the selector history holds.
const HISTORY: usize = 64;

/// `planes` as ETC1S under BasisLZ, with codebooks of at most
/// `endpoints` and `selectors` entries (16 000 at most each).
pub(crate) fn encode(planes: &[Plane], endpoints: usize, selectors: usize) -> Encoded {
    // Every block of every plane, in order.
    let mut blocks: Vec<[[u8; 3]; 16]> = Vec::new();
    let mut sizes: Vec<(usize, usize)> = Vec::new();
    for p in planes {
        let (across, down) = (p.width.div_ceil(4), p.height.div_ceil(4));
        sizes.push((across, down));
        for by in 0..down {
            for bx in 0..across {
                blocks.push(core::array::from_fn(|i| {
                    let x = (bx * 4 + i % 4).min(p.width - 1);
                    let y = (by * 4 + i / 4).min(p.height - 1);
                    let at = (y * p.width + x) * 3;
                    [p.rgb[at], p.rgb[at + 1], p.rgb[at + 2]]
                }));
            }
        }
    }
    // The endpoint codebook: the fitted endpoints, split into groups;
    // a group's entry is its mean colour and its commonest table.
    let fitted: Vec<([u8; 3], u8)> = blocks.iter().map(fit).collect();
    let mut distinct: BTreeMap<([u8; 3], u8), (u32, u32)> = BTreeMap::new();
    for f in &fitted {
        let next = distinct.len() as u32;
        distinct.entry(*f).or_insert((next, 0)).1 += 1;
    }
    let mut points = vec![([0.0f64; 4], 0.0f64); distinct.len()];
    for ((color, table), (id, count)) in &distinct {
        // A table's reach counts as a colour's worth of distance.
        let reach = f64::from(TABLES[*table as usize][3]) / 8.0;
        points[*id as usize] = (
            [
                f64::from(color[0]),
                f64::from(color[1]),
                f64::from(color[2]),
                reach,
            ],
            f64::from(*count),
        );
    }
    let (group_of, group_count) = split(&points, endpoints.clamp(1, 16_000));
    let mut sums = vec![([0.0f64; 3], 0.0f64, [0u32; 8]); group_count];
    for ((color, table), (id, count)) in &distinct {
        let g = &mut sums[group_of[*id as usize] as usize];
        let w = f64::from(*count);
        for c in 0..3 {
            g.0[c] += f64::from(color[c]) * w;
        }
        g.1 += w;
        g.2[*table as usize] += *count;
    }
    let book: Vec<([u8; 3], u8)> = sums
        .iter()
        .map(|(sum, w, tables)| {
            let color = sum.map(|v| ((v / w + 0.5) as u8).min(31));
            let table = (0..8).rev().max_by_key(|&t| tables[t]).unwrap_or(0) as u8;
            (color, table)
        })
        .collect();
    let endpoint_of: Vec<u32> = fitted
        .iter()
        .map(|f| group_of[distinct[f].0 as usize])
        .collect();
    // The selector codebook: each block's steps for its codebook
    // endpoint, split the same way; an entry is its group's mean step
    // at each texel.
    let chosen: Vec<[u8; 16]> = blocks
        .iter()
        .zip(&endpoint_of)
        .map(|(px, &e)| {
            let (color, table) = book[e as usize];
            steps(px, color.map(wide), table as usize).0
        })
        .collect();
    let mut patterns: BTreeMap<[u8; 16], (u32, u32)> = BTreeMap::new();
    for s in &chosen {
        let next = patterns.len() as u32;
        patterns.entry(*s).or_insert((next, 0)).1 += 1;
    }
    let mut points = vec![([0.0f64; 16], 0.0f64); patterns.len()];
    for (steps, (id, count)) in &patterns {
        points[*id as usize] = (steps.map(f64::from), f64::from(*count));
    }
    let (pattern_group, pattern_count) = split(&points, selectors.clamp(1, 16_000));
    let mut means = vec![([0.0f64; 16], 0.0f64); pattern_count];
    for (steps, (id, count)) in &patterns {
        let g = &mut means[pattern_group[*id as usize] as usize];
        let w = f64::from(*count);
        for i in 0..16 {
            g.0[i] += f64::from(steps[i]) * w;
        }
        g.1 += w;
    }
    let selector_book: Vec<[u8; 16]> = means
        .iter()
        .map(|(sum, w)| sum.map(|v| ((v / w + 0.5) as u8).min(3)))
        .collect();
    let selector_of: Vec<u32> = chosen
        .iter()
        .map(|s| pattern_group[patterns[s].0 as usize])
        .collect();

    // The endpoint codebook's stream: each entry as its difference from
    // the one before.
    let gray = book.iter().all(|(c, _)| c[0] == c[1] && c[1] == c[2]);
    let channels = if gray { 1 } else { 3 };
    let model_of = |last: u32| match last {
        0..=9 => 0,
        10..=21 => 1,
        _ => 2,
    };
    let mut color_counts = [[0u32; 32]; 3];
    let mut table_counts = [0u32; 8];
    let mut coded: Vec<(u32, [(usize, u32); 3])> = Vec::with_capacity(book.len());
    let (mut table, mut last) = (0u32, [16u32; 3]);
    for (color, t) in &book {
        let dt = (u32::from(*t) + 8 - table) & 7;
        table = u32::from(*t);
        table_counts[dt as usize] += 1;
        let mut deltas = [(0usize, 0u32); 3];
        for c in 0..channels {
            let model = model_of(last[c]);
            let d = (u32::from(color[c]) + 32 - last[c]) & 31;
            last[c] = u32::from(color[c]);
            color_counts[model][d as usize] += 1;
            deltas[c] = (model, d);
        }
        coded.push((dt, deltas));
    }
    let color_codes = color_counts.map(|c| Code::new(&c, 16));
    let table_code = Code::new(&table_counts, 16);
    let mut out = Out::default();
    for code in &color_codes {
        code.write(&mut out);
    }
    table_code.write(&mut out);
    out.put(u32::from(gray), 1);
    for (dt, deltas) in &coded {
        table_code.put(&mut out, *dt as usize);
        for &(model, d) in deltas.iter().take(channels) {
            color_codes[model].put(&mut out, d as usize);
        }
    }
    let endpoints_data = out.bytes;

    // The selector codebook's stream: the first entry plain, each next
    // as its rows' difference (xor) from the one before.
    let rows: Vec<[u8; 4]> = selector_book
        .iter()
        .map(|s| core::array::from_fn(|y| (0..4).fold(0u8, |v, x| v | (s[y * 4 + x] << (2 * x)))))
        .collect();
    let mut delta_counts = [0u32; 256];
    for pair in rows.windows(2) {
        for y in 0..4 {
            delta_counts[(pair[0][y] ^ pair[1][y]) as usize] += 1;
        }
    }
    let delta_code = Code::new(&delta_counts, 16);
    let mut out = Out::default();
    out.put(0, 3);
    delta_code.write(&mut out);
    for (i, r) in rows.iter().enumerate() {
        for y in 0..4 {
            if i == 0 {
                out.put(u32::from(r[y]), 8);
            } else {
                delta_code.put(&mut out, (rows[i - 1][y] ^ r[y]) as usize);
            }
        }
    }
    let selectors_data = out.bytes;

    // The slices: what each block's endpoint is predicted from, and its
    // selector as an index or a place in the recent ones. First what is
    // to be written, for the tables' counts; then the bits.
    enum Item {
        Pred(u32),
        Delta(u32),
        Selector(u32),
    }
    let (endpoint_count, selector_count) = (book.len(), selector_book.len());
    let mut pred_counts = vec![0u32; 257];
    let mut delta_counts = vec![0u32; endpoint_count.max(1)];
    let mut selector_counts = vec![0u32; selector_count + HISTORY + 1];
    let mut streams: Vec<Vec<Item>> = Vec::new();
    let mut first = 0;
    for &(across, down) in &sizes {
        let at = |bx: usize, by: usize| first + by * across + bx;
        let mut items = Vec::new();
        let mut recent = Recent {
            values: vec![0; HISTORY],
            rover: HISTORY / 2,
        };
        let mut last = 0u32;
        // Each block's prediction (0 the block to the left, 1 above, 2
        // above left, 3 a difference from the last), in block order,
        // which is the order the differences are read in.
        let mut preds = vec![3u32; across * down];
        let mut deltas = vec![0u32; across * down];
        for by in 0..down {
            for bx in 0..across {
                let e = endpoint_of[at(bx, by)];
                let pred = if bx > 0 && e == last {
                    0
                } else if by > 0 && e == endpoint_of[at(bx, by - 1)] {
                    1
                } else if bx > 0 && by > 0 && e == endpoint_of[at(bx - 1, by - 1)] {
                    2
                } else {
                    3
                };
                preds[by * across + bx] = pred;
                deltas[by * across + bx] =
                    (e + endpoint_count as u32 - last) % endpoint_count as u32;
                last = e;
            }
        }
        for by in 0..down {
            for bx in 0..across {
                if bx & 1 == 0 && by & 1 == 0 {
                    // Four blocks' predictions, two bits each: this
                    // one, the one to its right, then the two below.
                    let mut sym = 0;
                    for (k, (dx, dy)) in [(0, 0), (1, 0), (0, 1), (1, 1)].into_iter().enumerate() {
                        if bx + dx < across && by + dy < down {
                            sym |= preds[(by + dy) * across + bx + dx] << (2 * k);
                        }
                    }
                    pred_counts[sym as usize] += 1;
                    items.push(Item::Pred(sym));
                }
                if preds[by * across + bx] == 3 {
                    let d = deltas[by * across + bx];
                    delta_counts[d as usize] += 1;
                    items.push(Item::Delta(d));
                }
                let s = selector_of[at(bx, by)];
                let sym = match recent.values.iter().position(|&v| v == s) {
                    // A place in the recent ones is only theirs once
                    // something was put there; an unused place holds 0,
                    // which reads as selector 0 and is right if it is.
                    Some(i) => {
                        if i > 0 {
                            recent.values.swap(i / 2, i);
                        }
                        (selector_count + i) as u32
                    }
                    None => {
                        recent.add(s);
                        s
                    }
                };
                selector_counts[sym as usize] += 1;
                items.push(Item::Selector(sym));
            }
        }
        streams.push(items);
        first += across * down;
    }
    let pred_code = Code::new(&pred_counts, 16);
    let delta_code = Code::new(&delta_counts, 16);
    let selector_code = Code::new(&selector_counts, 16);
    let run_code = Code::new(&[1], 16);
    let mut out = Out::default();
    pred_code.write(&mut out);
    delta_code.write(&mut out);
    selector_code.write(&mut out);
    run_code.write(&mut out);
    out.put(HISTORY as u32, 13);
    let tables = out.bytes;
    let slices = streams
        .iter()
        .map(|items| {
            let mut out = Out::default();
            for item in items {
                match item {
                    Item::Pred(s) => pred_code.put(&mut out, *s as usize),
                    Item::Delta(d) => delta_code.put(&mut out, *d as usize),
                    Item::Selector(s) => selector_code.put(&mut out, *s as usize),
                }
            }
            out.bytes
        })
        .collect();
    Encoded {
        endpoint_count,
        selector_count,
        endpoints: endpoints_data,
        selectors: selectors_data,
        tables,
        slices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::etc1s::Codebooks;

    /// What is written reads back as the blocks the encoder chose: with
    /// codebooks large enough to hold every block's own fit, each texel
    /// is its block's base colour and step; with small ones the image
    /// is still close.
    #[test]
    fn slices_read_back() {
        let (w, h) = (37usize, 22usize);
        let mut rgb = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let v = (x * 6) as u8;
                rgb.extend_from_slice(&if x < 20 {
                    [v, 200 - v, (y * 10) as u8]
                } else {
                    [30, v, 255 - v]
                });
            }
        }
        let gray: Vec<u8> = (0..w * h).flat_map(|i| [(i % 251) as u8; 3]).collect();
        let planes = [
            Plane {
                width: w,
                height: h,
                rgb: rgb.clone(),
            },
            Plane {
                width: w,
                height: h,
                rgb: gray.clone(),
            },
            Plane {
                width: 5,
                height: 3,
                rgb: rgb[..45].to_vec(),
            },
        ];
        // What each block is on its own: its fitted colour and table,
        // and each texel's step.
        let alone = |p: &Plane| -> Vec<u8> {
            let mut out = vec![0u8; p.width * p.height * 3];
            for by in 0..p.height.div_ceil(4) {
                for bx in 0..p.width.div_ceil(4) {
                    let px: [[u8; 3]; 16] = core::array::from_fn(|i| {
                        let x = (bx * 4 + i % 4).min(p.width - 1);
                        let y = (by * 4 + i / 4).min(p.height - 1);
                        let at = (y * p.width + x) * 3;
                        [p.rgb[at], p.rgb[at + 1], p.rgb[at + 2]]
                    });
                    let (color, table) = fit(&px);
                    let (chosen, _) = steps(&px, color.map(wide), table as usize);
                    for i in 0..16 {
                        let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
                        if x < p.width && y < p.height {
                            for c in 0..3 {
                                out[(y * p.width + x) * 3 + c] =
                                    (wide(color[c]) + TABLES[table as usize][chosen[i] as usize])
                                        .clamp(0, 255) as u8;
                            }
                        }
                    }
                }
            }
            out
        };
        for most in [16_000, 24] {
            let e = encode(&planes, most, most);
            let books = Codebooks::read(
                e.endpoint_count,
                e.selector_count,
                &e.endpoints,
                &e.selectors,
                &e.tables,
            )
            .unwrap();
            assert!(e.endpoint_count <= most && e.selector_count <= most);
            for (plane, slice) in planes.iter().zip(&e.slices) {
                let back = books.slice(slice, plane.width, plane.height).unwrap();
                let own = alone(plane);
                if most > 24 {
                    assert_eq!(back, own);
                    continue;
                }
                // Codebooks of 24: further from each block's own fit,
                // by less than a wrong reference would be.
                let rms = (back
                    .iter()
                    .zip(&own)
                    .map(|(a, b)| f64::from(u32::from(a.abs_diff(*b)).pow(2)))
                    .sum::<f64>()
                    / back.len() as f64)
                    .sqrt();
                assert!(rms > 0.0 && rms < 16.0, "{rms}");
            }
        }
    }
}
