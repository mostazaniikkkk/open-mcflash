//! Bézier curve fitting to polylines (Schneider's algorithm, "An
//! Algorithm for Automatically Fitting Digitized Curves", Graphics Gems I).
//! Used so that shapes resulting from boolean operations get
//! editable curves again, and for "Optimize Curves" and "Smooth".

use crate::geom::*;
use crate::model::{dist, BezPath, Node};

fn sub(a: Pt, b: Pt) -> Pt {
    (a.0 - b.0, a.1 - b.1)
}
fn add(a: Pt, b: Pt) -> Pt {
    (a.0 + b.0, a.1 + b.1)
}
fn mul(a: Pt, s: f32) -> Pt {
    (a.0 * s, a.1 * s)
}
fn dot(a: Pt, b: Pt) -> f32 {
    a.0 * b.0 + a.1 * b.1
}
fn neg(a: Pt) -> Pt {
    (-a.0, -a.1)
}
fn norm(a: Pt) -> Pt {
    let l = (a.0 * a.0 + a.1 * a.1).sqrt();
    if l < 1e-9 {
        (0.0, 0.0)
    } else {
        (a.0 / l, a.1 / l)
    }
}

fn bez(b: &[Pt; 4], u: f32) -> Pt {
    cubic_point(b[0], b[1], b[2], b[3], u)
}

fn bez_d1(b: &[Pt; 4], u: f32) -> Pt {
    let v = 1.0 - u;
    let a = mul(sub(b[1], b[0]), 3.0 * v * v);
    let c = mul(sub(b[2], b[1]), 6.0 * v * u);
    let d = mul(sub(b[3], b[2]), 3.0 * u * u);
    add(add(a, c), d)
}

fn bez_d2(b: &[Pt; 4], u: f32) -> Pt {
    let a = mul(add(sub(b[2], mul(b[1], 2.0)), b[0]), 6.0 * (1.0 - u));
    let c = mul(add(sub(b[3], mul(b[2], 2.0)), b[1]), 6.0 * u);
    add(a, c)
}

fn chord(d: &[Pt]) -> Vec<f32> {
    let mut u = vec![0.0; d.len()];
    for i in 1..d.len() {
        u[i] = u[i - 1] + dist(d[i], d[i - 1]);
    }
    let total = *u.last().unwrap();
    if total > 0.0 {
        u.iter_mut().for_each(|x| *x /= total);
    }
    u
}

fn generate(d: &[Pt], u: &[f32], t1: Pt, t2: Pt) -> [Pt; 4] {
    let (first, last) = (d[0], d[d.len() - 1]);
    let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (i, &ui) in u.iter().enumerate() {
        let v = 1.0 - ui;
        let (b0, b1, b2, b3) = (v * v * v, 3.0 * v * v * ui, 3.0 * v * ui * ui, ui * ui * ui);
        let a0 = mul(t1, b1);
        let a1 = mul(t2, b2);
        c00 += dot(a0, a0);
        c01 += dot(a0, a1);
        c11 += dot(a1, a1);
        let tmp = sub(d[i], add(mul(first, b0 + b1), mul(last, b2 + b3)));
        x0 += dot(a0, tmp);
        x1 += dot(a1, tmp);
    }
    let det = c00 * c11 - c01 * c01;
    let (mut al, mut ar) = if det.abs() > 1e-12 { ((x0 * c11 - x1 * c01) / det, (c00 * x1 - c01 * x0) / det) } else { (0.0, 0.0) };
    let seg = dist(first, last);
    if al < 1e-6 * seg || ar < 1e-6 * seg {
        al = seg / 3.0;
        ar = seg / 3.0;
    }
    [first, add(first, mul(t1, al)), add(last, mul(t2, ar)), last]
}

fn max_err(d: &[Pt], b: &[Pt; 4], u: &[f32]) -> (f32, usize) {
    let n = d.len();
    let mut best = (0.0, n / 2);
    for i in 1..n - 1 {
        let p = bez(b, u[i]);
        let e = (p.0 - d[i].0).powi(2) + (p.1 - d[i].1).powi(2);
        if e >= best.0 {
            best = (e, i);
        }
    }
    (best.0, best.1.clamp(1, n - 2))
}

fn reparam(d: &[Pt], u: &[f32], b: &[Pt; 4]) -> Vec<f32> {
    d.iter()
        .zip(u)
        .map(|(p, &ui)| {
            let q = sub(bez(b, ui), *p);
            let q1 = bez_d1(b, ui);
            let q2 = bez_d2(b, ui);
            let den = dot(q1, q1) + dot(q, q2);
            if den.abs() < 1e-12 {
                ui
            } else {
                (ui - dot(q, q1) / den).clamp(0.0, 1.0)
            }
        })
        .collect()
}

fn fit_cubic(d: &[Pt], t1: Pt, t2: Pt, err2: f32, out: &mut Vec<[Pt; 4]>, depth: u32) {
    if d.len() == 2 {
        let l = dist(d[0], d[1]) / 3.0;
        out.push([d[0], add(d[0], mul(t1, l)), add(d[1], mul(t2, l)), d[1]]);
        return;
    }
    let mut u = chord(d);
    let mut b = generate(d, &u, t1, t2);
    let (mut e, mut split) = max_err(d, &b, &u);
    if e < err2 || depth > 14 {
        out.push(b);
        return;
    }
    if e < err2 * 16.0 {
        for _ in 0..6 {
            u = reparam(d, &u, &b);
            b = generate(d, &u, t1, t2);
            let r = max_err(d, &b, &u);
            e = r.0;
            split = r.1;
            if e < err2 {
                out.push(b);
                return;
            }
        }
    }
    let mut tc = norm(sub(d[split - 1], d[split + 1]));
    if tc == (0.0, 0.0) {
        tc = norm(sub(d[split - 1], d[split]));
    }
    fit_cubic(&d[..=split], t1, tc, err2, out, depth + 1);
    fit_cubic(&d[split..], neg(tc), t2, err2, out, depth + 1);
}

fn dedup(pts: &[Pt]) -> Vec<Pt> {
    let mut v: Vec<Pt> = Vec::with_capacity(pts.len());
    for &p in pts {
        if v.last().map_or(true, |&l| dist(l, p) > 1e-4) {
            v.push(p);
        }
    }
    v
}

/// Fits curves to a polyline. `err` = maximum error (document units);
/// vertices turning more than `corner_deg` are kept as corners.
pub fn fit_path(pts: &[Pt], closed: bool, err: f32, corner_deg: f32) -> BezPath {
    let mut p = dedup(pts);
    let mut closed = closed;
    if closed && p.len() > 1 && dist(p[0], *p.last().unwrap()) < 1e-4 {
        p.pop();
    }
    if closed && p.len() < 3 {
        closed = false;
    }
    if p.len() < 2 {
        return BezPath { nodes: p.into_iter().map(Node::corner).collect(), closed: false };
    }
    // Remove noise without losing the shape.
    p = if closed {
        let mut q = p.clone();
        q.push(p[0]);
        let mut s = simplify(&q, err * 0.2);
        s.pop();
        if s.len() >= 3 {
            s
        } else {
            p
        }
    } else {
        simplify(&p, err * 0.2)
    };
    let n = p.len();
    let cos_lim = corner_deg.to_radians().cos();
    let is_corner = |i: usize| -> bool {
        if !closed && (i == 0 || i == n - 1) {
            return true;
        }
        let (a, b, c) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
        dot(norm(sub(b, a)), norm(sub(c, b))) < cos_lim
    };
    let corners: Vec<usize> = (0..n).filter(|&i| is_corner(i)).collect();
    let err2 = err * err;

    let mut nodes: Vec<Node> = Vec::new();
    let push_run =|run: &[Pt], t1: Pt, t2: Pt, nodes: &mut Vec<Node>| {
        let mut segs = Vec::new();
        fit_cubic(run, t1, t2, err2, &mut segs, 0);
        if nodes.is_empty() {
            nodes.push(Node { p: segs[0][0], i: segs[0][0], o: segs[0][1], smooth: false });
        } else {
            nodes.last_mut().unwrap().o = segs[0][1];
        }
        for (k, s) in segs.iter().enumerate() {
            nodes.push(Node { p: s[3], i: s[2], o: s[3], smooth: k + 1 < segs.len() });
            if k + 1 < segs.len() {
                nodes.last_mut().unwrap().o = segs[k + 1][1];
            }
        }
    };

    if closed && corners.is_empty() {
        let mut run = p.clone();
        run.push(p[0]);
        let t = norm(sub(p[1], p[n - 1]));
        push_run(&run, t, neg(t), &mut nodes);
        let last = nodes.pop().unwrap();
        nodes[0].i = last.i;
        nodes[0].smooth = true;
        collapse_lines(&mut nodes, true, err);
        return BezPath { nodes, closed: true };
    }

    // Traversal from the first corner (closed) or from the start (open).
    let seq: Vec<Pt> = if closed { (0..=n).map(|k| p[(corners[0] + k) % n]).collect() } else { p.clone() };
    let is_c: Vec<bool> = if closed { (0..=n).map(|k| is_corner((corners[0] + k) % n)).collect() } else { (0..n).map(is_corner).collect() };
    let mut start = 0;
    for k in 1..seq.len() {
        if is_c[k] || k == seq.len() - 1 {
            let run = &seq[start..=k];
            let t1 = norm(sub(run[1], run[0]));
            let t2 = norm(sub(run[run.len() - 2], run[run.len() - 1]));
            push_run(run, t1, t2, &mut nodes);
            start = k;
        }
    }
    if closed {
        let last = nodes.pop().unwrap();
        nodes[0].i = last.i;
    }
    collapse_lines(&mut nodes, closed, err);
    BezPath { nodes, closed }
}

/// Nearly straight segments become lines (no handles).
fn collapse_lines(nodes: &mut [Node], closed: bool, err: f32) {
    let n = nodes.len();
    let segs: Vec<(usize, usize)> = (1..n).map(|k| (k - 1, k)).chain(if closed && n > 2 { Some((n - 1, 0)) } else { None }).collect();
    for (a, b) in segs {
        let (pa, pb) = (nodes[a].p, nodes[b].p);
        let off = |h: Pt| dist_to_segment(h, pa, pb);
        if off(nodes[a].o) < err * 0.05 && off(nodes[b].i) < err * 0.05 {
            nodes[a].o = pa;
            nodes[b].i = pb;
        }
    }
    for nd in nodes.iter_mut() {
        if !nd.has_in() || !nd.has_out() {
            nd.smooth = false;
        }
    }
}

/// Splits a path at its corners: so each line segment is selected
/// separately, as in Flash (one side of a rectangle is one piece).
pub fn split_at_corners(path: &BezPath) -> Vec<BezPath> {
    let n = path.nodes.len();
    if n < 2 {
        return vec![path.clone()];
    }
    let corner = |i: usize| !path.nodes[i].smooth;
    let corners: Vec<usize> = (0..n).filter(|&i| corner(i)).collect();
    if path.closed {
        if corners.is_empty() {
            return vec![path.clone()];
        }
        let mut out = Vec::new();
        for (ci, &c) in corners.iter().enumerate() {
            let next = corners[(ci + 1) % corners.len()];
            let mut nodes = vec![path.nodes[c]];
            let mut k = c;
            loop {
                k = (k + 1) % n;
                nodes.push(path.nodes[k]);
                if k == next {
                    break;
                }
            }
            finish_piece(&mut nodes);
            out.push(BezPath { nodes, closed: false });
        }
        return out;
    }
    let mut out = Vec::new();
    let mut start = 0;
    for k in 1..n {
        if corner(k) || k == n - 1 {
            let mut nodes = path.nodes[start..=k].to_vec();
            finish_piece(&mut nodes);
            out.push(BezPath { nodes, closed: false });
            start = k;
        }
    }
    out
}

fn finish_piece(nodes: &mut [Node]) {
    let l = nodes.len() - 1;
    nodes[0].i = nodes[0].p;
    nodes[0].smooth = false;
    nodes[l].o = nodes[l].p;
    nodes[l].smooth = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_circle_with_few_smooth_nodes() {
        let pts: Vec<Pt> = (0..200).map(|i| {
            let a = i as f32 / 200.0 * std::f32::consts::TAU;
            (100.0 * a.cos(), 100.0 * a.sin())
        }).collect();
        let p = fit_path(&pts, true, 0.2, 40.0);
        assert!(p.closed);
        assert!(p.nodes.len() <= 12, "nodos: {}", p.nodes.len());
        assert!(p.nodes.iter().all(|n| n.smooth));
    }

    #[test]
    fn keeps_square_corners_as_lines() {
        let mut pts = Vec::new();
        for i in 0..10 { pts.push((i as f32 * 10.0, 0.0)); }
        for i in 0..10 { pts.push((100.0, i as f32 * 10.0)); }
        for i in 0..10 { pts.push((100.0 - i as f32 * 10.0, 100.0)); }
        for i in 0..10 { pts.push((0.0, 100.0 - i as f32 * 10.0)); }
        let p = fit_path(&pts, true, 0.2, 40.0);
        assert_eq!(p.nodes.len(), 4);
        assert!(p.nodes.iter().all(|n| !n.has_in() && !n.has_out()));
        assert_eq!(split_at_corners(&p).len(), 4);
    }
}
