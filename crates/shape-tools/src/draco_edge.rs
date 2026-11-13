//! Draco's EdgeBreaker connectivity, written: the triangles as a walk
//! over the surface, a symbol a triangle (`docs/research.md` §67).
//!
//! Entry: [`walk`]. Ported from Draco's
//! `mesh_edgebreaker_encoder_impl.cc` (Apache-2.0,
//! `third_party/draco/`): the standard traversal, with its topology
//! split events, its start faces and its open boundaries. What the
//! decoder in `draco.rs` reads is what this writes.

use std::collections::BTreeMap;

/// A mesh's corners: corner `3 f + k` is face `f`'s `k`th.
pub(crate) struct Walk {
    /// The connectivity as the bitstream holds it, from the traversal
    /// type on.
    pub bytes: Vec<u8>,
    /// For each value in the order the decoder makes them: the corner
    /// whose vertex it is.
    pub entry_corner: Vec<i32>,
    /// For each vertex: its place in that order.
    pub vertex_entry: Vec<i32>,
    /// For each corner: the corner across its opposite edge, or −1.
    pub opposite: Vec<i32>,
    /// For each corner: its vertex.
    pub vertex: Vec<u32>,
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

fn varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// Bits into bytes, the low bit of each byte first.
#[derive(Default)]
struct BitsOut {
    bytes: Vec<u8>,
    bit: usize,
}

impl BitsOut {
    fn write(&mut self, n: u32, v: u32) {
        for i in 0..n {
            if self.bit.is_multiple_of(8) {
                self.bytes.push(0);
            }
            if (v >> i) & 1 == 1 {
                let last = self.bytes.len() - 1;
                self.bytes[last] |= 1 << (self.bit % 8);
            }
            self.bit += 1;
        }
    }
}

/// Bits under Draco's binary ANS coder (`RAnsBitEncoder`): the
/// probability of a zero in 256ths, the size, the bytes.
pub(crate) fn ans_bits(out: &mut Vec<u8>, bits: &[bool]) {
    let zeros = bits.iter().filter(|b| !**b).count();
    let total = bits.len().max(1);
    // Whole-number rounding of zeros / total · 256, kept within 1..=255.
    let p0 = ((zeros * 512 + total) / (2 * total)).clamp(1, 255) as u32;
    let p1 = 256 - p0;
    let mut state = 4096u32;
    let mut buf: Vec<u8> = Vec::new();
    for &bit in bits.iter().rev() {
        let share = if bit { p1 } else { p0 };
        if state >= 4096 * share {
            buf.push((state % 256) as u8);
            state /= 256;
        }
        let (quot, rem) = (state / share, state % share);
        state = quot * 256 + rem + if bit { 0 } else { p1 };
    }
    let state = state - 4096;
    if state < 1 << 6 {
        buf.push(state as u8);
    } else if state < 1 << 14 {
        buf.extend_from_slice(&((1u32 << 14) + state).to_le_bytes()[..2]);
    } else {
        buf.extend_from_slice(&((2u32 << 22) + state).to_le_bytes()[..3]);
    }
    out.push(p0 as u8);
    varint(out, buf.len() as u64);
    out.extend_from_slice(&buf);
}

const C: u32 = 0;
const S: u32 = 1;
const L: u32 = 3;
const R: u32 = 5;
const E: u32 = 7;

/// The triangles as an EdgeBreaker walk; `None` for a mesh the corner
/// table does not hold as it is: a triangle with a vertex twice, an
/// edge of more than two triangles or of two facing one way, a vertex
/// whose triangles are not one fan, or a point no triangle uses (the
/// walk would drop it, and the count with it).
pub(crate) fn walk(faces: &[[u32; 3]], points: usize) -> Option<Walk> {
    let corners = faces.len() * 3;
    let vertex: Vec<u32> = faces.iter().flatten().copied().collect();
    if faces.is_empty()
        || faces
            .iter()
            .any(|t| t[0] == t[1] || t[1] == t[2] || t[2] == t[0])
        || vertex.iter().any(|&v| v as usize >= points)
    {
        return None;
    }
    // Opposites: corner c faces the edge next(c) → prev(c).
    let mut by_edge: BTreeMap<(u32, u32), i32> = BTreeMap::new();
    for c in 0..corners as i32 {
        let e = (vertex[next(c) as usize], vertex[prev(c) as usize]);
        if by_edge.insert(e, c).is_some() {
            return None;
        }
    }
    let mut opposite = vec![-1i32; corners];
    for (&(a, b), &c) in &by_edge {
        if let Some(&o) = by_edge.get(&(b, a)) {
            opposite[c as usize] = o;
        }
    }
    let opp = |c: i32| if c < 0 { -1 } else { opposite[c as usize] };
    let vert = |c: i32| vertex[c as usize] as usize;
    let face = |c: i32| (c / 3) as usize;
    let swing_right = |c: i32| prev(opp(prev(c)));
    let swing_left = |c: i32| next(opp(next(c)));
    // One fan a vertex, and every point used.
    let mut count = vec![0u32; points];
    let mut first = vec![-1i32; points];
    for c in 0..corners as i32 {
        count[vert(c)] += 1;
        if first[vert(c)] < 0 {
            first[vert(c)] = c;
        }
    }
    for v in 0..points {
        if first[v] < 0 {
            return None;
        }
        // To the fan's left end (or once round), then count rightwards.
        let mut c = first[v];
        let mut steps = 0;
        while swing_left(c) >= 0 && swing_left(c) != first[v] && steps <= count[v] {
            c = swing_left(c);
            steps += 1;
        }
        let start = c;
        let mut seen = 1;
        while swing_right(c) >= 0 && swing_right(c) != start && seen <= count[v] {
            c = swing_right(c);
            seen += 1;
        }
        if seen != count[v] {
            return None;
        }
    }
    // FindHoles: each vertex's open boundary, if it is on one.
    let mut hole_of = vec![-1i32; points];
    let mut holes = 0i32;
    for i in 0..corners as i32 {
        if opp(i) >= 0 {
            continue;
        }
        let mut v = vert(next(i));
        if hole_of[v] != -1 {
            continue;
        }
        let mut c = i;
        while hole_of[v] == -1 {
            hole_of[v] = holes;
            c = next(c);
            while opp(c) >= 0 {
                c = next(opp(c));
            }
            v = vert(next(c));
        }
        holes += 1;
    }
    let mut hole_done = vec![false; holes as usize];
    let mut face_done = vec![false; faces.len()];
    let mut vert_done = vec![false; points];
    let mut symbols: Vec<u32> = Vec::new();
    let mut splits = 0u64;
    // (source symbol, split symbol, whether on the source's right edge)
    let mut events: Vec<(i64, i64, bool)> = Vec::new();
    let mut split_of: BTreeMap<usize, i64> = BTreeMap::new();
    let mut processed: Vec<i32> = Vec::new();
    let mut init_corners: Vec<i32> = Vec::new();
    let mut start_bits: Vec<bool> = Vec::new();

    // EncodeHole.
    let encode_hole =
        |start: i32, first_too: bool, vert_done: &mut Vec<bool>, hole_done: &mut Vec<bool>| {
            let mut c = prev(start);
            while opp(c) >= 0 {
                c = next(opp(c));
            }
            let start_vertex = vert(start);
            if first_too {
                vert_done[start_vertex] = true;
            }
            hole_done[hole_of[start_vertex] as usize] = true;
            let mut act = vert(prev(c));
            while act != start_vertex {
                vert_done[act] = true;
                c = next(c);
                while opp(c) >= 0 {
                    c = next(opp(c));
                }
                act = vert(prev(c));
            }
        };

    for c_id in 0..corners as i32 {
        let f = face(c_id);
        if face_done[f] {
            continue;
        }
        // FindInitFaceConfiguration.
        let mut corner = (3 * f) as i32;
        let mut interior = true;
        let mut start = corner;
        for _ in 0..3 {
            if opp(corner) < 0 {
                start = corner;
                interior = false;
                break;
            }
            if hole_of[vert(corner)] != -1 {
                let mut right = corner;
                while right >= 0 {
                    corner = right;
                    right = swing_right(right);
                }
                start = prev(corner);
                interior = false;
                break;
            }
            corner = next(corner);
        }
        start_bits.push(interior);
        let from = if interior {
            for c in [start, next(start), prev(start)] {
                vert_done[vert(c)] = true;
            }
            face_done[f] = true;
            init_corners.push(next(start));
            let o = opp(next(start));
            if o < 0 || face_done[face(o)] {
                continue;
            }
            o
        } else {
            encode_hole(next(start), true, &mut vert_done, &mut hole_done);
            start
        };
        // EncodeConnectivityFromCorner.
        let mut stack = vec![from];
        while let Some(&top) = stack.last() {
            let mut c = top;
            if c < 0 || face_done[face(c)] {
                stack.pop();
                continue;
            }
            loop {
                let id = symbols.len() as i64;
                let f = face(c);
                face_done[f] = true;
                processed.push(c);
                let v = vert(c);
                let on_boundary = hole_of[v] != -1;
                if !vert_done[v] {
                    vert_done[v] = true;
                    if !on_boundary {
                        symbols.push(C);
                        c = opp(next(c));
                        continue;
                    }
                }
                let (right, left) = (opp(next(c)), opp(prev(c)));
                let right_done = right < 0 || face_done[face(right)];
                let left_done = left < 0 || face_done[face(left)];
                let mut split_event = |neighbour: i32, on_right: bool| {
                    if neighbour >= 0
                        && let Some(&split) = split_of.get(&face(neighbour))
                    {
                        events.push((id, split, on_right));
                    }
                };
                if right_done {
                    split_event(right, true);
                    if left_done {
                        split_event(left, false);
                        symbols.push(E);
                        stack.pop();
                        break;
                    }
                    symbols.push(R);
                    c = left;
                } else if left_done {
                    split_event(left, false);
                    symbols.push(L);
                    c = right;
                } else {
                    symbols.push(S);
                    splits += 1;
                    if on_boundary && !hole_done[hole_of[v] as usize] {
                        encode_hole(c, false, &mut vert_done, &mut hole_done);
                    }
                    split_of.insert(f, id);
                    if let Some(top) = stack.last_mut() {
                        *top = left;
                    }
                    stack.push(right);
                    break;
                }
            }
        }
    }
    // The bytes.
    let mut bytes = vec![0u8];
    varint(&mut bytes, points as u64);
    varint(&mut bytes, faces.len() as u64);
    bytes.push(0);
    varint(&mut bytes, symbols.len() as u64);
    varint(&mut bytes, splits);
    varint(&mut bytes, events.len() as u64);
    let mut last = 0i64;
    for &(source, split, _) in &events {
        varint(&mut bytes, (source - last) as u64);
        varint(&mut bytes, (source - split) as u64);
        last = source;
    }
    let mut edge_bits = BitsOut::default();
    for &(_, _, on_right) in &events {
        edge_bits.write(1, u32::from(on_right));
    }
    bytes.extend_from_slice(&edge_bits.bytes);
    let mut symbol_bits = BitsOut::default();
    for &s in symbols.iter().rev() {
        symbol_bits.write(if s == C { 1 } else { 3 }, s);
    }
    varint(&mut bytes, symbol_bits.bytes.len() as u64);
    bytes.extend_from_slice(&symbol_bits.bytes);
    ans_bits(&mut bytes, &start_bits);

    // The order the decoder makes its values in: depth first from each
    // face it decodes, the last symbol's first, then the start faces.
    processed.reverse();
    processed.extend_from_slice(&init_corners);
    let mut entry_corner: Vec<i32> = Vec::with_capacity(points);
    let mut vertex_entry = vec![-1i32; points];
    let mut seen_face = vec![false; faces.len()];
    let mut visit = |c: i32, entry_corner: &mut Vec<i32>| -> bool {
        let v = vert(c);
        if vertex_entry[v] >= 0 {
            return false;
        }
        vertex_entry[v] = entry_corner.len() as i32;
        entry_corner.push(c);
        true
    };
    for &start in &processed {
        if seen_face[face(start)] {
            continue;
        }
        let mut stack = vec![start];
        visit(next(start), &mut entry_corner);
        visit(prev(start), &mut entry_corner);
        while let Some(&top) = stack.last() {
            let mut c = top;
            if c < 0 || seen_face[face(c)] {
                stack.pop();
                continue;
            }
            loop {
                seen_face[face(c)] = true;
                if visit(c, &mut entry_corner) && hole_of[vert(c)] == -1 {
                    c = opp(next(c));
                    if c < 0 {
                        return None;
                    }
                    continue;
                }
                let (right, left) = (opp(next(c)), opp(prev(c)));
                let right_done = right < 0 || seen_face[face(right)];
                let left_done = left < 0 || seen_face[face(left)];
                match (right_done, left_done) {
                    (true, true) => {
                        stack.pop();
                        break;
                    }
                    (true, false) => c = left,
                    (false, true) => c = right,
                    (false, false) => {
                        if let Some(top) = stack.last_mut() {
                            *top = left;
                        }
                        stack.push(right);
                        break;
                    }
                }
            }
        }
    }
    if entry_corner.len() != points {
        return None;
    }
    Some(Walk {
        bytes,
        entry_corner,
        vertex_entry,
        opposite,
        vertex,
    })
}

/// The mesh made walkable, where it is not: triangles with a vertex
/// twice dropped; at an edge of more than two triangles, or of two
/// facing one way, the triangles no longer joined; a vertex whose
/// triangles are several fans a point a fan; points no triangle uses
/// dropped. Gives the triangles over the new points, and each new
/// point's old one. The surface drawn is the same.
pub(crate) fn mended(faces: &[[u32; 3]], points: usize) -> (Vec<[u32; 3]>, Vec<u32>) {
    let kept: Vec<[u32; 3]> = faces
        .iter()
        .copied()
        .filter(|t| {
            t[0] != t[1] && t[1] != t[2] && t[2] != t[0] && t.iter().all(|&v| (v as usize) < points)
        })
        .collect();
    let corners = kept.len() * 3;
    let vertex: Vec<u32> = kept.iter().flatten().copied().collect();
    // An edge joins two triangles where it runs once each way.
    let mut uses: BTreeMap<(u32, u32), (u32, i32)> = BTreeMap::new();
    for c in 0..corners as i32 {
        let e = (vertex[next(c) as usize], vertex[prev(c) as usize]);
        let u = uses.entry(e).or_insert((0, c));
        u.0 += 1;
    }
    let mut opposite = vec![-1i32; corners];
    for (&(a, b), &(n, c)) in &uses {
        if n == 1
            && let Some(&(1, o)) = uses.get(&(b, a))
        {
            opposite[c as usize] = o;
        }
    }
    // Fans: corners of one vertex joined across those edges.
    let mut parent: Vec<usize> = (0..corners).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for c in 0..corners as i32 {
        // The corner of the same vertex in the triangle to the left.
        let o = opposite[next(c) as usize];
        if o >= 0 {
            let (x, y) = (
                find(&mut parent, c as usize),
                find(&mut parent, next(o) as usize),
            );
            parent[x.max(y)] = x.min(y);
        }
    }
    // A vertex's first fan keeps its point; the others get new ones.
    // Points come out in the order their first corner does.
    let mut point_of_fan: BTreeMap<usize, u32> = BTreeMap::new();
    let mut from: Vec<u32> = Vec::new();
    let mut out = kept.clone();
    for c in 0..corners {
        let root = find(&mut parent, c);
        let id = *point_of_fan.entry(root).or_insert_with(|| {
            from.push(vertex[c]);
            from.len() as u32 - 1
        });
        out[c / 3][c % 3] = id;
    }
    (out, from)
}
