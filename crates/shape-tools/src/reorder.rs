//! A GLB's triangles put in an order a vertex cache likes, and its
//! vertices in the order the triangles first use them: nothing drawn
//! changes, and the meshopt codecs that follow take fewer bytes
//! (`docs/research.md` §68).
//!
//! Entry: [`reorder`]. The triangle order is Tom Forsyth's
//! ("Linear-Speed Vertex Cache Optimisation", 2006): each vertex has a
//! score from its place in a cache of 32 and from how few triangles
//! still need it, and the triangle of the highest sum goes next.

use std::collections::BTreeMap;

use crate::jsonread::parse;

/// What [`reorder`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reordered {
    /// Indexed triangle primitives in the file.
    pub primitives: usize,
    /// How many of them were reordered (the rest share bytes with
    /// something that is not theirs to move, or are not plain data).
    pub reordered: usize,
}

const CACHE: usize = 32;

/// The triangles of `indices` in Forsyth's order.
fn cache_order(indices: &[u32], vertices: usize) -> Vec<u32> {
    let triangles = indices.len() / 3;
    // Scores: by place in the cache, and by triangles still to come.
    let by_place: Vec<f64> = (0..CACHE)
        .map(|p| {
            if p < 3 {
                0.75
            } else {
                let x = 1.0 - (p - 3) as f64 / (CACHE - 3) as f64;
                x * x.sqrt()
            }
        })
        .collect();
    let by_need = |live: u32| {
        if live == 0 {
            -1.0
        } else {
            2.0 / f64::from(live).sqrt()
        }
    };
    let mut live = vec![0u32; vertices];
    for &v in indices {
        live[v as usize] += 1;
    }
    // Each vertex's triangles, one run after another.
    let mut start = vec![0usize; vertices + 1];
    for v in 0..vertices {
        start[v + 1] = start[v] + live[v] as usize;
    }
    let mut fill = start.clone();
    let mut users = vec![0u32; indices.len()];
    for (i, &v) in indices.iter().enumerate() {
        users[fill[v as usize]] = (i / 3) as u32;
        fill[v as usize] += 1;
    }
    let mut place = vec![usize::MAX; vertices];
    let score_of = |live: u32, place: usize| {
        if live == 0 {
            -1.0
        } else {
            by_need(live) + if place < CACHE { by_place[place] } else { 0.0 }
        }
    };
    let mut score: Vec<f64> = (0..vertices)
        .map(|v| score_of(live[v], usize::MAX))
        .collect();
    let mut tri_score: Vec<f64> = (0..triangles)
        .map(|t| (0..3).map(|k| score[indices[3 * t + k] as usize]).sum())
        .collect();
    let mut done = vec![false; triangles];
    let mut cache: Vec<u32> = Vec::with_capacity(CACHE + 3);
    let mut out = Vec::with_capacity(indices.len());
    let mut best: Option<usize> = None;
    let mut scan = 0usize;
    for _ in 0..triangles {
        let t = match best {
            Some(t) => t,
            None => {
                // The best of all that are left.
                let mut top: Option<usize> = None;
                while scan < triangles && done[scan] {
                    scan += 1;
                }
                for t in scan..triangles {
                    if !done[t] && top.is_none_or(|b| tri_score[t] > tri_score[b]) {
                        top = Some(t);
                    }
                }
                match top {
                    Some(t) => t,
                    None => break,
                }
            }
        };
        done[t] = true;
        let tri = [indices[3 * t], indices[3 * t + 1], indices[3 * t + 2]];
        out.extend_from_slice(&tri);
        for &v in &tri {
            live[v as usize] -= 1;
        }
        // The triangle's vertices to the front of the cache.
        let mut next: Vec<u32> = tri.to_vec();
        next.extend(cache.iter().copied().filter(|v| !tri.contains(v)));
        cache = next;
        best = None;
        for (p, &v) in cache.iter().enumerate() {
            let v = v as usize;
            place[v] = if p < CACHE { p } else { usize::MAX };
            let now = score_of(live[v], place[v]);
            let by = now - score[v];
            score[v] = now;
            for &u in &users[start[v]..start[v + 1]] {
                let u = u as usize;
                if done[u] {
                    continue;
                }
                tri_score[u] += by;
                if best.is_none_or(|b| tri_score[u] > tri_score[b]) {
                    best = Some(u);
                }
            }
        }
        cache.truncate(CACHE);
    }
    out
}

/// `glb` with each indexed triangle primitive's triangles in a
/// vertex-cache order and its vertices (attributes and morph targets
/// alike) in the order of first use; the JSON is untouched. A
/// primitive is left as it is where its accessors are sparse or have
/// no view, where their counts differ, or where their bytes are also
/// read by something that is not moved with them.
pub fn reorder(glb: &[u8]) -> Result<(Vec<u8>, Reordered), String> {
    let word = |at: usize| -> Result<usize, String> {
        let b = glb.get(at..at + 4).ok_or("the GLB is cut short")?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    if !glb.starts_with(b"glTF") {
        return Err("not a GLB file".into());
    }
    let json_len = word(12)?;
    let json = glb.get(20..20 + json_len).ok_or("the GLB is cut short")?;
    let bin_at = 20 + json_len;
    let mut report = Reordered::default();
    if bin_at + 8 > glb.len() {
        return Ok((glb.to_vec(), report));
    }
    let bin_len = word(bin_at)?;
    let bin_from = bin_at + 8;
    let bin = glb
        .get(bin_from..bin_from + bin_len)
        .ok_or("the GLB is cut short")?;
    let doc = parse(json).map_err(|e| format!("the GLB's JSON: {e}"))?;
    if doc.get("buffers").items().len() != 1 || !doc.get("buffers").at(0).get("uri").is_null() {
        return Err("a GLB with one buffer, its own, is reordered".into());
    }
    if doc
        .get("extensionsUsed")
        .items()
        .iter()
        .any(|e| e.str().is_some_and(|s| s.contains("compression")))
    {
        return Err("the GLB is compressed already".into());
    }
    let accessors = doc.get("accessors").items();
    // Where an accessor's elements are: first byte, stride, element
    // size, count.
    let layout = |i: usize| -> Option<(usize, usize, usize, usize)> {
        let a = accessors.get(i)?;
        if !a.get("sparse").is_null() {
            return None;
        }
        let width = match a.get("componentType").index()? {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return None,
        };
        let n = match a.get("type").str()? {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" | "MAT2" => 4,
            "MAT3" => 9,
            "MAT4" => 16,
            _ => return None,
        };
        let view = doc.get("bufferViews").at(a.get("bufferView").index()?);
        let size = width * n;
        let stride = view.get("byteStride").index().unwrap_or(size);
        let at =
            view.get("byteOffset").index().unwrap_or(0) + a.get("byteOffset").index().unwrap_or(0);
        let count = a.get("count").index()?;
        let end = at + count.saturating_sub(1) * stride + size;
        (stride >= size && end <= bin.len()).then_some((at, stride, size, count))
    };
    // The primitives: their index accessor and their vertex accessors.
    struct Prim {
        indices: usize,
        vertex: Vec<usize>,
    }
    let mut prims: Vec<Prim> = Vec::new();
    for mesh in doc.get("meshes").items() {
        for p in mesh.get("primitives").items() {
            if p.get("mode").index().unwrap_or(4) != 4 {
                continue;
            }
            let Some(indices) = p.get("indices").index() else {
                continue;
            };
            report.primitives += 1;
            let mut vertex: Vec<usize> = Vec::new();
            for (_, a) in p.get("attributes").entries() {
                vertex.extend(a.index());
            }
            for t in p.get("targets").items() {
                for (_, a) in t.entries() {
                    vertex.extend(a.index());
                }
            }
            vertex.sort_unstable();
            vertex.dedup();
            prims.push(Prim { indices, vertex });
        }
    }
    // Primitives that share a vertex accessor move together.
    let mut group: Vec<usize> = (0..prims.len()).collect();
    fn find(group: &mut [usize], mut x: usize) -> usize {
        while group[x] != x {
            group[x] = group[group[x]];
            x = group[x];
        }
        x
    }
    let mut owner: BTreeMap<usize, usize> = BTreeMap::new();
    for (i, p) in prims.iter().enumerate() {
        for &a in &p.vertex {
            match owner.get(&a) {
                Some(&o) => {
                    let (x, y) = (find(&mut group, o), find(&mut group, i));
                    group[x.max(y)] = x.min(y);
                }
                None => {
                    owner.insert(a, i);
                }
            }
        }
    }
    let roots: Vec<usize> = (0..prims.len()).map(|i| find(&mut group, i)).collect();
    // A group is moved when all it reads is plain, of one count, and
    // read by nothing else: each accessor's bytes, and whose they are.
    let mut ok = vec![true; prims.len()];
    let mut spans: Vec<(usize, usize, usize)> = Vec::new();
    let mut index_group: BTreeMap<usize, usize> = BTreeMap::new();
    for (i, p) in prims.iter().enumerate() {
        let g = roots[i];
        match index_group.get(&p.indices) {
            Some(&other) if other != g => {
                ok[g] = false;
                ok[other] = false;
            }
            _ => {
                index_group.insert(p.indices, g);
            }
        }
        let index_ok = layout(p.indices).is_some_and(|(_, stride, size, count)| {
            stride == size
                && count % 3 == 0
                && accessors[p.indices].get("type").str() == Some("SCALAR")
                && accessors[p.indices].get("componentType").index() != Some(5126)
        });
        if !index_ok || p.vertex.is_empty() {
            ok[g] = false;
        }
        let mut count = None;
        for &a in &p.vertex {
            match layout(a) {
                Some((_, _, _, c)) if *count.get_or_insert(c) == c => {}
                _ => ok[g] = false,
            }
        }
    }
    let mut in_group = vec![usize::MAX; accessors.len()];
    for (i, p) in prims.iter().enumerate() {
        for &a in p.vertex.iter().chain([&p.indices]) {
            if a < in_group.len() {
                if in_group[a] != usize::MAX && in_group[a] != roots[i] {
                    ok[roots[i]] = false;
                    ok[in_group[a]] = false;
                }
                in_group[a] = roots[i];
            }
        }
    }
    for a in 0..accessors.len() {
        if let Some((at, stride, size, count)) = layout(a)
            && count > 0
        {
            spans.push((at, at + (count - 1) * stride + size, in_group[a]));
        } else if in_group[a] != usize::MAX {
            ok[in_group[a]] = false;
        }
    }
    spans.sort_unstable();
    let mut reach: Option<(usize, usize)> = None;
    for &(from, to, g) in &spans {
        if let Some((end, other)) = reach
            && from < end
            && other != g
        {
            for x in [g, other] {
                if x != usize::MAX {
                    ok[x] = false;
                }
            }
        }
        if reach.is_none_or(|(end, _)| to > end) {
            reach = Some((to, g));
        }
    }
    // Vertex counts must also agree across a group's primitives.
    let mut group_count: BTreeMap<usize, usize> = BTreeMap::new();
    for (i, p) in prims.iter().enumerate() {
        if let Some((_, _, _, c)) = p.vertex.first().and_then(|&a| layout(a))
            && *group_count.entry(roots[i]).or_insert(c) != c
        {
            ok[roots[i]] = false;
        }
    }
    let read_index = |a: usize, k: usize| -> u32 {
        let (at, _, size, _) = layout(a).unwrap_or((0, 1, 1, 0));
        let b = &bin[at + k * size..at + k * size + size];
        match size {
            1 => u32::from(b[0]),
            2 => u32::from(u16::from_le_bytes([b[0], b[1]])),
            _ => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        }
    };
    let mut out = glb.to_vec();
    for g in 0..prims.len() {
        if roots[g] != g || !ok[g] {
            continue;
        }
        let Some(&vertices) = group_count.get(&g) else {
            continue;
        };
        let members: Vec<usize> = (0..prims.len()).filter(|&i| roots[i] == g).collect();
        // Each index accessor once, in the order met.
        let mut index_accessors: Vec<usize> = Vec::new();
        for &i in &members {
            if !index_accessors.contains(&prims[i].indices) {
                index_accessors.push(prims[i].indices);
            }
        }
        let mut orders: Vec<Vec<u32>> = Vec::new();
        let mut valid = true;
        for &a in &index_accessors {
            let count = layout(a).map_or(0, |l| l.3);
            let indices: Vec<u32> = (0..count).map(|k| read_index(a, k)).collect();
            if indices.iter().any(|&v| v as usize >= vertices) {
                valid = false;
                break;
            }
            orders.push(cache_order(&indices, vertices));
        }
        if !valid {
            continue;
        }
        // New numbers in the order of first use; the unused after.
        let mut new_of = vec![u32::MAX; vertices];
        let mut old_of: Vec<u32> = Vec::with_capacity(vertices);
        for order in &orders {
            for &v in order {
                if new_of[v as usize] == u32::MAX {
                    new_of[v as usize] = old_of.len() as u32;
                    old_of.push(v);
                }
            }
        }
        for (v, new) in new_of.iter_mut().enumerate() {
            if *new == u32::MAX {
                *new = old_of.len() as u32;
                old_of.push(v as u32);
            }
        }
        for (&a, order) in index_accessors.iter().zip(&orders) {
            let (at, _, size, _) = layout(a).unwrap_or((0, 1, 1, 0));
            for (k, &v) in order.iter().enumerate() {
                let to = bin_from + at + k * size;
                let n = new_of[v as usize];
                out[to..to + size].copy_from_slice(&n.to_le_bytes()[..size]);
            }
        }
        let mut vertex: Vec<usize> = members
            .iter()
            .flat_map(|&i| prims[i].vertex.iter().copied())
            .collect();
        vertex.sort_unstable();
        vertex.dedup();
        for a in vertex {
            let (at, stride, size, _) = layout(a).unwrap_or((0, 1, 1, 0));
            for (new, &old) in old_of.iter().enumerate() {
                let from = at + old as usize * stride;
                let to = bin_from + at + new * stride;
                out[to..to + size].copy_from_slice(&bin[from..from + size]);
            }
        }
        report.reordered += members.len();
    }
    Ok((out, report))
}
