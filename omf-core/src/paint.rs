//! Paint operations on geometry: Brush/Eraser sweep,
//! booleans between regions (with i_overlay), Paint Bucket flood fill
//! and Pencil shape recognition in Straighten mode.

use crate::geom::*;
use crate::model::*;
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::clip::FloatClip;
use i_overlay::float::single::SingleFloatOverlay;
use i_overlay::float::slice::FloatSlice;
use i_overlay::string::clip::ClipRule;
use std::collections::HashMap;
use tiny_skia::{Paint, Pixmap, Stroke};

/// Region: set of closed contours in document coordinates
/// (even-odd: inner contours are holes).
pub type Region = Vec<Vec<[f32; 2]>>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tip {
    Round,
    Square,
    Oval,
    Slash,
}

impl Tip {
    pub fn parse(s: &str) -> Tip {
        match s {
            "square" => Tip::Square,
            "oval" => Tip::Oval,
            "slash" => Tip::Slash,
            _ => Tip::Round,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Tip::Round => "round",
            Tip::Square => "square",
            Tip::Oval => "oval",
            Tip::Slash => "slash",
        }
    }
}

/// Convex polygon of the tip, centered at the origin.
pub fn stamp(tip: Tip, size: f32) -> Vec<Pt> {
    let r = (size / 2.0).max(0.25);
    let ellipse = |rx: f32, ry: f32| {
        let n = ((rx.max(ry) * 1.5) as usize).clamp(12, 48);
        (0..n).map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            (rx * a.cos(), ry * a.sin())
        }).collect::<Vec<_>>()
    };
    match tip {
        Tip::Round => ellipse(r, r),
        Tip::Square => vec![(-r, -r), (r, -r), (r, r), (-r, r)],
        Tip::Oval => ellipse(r, r * 0.35),
        Tip::Slash => {
            let t = (r * 0.18).max(0.5);
            vec![(-r - t, r - t), (r - t, -r - t), (r + t, -r + t), (-r + t, r + t)]
        }
    }
}

/// Convex hull (monotone chain).
fn hull(mut pts: Vec<Pt>) -> Vec<Pt> {
    pts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: Pt, a: Pt, b: Pt| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let mut lower: Vec<Pt> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Pt> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn to_arr(v: &[Pt]) -> Vec<[f32; 2]> {
    v.iter().map(|p| [p.0, p.1]).collect()
}

fn flatten_shapes(shapes: Vec<Vec<Vec<[f32; 2]>>>) -> Region {
    shapes.into_iter().flatten().filter(|c| c.len() >= 3).collect()
}

/// Sweep of a tip along a stroke: union of the hulls of
/// each segment (this is how Flash's non-round tips work).
pub fn sweep(pts: &[Pt], tip: &[Pt]) -> Region {
    let place = |c: Pt| tip.iter().map(move |q| (q.0 + c.0, q.1 + c.1));
    let mut parts: Region = Vec::new();
    if pts.len() == 1 {
        parts.push(to_arr(&place(pts[0]).collect::<Vec<_>>()));
    }
    for w in pts.windows(2) {
        let h = hull(place(w[0]).chain(place(w[1])).collect());
        if h.len() >= 3 {
            parts.push(to_arr(&h));
        }
    }
    union(parts)
}

pub fn union(a: Region) -> Region {
    if a.is_empty() {
        return a;
    }
    let empty: Region = Vec::new();
    flatten_shapes(a.overlay(&empty, OverlayRule::Subject, FillRule::NonZero))
}

pub fn difference(a: &Region, b: &Region) -> Region {
    if a.is_empty() || b.is_empty() {
        return a.clone();
    }
    flatten_shapes(a.overlay(b, OverlayRule::Difference, FillRule::EvenOdd))
}

pub fn intersect(a: &Region, b: &Region) -> Region {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    flatten_shapes(a.overlay(b, OverlayRule::Intersect, FillRule::EvenOdd))
}

/// Union of several regions that may overlap each other.
pub fn union_all(rs: Vec<Region>) -> Region {
    let mut acc: Region = Vec::new();
    for r in rs {
        if r.is_empty() {
            continue;
        }
        acc = if acc.is_empty() { r } else { flatten_shapes(acc.overlay(&r, OverlayRule::Union, FillRule::EvenOdd)) };
    }
    acc
}

pub fn region_area(r: &Region) -> f32 {
    r.iter()
        .map(|c| {
            let mut a = 0.0;
            for i in 0..c.len() {
                let (p, q) = (c[i], c[(i + 1) % c.len()]);
                a += p[0] * q[1] - q[0] * p[1];
            }
            a / 2.0
        })
        .sum::<f32>()
        .abs()
}

pub fn region_bbox(r: &Region) -> BBox {
    let mut b = BBox::EMPTY;
    r.iter().flatten().for_each(|p| b.add((p[0], p[1])));
    b
}

/// Fill of a shape as a region in document coordinates.
pub fn fill_region(s: &Shape) -> Region {
    if !s.is_closed() || s.fill.is_none() {
        return Vec::new();
    }
    let parts: Region = flatten(&s.outline())
        .into_iter()
        .filter(|(p, c)| *c && p.len() >= 3)
        .map(|(p, _)| p.iter().map(|q| { let w = s.m.apply(q.0, q.1); [w.0, w.1] }).collect())
        .collect();
    if parts.is_empty() {
        return parts;
    }
    let empty: Region = Vec::new();
    flatten_shapes(parts.overlay(&empty, OverlayRule::Subject, FillRule::EvenOdd))
}

/// Stroke lines of a shape (polylines in the document).
pub fn stroke_lines(s: &Shape) -> Vec<Vec<[f32; 2]>> {
    if s.stroke.is_none() {
        return Vec::new();
    }
    flatten(&s.outline())
        .into_iter()
        .map(|(p, closed)| {
            let mut v: Vec<[f32; 2]> = p.iter().map(|q| { let w = s.m.apply(q.0, q.1); [w.0, w.1] }).collect();
            if closed && !v.is_empty() {
                v.push(v[0]);
            }
            v
        })
        .filter(|v| v.len() >= 2)
        .collect()
}

/// Area covered by the stroke (for "Paint Fills", which doesn't paint over lines).
pub fn stroke_region(s: &Shape) -> Region {
    let w = s.stroke_width * s.m.avg_scale();
    let tip = stamp(Tip::Round, w.max(0.5));
    union_all(stroke_lines(s).iter().map(|l| sweep(&l.iter().map(|p| (p[0], p[1])).collect::<Vec<_>>(), &tip)).collect())
}

/// Clips polylines, keeping only what lies outside the region.
pub fn lines_outside(lines: &[Vec<[f32; 2]>], region: &Region) -> Vec<Vec<[f32; 2]>> {
    let rule = ClipRule { invert: true, boundary_included: false };
    let mut out = Vec::new();
    for l in lines {
        out.extend(l.clip_by(region, FillRule::EvenOdd, rule).into_iter().filter(|p| p.len() >= 2));
    }
    out
}

/// Curve-fitting tolerance after a boolean operation (document units).
pub const FIT_ERR: f32 = 0.15;
pub const CORNER_DEG: f32 = 40.0;

fn pts_of(c: &[[f32; 2]]) -> Vec<Pt> {
    c.iter().map(|p| (p[0], p[1])).collect()
}

/// Centers some paths on the origin and returns the shape with its translation.
fn centered(mut paths: Vec<BezPath>, fill: Option<Rgba>, stroke: Option<Rgba>, width: f32) -> Option<Shape> {
    paths.retain(|p| p.nodes.len() >= 2);
    if paths.is_empty() {
        return None;
    }
    let tmp = Shape::new(Geometry::Bezier { paths: paths.clone() }, None, None, width, Matrix::IDENTITY);
    let c = tmp.local_bounds().center();
    for p in &mut paths {
        for n in &mut p.nodes {
            n.translate((-c.0, -c.1));
        }
    }
    Some(Shape::new(Geometry::Bezier { paths }, fill, stroke, width, Matrix::translate(c.0, c.1)))
}

/// Filled shape (no stroke) from a region, with the curves refitted.
pub fn region_shape(r: &Region, fill: Rgba) -> Option<Shape> {
    let r: Region = r.iter().filter(|c| c.len() >= 3).cloned().collect();
    if r.is_empty() || region_area(&r) < 0.05 {
        return None;
    }
    let paths = r.iter().map(|c| crate::fit::fit_path(&pts_of(c), true, FIT_ERR, CORNER_DEG)).collect();
    centered(paths, Some(fill), None, 1.0)
}

/// Clips polylines, keeping only what lies inside the region.
pub fn lines_inside(lines: &[Vec<[f32; 2]>], region: &Region) -> Vec<Vec<[f32; 2]>> {
    let rule = ClipRule { invert: false, boundary_included: true };
    let mut out = Vec::new();
    for l in lines {
        out.extend(l.clip_by(region, FillRule::EvenOdd, rule).into_iter().filter(|p| p.len() >= 2));
    }
    out
}

/// Stroke-only shape from loose polylines.
pub fn lines_shape(lines: &[Vec<[f32; 2]>], stroke: Rgba, width: f32) -> Option<Shape> {
    let paths = lines
        .iter()
        .filter(|l| l.len() >= 2)
        .map(|l| {
            let pts = pts_of(l);
            let closed = pts.len() > 2 && dist(pts[0], pts[pts.len() - 1]) < 1e-3;
            crate::fit::fit_path(&pts, closed, FIT_ERR, CORNER_DEG)
        })
        .collect();
    centered(paths, None, Some(stroke), width)
}

/// Is it a sliver? (very thin leftovers that booleans leave when clipping curves).
fn is_sliver(r: &Region) -> bool {
    let area = region_area(r);
    let perim: f32 = r
        .first()
        .map(|c| (0..c.len()).map(|i| dist((c[i][0], c[i][1]), (c[(i + 1) % c.len()][0], c[(i + 1) % c.len()][1]))).sum())
        .unwrap_or(0.0);
    // Average thickness ≈ 2·area/perimeter: below ~0.6 units it isn't visible.
    area < 0.25 || 2.0 * area < 0.6 * perim
}

/// Connected components of a region (each with its holes), without slivers.
pub fn components(r: &Region) -> Vec<Region> {
    if r.is_empty() {
        return Vec::new();
    }
    let empty: Region = Vec::new();
    r.overlay(&empty, OverlayRule::Subject, FillRule::EvenOdd)
        .into_iter()
        .map(|s| s.into_iter().filter(|c| c.len() >= 3).collect::<Region>())
        .filter(|s: &Region| !s.is_empty() && !is_sliver(s))
        .collect()
}

/// Cuts a region with lines (like a line crossing a fill in Flash).
pub fn slice(r: &Region, lines: &[Vec<[f32; 2]>]) -> Vec<Region> {
    if r.is_empty() || lines.is_empty() {
        return vec![r.clone()];
    }
    let paths: Vec<Vec<[f32; 2]>> = lines.iter().filter(|l| l.len() >= 2).cloned().collect();
    r.slice_by(&paths, FillRule::EvenOdd)
        .into_iter()
        .map(|s| s.into_iter().filter(|c| c.len() >= 3).collect::<Region>())
        .filter(|s: &Region| !s.is_empty() && !is_sliver(s))
        .collect()
}

/// Raw fill pieces: one per connected component.
pub fn fill_pieces(r: &Region, fill: Rgba) -> Vec<Shape> {
    components(r)
        .iter()
        .filter_map(|c| region_shape(c, fill))
        .map(|mut s| {
            s.raw = true;
            s
        })
        .collect()
}

/// Fill pieces with the fill style (color or gradient) of `src`.
pub fn fill_pieces_like(r: &Region, src: &Shape) -> Vec<Shape> {
    let Some(c) = src.fill else { return Vec::new() };
    fill_pieces(r, c).into_iter().map(|mut s| { s.take_fill_of(src); s }).collect()
}

/// Filled shape with the fill style of `src`.
pub fn region_shape_like(r: &Region, src: &Shape) -> Option<Shape> {
    region_shape(r, src.fill?).map(|mut s| { s.take_fill_of(src); s })
}

/// Raw stroke pieces, split at the corners.
pub fn stroke_pieces(lines: &[Vec<[f32; 2]>], stroke: Rgba, width: f32) -> Vec<Shape> {
    let mut out = Vec::new();
    for l in lines.iter().filter(|l| l.len() >= 2) {
        let pts = pts_of(l);
        let closed = pts.len() > 2 && dist(pts[0], pts[pts.len() - 1]) < 1e-3;
        let path = crate::fit::fit_path(&pts, closed, FIT_ERR, CORNER_DEG);
        for piece in crate::fit::split_at_corners(&path) {
            if let Some(mut s) = centered(vec![piece], None, Some(stroke), width) {
                s.raw = true;
                out.push(s);
            }
        }
    }
    out
}

/// Points where two sets of polylines intersect.
pub fn intersections(a: &[Vec<[f32; 2]>], b: &[Vec<[f32; 2]>]) -> Vec<Pt> {
    let mut out = Vec::new();
    let bb = |l: &[[f32; 2]]| {
        let mut r = BBox::EMPTY;
        l.iter().for_each(|p| r.add((p[0], p[1])));
        r
    };
    for la in a {
        let ba = bb(la);
        for lb in b {
            if !ba.intersects(&bb(lb)) {
                continue;
            }
            for sa in la.windows(2) {
                for sb in lb.windows(2) {
                    if let Some(p) = seg_intersection((sa[0][0], sa[0][1]), (sa[1][0], sa[1][1]), (sb[0][0], sb[0][1]), (sb[1][0], sb[1][1])) {
                        if !out.iter().any(|q: &Pt| dist(*q, p) < 0.05) {
                            out.push(p);
                        }
                    }
                }
            }
        }
    }
    out
}

fn seg_intersection(p: Pt, p2: Pt, q: Pt, q2: Pt) -> Option<Pt> {
    let r = (p2.0 - p.0, p2.1 - p.1);
    let s = (q2.0 - q.0, q2.1 - q.1);
    let den = r.0 * s.1 - r.1 * s.0;
    if den.abs() < 1e-9 {
        return None;
    }
    let qp = (q.0 - p.0, q.1 - p.1);
    let t = (qp.0 * s.1 - qp.1 * s.0) / den;
    let u = (qp.0 * r.1 - qp.1 * r.0) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| (p.0 + t * r.0, p.1 + t * r.1))
}

/// Splits polylines at the given points (those lying on them).
pub fn split_lines(lines: &[Vec<[f32; 2]>], cuts: &[Pt]) -> Vec<Vec<[f32; 2]>> {
    let mut out = Vec::new();
    for l in lines {
        let mut cur: Vec<[f32; 2]> = vec![l[0]];
        for w in l.windows(2) {
            let (a, b) = ((w[0][0], w[0][1]), (w[1][0], w[1][1]));
            let len = dist(a, b);
            let mut here: Vec<(f32, Pt)> = cuts
                .iter()
                .filter(|c| dist_to_segment(**c, a, b) < 0.05)
                .map(|c| (if len > 0.0 { dist(a, *c) / len } else { 0.0 }, *c))
                .filter(|(t, _)| *t > 1e-4 && *t < 1.0 - 1e-4)
                .collect();
            here.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
            for (_, c) in here {
                cur.push([c.0, c.1]);
                out.push(std::mem::take(&mut cur));
                cur.push([c.0, c.1]);
            }
            cur.push(w[1]);
        }
        out.push(cur);
    }
    out.into_iter()
        .filter(|l| l.len() >= 2 && l.windows(2).map(|w| dist((w[0][0], w[0][1]), (w[1][0], w[1][1]))).sum::<f32>() > 0.05)
        .collect()
}

/// Total length of some polylines.
pub fn total_length(ls: &[Vec<[f32; 2]>]) -> f32 {
    ls.iter().map(|l| l.windows(2).map(|w| dist((w[0][0], w[0][1]), (w[1][0], w[1][1]))).sum::<f32>()).sum()
}

// ───────────────────────── Paint Bucket ─────────────────────────

/// Fills the closed area containing `seed`, rasterizing the layer's shapes
/// onto a grid (`scale` cells per unit) within `area`.
/// `gap` = gap-closing radius in cells.
pub fn flood_region(shapes: &[Shape], area: BBox, scale: f32, seed: Pt, gap: i32) -> Result<Region, &'static str> {
    let w = (area.w() * scale).ceil().max(1.0) as usize;
    let h = (area.h() * scale).ceil().max(1.0) as usize;
    let mut pm = Pixmap::new(w as u32, h as u32).ok_or("Área demasiado grande")?;
    let base = Matrix::scale(scale, scale).mul(&Matrix::translate(-area.x0, -area.y0));
    let mut paint = Paint::default();
    paint.set_color_rgba8(0, 0, 0, 255);
    paint.anti_alias = false;
    for s in shapes.iter().flat_map(|s| s.leaves(&Matrix::IDENTITY)) {
        let Some(path) = crate::render::build_path(&s.outline()) else { continue };
        let t = base.mul(&s.m);
        if s.is_closed() && s.fill.is_some() {
            pm.fill_path(&path, &paint, tiny_skia::FillRule::EvenOdd, t.to_skia(), None);
        }
        if s.stroke.is_some() {
            let width = s.stroke_width.max(1.0 / (scale * s.m.avg_scale()));
            pm.stroke_path(&path, &paint, &Stroke { width, ..Default::default() }, t.to_skia(), None);
        }
    }
    let wall: Vec<bool> = pm.data().chunks(4).map(|px| px[3] > 0).collect();
    let blocked = dilate(&wall, w, h, gap);
    let sx = ((seed.0 - area.x0) * scale) as isize;
    let sy = ((seed.1 - area.y0) * scale) as isize;
    if sx < 0 || sy < 0 || sx as usize >= w || sy as usize >= h {
        return Err("Haz clic dentro del área visible");
    }
    let start = sy as usize * w + sx as usize;
    if blocked[start] {
        return Err("Haz clic dentro de un área cerrada");
    }
    // Flood fill (4-neighbors) through the free cells.
    let mut region = vec![false; w * h];
    let mut stack = vec![start];
    region[start] = true;
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        if x == 0 || y == 0 || x == w - 1 || y == h - 1 {
            return Err("No hay un área cerrada: prueba a «Cerrar huecos»");
        }
        for j in [i - 1, i + 1, i - w, i + w] {
            if !region[j] && !blocked[j] {
                region[j] = true;
                stack.push(j);
            }
        }
    }
    // Recover what the gap closing covered without crossing lines (geodesic
    // dilation) and extend 1 cell under the strokes so no slits are left.
    let mut frontier: Vec<usize> = (0..w * h).filter(|&i| region[i]).collect();
    for _ in 0..gap {
        let mut next = Vec::new();
        for &i in &frontier {
            let (x, y) = (i % w, i / w);
            for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                if !region[j] && !wall[j] {
                    region[j] = true;
                    next.push(j);
                }
            }
        }
        frontier = next;
    }
    let region = dilate(&region, w, h, 1);
    let loops = trace(&region, w, h);
    let eps = 0.6;
    Ok(loops
        .into_iter()
        .map(|l| {
            let pts: Vec<Pt> = l.iter().map(|&(x, y)| (x as f32, y as f32)).collect();
            let mut closed = pts.clone();
            closed.push(pts[0]);
            let mut s = simplify(&closed, eps);
            s.pop();
            s.iter().map(|p| [area.x0 + p.0 / scale, area.y0 + p.1 / scale]).collect::<Vec<_>>()
        })
        .filter(|c: &Vec<[f32; 2]>| c.len() >= 3)
        .collect())
}

fn dilate(m: &[bool], w: usize, h: usize, r: i32) -> Vec<bool> {
    if r <= 0 {
        return m.to_vec();
    }
    let r = r as usize;
    let mut tmp = vec![false; w * h];
    for y in 0..h {
        let row = &m[y * w..(y + 1) * w];
        let mut last: isize = -(r as isize) - 1;
        // Horizontal distance to the last marked pixel, in both directions.
        for x in 0..w {
            if row[x] {
                last = x as isize;
            }
            if x as isize - last <= r as isize {
                tmp[y * w + x] = true;
            }
        }
        let mut next: isize = (w + r + 1) as isize;
        for x in (0..w).rev() {
            if row[x] {
                next = x as isize;
            }
            if next - x as isize <= r as isize {
                tmp[y * w + x] = true;
            }
        }
    }
    let mut out = vec![false; w * h];
    for x in 0..w {
        let mut last: isize = -(r as isize) - 1;
        for y in 0..h {
            if tmp[y * w + x] {
                last = y as isize;
            }
            if y as isize - last <= r as isize {
                out[y * w + x] = true;
            }
        }
        let mut next: isize = (h + r + 1) as isize;
        for y in (0..h).rev() {
            if tmp[y * w + x] {
                next = y as isize;
            }
            if next - y as isize <= r as isize {
                out[y * w + x] = true;
            }
        }
    }
    out
}

/// Contours of a binary mask (edges between cells linked into loops).
fn trace(m: &[bool], w: usize, h: usize) -> Vec<Vec<(i32, i32)>> {
    let inside = |x: i32, y: i32| x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && m[y as usize * w + x as usize];
    let mut edges: HashMap<(i32, i32), Vec<(i32, i32)>> = HashMap::new();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if !inside(x, y) {
                continue;
            }
            if !inside(x, y - 1) {
                edges.entry((x, y)).or_default().push((x + 1, y));
            }
            if !inside(x + 1, y) {
                edges.entry((x + 1, y)).or_default().push((x + 1, y + 1));
            }
            if !inside(x, y + 1) {
                edges.entry((x + 1, y + 1)).or_default().push((x, y + 1));
            }
            if !inside(x - 1, y) {
                edges.entry((x, y + 1)).or_default().push((x, y));
            }
        }
    }
    let mut loops = Vec::new();
    let keys: Vec<(i32, i32)> = edges.keys().copied().collect();
    for k in keys {
        while let Some(next) = edges.get_mut(&k).and_then(|v| v.pop()) {
            let mut lp = vec![k];
            let mut cur = next;
            while cur != k {
                lp.push(cur);
                match edges.get_mut(&cur).and_then(|v| v.pop()) {
                    Some(n) => cur = n,
                    None => break,
                }
            }
            if lp.len() >= 3 {
                loops.push(lp);
            }
        }
    }
    loops
}

// ───────────────────────── Pencil: Straighten ─────────────────────────

pub enum Recognized {
    Line(Pt, Pt),
    Rect(BBox),
    Ellipse(BBox),
    Poly(Vec<Pt>, bool),
}

/// Shape recognition in the style of Flash's "Straighten" mode.
/// `px` = size of a screen pixel in document units.
pub fn straighten(raw: &[Pt], px: f32) -> Recognized {
    let simple = simplify(raw, 3.0 * px);
    let (first, last) = (raw[0], raw[raw.len() - 1]);
    let mut bb = BBox::EMPTY;
    raw.iter().for_each(|p| bb.add(*p));
    let size = bb.w().max(bb.h());
    let closed = raw.len() > 8 && dist(first, last) < (12.0 * px).max(size * 0.15);
    if !closed {
        let coarse = simplify(raw, (6.0 * px).max(size * 0.06));
        if coarse.len() <= 2 {
            return Recognized::Line(first, last);
        }
        return Recognized::Poly(simple, false);
    }
    let poly: Vec<Pt> = raw.to_vec();
    let mut area = 0.0;
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        area += p.0 * q.1 - q.0 * p.1;
    }
    let area = (area / 2.0).abs();
    let (w, h) = (bb.w().max(1e-3), bb.h().max(1e-3));
    let rect_ratio = area / (w * h);
    let ell_ratio = area / (std::f32::consts::PI * w * h / 4.0);
    let mut closed_pts = raw.to_vec();
    closed_pts.push(first);
    let corners = simplify(&closed_pts, (8.0 * px).max(size * 0.12)).len() - 1;
    if rect_ratio > 0.86 && (3..=6).contains(&corners) {
        Recognized::Rect(bb)
    } else if (ell_ratio - 1.0).abs() < 0.13 && corners >= 4 {
        Recognized::Ellipse(bb)
    } else {
        let mut s = simplify(&closed_pts, 3.0 * px);
        s.pop();
        Recognized::Poly(s, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_and_difference() {
        let tip = stamp(Tip::Round, 10.0);
        let a = sweep(&[(0.0, 0.0), (100.0, 0.0)], &tip);
        assert!((region_area(&a) - (1000.0 + 25.0 * std::f32::consts::PI)).abs() < 20.0);
        let b = sweep(&[(50.0, -20.0), (50.0, 20.0)], &tip);
        let d = difference(&a, &b);
        assert!(region_area(&d) < region_area(&a));
        assert!(region_area(&intersect(&a, &b)) > 50.0);
    }

    #[test]
    fn flood_inside_rect_outline() {
        let s = Shape::new(Geometry::Rect { w: 40.0, h: 40.0, r: 0.0 }, None, Some(Rgba::BLACK), 1.0, Matrix::translate(50.0, 50.0));
        let area = BBox { x0: 0.0, y0: 0.0, x1: 100.0, y1: 100.0 };
        let r = flood_region(&[s.clone()], area, 2.0, (50.0, 50.0), 0).unwrap();
        let a = region_area(&r);
        assert!(a > 1500.0 && a < 1700.0, "area {a}");
        assert!(flood_region(&[s], area, 2.0, (5.0, 5.0), 0).is_err());
    }

    #[test]
    fn straighten_recognizes_rect() {
        let mut pts = Vec::new();
        for i in 0..20 { pts.push((i as f32 * 5.0, 0.0)); }
        for i in 0..20 { pts.push((100.0, i as f32 * 3.0)); }
        for i in 0..20 { pts.push((100.0 - i as f32 * 5.0, 60.0)); }
        for i in 0..20 { pts.push((0.0, 60.0 - i as f32 * 3.0)); }
        pts.push((1.0, 1.0));
        assert!(matches!(straighten(&pts, 1.0), Recognized::Rect(_)));
    }

    #[test]
    fn eraser_clips_lines() {
        let lines = vec![vec![[0.0, 0.0], [100.0, 0.0]]];
        let hole = sweep(&[(50.0, -10.0), (50.0, 10.0)], &stamp(Tip::Square, 10.0));
        let out = lines_outside(&lines, &hole);
        assert_eq!(out.len(), 2);
    }
}
