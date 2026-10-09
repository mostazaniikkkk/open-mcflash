//! Flash 8 merge drawing model.
//!
//! A layer's raw shapes are independent pieces: fills (without
//! stroke) and stroke segments (without fill). When adding new art:
//! - a new fill covers what lies below: it cuts fills of another color,
//!   merges with those of the same color and erases the strokes it covers;
//! - a new stroke splits the fills it crosses and splits (and is split by)
//!   the strokes it crosses, as in Flash.

use crate::fit;
use crate::geom::*;
use crate::model::*;
use crate::paint;

/// Converts a shape into raw pieces: one fill and stroke segments
/// split at the corners. The first piece keeps the id.
pub fn to_raw_pieces(s: &Shape) -> Vec<Shape> {
    if s.is_container() {
        return s.leaves(&Matrix::IDENTITY).iter().flat_map(to_raw_pieces).collect();
    }
    let mut out = Vec::new();
    if s.is_closed() && s.fill.is_some() {
        let mut f = s.clone();
        f.stroke = None;
        f.raw = true;
        out.push(f);
    }
    if let Some(c) = s.stroke {
        for path in s.bez_paths() {
            for piece in fit::split_at_corners(&path) {
                let mut p = Shape::new(Geometry::Bezier { paths: vec![piece] }, None, Some(c), s.stroke_width, s.m);
                p.raw = true;
                p.tag = s.tag;
                out.push(p);
            }
        }
    }
    for (k, p) in out.iter_mut().enumerate() {
        p.id = if k == 0 { s.id } else { 0 };
    }
    out
}

fn lines_bbox(lines: &[Vec<[f32; 2]>]) -> BBox {
    let mut b = BBox::EMPTY;
    lines.iter().flatten().for_each(|p| b.add((p[0], p[1])));
    b
}

fn padded(s: &Shape) -> BBox {
    let mut b = s.world_bounds();
    let pad = if s.stroke.is_some() { s.stroke_width * s.m.avg_scale() / 2.0 + 0.5 } else { 0.5 };
    b.x0 -= pad;
    b.y0 -= pad;
    b.x1 += pad;
    b.y1 += pad;
    b
}

fn is_endpoint(lines: &[Vec<[f32; 2]>], p: Pt) -> bool {
    lines.iter().any(|l| {
        let (a, b) = (l[0], l[l.len() - 1]);
        dist((a[0], a[1]), p) < 0.05 || dist((b[0], b[1]), p) < 0.05
    })
}

fn with_first_id(mut v: Vec<Shape>, id: u32) -> Vec<Shape> {
    for (k, s) in v.iter_mut().enumerate() {
        s.id = if k == 0 { id } else { 0 };
    }
    v
}

/// Merges new pieces with existing ones in the same context.
/// New pieces that need to be created come out with id 0.
pub fn merge(existing: Vec<Shape>, new: Vec<Shape>) -> Vec<Shape> {
    // Temporary ids to recognize the new pieces during merging.
    let mut tmp = TMP_BASE;
    let mut next_tmp = || {
        tmp += 1;
        tmp
    };
    let new: Vec<Shape> = new
        .into_iter()
        .map(|mut s| {
            if s.id == 0 {
                s.id = next_tmp();
            }
            s
        })
        .collect();
    let (mut fills, mut strokes): (Vec<Shape>, Vec<Shape>) = existing.into_iter().partition(|s| s.fill.is_some());
    let (nfills, nstrokes): (Vec<Shape>, Vec<Shape>) = new.into_iter().partition(|s| s.fill.is_some());
    let mut fresh_fills: Vec<u32> = Vec::new();

    for nf in nfills {
        let mut r = paint::fill_region(&nf);
        if r.is_empty() {
            continue;
        }
        let mut touched = false;
        let mut id = nf.id;
        let mut out = Vec::new();
        for ef in fills.drain(..) {
            if !padded(&ef).intersects(&paint::region_bbox(&r)) {
                out.push(ef);
                continue;
            }
            let er = paint::fill_region(&ef);
            if paint::region_area(&paint::intersect(&r, &er)) < 0.01 {
                out.push(ef);
                continue;
            }
            touched = true;
            if ef.same_fill(&nf) {
                // Same color: they merge into a single fill.
                r = paint::union_all(vec![r, er]);
                if id == 0 || id > TMP_BASE {
                    id = ef.id;
                }
            } else {
                // Different color: the new one covers and cuts the one below.
                let rest = paint::difference(&er, &r);
                out.extend(with_first_id(paint::fill_pieces_like(&rest, &ef), ef.id));
            }
        }
        // Strokes left beneath the new fill disappear.
        let rb = paint::region_bbox(&r);
        let mut sout = Vec::new();
        for es in strokes.drain(..) {
            if !padded(&es).intersects(&rb) {
                sout.push(es);
                continue;
            }
            let lines = paint::stroke_lines(&es);
            let kept = paint::lines_outside(&lines, &r);
            if (paint::total_length(&lines) - paint::total_length(&kept)).abs() < 0.01 {
                sout.push(es);
                continue;
            }
            let w = es.stroke_width * es.m.avg_scale();
            sout.extend(with_first_id(paint::stroke_pieces(&kept, es.stroke.unwrap(), w), es.id));
        }
        strokes = sout;
        if touched {
            let mut pieces = with_first_id(paint::fill_pieces_like(&r, &nf), id);
            for p in pieces.iter_mut().filter(|p| p.id == 0) {
                p.id = next_tmp();
            }
            fresh_fills.extend(pieces.iter().map(|p| p.id));
            out.extend(pieces);
        } else {
            fresh_fills.push(nf.id);
            out.push(nf);
        }
        fills = out;
    }

    for ns in nstrokes {
        let lines = paint::stroke_lines(&ns);
        if lines.is_empty() {
            continue;
        }
        let lb = lines_bbox(&lines);
        // A line crossing a fill splits it into two pieces.
        let mut out = Vec::new();
        for ef in fills.drain(..) {
            let own = fresh_fills.contains(&ef.id);
            if own || !padded(&ef).intersects(&lb) {
                out.push(ef);
                continue;
            }
            let parts = paint::slice(&paint::fill_region(&ef), &lines);
            if parts.len() <= 1 {
                out.push(ef);
                continue;
            }
            let pieces: Vec<Shape> = parts
                .iter()
                .filter_map(|p| paint::region_shape_like(p, &ef))
                .map(|mut s| {
                    s.raw = true;
                    s
                })
                .collect();
            out.extend(with_first_id(pieces, ef.id));
        }
        fills = out;
        // Lines that cross are split at the crossing.
        let mut cuts: Vec<Pt> = Vec::new();
        let mut sout = Vec::new();
        for es in strokes.drain(..) {
            if !padded(&es).intersects(&lb) {
                sout.push(es);
                continue;
            }
            let el = paint::stroke_lines(&es);
            let pts: Vec<Pt> = paint::intersections(&el, &lines)
                .into_iter()
                .filter(|p| !(is_endpoint(&el, *p) && is_endpoint(&lines, *p)))
                .collect();
            if pts.is_empty() {
                sout.push(es);
                continue;
            }
            cuts.extend(pts.iter().copied());
            let pieces = paint::split_lines(&el, &pts);
            if pieces.len() <= el.len() {
                sout.push(es);
                continue;
            }
            let w = es.stroke_width * es.m.avg_scale();
            sout.extend(with_first_id(paint::stroke_pieces(&pieces, es.stroke.unwrap(), w), es.id));
        }
        let pieces = paint::split_lines(&lines, &cuts);
        if cuts.is_empty() || pieces.len() <= lines.len() {
            sout.push(ns);
        } else {
            let w = ns.stroke_width * ns.m.avg_scale();
            sout.extend(with_first_id(paint::stroke_pieces(&pieces, ns.stroke.unwrap(), w), ns.id));
        }
        strokes = sout;
    }

    fills
        .into_iter()
        .chain(strokes)
        .map(|mut s| {
            s.raw = true;
            if s.id > TMP_BASE {
                s.id = 0;
            }
            s
        })
        .collect()
}

const TMP_BASE: u32 = u32::MAX - 1_000_000;

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32, fill: Rgba, stroke: Option<Rgba>) -> Shape {
        let mut s = Shape::new(Geometry::Rect { w, h, r: 0.0 }, Some(fill), stroke, 1.0, Matrix::translate(x + w / 2.0, y + h / 2.0));
        s.id = 1;
        s
    }
    const RED: Rgba = Rgba { r: 255, g: 0, b: 0, a: 255 };
    const BLUE: Rgba = Rgba { r: 0, g: 0, b: 255, a: 255 };

    #[test]
    fn rect_becomes_fill_plus_four_sides() {
        let p = to_raw_pieces(&rect(0.0, 0.0, 100.0, 50.0, RED, Some(Rgba::BLACK)));
        assert_eq!(p.len(), 5);
        assert_eq!(p.iter().filter(|s| s.fill.is_some()).count(), 1);
    }

    #[test]
    fn same_color_merges_other_color_cuts() {
        let a = to_raw_pieces(&rect(0.0, 0.0, 100.0, 100.0, RED, None));
        let b = to_raw_pieces(&rect(50.0, 50.0, 100.0, 100.0, RED, None));
        let m = merge(a.clone(), b);
        assert_eq!(m.len(), 1, "mismo color: un solo relleno");
        assert!((paint::region_area(&paint::fill_region(&m[0])) - 17500.0).abs() < 30.0);

        let c = to_raw_pieces(&rect(50.0, 50.0, 100.0, 100.0, BLUE, None));
        let m = merge(a, c);
        assert_eq!(m.len(), 2);
        let red = m.iter().find(|s| s.fill == Some(RED)).unwrap();
        assert!((paint::region_area(&paint::fill_region(red)) - 7500.0).abs() < 30.0, "el azul recorta al rojo");
    }

    #[test]
    fn line_splits_fill_and_crossing_lines() {
        let a = to_raw_pieces(&rect(0.0, 0.0, 100.0, 100.0, RED, Some(Rgba::BLACK)));
        let mut line = Shape::new(Geometry::Line { dx: 0.0, dy: 140.0 }, None, Some(Rgba::BLACK), 1.0, Matrix::translate(50.0, 50.0));
        line.id = 9;
        let m = merge(a, to_raw_pieces(&line));
        let fills: Vec<_> = m.iter().filter(|s| s.fill.is_some()).collect();
        assert_eq!(fills.len(), 2, "la línea parte el relleno en dos");
        // 4 sides, of which top and bottom are split (6) + the line split into 3.
        assert_eq!(m.iter().filter(|s| s.stroke.is_some()).count(), 9);
    }
}
