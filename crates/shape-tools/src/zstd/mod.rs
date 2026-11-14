//! Zstandard, written: a frame of blocks, each its literals under a
//! Huffman code and its matches as sequences under FSE codes
//! (RFC 8878; `docs/research.md` §77).
//!
//! Entry: [`compress`]. ruzstd, which reads Zstandard here, writes it
//! at one level only: one candidate a position, no repeated offsets,
//! and code lengths from the order of the counts, not the counts.
//! This finds matches down a chain of the earlier places that begin
//! with the same four bytes, over everything written so far, looks a
//! byte ahead before it takes one, names an offset used a moment ago
//! by its place among the last three (which is what blocks of a fixed
//! size copied from their neighbours need), and makes each code from
//! what it is to carry. The bit writer, the FSE states and the Huffman
//! streams (`bits.rs`, `fse.rs`, `huff0.rs`) are ruzstd's own, copied.

mod bits;
mod fse;
mod huff0;

use bits::BitWriter;
use fse::FSETable;

/// The most a block holds of the data.
const BLOCK: usize = 128 * 1024;
/// The furthest back a match reaches: the window every reader allows.
const BACK: usize = 1 << 27;
/// The shortest match taken from a new offset, and from one of the
/// last three.
const MIN_MATCH: usize = 4;
const MIN_REPEAT: usize = 3;
/// How many earlier places with the same four bytes are tried.
const CHAIN: usize = 64;
/// A match this long is taken without looking further.
const GOOD: usize = 128;
/// The longest code a literal gets.
const LITERAL_BITS: u8 = 11;

type Writer<'a> = BitWriter<&'a mut Vec<u8>>;

/// Earlier places by the four bytes they begin with: the latest for
/// each hash, for each place the one before it with that hash, and how
/// far the places have been entered.
struct Chains {
    head: Vec<u32>,
    before: Vec<u32>,
    shift: u32,
    entered: usize,
}

const NONE: u32 = u32::MAX;

impl Chains {
    fn new(len: usize) -> Chains {
        let bits = (len.max(2).ilog2() + 1).clamp(10, 20);
        Chains {
            head: vec![NONE; 1 << bits],
            before: vec![NONE; len],
            shift: 32 - bits,
            entered: 0,
        }
    }

    fn hash(&self, data: &[u8], at: usize) -> usize {
        let v = u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
        (v.wrapping_mul(2_654_435_761) >> self.shift) as usize
    }

    /// Enters every place before `to` not entered yet (each once, in
    /// order, while four bytes follow it).
    fn enter(&mut self, data: &[u8], to: usize) {
        let to = to.min(data.len().saturating_sub(3));
        while self.entered < to {
            let h = self.hash(data, self.entered);
            self.before[self.entered] = self.head[h];
            self.head[h] = self.entered as u32;
            self.entered += 1;
        }
    }

    /// The longest match for `at` among the entered places, none
    /// reaching past `end`: its length and how far back.
    fn longest(&self, data: &[u8], at: usize, end: usize) -> Option<(usize, usize)> {
        if at + MIN_MATCH > end {
            return None;
        }
        let mut best: Option<(usize, usize)> = None;
        let mut from = self.head[self.hash(data, at)];
        let mut tries = CHAIN;
        while from != NONE && tries > 0 {
            let f = from as usize;
            if at - f > BACK {
                break;
            }
            let need = best.map_or(MIN_MATCH - 1, |b| b.0);
            // Only one that can be longer: its byte at the best length.
            if at + need < end && data[f + need] == data[at + need] {
                let len = common(data, f, at, end);
                if len > need {
                    best = Some((len, at - f));
                    if len >= GOOD {
                        break;
                    }
                }
            }
            from = self.before[f];
            tries -= 1;
        }
        best
    }
}

/// How many bytes from `a` and from `b` are the same, `b` not past `end`.
fn common(data: &[u8], a: usize, b: usize, end: usize) -> usize {
    let mut n = 0;
    while b + n < end && data[a + n] == data[b + n] {
        n += 1;
    }
    n
}

/// The sequences of `data[from..end]` (literals, match length, the
/// offset's value in the stream), matched against everything before;
/// where the literals after the last of them begin; and the last three
/// offsets after them, given those before.
fn sequences(
    data: &[u8],
    from: usize,
    end: usize,
    chains: &mut Chains,
    history: [usize; 3],
) -> (Vec<(u32, u32, u32)>, usize, [usize; 3]) {
    let mut out = Vec::new();
    let mut last = history;
    let mut at = from;
    let mut literals = from;
    // The best match at a place: one from the last offsets where it
    // is nearly as long, since it costs a bit or two to name.
    let choose = |chains: &Chains, at: usize, literals: usize, last: &[usize; 3]| {
        let named = if at > literals {
            *last
        } else {
            [last[1], last[2], last[0].saturating_sub(1)]
        };
        let mut repeat: Option<(usize, usize)> = None;
        for o in named {
            if o == 0 || o > at {
                continue;
            }
            let len = common(data, at - o, at, end);
            if len >= MIN_REPEAT && repeat.is_none_or(|r| len > r.0) {
                repeat = Some((len, o));
            }
        }
        match (repeat, chains.longest(data, at, end)) {
            (Some(r), Some(f)) if r.0 + 3 >= f.0 => Some(r),
            (Some(r), None) => Some(r),
            (_, f) => f,
        }
    };
    while at + MIN_REPEAT <= end {
        chains.enter(data, at);
        let Some(mut found) = choose(chains, at, literals, &last) else {
            at += 1;
            continue;
        };
        // One byte on: a longer match there is worth a literal.
        while found.0 < GOOD && at + 1 + MIN_REPEAT <= end {
            chains.enter(data, at + 1);
            match choose(chains, at + 1, literals, &last) {
                Some(next) if next.0 > found.0 + 1 => {
                    at += 1;
                    found = next;
                }
                _ => break,
            }
        }
        let (mut len, offset) = found;
        // The bytes before may match too.
        while at > literals && at > offset && data[at - 1] == data[at - 1 - offset] {
            at -= 1;
            len += 1;
        }
        let run = (at - literals) as u32;
        out.push((run, len as u32, offset_value(offset, run, &mut last)));
        at += len;
        literals = at;
    }
    chains.enter(data, end);
    (out, literals, last)
}

/// A literal length as its code, the bits after it and how many.
fn literal_code(len: u32) -> (u8, u32, usize) {
    match len {
        0..=15 => (len as u8, 0, 0),
        16..=17 => (16, len - 16, 1),
        18..=19 => (17, len - 18, 1),
        20..=21 => (18, len - 20, 1),
        22..=23 => (19, len - 22, 1),
        24..=27 => (20, len - 24, 2),
        28..=31 => (21, len - 28, 2),
        32..=39 => (22, len - 32, 3),
        40..=47 => (23, len - 40, 3),
        48..=63 => (24, len - 48, 4),
        64..=127 => (25, len - 64, 6),
        128..=255 => (26, len - 128, 7),
        256..=511 => (27, len - 256, 8),
        512..=1023 => (28, len - 512, 9),
        1024..=2047 => (29, len - 1024, 10),
        2048..=4095 => (30, len - 2048, 11),
        4096..=8191 => (31, len - 4096, 12),
        8192..=16383 => (32, len - 8192, 13),
        16384..=32767 => (33, len - 16384, 14),
        32768..=65535 => (34, len - 32768, 15),
        _ => (35, len - 65536, 16),
    }
}

/// A match length (3 or more) as its code, the bits after it and how
/// many.
fn match_code(len: u32) -> (u8, u32, usize) {
    match len {
        0..=34 => (len.saturating_sub(3) as u8, 0, 0),
        35..=36 => (32, len - 35, 1),
        37..=38 => (33, len - 37, 1),
        39..=40 => (34, len - 39, 1),
        41..=42 => (35, len - 41, 1),
        43..=46 => (36, len - 43, 2),
        47..=50 => (37, len - 47, 2),
        51..=58 => (38, len - 51, 3),
        59..=66 => (39, len - 59, 3),
        67..=82 => (40, len - 67, 4),
        83..=98 => (41, len - 83, 4),
        99..=130 => (42, len - 99, 5),
        131..=258 => (43, len - 131, 7),
        259..=514 => (44, len - 259, 8),
        515..=1026 => (45, len - 515, 9),
        1027..=2050 => (46, len - 1027, 10),
        2051..=4098 => (47, len - 2051, 11),
        4099..=8194 => (48, len - 4099, 12),
        8195..=16386 => (49, len - 8195, 13),
        16387..=32770 => (50, len - 16387, 14),
        32771..=65538 => (51, len - 32771, 15),
        _ => (52, len - 65539, 16),
    }
}

/// An offset's value in the stream as its code, the bits after it and
/// how many: the code is the place of its highest bit.
fn offset_code(value: u32) -> (u8, u32, usize) {
    let log = value.ilog2();
    (log as u8, value & ((1 << log) - 1), log as usize)
}

/// The value that names `offset` after `literals` literals, given the
/// last three offsets, which it updates: 1 to 3 for one of them (which
/// one shifts when no literal comes before), else the offset plus 3.
fn offset_value(offset: usize, literals: u32, last: &mut [usize; 3]) -> u32 {
    let [a, b, c] = *last;
    if literals > 0 {
        if offset == a {
            return 1;
        }
        if offset == b {
            *last = [b, a, c];
            return 2;
        }
        if offset == c {
            *last = [c, a, b];
            return 3;
        }
    } else {
        if offset == b {
            *last = [b, a, c];
            return 1;
        }
        if offset == c {
            *last = [c, a, b];
            return 2;
        }
        if a > 1 && offset == a - 1 {
            *last = [a - 1, a, b];
            return 3;
        }
    }
    *last = [offset, a, b];
    offset as u32 + 3
}

/// What a symbol of probability `p` in a table of `1 << log` costs, in
/// 256ths of a bit (the logarithm between powers of two taken as a
/// straight line: near enough to choose between two tables).
fn cost(p: u32, log: u32) -> u64 {
    let whole = p.ilog2();
    let part = ((u64::from(p) << 8) >> whole) - 256;
    u64::from(log - whole) * 256 - part
}

/// Counts as probabilities that add to a power of two, each symbol
/// seen given at least one, and that power: at most `max_log`, and
/// less where the counts are few. With `halved`, no symbol gets more
/// than half (so no state of the table costs no bits, which a stream
/// that ends where its bits end needs). Two symbols at least are seen.
fn normalized(counts: &[u32], max_log: u32, halved: bool) -> (Vec<i32>, u8) {
    let total: u64 = counts.iter().map(|c| u64::from(*c)).sum();
    let top = counts.len() as u32 - 1;
    // As the reference chooses it: fewer bits for fewer symbols, and
    // enough for every one seen to have a place.
    let least = (total.ilog2() + 1).min(top.ilog2() + 2);
    let few = (total - 1).ilog2().saturating_sub(2);
    let log = max_log.min(few).max(least).max(5).min(max_log);
    let size = 1u64 << log;
    let mut probs: Vec<i32> = counts
        .iter()
        .map(|c| match u64::from(*c) {
            0 => 0,
            c => ((c * size * 2 + total) / (total * 2)).max(1) as i32,
        })
        .collect();
    let mut sum: i64 = probs.iter().map(|p| i64::from(*p)).sum();
    let largest = |probs: &[i32], but: Option<usize>| -> usize {
        let mut best = usize::MAX;
        for (i, p) in probs.iter().enumerate() {
            if Some(i) != but && (best == usize::MAX || *p > probs[best]) {
                best = i;
            }
        }
        best
    };
    // Too many: from the largest, one at a time. Too few: to it.
    while sum > size as i64 {
        let i = largest(&probs, None);
        probs[i] -= 1;
        sum -= 1;
    }
    if sum < size as i64 {
        let i = largest(&probs, None);
        probs[i] += (size as i64 - sum) as i32;
    }
    if halved {
        let i = largest(&probs, None);
        let over = probs[i] - (size / 2) as i32;
        if over > 0 {
            probs[i] -= over;
            let next = largest(&probs, Some(i));
            probs[next] += over;
        }
    }
    (probs, log as u8)
}

/// Code lengths for the symbols counted, none over `limit` bits, that
/// fill the code space exactly. Two symbols at least are seen.
fn code_lengths(counts: &[u32], limit: u8) -> Vec<u8> {
    // Leaves by count, then symbol; the nodes made of two are made in
    // rising order, so the two smallest are at the front of one list
    // or the other.
    let mut leaves: Vec<(u64, usize)> = counts
        .iter()
        .enumerate()
        .filter(|c| *c.1 > 0)
        .map(|(s, c)| (u64::from(*c), s))
        .collect();
    leaves.sort_unstable();
    let n = leaves.len();
    let mut weight: Vec<u64> = leaves.iter().map(|l| l.0).collect();
    let mut parent = vec![0usize; 2 * n - 1];
    let (mut leaf, mut made) = (0, n);
    for node in n..2 * n - 1 {
        let mut sum = 0;
        for _ in 0..2 {
            let take = if leaf < n && (made >= weight.len() || weight[leaf] <= weight[made]) {
                leaf += 1;
                leaf - 1
            } else {
                made += 1;
                made - 1
            };
            sum += weight[take];
            parent[take] = node;
        }
        weight.push(sum);
    }
    let mut depth = vec![0u32; 2 * n - 1];
    for node in (0..2 * n - 2).rev() {
        depth[node] = depth[parent[node]] + 1;
    }
    // Leaves are in rising count, so in falling depth. Bring the deep
    // ones up to the limit, then pay for it: lengthen the longest
    // code under the limit until the space is not overfull, and
    // shorten the commonest of the longest codes until it is full.
    let mut len: Vec<u8> = depth[..n]
        .iter()
        .map(|d| (*d).min(u32::from(limit)) as u8)
        .collect();
    let space = 1u64 << limit;
    let mut sum: u64 = len.iter().map(|l| 1u64 << (limit - l)).sum();
    while sum > space {
        let i = (0..n)
            .find(|i| len[*i] < limit)
            .expect("a code under the limit");
        sum -= 1u64 << (limit - len[i] - 1);
        len[i] += 1;
    }
    while sum < space {
        let longest = *len.iter().max().unwrap_or(&0);
        let i = (0..n)
            .rev()
            .find(|i| len[*i] == longest)
            .expect("a longest code");
        sum += 1u64 << (limit - len[i]);
        len[i] -= 1;
    }
    let mut out = vec![0u8; counts.len()];
    for (l, (_, symbol)) in len.iter().zip(&leaves) {
        out[*symbol] = *l;
    }
    out
}

/// The literals as they are: a header of three bytes, then the bytes.
fn raw_literals(literals: &[u8], writer: &mut Writer) {
    writer.write_bits(0u8, 2);
    writer.write_bits(0b11u8, 2);
    writer.write_bits(literals.len() as u32, 20);
    writer.append_bytes(literals);
}

/// A Huffman code's description: the weights of all symbols but the
/// last, under an FSE code where that is smaller, else four bits
/// each. False where neither can say them.
fn weights_described(weights: &[u8], writer: &mut Writer) -> bool {
    let direct = weights.len().div_ceil(2);
    let mut counts = [0u32; 16];
    for w in weights {
        counts[usize::from(*w)] += 1;
    }
    let top = counts.iter().rposition(|c| *c > 0).unwrap_or(0);
    let several = counts.iter().filter(|c| **c > 0).count() > 1;
    if several && weights.len() >= 4 {
        let (probs, log) = normalized(&counts[..=top], 6, true);
        let mut coded = Vec::new();
        let mut inner = BitWriter::from(&mut coded);
        let table = fse::build_table_from_probabilities(&probs, log);
        fse::FSEEncoder::new(table, &mut inner).encode_interleaved(weights);
        inner.flush();
        if coded.len() < 128 && (coded.len() < direct || weights.len() > 128) {
            writer.write_bits(coded.len() as u8, 8);
            writer.append_bytes(&coded);
            return true;
        }
    }
    if weights.len() > 128 {
        return false;
    }
    writer.write_bits(weights.len() as u8 + 127, 8);
    for pair in weights.chunks(2) {
        writer.write_bits(pair.get(1).copied().unwrap_or(0), 4);
        writer.write_bits(pair[0], 4);
    }
    true
}

/// The literals under a Huffman code made from their counts, or as
/// they are where that is no smaller.
fn literals_section(literals: &[u8], writer: &mut Writer) {
    let mut counts = [0u32; 256];
    for b in literals {
        counts[usize::from(*b)] += 1;
    }
    let top = counts.iter().rposition(|c| *c > 0).unwrap_or(0);
    let several = counts.iter().filter(|c| **c > 0).count() > 1;
    // A code needs two symbols, and a few bytes are not worth one.
    if literals.len() < 32 || !several {
        raw_literals(literals, writer);
        return;
    }
    let lengths = code_lengths(&counts[..=top], LITERAL_BITS);
    let longest = *lengths.iter().max().unwrap_or(&0);
    let weights: Vec<usize> = lengths
        .iter()
        .map(|l| {
            if *l == 0 {
                0
            } else {
                usize::from(longest - l + 1)
            }
        })
        .collect();
    let table = huff0::HuffmanTable::build_from_weights(&weights);
    let start = writer.index();
    // One stream for a short run, four past it; the sizes' width by
    // how many literals there are.
    let (format, bits, four) = match literals.len() {
        0..256 => (0b00u8, 10, false),
        256..1024 => (0b01, 10, true),
        1024..16384 => (0b10, 14, true),
        _ => (0b11, 18, true),
    };
    writer.write_bits(2u8, 2);
    writer.write_bits(format, 2);
    writer.write_bits(literals.len() as u32, bits);
    let size_at = writer.index();
    writer.write_bits(0u32, bits);
    let before = writer.index();
    let all: Vec<u8> = weights[..weights.len() - 1]
        .iter()
        .map(|w| *w as u8)
        .collect();
    let said = weights_described(&all, writer);
    if said {
        let mut streams = huff0::HuffmanEncoder::new(&table, writer);
        if four {
            streams.encode4x(literals, false);
        } else {
            streams.encode(literals, false);
        }
    }
    let coded = (writer.index() - before) / 8;
    // The coded size must fit its field, and the whole must be a gain.
    if !said || coded >= 1 << bits || (writer.index() - start) / 8 >= literals.len() + 3 {
        writer.reset_to(start);
        raw_literals(literals, writer);
        return;
    }
    writer.change_bits(size_at, coded as u64, bits);
}

/// How one of the three kinds of code is carried in a block: by the
/// table every reader has, by one symbol that is the only one, or by
/// a table described in the block.
struct Coder {
    mode: u8,
    symbol: u8,
    table: Option<FSETable>,
}

impl Coder {
    /// The cheapest of the three for these codes.
    fn of(codes: impl Iterator<Item = u8>, max_log: u32, known: &[i32], known_log: u32) -> Coder {
        let mut counts = [0u32; 64];
        for c in codes {
            counts[usize::from(c)] += 1;
        }
        let top = counts.iter().rposition(|c| *c > 0).unwrap_or(0);
        let counts = &counts[..=top];
        let priced = |probs: &[i32], log: u32| -> u64 {
            counts
                .iter()
                .zip(probs)
                .map(|(c, p)| u64::from(*c) * cost(p.unsigned_abs().max(1), log))
                .sum()
        };
        let known_cost = (top < known.len()).then(|| priced(known, known_log));
        let ready = || Coder {
            mode: 0,
            symbol: 0,
            table: Some(fse::build_table_from_probabilities(known, known_log as u8)),
        };
        if counts.iter().filter(|c| **c > 0).count() == 1 {
            return match known_cost {
                Some(c) if c < 8 * 256 => ready(),
                _ => Coder {
                    mode: 1,
                    symbol: top as u8,
                    table: None,
                },
            };
        }
        let (probs, log) = normalized(counts, max_log, false);
        let table = fse::build_table_from_probabilities(&probs, log);
        let mut said = BitWriter::new();
        table.write_table(&mut said);
        let own = said.index() as u64 * 256 + priced(&probs, u32::from(log));
        match known_cost {
            Some(c) if c <= own => ready(),
            _ => Coder {
                mode: 2,
                symbol: 0,
                table: Some(table),
            },
        }
    }

    /// What the block says of the code before the sequences.
    fn describe(&self, writer: &mut Writer) {
        match (&self.table, self.mode) {
            (Some(table), 2) => table.write_table(writer),
            (None, _) => writer.write_bits(self.symbol, 8),
            _ => {}
        }
    }

    /// The state the last sequence's code is read from.
    fn start(&self, code: u8) -> usize {
        self.table.as_ref().map_or(0, |t| t.start_state(code).index)
    }

    /// Writes the bits that take the reader from `code`'s state to
    /// `state`, which becomes that state.
    fn step(&self, state: &mut usize, code: u8, writer: &mut Writer) {
        if let Some(table) = &self.table {
            let next = table.next_state(code, *state);
            writer.write_bits((*state - next.baseline) as u64, usize::from(next.num_bits));
            *state = next.index;
        }
    }

    /// Writes the state the reader starts in.
    fn finish(&self, state: usize, writer: &mut Writer) {
        if let Some(table) = &self.table {
            writer.write_bits(state as u64, table.table_size.ilog2() as usize);
        }
    }
}

/// The sequences under three codes chosen for them, written from the
/// last to the first, as the reader takes them.
fn sequences_section(seqs: &[(u32, u32, u32)], writer: &mut Writer) {
    match seqs.len() {
        0 => {
            writer.write_bits(0u8, 8);
            return;
        }
        n @ 1..=127 => writer.write_bits(n as u32, 8),
        n @ 128..=0x7EFF => {
            writer.write_bits(((n >> 8) | 0x80) as u8, 8);
            writer.write_bits(n as u8, 8);
        }
        n => {
            let rest = n - 0x7F00;
            writer.write_bits(255u8, 8);
            writer.write_bits(rest as u8, 8);
            writer.write_bits((rest >> 8) as u8, 8);
        }
    }
    let ll = Coder::of(seqs.iter().map(|s| literal_code(s.0).0), 9, fse::LL_DIST, 6);
    let ml = Coder::of(seqs.iter().map(|s| match_code(s.1).0), 9, fse::ML_DIST, 6);
    let of = Coder::of(seqs.iter().map(|s| offset_code(s.2).0), 8, fse::OF_DIST, 5);
    writer.write_bits((ll.mode << 6) | (of.mode << 4) | (ml.mode << 2), 8);
    ll.describe(writer);
    of.describe(writer);
    ml.describe(writer);
    let parts = |s: &(u32, u32, u32)| (literal_code(s.0), match_code(s.1), offset_code(s.2));
    let (l, m, o) = parts(&seqs[seqs.len() - 1]);
    let (mut ll_state, mut ml_state, mut of_state) = (ll.start(l.0), ml.start(m.0), of.start(o.0));
    writer.write_bits(l.1, l.2);
    writer.write_bits(m.1, m.2);
    writer.write_bits(o.1, o.2);
    for s in seqs[..seqs.len() - 1].iter().rev() {
        let (l, m, o) = parts(s);
        of.step(&mut of_state, o.0, writer);
        ml.step(&mut ml_state, m.0, writer);
        ll.step(&mut ll_state, l.0, writer);
        writer.write_bits(l.1, l.2);
        writer.write_bits(m.1, m.2);
        writer.write_bits(o.1, o.2);
    }
    ml.finish(ml_state, writer);
    of.finish(of_state, writer);
    ll.finish(ll_state, writer);
    // A last set bit marks where the bits end.
    let fill = writer.misaligned();
    writer.write_bits(1u32, if fill == 0 { 8 } else { fill });
}

/// `data` as one Zstandard frame that says its own size.
pub(crate) fn compress(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x28, 0xB5, 0x2F, 0xFD];
    let n = data.len();
    // One segment (the window is the content) with the size in 1, 2
    // or 4 bytes; past what a window may be, the largest window and
    // the size in 8.
    match n {
        0..256 => out.extend_from_slice(&[1 << 5, n as u8]),
        256..65792 => {
            out.push((1 << 6) | (1 << 5));
            out.extend_from_slice(&((n - 256) as u16).to_le_bytes());
        }
        65792..=BACK => {
            out.push((2 << 6) | (1 << 5));
            out.extend_from_slice(&(n as u32).to_le_bytes());
        }
        _ => {
            out.extend_from_slice(&[3 << 6, 17 << 3]);
            out.extend_from_slice(&(n as u64).to_le_bytes());
        }
    }
    let header = |out: &mut Vec<u8>, last: bool, kind: u32, size: usize| {
        let h = ((size as u32) << 3) | (kind << 1) | u32::from(last);
        out.extend_from_slice(&h.to_le_bytes()[..3]);
    };
    if n == 0 {
        header(&mut out, true, 0, 0);
        return out;
    }
    let mut chains = Chains::new(if n < NONE as usize { n } else { 0 });
    // The last three offsets, as the reader has them: a block written
    // as it is leaves them alone.
    let mut history = [1usize, 4, 8];
    let mut from = 0;
    while from < n {
        let end = (from + BLOCK).min(n);
        let last = end == n;
        let block = &data[from..end];
        // Places are kept in 32 bits: past them, blocks go as they are.
        if n >= NONE as usize {
            header(&mut out, last, 0, block.len());
            out.extend_from_slice(block);
            from = end;
            continue;
        }
        // One byte over and over is said once.
        if block.len() > 1 && block.iter().all(|b| *b == block[0]) {
            chains.enter(data, end);
            header(&mut out, last, 1, block.len());
            out.push(block[0]);
            from = end;
            continue;
        }
        let (seqs, tail, after) = sequences(data, from, end, &mut chains, history);
        let mut literals = Vec::with_capacity(block.len());
        let mut at = from;
        for s in &seqs {
            literals.extend_from_slice(&data[at..at + s.0 as usize]);
            at += (s.0 + s.1) as usize;
        }
        literals.extend_from_slice(&data[tail..end]);
        let mut packed = Vec::new();
        let mut writer = BitWriter::from(&mut packed);
        literals_section(&literals, &mut writer);
        sequences_section(&seqs, &mut writer);
        writer.flush();
        if packed.len() < block.len() {
            header(&mut out, last, 2, packed.len());
            out.extend_from_slice(&packed);
            history = after;
        } else {
            header(&mut out, last, 0, block.len());
            out.extend_from_slice(block);
        }
        from = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn back(packed: &[u8]) -> Vec<u8> {
        use std::io::Read;
        let mut d = ruzstd::decoding::StreamingDecoder::new(packed).expect("a frame");
        let mut out = Vec::new();
        d.read_to_end(&mut out).expect("its blocks");
        out
    }

    /// What is written reads back through ruzstd's decoder: nothing,
    /// a byte, runs, text that repeats, noise (which stays as it is),
    /// bytes of a few values with no pattern (the Huffman code alone),
    /// blocks of sixteen bytes copied from near and far (as UASTC's
    /// are), and data past one block of 128 KiB. Text and runs come
    /// out a twentieth of their size or less, and the copied blocks
    /// well under what ruzstd's own writer makes of them.
    #[test]
    fn what_is_written_reads_back() {
        let mut state = 7u32;
        let mut noise = move || {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (state >> 16) as u8
        };
        let text = b"the quick brown fox jumps over the lazy dog; ".repeat(300);
        let random: Vec<u8> = (0..70_000).map(|_| noise()).collect();
        // Mostly small values: a skewed code, some of it deep.
        let skewed: Vec<u8> = (0..150_000)
            .map(|_| {
                let (a, b) = (noise(), noise());
                if a < 200 {
                    b % 4
                } else if a < 250 {
                    b % 40
                } else {
                    b
                }
            })
            .collect();
        // Blocks of 16 bytes, most of them a copy of one before.
        let mut blocks: Vec<u8> = Vec::new();
        for i in 0..40_000usize {
            if i < 8 || noise() < 60 {
                blocks.extend((0..16).map(|_| noise()));
            } else {
                let back = [1, 2, 3, 256, 257, 1000][usize::from(noise()) % 6].min(i);
                let from = (i - back) * 16;
                let copy: Vec<u8> = blocks[from..from + 16].to_vec();
                blocks.extend(copy);
            }
        }
        let few: Vec<u8> = (0..200_000).map(|i| b"ab"[(i / 7) % 2]).collect();
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("nothing", Vec::new()),
            ("a byte", vec![9]),
            ("three bytes", vec![1, 2, 3]),
            ("a run", vec![5; 1000]),
            ("a long run", vec![0; 400_000]),
            ("text", text),
            ("noise", random.clone()),
            ("skewed", skewed),
            ("blocks", blocks),
            ("two letters", few),
            ("255 bytes", random[..255].to_vec()),
            ("256 bytes", random[..256].to_vec()),
            ("65791 bytes", random[..65_791].to_vec()),
        ];
        for (name, data) in cases {
            let packed = compress(&data);
            assert!(
                back(&packed) == data,
                "{name}: {} bytes do not read back",
                data.len()
            );
            let theirs = || {
                ruzstd::encoding::compress_to_vec(
                    data.as_slice(),
                    ruzstd::encoding::CompressionLevel::Fastest,
                )
                .len()
            };
            match name {
                "noise" => assert!(packed.len() <= data.len() + 16, "{name}"),
                "text" | "a long run" | "two letters" => {
                    assert!(packed.len() * 20 < data.len(), "{name}: {}", packed.len());
                }
                "skewed" => assert!(
                    packed.len() * 20 < theirs() * 19,
                    "{name}: {}",
                    packed.len()
                ),
                "blocks" => assert!(packed.len() * 10 < theirs() * 9, "{name}: {}", packed.len()),
                _ => {}
            }
        }
    }
}
