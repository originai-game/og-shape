//! Draco-compressed triangle meshes, decoded (`docs/research.md` §44):
//! what glTF's `KHR_draco_mesh_compression` holds. Written from the
//! Draco bitstream specification (version 2.2; Google, Apache-2.0; see
//! `third_party/draco/`), whose names the comments keep so the two
//! can be read side by side.
//!
//! Entry: [`decode`]. Read: sequential and EdgeBreaker connectivity
//! (standard and valence), both traversals, and the attributes'
//! values by every prediction the specification has but the normals',
//! which are parsed and not computed (the importer recomputes normals).
//! Integer work throughout; a quantized value is scaled in f32 as the
//! specification scales it.

use std::collections::BTreeMap;

/// One attribute of a decoded mesh.
pub(crate) struct Attribute {
    /// The id glTF names it by.
    pub id: u32,
    /// Numbers a point.
    pub components: usize,
    /// Per point, its numbers. Empty for normals (not computed).
    pub values: Vec<f64>,
    /// The values are whole numbers of this many bytes, as stored
    /// (negative: signed); 0 for floats.
    pub integer: i8,
}

/// A decoded mesh: triangles over points, and each attribute a point.
pub(crate) struct Decoded {
    pub faces: Vec<[u32; 3]>,
    pub points: usize,
    pub attributes: Vec<Attribute>,
}

type Res<T> = Result<T, String>;

fn short<T>() -> Res<T> {
    Err("the Draco data is cut short".into())
}

fn bad<T>(what: &str) -> Res<T> {
    Err(format!("the Draco data is not valid: {what}"))
}

struct Reader<'a> {
    d: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> Res<&'a [u8]> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.d.len());
        let Some(end) = end else { return short() };
        let b = &self.d[self.at..end];
        self.at = end;
        Ok(b)
    }
    fn u8(&mut self) -> Res<u8> {
        Ok(self.bytes(1)?[0])
    }
    fn u16(&mut self) -> Res<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Res<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i32(&mut self) -> Res<i32> {
        Ok(self.u32()? as i32)
    }
    fn f32(&mut self) -> Res<f32> {
        Ok(f32::from_bits(self.u32()?))
    }
    /// LEB128.
    fn var(&mut self) -> Res<u64> {
        let mut v = 0u64;
        for shift in 0..10 {
            let b = self.u8()?;
            v |= u64::from(b & 0x7f) << (7 * shift);
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        bad("a number of more than ten bytes")
    }
    /// A count of things the data left could hold at `per_byte` of
    /// them a byte: a count is believed only as far as the bytes go.
    fn count(&mut self, per_byte: usize) -> Res<usize> {
        let n = self.var()?;
        let room = ((self.d.len() - self.at) as u64 + 1).saturating_mul(per_byte as u64);
        if n > room {
            return bad("a count larger than the data");
        }
        Ok(n as usize)
    }
}

/// Bits of a byte run, the low bit of each byte first.
struct Bits<'a> {
    d: &'a [u8],
    bit: usize,
}

impl Bits<'_> {
    fn read(&mut self, n: u32) -> Res<u32> {
        let mut v = 0u32;
        for i in 0..n {
            let Some(byte) = self.d.get(self.bit / 8) else {
                return short();
            };
            v |= u32::from((byte >> (self.bit % 8)) & 1) << i;
            self.bit += 1;
        }
        Ok(v)
    }
}

const L_RANS_BASE: u32 = 4096;

/// An ANS decoder over a buffer read from its end.
struct Ans<'a> {
    buf: &'a [u8],
    off: usize,
    state: u32,
}

impl<'a> Ans<'a> {
    /// RansInitDecoder.
    fn new(buf: &'a [u8], base: u32) -> Res<Ans<'a>> {
        let n = buf.len();
        let Some(&last) = buf.last() else {
            return short();
        };
        let take = usize::from(last >> 6) + 1;
        if n < take {
            return short();
        }
        let mut state = 0u32;
        for k in 0..take {
            state |= u32::from(buf[n - take + k]) << (8 * k);
        }
        state &= (1u32 << (8 * take as u32 - 2)) - 1;
        Ok(Ans {
            buf,
            off: n - take,
            state: state.wrapping_add(base),
        })
    }

    /// RabsDescRead: one bit that is zero with probability `p0` / 256.
    fn bit(&mut self, p0: u8) -> bool {
        let p = 256 - u32::from(p0);
        if self.state < L_RANS_BASE && self.off > 0 {
            self.off -= 1;
            self.state = self.state * 256 + u32::from(self.buf[self.off]);
        }
        let x = self.state;
        let (quot, rem) = (x / 256, x % 256);
        let xn = quot * p;
        let val = rem < p;
        self.state = if val {
            xn + rem
        } else {
            x.wrapping_sub(xn).wrapping_sub(p)
        };
        val
    }

    /// RansRead: one symbol of a table.
    fn symbol(&mut self, base: u32, precision: u32, table: &Table) -> Res<u32> {
        while self.state < base && self.off > 0 {
            self.off -= 1;
            self.state = self.state * 256 + u32::from(self.buf[self.off]);
        }
        let (quo, rem) = (self.state / precision, self.state % precision);
        let Some(&sym) = table.lut.get(rem as usize) else {
            return bad("a symbol outside its table");
        };
        let (prob, cum) = table.probs[sym as usize];
        self.state = (u64::from(quo) * u64::from(prob) + u64::from(rem) - u64::from(cum)) as u32;
        Ok(sym)
    }
}

/// A symbol table: each symbol's probability and the sum before it, and
/// the symbol of each remainder.
struct Table {
    probs: Vec<(u32, u32)>,
    lut: Vec<u32>,
}

/// BuildSymbolTables.
fn table(r: &mut Reader, symbols: usize, precision: u32) -> Res<Table> {
    let mut probs = vec![(0u32, 0u32); symbols];
    let mut i = 0;
    while i < symbols {
        let data = r.u8()?;
        let token = data & 3;
        if token == 3 {
            i += usize::from(data >> 2) + 1;
        } else {
            let mut prob = u32::from(data >> 2);
            for j in 0..u32::from(token) {
                prob |= u32::from(r.u8()?) << (8 * (j + 1) - 2);
            }
            probs[i].0 = prob;
            i += 1;
        }
    }
    let mut lut = Vec::with_capacity(precision as usize);
    let mut cum = 0u32;
    for (s, p) in probs.iter_mut().enumerate() {
        p.1 = cum;
        cum = cum.saturating_add(p.0);
        if cum > precision {
            return bad("symbol probabilities over their sum");
        }
        lut.resize(cum as usize, s as u32);
    }
    Ok(Table { probs, lut })
}

/// DecodeSymbols: `values` symbols, `components` of them a bit length
/// when tagged.
fn symbols(r: &mut Reader, values: usize, components: usize) -> Res<Vec<u32>> {
    let mut out = Vec::with_capacity(values);
    match r.u8()? {
        0 => {
            // Tagged: a bit length a point, then that many bits a number.
            let n = r.count(64)?;
            let t = table(r, n, 4096)?;
            let size = r.count(1)?;
            let mut ans = Ans::new(r.bytes(size)?, 16384)?;
            let mut bits = Bits {
                d: &r.d[r.at..],
                bit: 0,
            };
            let mut i = 0;
            while i < values {
                let size = ans.symbol(16384, 4096, &t)?;
                if size > 32 {
                    return bad("a number of more than 32 bits");
                }
                for _ in 0..components.max(1) {
                    out.push(bits.read(size)?);
                }
                i += components.max(1);
            }
            r.at += bits.bit.div_ceil(8);
        }
        1 => {
            let max_bits = u32::from(r.u8()?);
            let n = r.count(64)?;
            let precision_bits = (3 * max_bits / 2).clamp(12, 20);
            let precision = 1u32 << precision_bits;
            let t = table(r, n, precision)?;
            let size = r.count(1)?;
            let mut ans = Ans::new(r.bytes(size)?, precision * 4)?;
            for _ in 0..values {
                out.push(ans.symbol(precision * 4, precision, &t)?);
            }
        }
        _ => return bad("a symbol scheme that is neither tagged nor raw"),
    }
    Ok(out)
}

fn next(c: i32) -> i32 {
    if c < 0 {
        c
    } else if c % 3 == 2 {
        c - 2
    } else {
        c + 1
    }
}

fn prev(c: i32) -> i32 {
    if c < 0 {
        c
    } else if c % 3 == 0 {
        c + 2
    } else {
        c - 1
    }
}

/// The corner table EdgeBreaker builds.
struct Mesh {
    /// Per corner, its vertex (`face_to_vertex`, `corner_to_vertex_map_[0]`).
    ctv: Vec<i32>,
    /// Per corner, the corner across its opposite edge.
    opp: Vec<i32>,
    /// Per vertex, one of its corners (its left-most on a boundary).
    vertex_corner: Vec<i32>,
    /// Per vertex, its corners so far (stale ones are checked).
    corners_of: Vec<Vec<i32>>,
    hole: Vec<bool>,
    faces: usize,
}

impl Mesh {
    fn opposite(&self, c: i32) -> i32 {
        if c < 0 {
            return -1;
        }
        self.opp.get(c as usize).copied().unwrap_or(-1)
    }
    fn set_opposite(&mut self, a: i32, b: i32) -> Res<()> {
        let n = self.opp.len() as i32;
        if a < 0 || b < 0 || a >= n || b >= n {
            return bad("a corner outside the mesh");
        }
        self.opp[a as usize] = b;
        self.opp[b as usize] = a;
        Ok(())
    }
    fn vert(&self, c: i32) -> Res<i32> {
        match self.ctv.get(c.max(0) as usize) {
            Some(&v) if c >= 0 && v >= 0 => Ok(v),
            _ => bad("a corner without a vertex"),
        }
    }
    /// MapCornerToVertex.
    fn map(&mut self, c: i32, v: i32) -> Res<()> {
        if c < 0 || c as usize >= self.ctv.len() || v < 0 || v as usize >= self.vertex_corner.len()
        {
            return bad("a vertex outside the mesh");
        }
        self.ctv[c as usize] = v;
        self.vertex_corner[v as usize] = c;
        self.corners_of[v as usize].push(c);
        Ok(())
    }
    fn swing_left(&self, c: i32) -> i32 {
        next(self.opposite(next(c)))
    }
    fn swing_right(&self, c: i32) -> i32 {
        prev(self.opposite(prev(c)))
    }
}

/// A decoder's view of the corners: the positions', or an attribute's
/// own, cut at its seams.
struct Seams {
    /// Per corner: the edge opposite it is a seam.
    edge: Vec<bool>,
    /// Per position vertex: it is on a seam.
    vertex: Vec<bool>,
    /// Per corner, its attribute vertex.
    ctv: Vec<i32>,
    /// Per attribute vertex, its left-most corner.
    left_most: Vec<i32>,
}

struct View<'a> {
    m: &'a Mesh,
    s: Option<&'a Seams>,
}

impl View<'_> {
    fn vert(&self, c: i32) -> Res<i32> {
        match self.s {
            None => self.m.vert(c),
            Some(s) => match s.ctv.get(c.max(0) as usize) {
                Some(&v) if c >= 0 && v >= 0 => Ok(v),
                _ => bad("a corner without a vertex"),
            },
        }
    }
    fn vertices(&self) -> usize {
        match self.s {
            None => self.m.vertex_corner.len(),
            Some(s) => s.left_most.len(),
        }
    }
    fn opposite(&self, c: i32) -> i32 {
        if c < 0 {
            return -1;
        }
        if self.s.is_some_and(|s| s.edge[c as usize]) {
            return -1;
        }
        self.m.opposite(c)
    }
    fn swing_left(&self, c: i32) -> i32 {
        next(self.opposite(next(c)))
    }
    fn swing_right(&self, c: i32) -> i32 {
        prev(self.opposite(prev(c)))
    }
    fn on_boundary(&self, v: i32) -> Res<bool> {
        match self.s {
            None => {
                let c = self.m.vertex_corner[v as usize];
                Ok(c < 0 || self.m.swing_left(c) < 0)
            }
            Some(s) => {
                let c = s.left_most[v as usize];
                Ok(c < 0 || s.vertex[self.m.vert(c)? as usize])
            }
        }
    }
}

/// Per attribute with its own connectivity: the probability of no seam
/// and the seam bits.
type SeamBits<'a> = Vec<(u8, &'a [u8])>;

const C: u32 = 0;
const S: u32 = 1;
const L: u32 = 3;
const R: u32 = 5;
const E: u32 = 7;

/// EdgeBreaker connectivity: the mesh, and each attribute's seam bits
/// still to read.
fn edgebreaker<'a>(r: &mut Reader<'a>) -> Res<(Mesh, SeamBits<'a>)> {
    let traversal = r.u8()?;
    if traversal != 0 && traversal != 2 {
        return bad("an EdgeBreaker traversal that is neither standard nor valence");
    }
    let valence = traversal == 2;
    let vertices = r.count(96)?;
    let faces = r.count(32)?;
    let attribute_data = usize::from(r.u8()?);
    let n_symbols = r.count(32)?;
    let n_splits = r.count(32)?;
    if n_symbols > faces || faces > u32::MAX as usize / 3 {
        return bad("more symbols than faces");
    }
    // Topology split events.
    let n_events = r.count(1)?;
    let mut source_ids = Vec::with_capacity(n_events);
    let mut split_ids = Vec::with_capacity(n_events);
    let mut last = 0i64;
    for _ in 0..n_events {
        let source = r.var()? as i64 + last;
        let split = source - r.var()? as i64;
        last = source;
        source_ids.push(source);
        split_ids.push(split);
    }
    let mut edge_bits = Vec::with_capacity(n_events);
    {
        let mut bits = Bits {
            d: &r.d[r.at..],
            bit: 0,
        };
        for _ in 0..n_events {
            edge_bits.push(bits.read(1)?);
        }
        r.at += bits.bit.div_ceil(8);
    }
    // The traversal's buffers.
    let mut symbol_bits = None;
    if !valence {
        let size = r.count(1)?;
        symbol_bits = Some(Bits {
            d: r.bytes(size)?,
            bit: 0,
        });
    }
    let start_prob = r.u8()?;
    let size = r.count(1)?;
    let start_faces = r.bytes(size)?;
    let mut seams = Vec::with_capacity(attribute_data);
    for _ in 0..attribute_data {
        let prob = r.u8()?;
        let size = r.count(1)?;
        seams.push((prob, r.bytes(size)?));
    }
    let nv = vertices + n_splits;
    let mut valences = vec![0i32; if valence { nv } else { 0 }];
    let mut contexts: Vec<Vec<u32>> = Vec::new();
    if valence {
        for _ in 0..6 {
            let n = r.var()? as usize;
            if n > faces {
                return bad("more context symbols than faces");
            }
            contexts.push(if n > 0 { symbols(r, n, 1)? } else { Vec::new() });
        }
    }
    let mut m = Mesh {
        ctv: vec![-1; faces * 3],
        opp: vec![-1; faces * 3],
        vertex_corner: vec![-1; nv],
        corners_of: vec![Vec::new(); nv],
        hole: vec![true; nv],
        faces: 0,
    };
    let mut stack: Vec<i32> = Vec::new();
    let mut split_at: BTreeMap<i64, Vec<i32>> = BTreeMap::new();
    let mut last_vert: i32 = -1;
    let mut context: i32 = -1;
    let empty = || "the Draco data is not valid: EdgeBreaker ran out of open edges".to_string();
    let bump = |valences: &mut Vec<i32>, v: i32, by: i32| {
        if let Some(x) = valences.get_mut(v as usize) {
            *x += by;
        }
    };
    for id in 0..n_symbols {
        let symbol = if valence {
            if context < 0 {
                E
            } else {
                let Some(s) = contexts[context as usize].pop() else {
                    return bad("a valence context out of symbols");
                };
                *[C, S, L, R, E]
                    .get(s as usize)
                    .ok_or("the Draco data is not valid: an EdgeBreaker symbol over 4")?
            }
        } else {
            let bits = symbol_bits.as_mut().ok_or_else(empty)?;
            let mut s = bits.read(1)?;
            if s != C {
                s |= bits.read(2)? << 1;
            }
            s
        };
        let c = (3 * id) as i32;
        let mut check_split = false;
        let (vert, nxt, prv);
        match symbol {
            C => {
                let a = *stack.last().ok_or_else(empty)?;
                let mut b = prev(a);
                let mut guard = 0;
                while m.opposite(b) >= 0 {
                    b = prev(m.opposite(b));
                    guard += 1;
                    if guard > faces * 3 {
                        return bad("a fan that does not end");
                    }
                }
                m.set_opposite(a, c + 1)?;
                m.set_opposite(b, c + 2)?;
                *stack.last_mut().ok_or_else(empty)? = c;
                vert = m.vert(next(a))?;
                nxt = m.vert(next(b))?;
                prv = m.vert(prev(a))?;
                bump(&mut valences, nxt, 1);
                bump(&mut valences, prv, 1);
                m.hole[vert as usize] = false;
                m.map(c, vert)?;
                m.map(c + 1, nxt)?;
                m.map(c + 2, prv)?;
            }
            S => {
                let b = stack.pop().ok_or_else(empty)?;
                if let Some(corners) = split_at.remove(&(id as i64)) {
                    stack.extend(corners);
                }
                let a = *stack.last().ok_or_else(empty)?;
                m.set_opposite(a, c + 2)?;
                m.set_opposite(b, c + 1)?;
                *stack.last_mut().ok_or_else(empty)? = c;
                vert = m.vert(prev(a))?;
                nxt = m.vert(next(a))?;
                prv = m.vert(prev(b))?;
                m.map(c, vert)?;
                m.map(c + 1, nxt)?;
                m.map(c + 2, prv)?;
                // The two fans' first vertices are one: `vertex_n` goes.
                let gone = m.vert(next(b))?;
                if gone != vert {
                    if valence {
                        let add = valences[gone as usize];
                        bump(&mut valences, vert, add);
                    }
                    for k in std::mem::take(&mut m.corners_of[gone as usize]) {
                        if m.ctv[k as usize] == gone {
                            m.ctv[k as usize] = vert;
                            m.corners_of[vert as usize].push(k);
                        }
                    }
                    m.vertex_corner[gone as usize] = -1;
                }
                bump(&mut valences, nxt, 1);
                bump(&mut valences, prv, 1);
            }
            R | L => {
                let a = *stack.last().ok_or_else(empty)?;
                m.set_opposite(if symbol == R { c + 2 } else { c + 1 }, a)?;
                *stack.last_mut().ok_or_else(empty)? = c;
                check_split = true;
                last_vert += 1;
                if symbol == R {
                    vert = m.vert(prev(a))?;
                    nxt = m.vert(next(a))?;
                    prv = last_vert;
                    bump(&mut valences, vert, 1);
                    bump(&mut valences, nxt, 1);
                    bump(&mut valences, prv, 2);
                } else {
                    vert = m.vert(next(a))?;
                    nxt = last_vert;
                    prv = m.vert(prev(a))?;
                    bump(&mut valences, vert, 1);
                    bump(&mut valences, nxt, 2);
                    bump(&mut valences, prv, 1);
                }
                m.map(c + 2, prv)?;
                m.map(c, vert)?;
                m.map(c + 1, nxt)?;
            }
            E => {
                stack.push(c);
                check_split = true;
                vert = last_vert + 1;
                nxt = vert + 1;
                prv = nxt + 1;
                last_vert = prv;
                for v in [vert, nxt, prv] {
                    bump(&mut valences, v, 2);
                }
                m.map(c, vert)?;
                m.map(c + 1, nxt)?;
                m.map(c + 2, prv)?;
            }
            _ => return bad("an EdgeBreaker symbol"),
        }
        if valence {
            let v = valences.get(nxt as usize).copied().unwrap_or(0);
            context = v.clamp(2, 7) - 2;
        }
        if check_split {
            let encoder_id = (n_symbols - id - 1) as i64;
            while source_ids.last() == Some(&encoder_id) {
                source_ids.pop();
                let (Some(edge), Some(split)) = (edge_bits.pop(), split_ids.pop()) else {
                    return bad("a topology split without its edge");
                };
                let top = *stack.last().ok_or_else(empty)?;
                let corner = if edge == 1 { next(top) } else { prev(top) };
                split_at
                    .entry(n_symbols as i64 - split - 1)
                    .or_default()
                    .push(corner);
            }
        }
    }
    m.faces = n_symbols;
    // ProcessInteriorEdges: the faces that close a start.
    let mut ans = Ans::new(start_faces, L_RANS_BASE)?;
    while let Some(a) = stack.pop() {
        if !ans.bit(start_prob) {
            continue;
        }
        if m.faces >= faces {
            return bad("more faces than the header counts");
        }
        let mut b = prev(a);
        let mut cc = next(a);
        let mut guard = 0;
        while m.opposite(b) >= 0 || m.opposite(cc) >= 0 {
            if m.opposite(b) >= 0 {
                b = prev(m.opposite(b));
            }
            if m.opposite(cc) >= 0 {
                cc = next(m.opposite(cc));
            }
            guard += 1;
            if guard > faces * 3 {
                return bad("a fan that does not end");
            }
        }
        let c = (m.faces * 3) as i32;
        m.set_opposite(c, a)?;
        m.set_opposite(c + 1, b)?;
        m.set_opposite(c + 2, cc)?;
        let (na, nb, nc) = (m.vert(next(a))?, m.vert(next(b))?, m.vert(next(cc))?);
        m.map(c, nb)?;
        m.map(c + 1, nc)?;
        m.map(c + 2, na)?;
        for v in [na, nb, nc] {
            m.hole[v as usize] = false;
        }
        m.faces += 1;
    }
    if m.faces != faces {
        return bad("fewer faces than the header counts");
    }
    Ok((m, seams))
}

/// DecodeAttributeSeams and RecomputeVerticesInternal for one attribute.
fn seams_of(m: &Mesh, prob: u8, data: &[u8]) -> Res<Seams> {
    let corners = m.faces * 3;
    let mut s = Seams {
        edge: vec![false; corners],
        vertex: vec![false; m.vertex_corner.len()],
        ctv: vec![-1; corners],
        left_most: Vec::new(),
    };
    let mut ans = Ans::new(data, L_RANS_BASE)?;
    for c in 0..corners as i32 {
        let o = m.opposite(c);
        let seam = if o < 0 {
            true
        } else if o >= c {
            let on = ans.bit(prob);
            if on {
                s.edge[o as usize] = true;
            }
            on
        } else {
            s.edge[c as usize]
        };
        if seam {
            s.edge[c as usize] = true;
            s.vertex[m.vert(next(c))? as usize] = true;
            s.vertex[m.vert(prev(c))? as usize] = true;
        }
    }
    let view = View { m, s: None };
    let limit = corners + 1;
    for v in 0..m.vertex_corner.len() {
        let c = m.vertex_corner[v];
        if c < 0 {
            continue;
        }
        let mut id = s.left_most.len() as i32;
        let mut first = c;
        if s.vertex[v] {
            // To the corner left of which a seam (or the boundary) lies.
            let cut = |k: i32| {
                let n = next(k);
                if n < 0 || s.edge[n as usize] {
                    -1
                } else {
                    next(m.opposite(n))
                }
            };
            let mut act = cut(first);
            let mut guard = 0;
            while act >= 0 {
                first = act;
                act = cut(act);
                guard += 1;
                if guard > limit {
                    return bad("a fan that does not end");
                }
            }
        }
        s.ctv[first as usize] = id;
        s.left_most.push(first);
        let mut act = view.swing_right(first);
        let mut guard = 0;
        while act >= 0 && act != first {
            if s.edge[next(act) as usize] {
                id = s.left_most.len() as i32;
                s.left_most.push(act);
            }
            s.ctv[act as usize] = id;
            act = view.swing_right(act);
            guard += 1;
            if guard > limit {
                return bad("a fan that does not end");
            }
        }
    }
    Ok(s)
}

/// The order a decoder's values come in: the corner each was first met
/// at, and each vertex's place in that order.
struct Sequence {
    entry_corner: Vec<i32>,
    vertex_entry: Vec<i32>,
}

/// GenerateSequence for EdgeBreaker: depth first, or by prediction degree.
fn sequence(view: &View, by_degree: bool) -> Res<Sequence> {
    let faces = view.m.faces;
    let nv = view.vertices();
    let mut seq = Sequence {
        entry_corner: Vec::new(),
        vertex_entry: vec![-1; nv],
    };
    let mut face_seen = vec![false; faces];
    let mut seen = vec![false; nv];
    let mut degree = vec![0u32; if by_degree { nv } else { 0 }];
    let visit = |seq: &mut Sequence, seen: &mut Vec<bool>, c: i32| -> Res<bool> {
        let v = view.vert(c)? as usize;
        if seen[v] {
            return Ok(false);
        }
        seen[v] = true;
        seq.vertex_entry[v] = seq.entry_corner.len() as i32;
        seq.entry_corner.push(c);
        Ok(true)
    };
    let face_done = |face_seen: &Vec<bool>, c: i32| c < 0 || face_seen[c as usize / 3];
    let right = |c: i32| view.opposite(next(c));
    let left = |c: i32| view.opposite(prev(c));
    for f in 0..faces {
        let start = (3 * f) as i32;
        if by_degree {
            let mut stacks: [Vec<i32>; 3] = [vec![start], Vec::new(), Vec::new()];
            let mut best = 0usize;
            visit(&mut seq, &mut seen, next(start))?;
            visit(&mut seq, &mut seen, prev(start))?;
            visit(&mut seq, &mut seen, start)?;
            loop {
                let mut popped = None;
                for (i, stack) in stacks.iter_mut().enumerate().skip(best) {
                    if let Some(c) = stack.pop() {
                        best = i;
                        popped = Some(c);
                        break;
                    }
                }
                let Some(mut c) = popped else { break };
                if face_done(&face_seen, c) {
                    continue;
                }
                loop {
                    face_seen[c as usize / 3] = true;
                    visit(&mut seq, &mut seen, c)?;
                    let (rc, lc) = (right(c), left(c));
                    let (r_done, l_done) = (face_done(&face_seen, rc), face_done(&face_seen, lc));
                    let mut priority = |corner: i32| -> Res<usize> {
                        let tip = view.vert(corner)? as usize;
                        if seen[tip] {
                            return Ok(0);
                        }
                        degree[tip] += 1;
                        Ok(if degree[tip] > 1 { 1 } else { 2 })
                    };
                    if !l_done {
                        let p = priority(lc)?;
                        if r_done && p <= best {
                            c = lc;
                            continue;
                        }
                        stacks[p].push(lc);
                        best = best.min(p);
                    }
                    if !r_done {
                        let p = priority(rc)?;
                        if p <= best {
                            c = rc;
                            continue;
                        }
                        stacks[p].push(rc);
                        best = best.min(p);
                    }
                    break;
                }
            }
            continue;
        }
        if face_seen[f] {
            continue;
        }
        let mut stack = vec![start];
        visit(&mut seq, &mut seen, next(start))?;
        visit(&mut seq, &mut seen, prev(start))?;
        while let Some(&top) = stack.last() {
            let mut c = top;
            if face_done(&face_seen, c) {
                stack.pop();
                continue;
            }
            loop {
                face_seen[c as usize / 3] = true;
                let v = view.vert(c)?;
                if visit(&mut seq, &mut seen, c)? && !view.on_boundary(v)? {
                    c = right(c);
                    if c < 0 {
                        return bad("an inner vertex on an open edge");
                    }
                    continue;
                }
                let (rc, lc) = (right(c), left(c));
                match (face_done(&face_seen, rc), face_done(&face_seen, lc)) {
                    (true, true) => {
                        stack.pop();
                        break;
                    }
                    (true, false) => c = lc,
                    (false, true) => c = rc,
                    (false, false) => {
                        if let Some(top) = stack.last_mut() {
                            *top = lc;
                        }
                        stack.push(rc);
                        break;
                    }
                }
            }
        }
    }
    Ok(seq)
}

/// What a prediction needs of the mesh: for each value in order, its
/// corner; each vertex's place; the decoder's view of the corners.
struct Ctx<'a> {
    view: Option<View<'a>>,
    seq: &'a Sequence,
    /// The positions as whole numbers, by their own order, and for each
    /// of this decoder's values the position's place.
    positions: Option<(&'a [i32], Vec<i32>)>,
}

/// The prediction transform: wrap (within `min..=max`), or none.
#[derive(Clone, Copy)]
struct Wrap {
    min: i32,
    max: i32,
    on: bool,
}

impl Wrap {
    fn original(&self, predicted: i32, correction: i32) -> i32 {
        if !self.on {
            return predicted.wrapping_add(correction);
        }
        let mut v = predicted
            .clamp(self.min, self.max.max(self.min))
            .wrapping_add(correction);
        let dif = 1i32.wrapping_add(self.max).wrapping_sub(self.min);
        if v > self.max {
            v = v.wrapping_sub(dif);
        } else if v < self.min {
            v = v.wrapping_add(dif);
        }
        v
    }
}

/// The parallelogram across the edge opposite corner `ci`, if its three
/// values are already there.
fn parallelogram(
    ctx: &Ctx,
    entry: usize,
    ci: i32,
    data: &[i32],
    n: usize,
    out: &mut [i32],
) -> Res<bool> {
    let Some(view) = &ctx.view else {
        return Ok(false);
    };
    let oci = view.opposite(ci);
    if oci < 0 {
        return Ok(false);
    }
    let at = |c: i32| -> Res<usize> {
        let e = ctx.seq.vertex_entry[view.vert(c)? as usize];
        Ok(if e < 0 { usize::MAX } else { e as usize })
    };
    let (o, a, b) = (at(oci)?, at(next(oci))?, at(prev(oci))?);
    if o < entry && a < entry && b < entry {
        for c in 0..n {
            out[c] = data[a * n + c]
                .wrapping_add(data[b * n + c])
                .wrapping_sub(data[o * n + c]);
        }
        return Ok(true);
    }
    Ok(false)
}

/// IntSqrt.
fn int_sqrt(number: u64) -> u64 {
    if number == 0 {
        return 0;
    }
    let mut act = number;
    let mut root = 1u64;
    while act >= 2 {
        root *= 2;
        act /= 4;
    }
    loop {
        root = (root + number / root) / 2;
        if root.saturating_mul(root) <= number {
            return root;
        }
    }
}

/// One attribute's whole numbers, undone of their prediction.
#[allow(clippy::too_many_arguments)]
fn integers(r: &mut Reader, ctx: &Ctx, entries: usize, n: usize, normals: bool) -> Res<Vec<i32>> {
    let scheme = r.u8()? as i8;
    let mut transform = 0i8;
    if scheme != -2 {
        transform = r.u8()? as i8;
    }
    let values = entries * n;
    let compressed = r.u8()?;
    let mut raw: Vec<u32> = if compressed > 0 {
        symbols(r, values, n)?
    } else {
        // Stored outright, in a number of bytes each.
        let bytes = usize::from(r.u8()?);
        if bytes == 0 || bytes > 4 {
            return bad("whole numbers of no bytes or more than four");
        }
        let b = r.bytes(
            values
                .checked_mul(bytes)
                .ok_or("the Draco data is not valid: a count")?,
        )?;
        b.chunks_exact(bytes)
            .map(|c| c.iter().rev().fold(0u32, |v, x| (v << 8) | u32::from(*x)))
            .collect()
    };
    if raw.len() < values {
        return short();
    }
    raw.truncate(values);
    // Corrections are signed but under the normals' own transform.
    let signed: Vec<i32> = if transform == 3 {
        raw.iter().map(|&v| v as i32).collect()
    } else {
        raw.iter()
            .map(|&v| {
                let half = (v >> 1) as i32;
                if v & 1 == 0 { half } else { -half - 1 }
            })
            .collect()
    };
    if scheme == -2 {
        return Ok(signed);
    }
    // The prediction's own data, then its transform's.
    let mut creases: [Vec<bool>; 4] = Default::default();
    let mut orientations: Vec<bool> = Vec::new();
    let mut wrap = Wrap {
        min: 0,
        max: 0,
        on: false,
    };
    let rans_bits = |r: &mut Reader, count: usize| -> Res<Vec<bool>> {
        let prob = r.u8()?;
        let size = r.count(1)?;
        let mut ans = Ans::new(r.bytes(size)?, L_RANS_BASE)?;
        Ok((0..count).map(|_| ans.bit(prob)).collect())
    };
    let mut read_transform = |r: &mut Reader| -> Res<()> {
        match transform {
            1 => {
                wrap = Wrap {
                    min: r.i32()?,
                    max: r.i32()?,
                    on: true,
                };
            }
            3 => {
                r.i32()?;
                r.i32()?;
            }
            _ => {}
        }
        Ok(())
    };
    match scheme {
        4 => {
            for flags in &mut creases {
                let count = r.var()? as usize;
                if count > entries.saturating_mul(4) {
                    return bad("more crease flags than values");
                }
                if count > 0 {
                    *flags = rans_bits(r, count)?;
                }
            }
            read_transform(r)?;
        }
        5 => {
            let count = r.u32()? as usize;
            if count > entries {
                return bad("more orientations than values");
            }
            let bits = rans_bits(r, count)?;
            let mut last = true;
            for b in bits {
                if !b {
                    last = !last;
                }
                orientations.push(last);
            }
            read_transform(r)?;
        }
        6 => {
            read_transform(r)?;
            rans_bits(r, entries)?;
        }
        0 | 1 => read_transform(r)?,
        _ => return bad("a prediction scheme the specification does not have"),
    }
    if normals || values == 0 {
        // Parsed; not computed.
        return Ok(Vec::new());
    }
    let mesh_scheme = scheme != 0;
    if mesh_scheme && ctx.view.is_none() {
        return bad("a mesh prediction without EdgeBreaker connectivity");
    }
    let mut out = signed.clone();
    let mut pred = vec![0i32; n];
    let put = |out: &mut Vec<i32>, p: usize, pred: &[i32]| {
        for c in 0..n {
            out[p * n + c] = wrap.original(pred[c], signed[p * n + c]);
        }
    };
    let before = |out: &Vec<i32>, p: usize| -> Vec<i32> { out[(p - 1) * n..p * n].to_vec() };
    match scheme {
        0 => {
            for p in 0..entries {
                let from = if p == 0 { vec![0; n] } else { before(&out, p) };
                put(&mut out, p, &from);
            }
        }
        1 => {
            put(&mut out, 0, &pred);
            for p in 1..entries {
                let corner = ctx.seq.entry_corner[p];
                if parallelogram(ctx, p, corner, &out, n, &mut pred)? {
                    let q = pred.clone();
                    put(&mut out, p, &q);
                } else {
                    let from = before(&out, p);
                    put(&mut out, p, &from);
                }
            }
        }
        4 => {
            let view = ctx.view.as_ref().ok_or("no view")?;
            put(&mut out, 0, &pred);
            let mut preds = vec![vec![0i32; n]; 4];
            let mut used_flags = [0usize; 4];
            for p in 1..entries {
                let start = ctx.seq.entry_corner[p];
                let mut corner = start;
                let mut count = 0usize;
                let mut first_pass = true;
                let mut guard = 0;
                while corner >= 0 {
                    if parallelogram(ctx, p, corner, &out, n, &mut preds[count])? {
                        count += 1;
                        if count == 4 {
                            break;
                        }
                    }
                    corner = if first_pass {
                        view.swing_left(corner)
                    } else {
                        view.swing_right(corner)
                    };
                    if corner == start {
                        break;
                    }
                    if corner < 0 && first_pass {
                        first_pass = false;
                        corner = view.swing_right(start);
                    }
                    guard += 1;
                    if guard > view.m.faces * 3 {
                        return bad("a fan that does not end");
                    }
                }
                let mut sum = vec![0i32; n];
                let mut used = 0i32;
                for one in preds.iter().take(count) {
                    let context = count - 1;
                    let Some(&crease) = creases[context].get(used_flags[context]) else {
                        return bad("a parallelogram without its crease flag");
                    };
                    used_flags[context] += 1;
                    if !crease {
                        used += 1;
                        for c in 0..n {
                            sum[c] = sum[c].wrapping_add(one[c]);
                        }
                    }
                }
                if used == 0 {
                    let from = before(&out, p);
                    put(&mut out, p, &from);
                } else {
                    for v in &mut sum {
                        *v /= used;
                    }
                    put(&mut out, p, &sum);
                }
            }
        }
        5 => {
            let view = ctx.view.as_ref().ok_or("no view")?;
            let Some((positions, place)) = &ctx.positions else {
                return bad("texture coordinates predicted without positions");
            };
            if n != 2 {
                return bad("texture coordinates that are not pairs");
            }
            let pos = |entry: usize| -> Res<[i64; 3]> {
                let k = *place
                    .get(entry)
                    .ok_or("the Draco data is not valid: a position")?;
                let Some(p) = positions.get(k.max(0) as usize * 3..k.max(0) as usize * 3 + 3)
                else {
                    return bad("a position outside the mesh");
                };
                Ok([i64::from(p[0]), i64::from(p[1]), i64::from(p[2])])
            };
            for p in 0..entries {
                let corner = ctx.seq.entry_corner[p];
                let entry_of = |c: i32| -> Res<usize> {
                    let e = ctx.seq.vertex_entry[view.vert(c)? as usize];
                    Ok(if e < 0 { usize::MAX } else { e as usize })
                };
                let (ne, pe) = (entry_of(next(corner))?, entry_of(prev(corner))?);
                let uv =
                    |out: &Vec<i32>, e: usize| [i64::from(out[e * 2]), i64::from(out[e * 2 + 1])];
                let mut predicted: Option<[i32; 2]> = None;
                if pe < p && ne < p {
                    let (n_uv, p_uv) = (uv(&out, ne), uv(&out, pe));
                    if p_uv == n_uv {
                        predicted = Some([p_uv[0] as i32, p_uv[1] as i32]);
                    } else {
                        let (tip, np, pp) = (pos(p)?, pos(ne)?, pos(pe)?);
                        let sub =
                            |a: [i64; 3], b: [i64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
                        let dot = |a: [i64; 3], b: [i64; 3]| {
                            a[0].wrapping_mul(b[0])
                                .wrapping_add(a[1].wrapping_mul(b[1]))
                                .wrapping_add(a[2].wrapping_mul(b[2]))
                        };
                        let pn = sub(pp, np);
                        let pn2 = dot(pn, pn);
                        if pn2 != 0 {
                            let cn = sub(tip, np);
                            let cn_dot_pn = dot(cn, pn);
                            let pn_uv = [p_uv[0] - n_uv[0], p_uv[1] - n_uv[1]];
                            let x_uv = [
                                pn_uv[0]
                                    .wrapping_mul(cn_dot_pn)
                                    .wrapping_add(n_uv[0].wrapping_mul(pn2)),
                                pn_uv[1]
                                    .wrapping_mul(cn_dot_pn)
                                    .wrapping_add(n_uv[1].wrapping_mul(pn2)),
                            ];
                            let x_pos = [
                                np[0] + pn[0].wrapping_mul(cn_dot_pn) / pn2,
                                np[1] + pn[1].wrapping_mul(cn_dot_pn) / pn2,
                                np[2] + pn[2].wrapping_mul(cn_dot_pn) / pn2,
                            ];
                            let cx = sub(tip, x_pos);
                            let cx2 = dot(cx, cx);
                            let norm = int_sqrt((cx2 as u64).wrapping_mul(pn2 as u64)) as i64;
                            let cx_uv =
                                [pn_uv[1].wrapping_mul(norm), (-pn_uv[0]).wrapping_mul(norm)];
                            let Some(orientation) = orientations.pop() else {
                                return bad("a texture coordinate without its side");
                            };
                            let t = if orientation {
                                [
                                    x_uv[0].wrapping_add(cx_uv[0]),
                                    x_uv[1].wrapping_add(cx_uv[1]),
                                ]
                            } else {
                                [
                                    x_uv[0].wrapping_sub(cx_uv[0]),
                                    x_uv[1].wrapping_sub(cx_uv[1]),
                                ]
                            };
                            predicted = Some([(t[0] / pn2) as i32, (t[1] / pn2) as i32]);
                        }
                    }
                }
                let predicted = match predicted {
                    Some(v) => v,
                    None => {
                        // The value at the neighbour already there, or
                        // the one before, or nothing.
                        let from = if ne < p {
                            Some(ne)
                        } else if pe < p {
                            Some(pe)
                        } else if p > 0 {
                            Some(p - 1)
                        } else {
                            None
                        };
                        from.map_or([0, 0], |e| [out[e * 2], out[e * 2 + 1]])
                    }
                };
                put(&mut out, p, &predicted);
            }
        }
        _ => return bad("a prediction scheme the specification does not have"),
    }
    Ok(out)
}

/// The bytes of a Draco data type; negative for signed, 0 for floats.
fn integer_bytes(data_type: u8) -> Res<i8> {
    Ok(match data_type {
        1 => -1,
        2 | 11 => 1,
        3 => -2,
        4 => 2,
        5 => -4,
        6 => 4,
        9 => 0,
        _ => return bad("an attribute of 64-bit numbers"),
    })
}

/// A Draco triangle mesh: its faces over points, and each attribute's
/// values a point.
pub(crate) fn decode(data: &[u8]) -> Res<Decoded> {
    let mut r = Reader { d: data, at: 0 };
    if r.bytes(5)? != b"DRACO" {
        return bad("it does not start with DRACO");
    }
    let (major, minor) = (r.u8()?, r.u8()?);
    if (major, minor) != (2, 2) {
        return Err(format!(
            "Draco bitstream version {major}.{minor} (only 2.2, which glTF requires)"
        ));
    }
    let (kind, method) = (r.u8()?, r.u8()?);
    if kind != 1 {
        return Err("a Draco point cloud, not a triangle mesh".into());
    }
    let flags = r.u16()?;
    if flags & 0x8000 != 0 {
        metadata(&mut r)?;
    }
    // Connectivity.
    let mut mesh = None;
    let mut seam_data = Vec::new();
    let mut faces: Vec<[u32; 3]> = Vec::new();
    let mut points = 0usize;
    match method {
        0 => {
            let n_faces = r.count(32)?;
            points = r.count(96)?;
            match r.u8()? {
                0 => {
                    let s = symbols(&mut r, n_faces * 3, 1)?;
                    let mut last = 0i32;
                    for f in s.as_chunks::<3>().0 {
                        let mut t = [0u32; 3];
                        for (k, &v) in f.iter().enumerate() {
                            let diff = (v >> 1) as i32;
                            last = last.wrapping_add(if v & 1 != 0 { -diff } else { diff });
                            t[k] = last as u32;
                        }
                        faces.push(t);
                    }
                }
                1 => {
                    for _ in 0..n_faces {
                        let mut t = [0u32; 3];
                        for v in &mut t {
                            *v = if points < 256 {
                                u32::from(r.u8()?)
                            } else if points < 1 << 16 {
                                u32::from(r.u16()?)
                            } else if points < 1 << 21 {
                                r.var()? as u32
                            } else {
                                r.u32()?
                            };
                        }
                        faces.push(t);
                    }
                }
                _ => return bad("a way of storing indices"),
            }
        }
        1 => {
            let (m, s) = edgebreaker(&mut r)?;
            mesh = Some(m);
            seam_data = s;
        }
        _ => return bad("a mesh encoding that is neither sequential nor EdgeBreaker"),
    }
    // The attribute decoders' headers.
    let decoders = usize::from(r.u8()?);
    let mut heads = Vec::with_capacity(decoders);
    for _ in 0..decoders {
        heads.push(if mesh.is_some() {
            (r.u8()? as i8, r.u8()?, r.u8()?)
        } else {
            (-1, 0, 0)
        });
    }
    struct Att {
        kind: u8,
        data_type: u8,
        components: usize,
        id: u32,
        decoder: u8,
    }
    let mut attributes: Vec<Vec<Att>> = Vec::with_capacity(decoders);
    for _ in 0..decoders {
        let n = r.count(1)?;
        let mut list = Vec::with_capacity(n);
        for _ in 0..n {
            let (kind, data_type, components) = (r.u8()?, r.u8()?, usize::from(r.u8()?));
            r.u8()?;
            let id = r.var()? as u32;
            if components == 0 {
                return bad("an attribute of no components");
            }
            list.push(Att {
                kind,
                data_type,
                components,
                id,
                decoder: 0,
            });
        }
        for a in &mut list {
            a.decoder = r.u8()?;
        }
        attributes.push(list);
    }
    // EdgeBreaker: each attribute's seams, the points, each decoder's
    // order of values.
    let mut seams: Vec<Seams> = Vec::new();
    let mut corner_point: Vec<u32> = Vec::new();
    let mut sequences: Vec<Sequence> = Vec::new();
    let view_of = |m: &'_ Mesh, seams: &'_ [Seams], head: (i8, u8, u8)| -> Res<Option<usize>> {
        let _ = m;
        if head.1 == 0 || head.0 < 0 {
            return Ok(None);
        }
        if head.0 as usize >= seams.len() {
            return bad("an attribute without its connectivity");
        }
        Ok(Some(head.0 as usize))
    };
    if let Some(m) = &mut mesh {
        // A boundary vertex's corner: its left-most.
        for v in 0..m.vertex_corner.len() {
            let first = m.vertex_corner[v];
            if !m.hole[v] || first < 0 {
                continue;
            }
            let (mut c, mut act) = (first, m.swing_left(first));
            let mut guard = 0;
            while act >= 0 && act != first {
                c = act;
                act = m.swing_left(act);
                guard += 1;
                if guard > m.faces * 3 {
                    return bad("a fan that does not end");
                }
            }
            if act != first {
                m.vertex_corner[v] = c;
            }
        }
        for (prob, data) in &seam_data {
            seams.push(seams_of(m, *prob, data)?);
        }
        // Attribute_AssignPointsToCorners.
        let corners = m.faces * 3;
        corner_point = vec![u32::MAX; corners];
        let differs = |a: i32, b: i32| seams.iter().any(|s| s.ctv[a as usize] != s.ctv[b as usize]);
        for v in 0..m.vertex_corner.len() {
            let c = m.vertex_corner[v];
            if c < 0 {
                continue;
            }
            let mut first = c;
            if !m.hole[v] {
                'attrs: for s in &seams {
                    if !s.vertex[v] {
                        continue;
                    }
                    let id = s.ctv[c as usize];
                    let mut act = m.swing_right(c);
                    let mut guard = 0;
                    while act >= 0 && act != c {
                        if s.ctv[act as usize] != id {
                            first = act;
                            break 'attrs;
                        }
                        act = m.swing_right(act);
                        guard += 1;
                        if guard > corners {
                            return bad("a fan that does not end");
                        }
                    }
                }
            }
            corner_point[first as usize] = points as u32;
            points += 1;
            let mut before = first;
            let mut act = m.swing_right(first);
            let mut guard = 0;
            while act >= 0 && act != first {
                if differs(act, before) {
                    corner_point[act as usize] = points as u32;
                    points += 1;
                } else {
                    corner_point[act as usize] = corner_point[before as usize];
                }
                before = act;
                act = m.swing_right(act);
                guard += 1;
                if guard > corners {
                    return bad("a fan that does not end");
                }
            }
        }
        if corner_point.contains(&u32::MAX) {
            return bad("a corner no vertex reaches");
        }
        faces = corner_point.as_chunks::<3>().0.to_vec();
        for head in &heads {
            let view = View {
                m,
                s: view_of(m, &seams, *head)?.map(|k| &seams[k]),
            };
            sequences.push(sequence(&view, head.2 == 1)?);
        }
    } else {
        for _ in 0..decoders {
            sequences.push(Sequence {
                entry_corner: (0..points as i32).collect(),
                vertex_entry: Vec::new(),
            });
        }
    }
    // Each decoder's values a point: `indices_map_`.
    let mut point_entry: Vec<Vec<i32>> = Vec::with_capacity(decoders);
    for (i, seq) in sequences.iter().enumerate() {
        let Some(m) = &mesh else {
            point_entry.push((0..points as i32).collect());
            continue;
        };
        let view = View {
            m,
            s: view_of(m, &seams, heads[i])?.map(|k| &seams[k]),
        };
        let mut map = vec![-1i32; points];
        for c in 0..(m.faces * 3) as i32 {
            map[corner_point[c as usize] as usize] = seq.vertex_entry[view.vert(c)? as usize];
        }
        point_entry.push(map);
    }
    // The values.
    let mut out = Decoded {
        faces,
        points,
        attributes: Vec::new(),
    };
    let mut position_ints: Option<(Vec<i32>, usize)> = None;
    for (i, list) in attributes.iter().enumerate() {
        let seq = &sequences[i];
        let entries = seq.entry_corner.len();
        let view = match &mesh {
            Some(m) => Some(View {
                m,
                s: view_of(m, &seams, heads[i])?.map(|k| &seams[k]),
            }),
            None => None,
        };
        // For texture coordinates: each of this decoder's values' position.
        let positions = match (&position_ints, &mesh) {
            (Some((ints, dec)), Some(_)) => {
                let place = seq
                    .entry_corner
                    .iter()
                    .map(|&c| point_entry[*dec][corner_point[c as usize] as usize])
                    .collect();
                Some((ints.as_slice(), place))
            }
            _ => None,
        };
        let ctx = Ctx {
            view,
            seq,
            positions,
        };
        // DecodePortableAttributes.
        let mut portable: Vec<Vec<i32>> = Vec::with_capacity(list.len());
        let mut floats: Vec<Vec<f64>> = vec![Vec::new(); list.len()];
        for (j, a) in list.iter().enumerate() {
            match a.decoder {
                0 => {
                    // Stored outright, in the attribute's own type.
                    let size = integer_bytes(a.data_type)?;
                    let bytes = if size == 0 {
                        4
                    } else {
                        size.unsigned_abs() as usize
                    };
                    let total = entries
                        .checked_mul(bytes * a.components)
                        .ok_or("the Draco data is not valid: a count")?;
                    let b = r.bytes(total)?;
                    floats[j] = b
                        .chunks_exact(bytes)
                        .map(|c| {
                            let raw = c.iter().rev().fold(0u32, |v, x| (v << 8) | u32::from(*x));
                            match size {
                                0 => f64::from(f32::from_bits(raw)),
                                -1 => f64::from(raw as u8 as i8),
                                -2 => f64::from(raw as u16 as i16),
                                -4 => f64::from(raw as i32),
                                _ => f64::from(raw),
                            }
                        })
                        .collect();
                    portable.push(Vec::new());
                }
                1..=3 => {
                    let normals = a.decoder == 3;
                    let n = if normals { 2 } else { a.components };
                    portable.push(integers(&mut r, &ctx, entries, n, normals)?);
                }
                _ => return bad("an attribute encoder the specification does not have"),
            }
        }
        // DecodeDataNeededByPortableTransforms, and the values.
        for (j, a) in list.iter().enumerate() {
            let size = integer_bytes(a.data_type)?;
            let mut integer = size;
            let per_entry: Vec<f64> = match a.decoder {
                0 => {
                    if size == 0 {
                        integer = 0;
                    }
                    std::mem::take(&mut floats[j])
                }
                1 => portable[j].iter().map(|&v| f64::from(v)).collect(),
                2 => {
                    let mut min = Vec::with_capacity(a.components);
                    for _ in 0..a.components {
                        min.push(r.f32()?);
                    }
                    let range = r.f32()?;
                    let bits = u32::from(r.u8()?);
                    if bits == 0 || bits > 30 {
                        return bad("a quantization of no bits or more than 30");
                    }
                    let factor = 1.0f32 / ((1u32 << bits) - 1) as f32;
                    integer = 0;
                    if a.kind == 0 && position_ints.is_none() {
                        position_ints = Some((portable[j].clone(), i));
                    }
                    portable[j]
                        .iter()
                        .enumerate()
                        .map(|(k, &v)| {
                            // DequantizeFloat, in f32 as it is specified.
                            let norm = (v.unsigned_abs() as f32) * factor;
                            let norm = if v < 0 { -norm } else { norm };
                            f64::from(norm * range + min[k % a.components])
                        })
                        .collect()
                }
                _ => {
                    r.u8()?;
                    Vec::new()
                }
            };
            if a.kind == 0 && a.decoder == 1 && position_ints.is_none() {
                position_ints = Some((portable[j].clone(), i));
            }
            // By point.
            let n = a.components;
            let mut values = Vec::new();
            if !per_entry.is_empty() {
                if per_entry.len() != entries * n {
                    return bad("an attribute with fewer values than points");
                }
                values = vec![0.0; points * n];
                for (p, &e) in point_entry[i].iter().enumerate() {
                    if e < 0 || e as usize >= entries {
                        return bad("a point without a value");
                    }
                    let e = e as usize;
                    values[p * n..p * n + n].copy_from_slice(&per_entry[e * n..e * n + n]);
                }
            }
            out.attributes.push(Attribute {
                id: a.id,
                components: n,
                values,
                integer,
            });
        }
    }
    Ok(out)
}

/// The metadata, read past.
fn metadata(r: &mut Reader) -> Res<()> {
    fn element(r: &mut Reader, depth: usize) -> Res<()> {
        if depth > 64 {
            return bad("metadata nested too deep");
        }
        let entries = r.count(1)?;
        for _ in 0..entries {
            let n = usize::from(r.u8()?);
            r.bytes(n)?;
            let n = usize::from(r.u8()?);
            r.bytes(n)?;
        }
        let subs = r.count(1)?;
        for _ in 0..subs {
            let n = usize::from(r.u8()?);
            r.bytes(n)?;
            element(r, depth + 1)?;
        }
        Ok(())
    }
    let n = r.count(1)?;
    for _ in 0..n {
        r.var()?;
        element(r, 0)?;
    }
    element(r, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Wavefront file's triangles, each three positions.
    fn obj_triangles(text: &str) -> Vec<[[f64; 3]; 3]> {
        let mut points = Vec::new();
        let mut out = Vec::new();
        for line in text.lines() {
            let mut words = line.split_whitespace();
            match words.next() {
                Some("v") => {
                    let p: Vec<f64> = words.map(|w| w.parse().unwrap()).collect();
                    points.push([p[0], p[1], p[2]]);
                }
                Some("f") => {
                    let ids: Vec<usize> = words
                        .map(|w| w.split('/').next().unwrap().parse::<usize>().unwrap() - 1)
                        .collect();
                    for k in 1..ids.len() - 1 {
                        out.push([points[ids[0]], points[ids[k]], points[ids[k + 1]]]);
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Draco's own test files (`third_party/draco/testdata`), at
    /// compression levels that between them use sequential and
    /// EdgeBreaker connectivity, both traversals, and the difference,
    /// parallelogram, constrained multi-parallelogram and texture
    /// coordinate predictions: each decodes to the triangles of the
    /// Wavefront file it was made from, to its quantization.
    #[test]
    fn draco_s_test_files_decode_to_their_sources() {
        macro_rules! file {
            ($name:literal) => {
                include_bytes!(concat!("../../../third_party/draco/testdata/", $name)).as_slice()
            };
        }
        let cases: [(&[u8], &[u8], usize); 6] = [
            (
                file!("test_nm.obj.edgebreaker.cl10.2.2.drc"),
                file!("test_nm.obj"),
                0,
            ),
            (
                file!("test_nm.obj.edgebreaker.cl4.2.2.drc"),
                file!("test_nm.obj"),
                0,
            ),
            (
                file!("test_nm.obj.sequential.cl3.2.2.drc"),
                file!("test_nm.obj"),
                0,
            ),
            (
                file!("cube_att.obj.edgebreaker.cl10.2.2.drc"),
                file!("cube_att.obj"),
                24,
            ),
            (
                file!("cube_att.obj.edgebreaker.cl4.2.2.drc"),
                file!("cube_att.obj"),
                24,
            ),
            (
                file!("cube_att.obj.sequential.cl3.2.2.drc"),
                file!("cube_att.obj"),
                24,
            ),
        ];
        for (k, (packed, source, uvs)) in cases.into_iter().enumerate() {
            let d = decode(packed).unwrap_or_else(|e| panic!("case {k}: {e}"));
            let mut want = obj_triangles(std::str::from_utf8(source).unwrap());
            assert_eq!(d.faces.len(), want.len(), "case {k}");
            let position = d
                .attributes
                .iter()
                .find(|a| a.components == 3 && !a.values.is_empty())
                .unwrap();
            assert_eq!(position.values.len(), d.points * 3);
            for f in &d.faces {
                let t = f.map(|v| {
                    let p = &position.values[v as usize * 3..v as usize * 3 + 3];
                    [p[0], p[1], p[2]]
                });
                // The same triangle, from any of its corners.
                let found = want.iter().position(|w| {
                    (0..3).any(|turn| {
                        (0..3)
                            .all(|c| (0..3).all(|x| (t[c][x] - w[(c + turn) % 3][x]).abs() < 2e-3))
                    })
                });
                want.swap_remove(
                    found.unwrap_or_else(|| panic!("case {k}: {t:?} is not in the source")),
                );
            }
            // The cube's texture coordinates: a pair a point, in 0..=1.
            let uv = d.attributes.iter().find(|a| a.components == 2);
            assert_eq!(uv.map_or(0, |a| a.values.len()), uvs * 2, "case {k}");
            assert!(uv.is_none_or(|a| a.values.iter().all(|v| (-1e-3..=1.001).contains(v))));
        }
        // Damaged, a file is an error or some mesh, and not a panic.
        for (packed, _, _) in cases {
            for at in 0..packed.len() {
                for flip in [0xff, 0x55] {
                    let mut broken = packed.to_vec();
                    broken[at] ^= flip;
                    let _ = decode(&broken);
                }
            }
        }
        // Cut short, a file is an error and not a panic.
        let whole = file!("test_nm.obj.edgebreaker.cl10.2.2.drc");
        for cut in [0, 5, 11, 40, whole.len() / 2, whole.len() - 1] {
            assert!(decode(&whole[..cut]).is_err(), "cut at {cut}");
        }
    }
}
