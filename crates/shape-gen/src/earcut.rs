//! Polygon triangulation: mapbox/earcut 3.0.2 as three.js ships it
//! (`src/extras/lib/earcut.js`), ported line for line so a shape's
//! triangles, and their order, are three.js's. Ear slicing over a doubly
//! linked ring, holes bridged into the outer ring (Eberly), a z-order hash
//! past 80 points, and the three fallbacks (filter collinear points, cure
//! local self-intersections, split the ring) for what slicing cannot
//! finish. Basic arithmetic only.

const NIL: usize = usize::MAX;

#[derive(Clone, Copy)]
struct Node {
    i: usize,
    x: f64,
    y: f64,
    prev: usize,
    next: usize,
    z: i32,
    prev_z: usize,
    next_z: usize,
    steiner: bool,
}

struct Ring {
    nodes: Vec<Node>,
}

/// Triangulates `data` (x y pairs: the outer ring, then each hole from its
/// index in `holes`), returning vertex indices, three per triangle.
pub fn earcut(data: &[f64], holes: &[usize]) -> Vec<usize> {
    let dim = 2;
    let has_holes = !holes.is_empty();
    let outer_len = if has_holes {
        holes[0] * dim
    } else {
        data.len()
    };
    let mut r = Ring { nodes: Vec::new() };
    let mut triangles = Vec::new();
    let mut outer = r.linked_list(data, 0, outer_len, dim, true);
    if outer == NIL || r.n(outer).next == r.n(outer).prev {
        return triangles;
    }
    if has_holes {
        outer = r.eliminate_holes(data, holes, outer, dim);
    }
    let (mut min_x, mut min_y, mut inv_size) = (0.0, 0.0, 0.0);
    if data.len() > 80 * dim {
        min_x = data[0];
        min_y = data[1];
        let (mut max_x, mut max_y) = (min_x, min_y);
        let mut i = dim;
        while i < outer_len {
            let (x, y) = (data[i], data[i + 1]);
            if x < min_x {
                min_x = x;
            }
            if y < min_y {
                min_y = y;
            }
            if x > max_x {
                max_x = x;
            }
            if y > max_y {
                max_y = y;
            }
            i += dim;
        }
        inv_size = f64::max(max_x - min_x, max_y - min_y);
        inv_size = if inv_size != 0.0 {
            32767.0 / inv_size
        } else {
            0.0
        };
    }
    r.earcut_linked(outer, &mut triangles, min_x, min_y, inv_size, 0);
    triangles
}

fn signed_area(data: &[f64], start: usize, end: usize, dim: usize) -> f64 {
    let mut sum = 0.0;
    let mut j = end - dim;
    let mut i = start;
    while i < end {
        sum += (data[j] - data[i]) * (data[i + 1] + data[j + 1]);
        j = i;
        i += dim;
    }
    sum
}

/// `x * invSize | 0`: JavaScript's ToInt32 of a value in 0..=32767.
fn to_i32(v: f64) -> i32 {
    v as i32
}

fn z_order(x: f64, y: f64, min_x: f64, min_y: f64, inv_size: f64) -> i32 {
    let mut x = to_i32((x - min_x) * inv_size);
    let mut y = to_i32((y - min_y) * inv_size);
    x = (x | (x << 8)) & 0x00FF_00FF;
    x = (x | (x << 4)) & 0x0F0F_0F0F;
    x = (x | (x << 2)) & 0x3333_3333;
    x = (x | (x << 1)) & 0x5555_5555;
    y = (y | (y << 8)) & 0x00FF_00FF;
    y = (y | (y << 4)) & 0x0F0F_0F0F;
    y = (y | (y << 2)) & 0x3333_3333;
    y = (y | (y << 1)) & 0x5555_5555;
    x | (y << 1)
}

#[allow(clippy::too_many_arguments)]
fn point_in_triangle(
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    cx: f64,
    cy: f64,
    px: f64,
    py: f64,
) -> bool {
    (cx - px) * (ay - py) >= (ax - px) * (cy - py)
        && (ax - px) * (by - py) >= (bx - px) * (ay - py)
        && (bx - px) * (cy - py) >= (cx - px) * (by - py)
}

#[allow(clippy::too_many_arguments)]
fn point_in_triangle_except_first(
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    cx: f64,
    cy: f64,
    px: f64,
    py: f64,
) -> bool {
    !(ax == px && ay == py) && point_in_triangle(ax, ay, bx, by, cx, cy, px, py)
}

fn sign(v: f64) -> i32 {
    if v > 0.0 {
        1
    } else if v < 0.0 {
        -1
    } else {
        0
    }
}

impl Ring {
    fn n(&self, i: usize) -> &Node {
        &self.nodes[i]
    }

    fn m(&mut self, i: usize) -> &mut Node {
        &mut self.nodes[i]
    }

    fn create(&mut self, i: usize, x: f64, y: f64) -> usize {
        self.nodes.push(Node {
            i,
            x,
            y,
            prev: NIL,
            next: NIL,
            z: 0,
            prev_z: NIL,
            next_z: NIL,
            steiner: false,
        });
        self.nodes.len() - 1
    }

    fn insert(&mut self, i: usize, x: f64, y: f64, last: usize) -> usize {
        let p = self.create(i, x, y);
        if last == NIL {
            self.m(p).prev = p;
            self.m(p).next = p;
        } else {
            let ln = self.n(last).next;
            self.m(p).next = ln;
            self.m(p).prev = last;
            self.m(ln).prev = p;
            self.m(last).next = p;
        }
        p
    }

    fn remove(&mut self, p: usize) {
        let Node {
            prev,
            next,
            prev_z,
            next_z,
            ..
        } = *self.n(p);
        self.m(next).prev = prev;
        self.m(prev).next = next;
        if prev_z != NIL {
            self.m(prev_z).next_z = next_z;
        }
        if next_z != NIL {
            self.m(next_z).prev_z = prev_z;
        }
    }

    fn equals(&self, a: usize, b: usize) -> bool {
        let (a, b) = (self.n(a), self.n(b));
        a.x == b.x && a.y == b.y
    }

    fn area(&self, p: usize, q: usize, r: usize) -> f64 {
        let (p, q, r) = (self.n(p), self.n(q), self.n(r));
        (q.y - p.y) * (r.x - q.x) - (q.x - p.x) * (r.y - q.y)
    }

    fn linked_list(
        &mut self,
        data: &[f64],
        start: usize,
        end: usize,
        dim: usize,
        clockwise: bool,
    ) -> usize {
        let mut last = NIL;
        if clockwise == (signed_area(data, start, end, dim) > 0.0) {
            let mut i = start;
            while i < end {
                last = self.insert(i / dim, data[i], data[i + 1], last);
                i += dim;
            }
        } else {
            let mut i = end as isize - dim as isize;
            while i >= start as isize {
                let iu = i as usize;
                last = self.insert(iu / dim, data[iu], data[iu + 1], last);
                i -= dim as isize;
            }
        }
        if last != NIL && self.equals(last, self.n(last).next) {
            self.remove(last);
            last = self.n(last).next;
        }
        last
    }

    fn filter_points(&mut self, start: usize, end: usize) -> usize {
        if start == NIL {
            return start;
        }
        let mut end = if end == NIL { start } else { end };
        let mut p = start;
        loop {
            let mut again = false;
            let (prev, next) = (self.n(p).prev, self.n(p).next);
            if !self.n(p).steiner && (self.equals(p, next) || self.area(prev, p, next) == 0.0) {
                self.remove(p);
                p = self.n(p).prev;
                end = p;
                if p == self.n(p).next {
                    break;
                }
                again = true;
            } else {
                p = self.n(p).next;
            }
            if !(again || p != end) {
                break;
            }
        }
        end
    }

    fn earcut_linked(
        &mut self,
        ear: usize,
        triangles: &mut Vec<usize>,
        min_x: f64,
        min_y: f64,
        inv_size: f64,
        pass: u8,
    ) {
        if ear == NIL {
            return;
        }
        let mut ear = ear;
        if pass == 0 && inv_size != 0.0 {
            self.index_curve(ear, min_x, min_y, inv_size);
        }
        let mut stop = ear;
        while self.n(ear).prev != self.n(ear).next {
            let prev = self.n(ear).prev;
            let next = self.n(ear).next;
            let is_ear = if inv_size != 0.0 {
                self.is_ear_hashed(ear, min_x, min_y, inv_size)
            } else {
                self.is_ear(ear)
            };
            if is_ear {
                triangles.extend([self.n(prev).i, self.n(ear).i, self.n(next).i]);
                self.remove(ear);
                ear = self.n(next).next;
                stop = self.n(next).next;
                continue;
            }
            ear = next;
            if ear == stop {
                if pass == 0 {
                    let e = self.filter_points(ear, NIL);
                    self.earcut_linked(e, triangles, min_x, min_y, inv_size, 1);
                } else if pass == 1 {
                    let e = self.filter_points(ear, NIL);
                    let e = self.cure_local_intersections(e, triangles);
                    self.earcut_linked(e, triangles, min_x, min_y, inv_size, 2);
                } else if pass == 2 {
                    self.split_earcut(ear, triangles, min_x, min_y, inv_size);
                }
                break;
            }
        }
    }

    fn is_ear(&self, ear: usize) -> bool {
        let (a, b, c) = (self.n(ear).prev, ear, self.n(ear).next);
        if self.area(a, b, c) >= 0.0 {
            return false;
        }
        let (na, nb, nc) = (*self.n(a), *self.n(b), *self.n(c));
        let (ax, bx, cx, ay, by, cy) = (na.x, nb.x, nc.x, na.y, nb.y, nc.y);
        let x0 = ax.min(bx).min(cx);
        let y0 = ay.min(by).min(cy);
        let x1 = ax.max(bx).max(cx);
        let y1 = ay.max(by).max(cy);
        let mut p = nc.next;
        while p != a {
            let np = self.n(p);
            if np.x >= x0
                && np.x <= x1
                && np.y >= y0
                && np.y <= y1
                && point_in_triangle_except_first(ax, ay, bx, by, cx, cy, np.x, np.y)
                && self.area(np.prev, p, np.next) >= 0.0
            {
                return false;
            }
            p = np.next;
        }
        true
    }

    fn is_ear_hashed(&self, ear: usize, min_x: f64, min_y: f64, inv_size: f64) -> bool {
        let (a, b, c) = (self.n(ear).prev, ear, self.n(ear).next);
        if self.area(a, b, c) >= 0.0 {
            return false;
        }
        let (na, nb, nc) = (*self.n(a), *self.n(b), *self.n(c));
        let (ax, bx, cx, ay, by, cy) = (na.x, nb.x, nc.x, na.y, nb.y, nc.y);
        let x0 = ax.min(bx).min(cx);
        let y0 = ay.min(by).min(cy);
        let x1 = ax.max(bx).max(cx);
        let y1 = ay.max(by).max(cy);
        let min_z = z_order(x0, y0, min_x, min_y, inv_size);
        let max_z = z_order(x1, y1, min_x, min_y, inv_size);
        let inside = |q: usize| {
            let nq = self.n(q);
            nq.x >= x0
                && nq.x <= x1
                && nq.y >= y0
                && nq.y <= y1
                && q != a
                && q != c
                && point_in_triangle_except_first(ax, ay, bx, by, cx, cy, nq.x, nq.y)
                && self.area(nq.prev, q, nq.next) >= 0.0
        };
        let mut p = nb.prev_z;
        let mut n = nb.next_z;
        while p != NIL && self.n(p).z >= min_z && n != NIL && self.n(n).z <= max_z {
            if inside(p) {
                return false;
            }
            p = self.n(p).prev_z;
            if inside(n) {
                return false;
            }
            n = self.n(n).next_z;
        }
        while p != NIL && self.n(p).z >= min_z {
            if inside(p) {
                return false;
            }
            p = self.n(p).prev_z;
        }
        while n != NIL && self.n(n).z <= max_z {
            if inside(n) {
                return false;
            }
            n = self.n(n).next_z;
        }
        true
    }

    fn cure_local_intersections(&mut self, start: usize, triangles: &mut Vec<usize>) -> usize {
        let mut start = start;
        let mut p = start;
        loop {
            let a = self.n(p).prev;
            let b = self.n(self.n(p).next).next;
            let pn = self.n(p).next;
            if !self.equals(a, b)
                && self.intersects(a, p, pn, b)
                && self.locally_inside(a, b)
                && self.locally_inside(b, a)
            {
                triangles.extend([self.n(a).i, self.n(p).i, self.n(b).i]);
                self.remove(p);
                let pn = self.n(p).next;
                self.remove(pn);
                p = b;
                start = b;
            }
            p = self.n(p).next;
            if p == start {
                break;
            }
        }
        self.filter_points(p, NIL)
    }

    fn split_earcut(
        &mut self,
        start: usize,
        triangles: &mut Vec<usize>,
        min_x: f64,
        min_y: f64,
        inv_size: f64,
    ) {
        let mut a = start;
        loop {
            let mut b = self.n(self.n(a).next).next;
            while b != self.n(a).prev {
                if self.n(a).i != self.n(b).i && self.is_valid_diagonal(a, b) {
                    let mut c = self.split_polygon(a, b);
                    let an = self.n(a).next;
                    a = self.filter_points(a, an);
                    let cn = self.n(c).next;
                    c = self.filter_points(c, cn);
                    self.earcut_linked(a, triangles, min_x, min_y, inv_size, 0);
                    self.earcut_linked(c, triangles, min_x, min_y, inv_size, 0);
                    return;
                }
                b = self.n(b).next;
            }
            a = self.n(a).next;
            if a == start {
                break;
            }
        }
    }

    fn eliminate_holes(
        &mut self,
        data: &[f64],
        holes: &[usize],
        outer: usize,
        dim: usize,
    ) -> usize {
        let mut queue = Vec::new();
        for (i, &h) in holes.iter().enumerate() {
            let start = h * dim;
            let end = if i + 1 < holes.len() {
                holes[i + 1] * dim
            } else {
                data.len()
            };
            let list = self.linked_list(data, start, end, dim, false);
            if list == NIL {
                continue;
            }
            if list == self.n(list).next {
                self.m(list).steiner = true;
            }
            queue.push(self.leftmost(list));
        }
        // JavaScript's sort is stable, as `sort_by`; a NaN comparison is 0.
        shape_core::sort::sort_by(&mut queue, |&a, &b| {
            let r = self.compare_xy_slope(a, b);
            r.partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut outer = outer;
        for h in queue {
            outer = self.eliminate_hole(h, outer);
        }
        outer
    }

    fn compare_xy_slope(&self, a: usize, b: usize) -> f64 {
        let (na, nb) = (self.n(a), self.n(b));
        let mut result = na.x - nb.x;
        if result == 0.0 {
            result = na.y - nb.y;
            if result == 0.0 {
                let (an, bn) = (self.n(na.next), self.n(nb.next));
                let a_slope = (an.y - na.y) / (an.x - na.x);
                let b_slope = (bn.y - nb.y) / (bn.x - nb.x);
                result = a_slope - b_slope;
            }
        }
        result
    }

    fn eliminate_hole(&mut self, hole: usize, outer: usize) -> usize {
        let Some(bridge) = self.find_hole_bridge(hole, outer) else {
            return outer;
        };
        let bridge_reverse = self.split_polygon(bridge, hole);
        let brn = self.n(bridge_reverse).next;
        self.filter_points(bridge_reverse, brn);
        let bn = self.n(bridge).next;
        self.filter_points(bridge, bn)
    }

    fn find_hole_bridge(&self, hole: usize, outer: usize) -> Option<usize> {
        let mut p = outer;
        let (hx, hy) = (self.n(hole).x, self.n(hole).y);
        let mut qx = f64::NEG_INFINITY;
        let mut m = NIL;
        if self.equals(hole, p) {
            return Some(p);
        }
        loop {
            let np = *self.n(p);
            let nn = *self.n(np.next);
            if self.equals(hole, np.next) {
                return Some(np.next);
            } else if hy <= np.y && hy >= nn.y && nn.y != np.y {
                let x = np.x + (hy - np.y) * (nn.x - np.x) / (nn.y - np.y);
                if x <= hx && x > qx {
                    qx = x;
                    m = if np.x < nn.x { p } else { np.next };
                    if x == hx {
                        return Some(m);
                    }
                }
            }
            p = np.next;
            if p == outer {
                break;
            }
        }
        if m == NIL {
            return None;
        }
        let stop = m;
        let (mx, my) = (self.n(m).x, self.n(m).y);
        let mut tan_min = f64::INFINITY;
        p = m;
        loop {
            let np = *self.n(p);
            if hx >= np.x
                && np.x >= mx
                && hx != np.x
                && point_in_triangle(
                    if hy < my { hx } else { qx },
                    hy,
                    mx,
                    my,
                    if hy < my { qx } else { hx },
                    hy,
                    np.x,
                    np.y,
                )
            {
                let tan = (hy - np.y).abs() / (hx - np.x);
                let nm = self.n(m);
                if self.locally_inside(p, hole)
                    && (tan < tan_min
                        || (tan == tan_min
                            && (np.x > nm.x
                                || (np.x == nm.x && self.sector_contains_sector(m, p)))))
                {
                    m = p;
                    tan_min = tan;
                }
            }
            p = np.next;
            if p == stop {
                break;
            }
        }
        Some(m)
    }

    fn sector_contains_sector(&self, m: usize, p: usize) -> bool {
        let (nm, np) = (self.n(m), self.n(p));
        self.area(nm.prev, m, np.prev) < 0.0 && self.area(np.next, m, nm.next) < 0.0
    }

    fn index_curve(&mut self, start: usize, min_x: f64, min_y: f64, inv_size: f64) {
        let mut p = start;
        loop {
            if self.n(p).z == 0 {
                let (x, y) = (self.n(p).x, self.n(p).y);
                self.m(p).z = z_order(x, y, min_x, min_y, inv_size);
            }
            let (prev, next) = (self.n(p).prev, self.n(p).next);
            self.m(p).prev_z = prev;
            self.m(p).next_z = next;
            p = next;
            if p == start {
                break;
            }
        }
        let pz = self.n(p).prev_z;
        self.m(pz).next_z = NIL;
        self.m(p).prev_z = NIL;
        self.sort_linked(p);
    }

    fn sort_linked(&mut self, list: usize) {
        let mut list = list;
        let mut in_size = 1;
        loop {
            let mut p = list;
            list = NIL;
            let mut tail = NIL;
            let mut num_merges = 0;
            while p != NIL {
                num_merges += 1;
                let mut q = p;
                let mut p_size = 0;
                for _ in 0..in_size {
                    p_size += 1;
                    q = self.n(q).next_z;
                    if q == NIL {
                        break;
                    }
                }
                let mut q_size = in_size;
                while p_size > 0 || (q_size > 0 && q != NIL) {
                    let e;
                    if p_size != 0 && (q_size == 0 || q == NIL || self.n(p).z <= self.n(q).z) {
                        e = p;
                        p = self.n(p).next_z;
                        p_size -= 1;
                    } else {
                        e = q;
                        q = self.n(q).next_z;
                        q_size -= 1;
                    }
                    if tail != NIL {
                        self.m(tail).next_z = e;
                    } else {
                        list = e;
                    }
                    self.m(e).prev_z = tail;
                    tail = e;
                }
                p = q;
            }
            self.m(tail).next_z = NIL;
            in_size *= 2;
            if num_merges <= 1 {
                break;
            }
        }
    }

    fn leftmost(&self, start: usize) -> usize {
        let mut p = start;
        let mut leftmost = start;
        loop {
            let (np, nl) = (self.n(p), self.n(leftmost));
            if np.x < nl.x || (np.x == nl.x && np.y < nl.y) {
                leftmost = p;
            }
            p = np.next;
            if p == start {
                break;
            }
        }
        leftmost
    }

    fn is_valid_diagonal(&self, a: usize, b: usize) -> bool {
        let (na, nb) = (self.n(a), self.n(b));
        self.n(na.next).i != nb.i
            && self.n(na.prev).i != nb.i
            && !self.intersects_polygon(a, b)
            && ((self.locally_inside(a, b)
                && self.locally_inside(b, a)
                && self.middle_inside(a, b)
                && (self.area(na.prev, a, nb.prev) != 0.0 || self.area(a, nb.prev, b) != 0.0))
                || (self.equals(a, b)
                    && self.area(na.prev, a, na.next) > 0.0
                    && self.area(nb.prev, b, nb.next) > 0.0))
    }

    fn intersects(&self, p1: usize, q1: usize, p2: usize, q2: usize) -> bool {
        let o1 = sign(self.area(p1, q1, p2));
        let o2 = sign(self.area(p1, q1, q2));
        let o3 = sign(self.area(p2, q2, p1));
        let o4 = sign(self.area(p2, q2, q1));
        if o1 != o2 && o3 != o4 {
            return true;
        }
        (o1 == 0 && self.on_segment(p1, p2, q1))
            || (o2 == 0 && self.on_segment(p1, q2, q1))
            || (o3 == 0 && self.on_segment(p2, p1, q2))
            || (o4 == 0 && self.on_segment(p2, q1, q2))
    }

    fn on_segment(&self, p: usize, q: usize, r: usize) -> bool {
        let (p, q, r) = (self.n(p), self.n(q), self.n(r));
        q.x <= p.x.max(r.x) && q.x >= p.x.min(r.x) && q.y <= p.y.max(r.y) && q.y >= p.y.min(r.y)
    }

    fn intersects_polygon(&self, a: usize, b: usize) -> bool {
        let (ai, bi) = (self.n(a).i, self.n(b).i);
        let mut p = a;
        loop {
            let np = self.n(p);
            let nn = self.n(np.next);
            if np.i != ai
                && nn.i != ai
                && np.i != bi
                && nn.i != bi
                && self.intersects(p, np.next, a, b)
            {
                return true;
            }
            p = np.next;
            if p == a {
                break;
            }
        }
        false
    }

    fn locally_inside(&self, a: usize, b: usize) -> bool {
        let na = self.n(a);
        if self.area(na.prev, a, na.next) < 0.0 {
            self.area(a, b, na.next) >= 0.0 && self.area(a, na.prev, b) >= 0.0
        } else {
            self.area(a, b, na.prev) < 0.0 || self.area(a, na.next, b) < 0.0
        }
    }

    fn middle_inside(&self, a: usize, b: usize) -> bool {
        let mut p = a;
        let mut inside = false;
        let px = (self.n(a).x + self.n(b).x) / 2.0;
        let py = (self.n(a).y + self.n(b).y) / 2.0;
        loop {
            let np = self.n(p);
            let nn = self.n(np.next);
            if ((np.y > py) != (nn.y > py))
                && nn.y != np.y
                && (px < (nn.x - np.x) * (py - np.y) / (nn.y - np.y) + np.x)
            {
                inside = !inside;
            }
            p = np.next;
            if p == a {
                break;
            }
        }
        inside
    }

    fn split_polygon(&mut self, a: usize, b: usize) -> usize {
        let (ai, ax, ay) = (self.n(a).i, self.n(a).x, self.n(a).y);
        let (bi, bx, by) = (self.n(b).i, self.n(b).x, self.n(b).y);
        let a2 = self.create(ai, ax, ay);
        let b2 = self.create(bi, bx, by);
        let an = self.n(a).next;
        let bp = self.n(b).prev;
        self.m(a).next = b;
        self.m(b).prev = a;
        self.m(a2).next = an;
        self.m(an).prev = a2;
        self.m(b2).next = a2;
        self.m(a2).prev = b2;
        self.m(bp).next = b2;
        self.m(b2).prev = bp;
        b2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_square_with_a_hole_takes_eight_triangles() {
        let data = [
            0.0, 0.0, 4.0, 0.0, 4.0, 4.0, 0.0, 4.0, 1.0, 1.0, 1.0, 3.0, 3.0, 3.0, 3.0, 1.0,
        ];
        let t = earcut(&data, &[4]);
        assert_eq!(t.len() / 3, 8);
        // The triangles cover the square less the hole: 16 - 4.
        let area: f64 = t
            .chunks(3)
            .map(|c| {
                let p = |i: usize| (data[2 * c[i]], data[2 * c[i] + 1]);
                let (a, b, c) = (p(0), p(1), p(2));
                ((b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1)).abs() / 2.0
            })
            .sum();
        assert!((area - 12.0).abs() < 1e-12);
    }
}
