//! Decodes the ETC1S blocks of a KTX2 file under BasisLZ
//! (`KHR_texture_basisu`'s small mode) to RGBA: the two codebooks every
//! image of the file shares, and a slice's blocks as references into
//! them (`docs/research.md` §57).
//!
//! Entry: [`Codebooks::read`] and [`Codebooks::slice`]. Written from
//! the KTX 2.0 specification's BasisLZ appendices. An ETC1S block is
//! one base colour (5 bits a channel) and one of ETC1's eight
//! intensity tables for all 16 texels, each texel one of the table's
//! four steps.

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

/// Bits from a byte string, the first the lowest of byte 0; past the
/// end they are 0, and `over` says so.
struct Bits<'a> {
    data: &'a [u8],
    at: usize,
    over: bool,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Bits {
            data,
            at: 0,
            over: false,
        }
    }

    fn bit(&mut self) -> u32 {
        let byte = self.data.get(self.at / 8).copied();
        if byte.is_none() {
            self.over = true;
        }
        let v = u32::from(byte.unwrap_or(0) >> (self.at % 8)) & 1;
        self.at += 1;
        v
    }

    fn take(&mut self, n: u32) -> u32 {
        (0..n).fold(0, |v, k| v | (self.bit() << k))
    }

    /// A number in chunks of `width` bits, each with one more bit that
    /// says another follows.
    fn chunks(&mut self, width: u32) -> u32 {
        let (mut v, mut shift) = (0u32, 0);
        loop {
            let s = self.take(width + 1);
            v |= (s & ((1 << width) - 1)) << shift;
            shift += width;
            if s >> width == 0 || shift >= 32 {
                return v;
            }
        }
    }
}

/// A canonical Huffman code (Deflate's): each length's first code and
/// its symbols in order.
struct Huffman {
    /// Per length 1..=16: the first code of that length, how many
    /// there are, and where its symbols start in `symbols`.
    lengths: [(u32, u32, u32); 17],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(sizes: &[u8]) -> Result<Self, String> {
        let mut counts = [0u32; 17];
        for &s in sizes {
            counts[s as usize] += 1;
        }
        counts[0] = 0;
        let mut lengths = [(0u32, 0u32, 0u32); 17];
        let (mut code, mut at) = (0u32, 0u32);
        for len in 1..17 {
            code = (code + counts[len - 1]) << 1;
            if u64::from(code) + u64::from(counts[len]) > 1 << len {
                return Err("a Huffman table with more codes than its lengths hold".into());
            }
            lengths[len] = (code, counts[len], at);
            at += counts[len];
        }
        let mut symbols = vec![0u16; at as usize];
        let mut next = lengths.map(|l| l.2);
        for (sym, &s) in sizes.iter().enumerate() {
            if s > 0 {
                symbols[next[s as usize] as usize] = sym as u16;
                next[s as usize] += 1;
            }
        }
        Ok(Huffman { lengths, symbols })
    }

    /// The table a stream holds: its symbols' code lengths, themselves
    /// run-length and Huffman coded.
    fn read(bits: &mut Bits) -> Result<Self, String> {
        const ORDER: [usize; 21] = [
            17, 18, 19, 20, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15, 16,
        ];
        let total = bits.take(14) as usize;
        if total == 0 {
            return Huffman::new(&[]);
        }
        let count = bits.take(5) as usize;
        if !(1..=21).contains(&count) {
            return Err("a Huffman table with no code-length codes".into());
        }
        let mut small = [0u8; 21];
        for &slot in ORDER.iter().take(count) {
            small[slot] = bits.take(3) as u8;
        }
        let small = Huffman::new(&small)?;
        let mut sizes: Vec<u8> = Vec::with_capacity(total);
        while sizes.len() < total {
            let sym = small.decode(bits)?;
            let (value, run) = match sym {
                0..=16 => (sym as u8, 1),
                17 => (0, bits.take(3) as usize + 3),
                18 => (0, bits.take(7) as usize + 11),
                _ => {
                    let Some(&last) = sizes.last() else {
                        return Err("a Huffman table that repeats nothing".into());
                    };
                    let run = if sym == 19 {
                        bits.take(2) as usize + 3
                    } else {
                        bits.take(7) as usize + 7
                    };
                    (last, run)
                }
            };
            if sizes.len() + run > total || bits.over {
                return Err("a Huffman table longer than it says".into());
            }
            sizes.resize(sizes.len() + run, value);
        }
        Huffman::new(&sizes)
    }

    fn decode(&self, bits: &mut Bits) -> Result<u32, String> {
        let mut code = 0u32;
        for len in 1..17 {
            code = (code << 1) | bits.bit();
            let (first, count, at) = self.lengths[len];
            if code >= first && code - first < count {
                return Ok(u32::from(self.symbols[(at + code - first) as usize]));
            }
        }
        Err("a Huffman code no symbol has".into())
    }
}

/// A recently used list: a new value overwrites one in the second half
/// in turn; a value used moves halfway to the front.
struct Recent {
    values: Vec<u32>,
    rover: usize,
}

impl Recent {
    fn new(n: usize) -> Self {
        Recent {
            values: vec![0; n],
            rover: n / 2,
        }
    }

    fn add(&mut self, v: u32) {
        if self.values.is_empty() {
            return;
        }
        self.values[self.rover] = v;
        self.rover += 1;
        if self.rover == self.values.len() {
            self.rover = self.values.len() / 2;
        }
    }

    fn used(&mut self, i: usize) {
        if i > 0 {
            self.values.swap(i / 2, i);
        }
    }
}

/// What every image of a BasisLZ file shares.
pub(crate) struct Codebooks {
    /// Each endpoint's 5-bit colour and intensity table.
    endpoints: Vec<([u8; 3], u8)>,
    /// Each selector's four rows, two bits a texel from the lowest.
    selectors: Vec<[u8; 4]>,
    endpoint_pred: Huffman,
    endpoint_delta: Huffman,
    selector: Huffman,
    run: Huffman,
    history: usize,
}

impl Codebooks {
    /// The codebooks and the slices' Huffman tables, from the three
    /// parts of the file's global data.
    pub(crate) fn read(
        endpoint_count: usize,
        selector_count: usize,
        endpoints_data: &[u8],
        selectors_data: &[u8],
        tables_data: &[u8],
    ) -> Result<Self, String> {
        let mut bits = Bits::new(endpoints_data);
        let color = [
            Huffman::read(&mut bits)?,
            Huffman::read(&mut bits)?,
            Huffman::read(&mut bits)?,
        ];
        let inten = Huffman::read(&mut bits)?;
        let gray = bits.take(1) == 1;
        let mut endpoints = Vec::with_capacity(endpoint_count);
        let (mut table, mut last) = (0u32, [16u32; 3]);
        for _ in 0..endpoint_count {
            table = (table + inten.decode(&mut bits)?) & 7;
            for value in last.iter_mut().take(if gray { 1 } else { 3 }) {
                let model = match *value {
                    0..=9 => &color[0],
                    10..=21 => &color[1],
                    _ => &color[2],
                };
                *value = (*value + model.decode(&mut bits)?) & 31;
            }
            if gray {
                last = [last[0]; 3];
            }
            endpoints.push((last.map(|v| v as u8), table as u8));
        }
        if bits.over {
            return Err("the endpoint codebook is cut short".into());
        }
        let mut bits = Bits::new(selectors_data);
        if bits.take(2) != 0 {
            return Err("a selector codebook of another kind".into());
        }
        let mut selectors: Vec<[u8; 4]> = Vec::with_capacity(selector_count);
        if bits.take(1) == 1 {
            for _ in 0..selector_count {
                selectors.push(core::array::from_fn(|_| bits.take(8) as u8));
            }
        } else {
            let delta = Huffman::read(&mut bits)?;
            let mut rows: [u8; 4] = core::array::from_fn(|_| bits.take(8) as u8);
            for i in 0..selector_count {
                if i > 0 {
                    for row in &mut rows {
                        *row ^= delta.decode(&mut bits)? as u8;
                    }
                }
                selectors.push(rows);
            }
        }
        if bits.over {
            return Err("the selector codebook is cut short".into());
        }
        let mut bits = Bits::new(tables_data);
        let endpoint_pred = Huffman::read(&mut bits)?;
        let endpoint_delta = Huffman::read(&mut bits)?;
        let selector = Huffman::read(&mut bits)?;
        let run = Huffman::read(&mut bits)?;
        let history = bits.take(13) as usize;
        if bits.over {
            return Err("the slice tables are cut short".into());
        }
        Ok(Codebooks {
            endpoints,
            selectors,
            endpoint_pred,
            endpoint_delta,
            selector,
            run,
            history,
        })
    }

    /// One slice of `width` × `height` texels as RGB, 3 bytes a texel,
    /// rows from the top.
    pub(crate) fn slice(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
    ) -> Result<Vec<u8>, String> {
        const REPEAT: u32 = 4 * 4 * 4 * 4;
        let (across, down) = (width.div_ceil(4), height.div_ceil(4));
        let (endpoint_count, selector_count) = (self.endpoints.len(), self.selectors.len());
        let mut bits = Bits::new(data);
        // The endpoints of the last two rows of blocks, and the lower
        // half of each 2×2 group's prediction.
        let mut rows = [vec![0usize; across], vec![0usize; across]];
        let mut below = vec![0u32; across.div_ceil(2)];
        let mut recent = Recent::new(self.history);
        let (mut pred_bits, mut last_pred, mut pred_run) = (0u32, 0u32, 0u32);
        let (mut selector_run, mut last_endpoint) = (0u32, 0usize);
        let mut out = vec![0u8; width * height * 3];
        for by in 0..down {
            let row = by & 1;
            for bx in 0..across {
                if bx & 1 == 0 {
                    if by & 1 == 0 {
                        if pred_run > 0 {
                            pred_run -= 1;
                            pred_bits = last_pred;
                        } else {
                            pred_bits = self.endpoint_pred.decode(&mut bits)?;
                            if pred_bits == REPEAT {
                                pred_run = bits.chunks(4) + 2;
                                pred_bits = last_pred;
                            } else {
                                last_pred = pred_bits;
                            }
                        }
                        below[bx >> 1] = pred_bits >> 4;
                    } else {
                        pred_bits = below[bx >> 1];
                    }
                }
                let pred = pred_bits & 3;
                pred_bits >>= 2;
                let endpoint = match pred {
                    0 if bx > 0 => last_endpoint,
                    1 if by > 0 => rows[row ^ 1][bx],
                    2 if bx > 0 && by > 0 => rows[row ^ 1][bx - 1],
                    3 => {
                        let e = self.endpoint_delta.decode(&mut bits)? as usize + last_endpoint;
                        if e >= endpoint_count {
                            e - endpoint_count
                        } else {
                            e
                        }
                    }
                    _ => return Err("a block that reuses a block that is not there".into()),
                };
                rows[row][bx] = endpoint;
                last_endpoint = endpoint;
                let mut sym = selector_count as u32;
                if selector_run > 0 {
                    selector_run -= 1;
                } else {
                    sym = self.selector.decode(&mut bits)?;
                    if sym as usize == selector_count + self.history {
                        let run = self.run.decode(&mut bits)?;
                        selector_run = if run == 63 {
                            bits.chunks(7) + 3
                        } else {
                            run + 3
                        } - 1;
                        sym = selector_count as u32;
                    }
                }
                let selector = if sym as usize >= selector_count {
                    let i = sym as usize - selector_count;
                    let v = *recent
                        .values
                        .get(i)
                        .ok_or("a selector from a history the file has not")?;
                    recent.used(i);
                    v as usize
                } else {
                    recent.add(sym);
                    sym as usize
                };
                if bits.over {
                    return Err("the slice is cut short".into());
                }
                let (Some(&(color, table)), Some(texels)) =
                    (self.endpoints.get(endpoint), self.selectors.get(selector))
                else {
                    return Err("a block of an endpoint or selector the file has not".into());
                };
                let base = color.map(|c| i32::from((c << 3) | (c >> 2)));
                for ty in 0..4 {
                    for tx in 0..4 {
                        let (x, y) = (bx * 4 + tx, by * 4 + ty);
                        if x >= width || y >= height {
                            continue;
                        }
                        let step = TABLES[table as usize][((texels[ty] >> (2 * tx)) & 3) as usize];
                        let at = (y * width + x) * 3;
                        for c in 0..3 {
                            out[at + c] = (base[c] + step).clamp(0, 255) as u8;
                        }
                    }
                }
            }
        }
        Ok(out)
    }
}
