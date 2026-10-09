//! Flash shape tween, with shape hints.
//!
//! Fills and strokes of the start frame are paired with those of the
//! end frame by stacking order; each outline is resampled by arc
//! length and aligned with its partner (rotating the ring to minimize
//! displacement, or segment by segment between shape hints). Anything without
//! a partner shrinks (or grows) from its center.

use crate::geom::*;
use crate::model::*;

fn signed_area(r: &[Pt]) -> f32 {
    let mut a = 0.0;
    for i in 0..r.len() {
        let (p, q) = (r[i], r[(i + 1) % r.len()]);
        a += p.0 * q.1 - q.0 * p.1;
    }
    a / 2.0
}

fn length(r: &[Pt], closed: bool) -> f32 {
    let mut l: f32 = r.windows(2).map(|w| dist(w[0], w[1])).sum();
    if closed && r.len() > 1 {
        l += dist(r[r.len() - 1], r[0]);
    }
    l
}

fn centroid(r: &[Pt]) -> Pt {
    let n = r.len().max(1) as f32;
    (r.iter().map(|p| p.0).sum::<f32>() / n, r.iter().map(|p| p.1).sum::<f32>() / n)
}

/// `n` points equally spaced by arc length.
fn resample(r: &[Pt], closed: bool, n: usize) -> Vec<Pt> {
    if r.is_empty() {
        return vec![(0.0, 0.0); n];
    }
    if r.len() == 1 {
        return vec![r[0]; n];
    }
    let mut pts = r.to_vec();
    if closed {
        pts.push(r[0]);
    }
    let total = length(&pts, false);
    if total < 1e-6 {
        return vec![r[0]; n];
    }
    let steps = if closed { n } else { n - 1 };
    let step = total / steps as f32;
    let mut out = Vec::with_capacity(n);
    let (mut seg, mut acc) = (0usize, 0.0f32);
    for k in 0..n {
        let target = (k as f32 * step).min(total);
        while seg + 1 < pts.len() - 1 && acc + dist(pts[seg], pts[seg + 1]) < target {
            acc += dist(pts[seg], pts[seg + 1]);
            seg += 1;
        }
        let l = dist(pts[seg], pts[seg + 1]).max(1e-9);
        let u = ((target - acc) / l).clamp(0.0, 1.0);
        out.push((lerp(pts[seg].0, pts[seg + 1].0, u), lerp(pts[seg].1, pts[seg + 1].1, u)));
    }
    out
}

/// Rotates ring `b` so it resembles `a` as closely as possible.
fn align_ring(a: &[Pt], b: &[Pt]) -> Vec<Pt> {
    let n = a.len();
    let mut best = (f32::INFINITY, 0);
    for s in 0..n {
        let d: f32 = (0..n).step_by(2).map(|i| { let q = b[(i + s) % n]; (a[i].0 - q.0).powi(2) + (a[i].1 - q.1).powi(2) }).sum();
        if d < best.0 {
            best = (d, s);
        }
    }
    (0..n).map(|i| b[(i + best.1) % n]).collect()
}

fn nearest_index(r: &[Pt], p: Pt) -> (usize, f32) {
    r.iter().enumerate().map(|(i, q)| (i, dist(*q, p))).fold((0, f32::INFINITY), |a, b| if b.1 < a.1 { b } else { a })
}

/// Cyclic segment of `r` from `i` to `j` (inclusive).
fn cyc(r: &[Pt], i: usize, j: usize) -> Vec<Pt> {
    let n = r.len();
    let mut v = vec![r[i]];
    let mut k = i;
    while k != j {
        k = (k + 1) % n;
        v.push(r[k]);
    }
    v
}

fn corners(r: &[Pt]) -> Vec<Pt> {
    let n = r.len();
    let mut out: Vec<Pt> = Vec::new();
    for i in 0..n {
        let (a, b, c) = (r[(i + n - 1) % n], r[i], r[(i + 1) % n]);
        let (u, v) = ((b.0 - a.0, b.1 - a.1), (c.0 - b.0, c.1 - b.1));
        let (lu, lv) = (u.0.hypot(u.1), v.0.hypot(v.1));
        if lu < 1e-6 || lv < 1e-6 {
            continue;
        }
        let cos = (u.0 * v.0 + u.1 * v.1) / (lu * lv);
        if cos < 0.8 && !out.iter().any(|q| dist(*q, b) < 1.0) {
            out.push(b);
        }
    }
    out
}

/// Corners of A paired with those of B (with the index offset that fits best).
fn corner_hints(a: &[Pt], b: &[Pt]) -> Vec<(Pt, Pt)> {
    let (ca, cb) = (corners(a), corners(b));
    if ca.len() < 2 || ca.len() != cb.len() {
        return Vec::new();
    }
    let n = ca.len();
    let (ga, gb) = (centroid(a), centroid(b));
    let rel = |p: Pt, g: Pt| (p.0 - g.0, p.1 - g.1);
    let best = (0..n)
        .min_by(|&s, &t| {
            let cost = |s: usize| -> f32 { (0..n).map(|i| { let (x, y) = (rel(ca[i], ga), rel(cb[(i + s) % n], gb)); (x.0 - y.0).powi(2) + (x.1 - y.1).powi(2) }).sum() };
            cost(s).partial_cmp(&cost(t)).unwrap()
        })
        .unwrap();
    (0..n).map(|i| (ca[i], cb[(i + best) % n])).collect()
}

/// Pairs two closed rings respecting the shape hints.
fn pair_rings(a: &[Pt], b: &[Pt], hints: &[(Pt, Pt)]) -> (Vec<Pt>, Vec<Pt>) {
    let n = ((length(a, true).max(length(b, true)) / 2.5) as usize).clamp(24, 200);
    let mut b = b.to_vec();
    if signed_area(a).signum() != signed_area(&b).signum() {
        b.reverse();
    }
    // No user hints: if both outlines have the same corners,
    // they are paired corner to corner (rectangle → rectangle stays clean).
    let auto;
    let hints = if hints.is_empty() {
        auto = corner_hints(a, &b);
        &auto[..]
    } else {
        hints
    };
    // Hints that fall on this outline.
    let dense_a = resample(a, true, n * 4);
    let dense_b = resample(&b, true, n * 4);
    let mut idx: Vec<(usize, usize)> = hints
        .iter()
        .filter_map(|(ha, hb)| {
            let (ia, da) = nearest_index(&dense_a, *ha);
            let (ib, db) = nearest_index(&dense_b, *hb);
            (da < 15.0 && db < 15.0).then_some((ia, ib))
        })
        .collect();
    idx.sort();
    idx.dedup_by_key(|x| x.0);
    if idx.is_empty() {
        let ra = resample(a, true, n);
        let rb = resample(&b, true, n);
        return (ra.clone(), align_ring(&ra, &rb));
    }
    if idx.len() == 1 {
        // One hint fixes the starting point of both rings.
        let (ia, ib) = idx[0];
        let sa: Vec<Pt> = (0..dense_a.len()).map(|k| dense_a[(ia + k) % dense_a.len()]).collect();
        let sb: Vec<Pt> = (0..dense_b.len()).map(|k| dense_b[(ib + k) % dense_b.len()]).collect();
        return (resample(&sa, true, n), resample(&sb, true, n));
    }
    // Several hints: segment by segment, each with its proportional points.
    let total = length(&dense_a, true);
    let (mut ra, mut rb) = (Vec::new(), Vec::new());
    for k in 0..idx.len() {
        let (ia0, ib0) = idx[k];
        let (ia1, ib1) = idx[(k + 1) % idx.len()];
        let pa = cyc(&dense_a, ia0, ia1);
        let pb = cyc(&dense_b, ib0, ib1);
        let m = ((length(&pa, false) / total * n as f32).round() as usize).max(2);
        let sa = resample(&pa, false, m + 1);
        let sb = resample(&pb, false, m + 1);
        ra.extend_from_slice(&sa[..m]);
        rb.extend_from_slice(&sb[..m]);
    }
    (ra, rb)
}

fn world_contours(s: &Shape) -> Vec<(Vec<Pt>, bool)> {
    flatten(&s.outline())
        .into_iter()
        .map(|(pts, closed)| (pts.iter().map(|p| s.m.apply(p.0, p.1)).collect::<Vec<_>>(), closed))
        .filter(|(p, _)| p.len() >= 2)
        .collect()
}

fn mk_shape(base: &Shape, rings: Vec<(Vec<Pt>, bool)>, fill: Option<Rgba>, stroke: Option<Rgba>, width: f32) -> Shape {
    let paths = rings.into_iter().map(|(pts, closed)| BezPath { nodes: pts.into_iter().map(Node::corner).collect(), closed }).collect();
    Shape { id: base.id, geom: Geometry::Bezier { paths }, fill, stroke, stroke_width: width, m: Matrix::IDENTITY, raw: base.raw, tag: base.tag, grad: None, bmp: base.bmp_world() }
}

fn lerp_color(a: Option<Rgba>, b: Option<Rgba>, t: f32) -> Option<Rgba> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.lerp(&y, t)),
        (Some(x), None) => Some(Rgba { a: (x.a as f32 * (1.0 - t)) as u8, ..x }),
        (None, Some(y)) => Some(Rgba { a: (y.a as f32 * t) as u8, ..y }),
        _ => None,
    }
}

fn morph_pair(a: &Shape, b: &Shape, t: f32, hints: &[(Pt, Pt)]) -> Shape {
    let closed = a.fill.is_some();
    let sort = |mut v: Vec<(Vec<Pt>, bool)>| {
        if closed {
            v.retain(|c| c.1);
            v.sort_by(|x, y| signed_area(&y.0).abs().partial_cmp(&signed_area(&x.0).abs()).unwrap());
        }
        v
    };
    let ca = sort(world_contours(a));
    let cb = sort(world_contours(b));
    let count = ca.len().max(cb.len());
    let mut rings = Vec::new();
    for i in 0..count {
        let (ra, rb) = match (ca.get(i), cb.get(i)) {
            (Some(x), Some(y)) => {
                if closed {
                    pair_rings(&x.0, &y.0, hints)
                } else {
                    let n = ((length(&x.0, false).max(length(&y.0, false)) / 2.5) as usize).clamp(8, 200);
                    let ra = resample(&x.0, false, n);
                    let mut rb = resample(&y.0, false, n);
                    if dist(ra[0], rb[0]) + dist(ra[n - 1], rb[n - 1]) > dist(ra[0], rb[n - 1]) + dist(ra[n - 1], rb[0]) {
                        rb.reverse();
                    }
                    (ra, rb)
                }
            }
            // Outline without a partner: appears from or vanishes into a point.
            (Some(x), None) => {
                let r = resample(&x.0, closed, 32);
                let c = centroid(&r);
                (r, vec![c; 32])
            }
            (None, Some(y)) => {
                let r = resample(&y.0, closed, 32);
                let c = centroid(&r);
                (vec![c; 32], r)
            }
            _ => continue,
        };
        let pts = ra.iter().zip(&rb).map(|(p, q)| (lerp(p.0, q.0, t), lerp(p.1, q.1, t))).collect();
        rings.push((pts, closed));
    }
    let wa = a.stroke_width * a.m.avg_scale();
    let wb = b.stroke_width * b.m.avg_scale();
    let mut out = mk_shape(a, rings, lerp_color(a.fill, b.fill, t), lerp_color(a.stroke, b.stroke, t), lerp(wa, wb, t));
    // The resulting shape is in document coordinates: so is the gradient.
    out.grad = match (a.grad_world(), b.grad_world()) {
        (Some(x), Some(y)) => Some(x.lerp(&y, t)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        _ => None,
    };
    if let Some(g) = &out.grad {
        out.fill = out.fill.or(Some(g.base_color()));
    }
    out
}

/// Shape that appears or disappears without a partner: scales from its center.
fn grow(s: &Shape, k: f32, alpha: f32) -> Shape {
    let c = s.world_bounds().center();
    let mut o = s.clone();
    o.m = Matrix::about(c, &Matrix::scale(k.max(0.001), k.max(0.001))).mul(&s.m);
    let fade = |c: Option<Rgba>| c.map(|c| Rgba { a: (c.a as f32 * alpha) as u8, ..c });
    o.fill = fade(o.fill);
    o.stroke = fade(o.stroke);
    o
}

/// Chained stroke: polyline in the document, closed or not, with its style.
struct Line {
    pts: Vec<Pt>,
    closed: bool,
    color: Rgba,
    width: f32,
    base: Shape,
}

/// Joins stroke segments that touch at their ends (the 4 sides of a
/// raw rectangle form a single closed outline).
fn chain_lines(shapes: &[Shape]) -> Vec<Line> {
    let mut pieces: Vec<Line> = shapes
        .iter()
        .filter(|s| !(s.fill.is_some() && s.is_closed()) || s.stroke.is_some())
        .filter_map(|s| s.stroke.map(|c| (s, c)))
        .flat_map(|(s, c)| {
            world_contours(s).into_iter().map(move |(pts, closed)| Line { pts, closed, color: c, width: s.stroke_width * s.m.avg_scale(), base: s.clone() })
        })
        .collect();
    let mut out = Vec::new();
    while let Some(mut cur) = pieces.pop() {
        if !cur.closed {
            loop {
                let (start, end) = (cur.pts[0], *cur.pts.last().unwrap());
                let Some(i) = pieces.iter().position(|p| !p.closed && p.color == cur.color && {
                    let (a, b) = (p.pts[0], *p.pts.last().unwrap());
                    dist(a, end) < 0.5 || dist(b, end) < 0.5 || dist(a, start) < 0.5 || dist(b, start) < 0.5
                }) else { break };
                let mut p = pieces.remove(i).pts;
                let (a, b) = (p[0], *p.last().unwrap());
                if dist(a, end) < 0.5 {
                    cur.pts.extend(p.into_iter().skip(1));
                } else if dist(b, end) < 0.5 {
                    p.reverse();
                    cur.pts.extend(p.into_iter().skip(1));
                } else {
                    if dist(a, start) < 0.5 {
                        p.reverse();
                    }
                    p.pop();
                    p.extend(cur.pts);
                    cur.pts = p;
                }
            }
            if cur.pts.len() > 2 && dist(cur.pts[0], *cur.pts.last().unwrap()) < 0.5 {
                cur.pts.pop();
                cur.closed = true;
            }
        }
        out.push(cur);
    }
    out.sort_by(|x, y| length(&y.pts, y.closed).partial_cmp(&length(&x.pts, x.closed)).unwrap());
    out
}

fn morph_lines(a: &Line, b: &Line, t: f32, hints: &[(Pt, Pt)]) -> Shape {
    let (ra, rb) = if a.closed && b.closed {
        pair_rings(&a.pts, &b.pts, hints)
    } else {
        let n = ((length(&a.pts, a.closed).max(length(&b.pts, b.closed)) / 2.5) as usize).clamp(8, 200);
        let ra = resample(&a.pts, a.closed, n);
        let mut rb = resample(&b.pts, b.closed, n);
        if dist(ra[0], rb[0]) + dist(ra[n - 1], rb[n - 1]) > dist(ra[0], rb[n - 1]) + dist(ra[n - 1], rb[0]) {
            rb.reverse();
        }
        (ra, rb)
    };
    let pts = ra.iter().zip(&rb).map(|(p, q)| (lerp(p.0, q.0, t), lerp(p.1, q.1, t))).collect();
    let mut s = mk_shape(&a.base, vec![(pts, a.closed && b.closed)], None, Some(a.color.lerp(&b.color, t)), lerp(a.width, b.width, t));
    s.fill = None;
    s
}

pub fn shape_tween(a: &[Shape], b: &[Shape], t: f32, ha: &[Pt], hb: &[Pt]) -> Vec<Shape> {
    let leaves = |v: &[Shape]| v.iter().flat_map(|s| s.leaves(&Matrix::IDENTITY)).collect::<Vec<_>>();
    let (la, lb) = (leaves(a), leaves(b));
    let hints: Vec<(Pt, Pt)> = ha.iter().zip(hb).map(|(x, y)| (*x, *y)).collect();
    let mut out = Vec::new();
    // Fills: paired by stacking order (an object's stroke is handled separately).
    let fills = |v: &[Shape]| -> Vec<Shape> {
        v.iter()
            .filter(|s| s.fill.is_some() && s.is_closed())
            .map(|s| {
                let mut f = s.clone();
                f.stroke = None;
                f
            })
            .collect()
    };
    let (fa, fb) = (fills(&la), fills(&lb));
    for i in 0..fa.len().max(fb.len()) {
        match (fa.get(i), fb.get(i)) {
            (Some(x), Some(y)) => out.push(morph_pair(x, y, t, &hints)),
            (Some(x), None) => out.push(grow(x, 1.0 - t, 1.0 - t)),
            (None, Some(y)) => out.push(grow(y, t, t)),
            _ => {}
        }
    }
    // Strokes: chained and paired by length.
    let (sa, sb) = (chain_lines(&la), chain_lines(&lb));
    for i in 0..sa.len().max(sb.len()) {
        match (sa.get(i), sb.get(i)) {
            (Some(x), Some(y)) => out.push(morph_lines(x, y, t, &hints)),
            (Some(x), None) => {
                let s = mk_shape(&x.base, vec![(x.pts.clone(), x.closed)], None, Some(x.color), x.width);
                out.push(grow(&s, 1.0 - t, 1.0 - t));
            }
            (None, Some(y)) => {
                let s = mk_shape(&y.base, vec![(y.pts.clone(), y.closed)], None, Some(y.color), y.width);
                out.push(grow(&s, t, t));
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Shape {
        let mut s = Shape::new(Geometry::Rect { w, h, r: 0.0 }, Some(Rgba::BLACK), None, 1.0, Matrix::translate(x + w / 2.0, y + h / 2.0));
        s.id = 1;
        s
    }

    #[test]
    fn square_to_wide_rect_halfway() {
        let a = rect(0.0, 0.0, 100.0, 100.0);
        let b = rect(0.0, 0.0, 300.0, 100.0);
        let m = shape_tween(&[a], &[b], 0.5, &[], &[]);
        assert_eq!(m.len(), 1);
        let bb = m[0].world_bounds();
        assert!((bb.w() - 200.0).abs() < 2.0, "{bb:?}");
        let area = crate::paint::region_area(&crate::paint::fill_region(&m[0]));
        assert!((area - 20000.0).abs() < 400.0, "{area}");
    }

    #[test]
    fn square_to_circle_keeps_area_in_between() {
        let a = rect(0.0, 0.0, 100.0, 100.0);
        let mut b = Shape::new(Geometry::Ellipse { w: 100.0, h: 100.0 }, Some(Rgba::WHITE), None, 1.0, Matrix::translate(50.0, 50.0));
        b.id = 2;
        let m = shape_tween(&[a], &[b], 0.5, &[], &[]);
        let area = crate::paint::region_area(&crate::paint::fill_region(&m[0]));
        assert!(area > 7800.0 && area < 10000.0, "{area}");
        assert_eq!(m[0].fill.unwrap().r, 128);
    }

    #[test]
    fn raw_rect_outline_morphs_as_one_ring() {
        // Raw rectangle (4 loose sides) → oval outline: a single ring, no spirals.
        let mut sq = rect(0.0, 0.0, 100.0, 100.0);
        sq.fill = None;
        sq.stroke = Some(Rgba::BLACK);
        let sides = crate::raw::to_raw_pieces(&sq);
        assert_eq!(sides.len(), 4);
        let mut ov = Shape::new(Geometry::Ellipse { w: 100.0, h: 100.0 }, None, Some(Rgba::BLACK), 1.0, Matrix::translate(50.0, 50.0));
        ov.id = 2;
        let m = shape_tween(&sides, &[ov], 0.5, &[], &[]);
        assert_eq!(m.len(), 1);
        assert!(m[0].is_closed() || matches!(&m[0].geom, Geometry::Bezier { paths } if paths[0].closed));
        let b = m[0].world_bounds();
        assert!(b.w() > 95.0 && b.w() < 101.0 && b.h() > 95.0, "{b:?}");
    }

    #[test]
    fn unmatched_shape_shrinks() {
        let a = rect(0.0, 0.0, 100.0, 100.0);
        let m = shape_tween(&[a], &[], 0.5, &[], &[]);
        assert!((m[0].world_bounds().w() - 50.0).abs() < 0.5);
    }

    #[test]
    fn hints_change_correspondence() {
        let a = rect(0.0, 0.0, 100.0, 100.0);
        let b = rect(200.0, 0.0, 100.0, 100.0);
        // No hints: clean translation. With a hint that takes the top-left
        // corner to the bottom-right, the intermediate shape rotates.
        let plain = shape_tween(&[a.clone()], &[b.clone()], 0.5, &[], &[]);
        let hinted = shape_tween(&[a], &[b], 0.5, &[(0.0, 0.0)], &[(300.0, 100.0)]);
        let pa = crate::paint::region_area(&crate::paint::fill_region(&plain[0]));
        let pb = crate::paint::region_area(&crate::paint::fill_region(&hinted[0]));
        assert!((pa - 10000.0).abs() < 300.0);
        assert!(pb < pa * 0.9, "el consejo deforma la transición: {pb}");
    }
}
