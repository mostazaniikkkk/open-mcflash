//! Flash drawing model in the editor: merging of raw shapes, floating
//! selection, cutting with the selection rectangle, bending lines and corners,
//! groups and drawing objects, Modify > Shape commands and snapping.

use super::*;
use crate::paint::{self, Region};
use crate::{fit, raw};
use tiny_skia::{FilterQuality, Paint, Pattern, SpreadMode, Stroke, Transform};

const MATCH_EPS: f32 = 0.08;

/// Segment of a path affected by a bend: (shape, subpath, a, b, reversed).
/// Span being bent: (shape, subpath, start node, end node, u0, u1),
/// where [u0, u1] is the piece of the bent curve it occupies (u0 > u1: reversed).
/// The first one is the span being dragged (0 → 1).
pub(crate) type SegRef = (u32, usize, usize, usize, f32, f32);

fn cubic_of(a: &Node, b: &Node) -> [Pt; 4] {
    if !a.has_out() && !b.has_in() {
        // Straight line: handles at the thirds.
        let t = |k: f32| (a.p.0 + (b.p.0 - a.p.0) * k, a.p.1 + (b.p.1 - a.p.1) * k);
        [a.p, t(1.0 / 3.0), t(2.0 / 3.0), b.p]
    } else {
        [a.p, a.o, b.i, b.p]
    }
}

/// Parameter of the point on the curve closest to `p`, and the distance.
fn project_cubic(c: &[Pt; 4], p: Pt) -> (f32, f32) {
    let at = |u: f32| cubic_point(c[0], c[1], c[2], c[3], u);
    let mut best = (0.0, f32::INFINITY);
    for i in 0..=64 {
        let u = i as f32 / 64.0;
        let d = dist(at(u), p);
        if d < best.1 {
            best = (u, d);
        }
    }
    let mut step = 1.0 / 64.0;
    for _ in 0..20 {
        step /= 2.0;
        for u in [best.0 - step, best.0 + step] {
            let u = u.clamp(0.0, 1.0);
            let d = dist(at(u), p);
            if d < best.1 {
                best = (u, d);
            }
        }
    }
    best
}

/// Piece [u0, u1] of a cubic (reversed if u0 > u1).
fn sub_cubic(c: &[Pt; 4], u0: f32, u1: f32) -> [Pt; 4] {
    let (lo, hi, rev) = if u0 <= u1 { (u0, u1, false) } else { (u1, u0, true) };
    let (a1, a2, m, _, _) = split_cubic(c[0], c[1], c[2], c[3], hi.max(1e-6));
    let left = [c[0], a1, a2, m];
    let k = if hi > 1e-6 { lo / hi } else { 0.0 };
    let (_, _, m2, b1, b2) = split_cubic(left[0], left[1], left[2], left[3], k);
    let out = [m2, b1, b2, m];
    if rev {
        [out[3], out[2], out[1], out[0]]
    } else {
        out
    }
}

pub(crate) fn world_paths(s: &Shape) -> Vec<BezPath> {
    let mut paths = s.bez_paths();
    for p in &mut paths {
        for n in &mut p.nodes {
            n.p = s.m.apply(n.p.0, n.p.1);
            n.i = s.m.apply(n.i.0, n.i.1);
            n.o = s.m.apply(n.o.0, n.o.1);
        }
    }
    paths
}

fn local_vec(m: &Matrix, d: Pt) -> Pt {
    match m.invert() {
        Some(inv) => (inv.a * d.0 + inv.c * d.1, inv.b * d.0 + inv.d * d.1),
        None => d,
    }
}

fn path_polyline(p: &BezPath) -> (Vec<Pt>, bool) {
    let s = Shape::new(Geometry::Bezier { paths: vec![p.clone()] }, None, None, 1.0, Matrix::IDENTITY);
    flatten(&s.outline()).into_iter().next().unwrap_or((Vec::new(), p.closed))
}

fn chaikin(pts: &[Pt], closed: bool) -> Vec<Pt> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let n = pts.len();
    let mut out = Vec::with_capacity(n * 2);
    if !closed {
        out.push(pts[0]);
    }
    let segs = if closed { n } else { n - 1 };
    for i in 0..segs {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        out.push((0.75 * a.0 + 0.25 * b.0, 0.75 * a.1 + 0.25 * b.1));
        out.push((0.25 * a.0 + 0.75 * b.0, 0.25 * a.1 + 0.75 * b.1));
    }
    if !closed {
        out.push(pts[n - 1]);
    }
    out
}

impl Editor {
    // ───────────────────────── Context (group editing) ─────────────────────────

    pub(crate) fn scope(&self) -> Option<u32> {
        self.doc.scopes.last().map(|s| s.tag)
    }

    pub(crate) fn scope_shapes(&self, li: usize) -> Vec<Shape> {
        let scope = self.scope();
        self.doc.frame_shapes(li, self.frame).into_iter().filter(|s| s.tag == scope).collect()
    }

    // ───────────────────────── Merging ─────────────────────────

    /// Adds art drawn with a tool: a drawing object (J mode) or raw pieces
    /// that merge with what is already on the layer.
    pub(crate) fn add_art(&mut self, mut s: Shape) {
        if !self.can_draw() {
            return;
        }
        if s.fill == Some(self.fill) {
            self.style_new_fill(&mut s);
        }
        let base = self.doc.clone();
        if self.object_mode {
            self.sel = self.insert_shapes(vec![s], None);
        } else {
            let pieces = raw::to_raw_pieces(&s);
            self.merge_raw(self.layer, pieces);
        }
        self.commit_from(base);
    }

    /// Adds a fill coming from the brush or paint bucket.
    pub(crate) fn add_region(&mut self, r: &Region, fill: Rgba, behind: bool) {
        if r.is_empty() || !self.can_draw() {
            return;
        }
        let base = self.doc.clone();
        if self.object_mode {
            if let Some(mut s) = paint::region_shape(r, fill) {
                if fill == self.fill {
                    self.style_new_fill(&mut s);
                }
                let at = if behind { Some(0) } else { None };
                self.insert_shapes(vec![s], at);
            }
        } else {
            let mut pieces = paint::fill_pieces(r, fill);
            if fill == self.fill {
                self.style_new_fills(&mut pieces);
            }
            self.merge_raw(self.layer, pieces);
        }
        self.commit_from(base);
    }

    /// Merges raw pieces into layer `li` (current context) and returns the new resulting ids.
    pub(crate) fn merge_raw(&mut self, li: usize, pieces: Vec<Shape>) -> Vec<u32> {
        let scope = self.scope();
        let floating = self.floating.clone();
        let k = self.doc.layers[li].ensure_key(self.frame);
        let v = &mut self.doc.layers[li].keys.get_mut(&k).unwrap().shapes;
        let is_base = |s: &Shape| s.raw && s.tag == scope && !floating.contains(&s.id);
        let existing: Vec<Shape> = v.iter().filter(|s| is_base(s)).cloned().collect();
        v.retain(|s| !is_base(s));
        let merged = raw::merge(existing, pieces);
        self.put_shapes(li, k, scope, merged)
    }

    /// Inserts already computed shapes (assigns ids, context and stacking order).
    fn put_shapes(&mut self, li: usize, k: u32, scope: Option<u32>, shapes: Vec<Shape>) -> Vec<u32> {
        let mut fresh = Vec::new();
        let mut ready = Vec::new();
        for mut s in shapes {
            if s.id == 0 {
                s.id = self.doc.alloc_id();
                fresh.push(s.id);
            }
            s.tag = scope;
            ready.push(s);
        }
        let floating = self.floating.clone();
        let v = &mut self.doc.layers[li].keys.get_mut(&k).unwrap().shapes;
        v.extend(ready);
        normalize_order(v, scope, &floating);
        let present: Vec<u32> = v.iter().map(|s| s.id).collect();
        self.sel.retain(|id| present.contains(id));
        fresh
    }

    /// Moved or pasted raw pieces stay "floating" above the rest until
    /// they are deselected; then they merge (as in Flash).
    pub(crate) fn mark_floating(&mut self) {
        for (_, s) in self.selected_shapes() {
            if s.raw && !self.floating.contains(&s.id) {
                self.floating.push(s.id);
            }
        }
    }

    pub(crate) fn settle(&mut self) {
        if self.floating.is_empty() {
            return;
        }
        let fl = std::mem::take(&mut self.floating);
        let base = self.doc.clone();
        for li in 0..self.doc.layers.len() {
            if !self.doc.frame_shapes(li, self.frame).iter().any(|s| s.raw && fl.contains(&s.id)) {
                continue;
            }
            let k = self.doc.layers[li].ensure_key(self.frame);
            let v = &mut self.doc.layers[li].keys.get_mut(&k).unwrap().shapes;
            let pieces: Vec<Shape> = v.iter().filter(|s| s.raw && fl.contains(&s.id)).cloned().collect();
            let scope = pieces[0].tag;
            v.retain(|s| !(s.raw && fl.contains(&s.id)));
            let existing: Vec<Shape> = v.iter().filter(|s| s.raw && s.tag == scope).cloned().collect();
            v.retain(|s| !(s.raw && s.tag == scope));
            let merged = raw::merge(existing, pieces);
            self.put_shapes(li, k, scope, merged);
        }
        self.commit_from(base);
    }

    // ───────────────────────── Area selection (cuts raw shapes) ─────────────────────────

    pub(crate) fn marquee_select(&mut self, r: BBox, add_to: Vec<u32>) {
        let rect: Region = vec![vec![[r.x0, r.y0], [r.x1, r.y0], [r.x1, r.y1], [r.x0, r.y1]]];
        let inside = |s: &Shape| {
            let b = s.world_bounds();
            b.x0 >= r.x0 && b.x1 <= r.x1 && b.y0 >= r.y0 && b.y1 <= r.y1
        };
        self.area_select(&rect, r, &inside, add_to);
    }

    /// Lasso: selects what lies inside the polygon and cuts the raw shapes it crosses.
    pub(crate) fn lasso_select(&mut self, poly: &[Pt], add_to: Vec<u32>) {
        if poly.len() < 3 {
            return;
        }
        let area: Region = vec![poly.iter().map(|p| [p.0, p.1]).collect()];
        let bb = poly.iter().fold(BBox::EMPTY, |mut b, p| {
            b.add(*p);
            b
        });
        let inside = |s: &Shape| {
            let leaves = s.leaves(&Matrix::IDENTITY);
            let pts: Vec<Pt> = leaves
                .iter()
                .flat_map(|l| flatten(&l.outline()).into_iter().flat_map(move |(v, _)| v.into_iter().map(move |q| l.m.apply(q.0, q.1))))
                .collect();
            !pts.is_empty() && pts.iter().all(|q| point_in_polygon(*q, poly))
        };
        self.area_select(&area, bb, &inside, add_to);
    }

    /// Area selection: whatever lies inside is selected whole; raw shapes
    /// partly inside are cut along the area's edge, as in Flash.
    fn area_select(&mut self, rect: &Region, r: BBox, inside: &dyn Fn(&Shape) -> bool, add_to: Vec<u32>) {
        let rect = rect.clone();
        let mut sel = add_to;
        let base = self.doc.clone();
        for li in self.editable_layers() {
            let shapes = self.scope_shapes(li);
            let mut replace: Vec<(u32, Vec<Shape>, Vec<Shape>)> = Vec::new();
            for s in shapes {
                let b = s.world_bounds();
                if !b.intersects(&r) {
                    continue;
                }
                if inside(&s) {
                    if !sel.contains(&s.id) {
                        sel.push(s.id);
                    }
                    continue;
                }
                if !s.raw {
                    continue;
                }
                // Raw shape partly inside: cut by the rectangle, as in Flash.
                if s.fill.is_some() {
                    let fr = paint::fill_region(&s);
                    let inn = paint::intersect(&fr, &rect);
                    if paint::region_area(&inn) < 0.01 {
                        continue;
                    }
                    let out = paint::fill_pieces_like(&paint::difference(&fr, &rect), &s);
                    replace.push((s.id, out, paint::fill_pieces_like(&inn, &s)));
                } else if let Some(c) = s.stroke {
                    let lines = paint::stroke_lines(&s);
                    let inn = paint::lines_inside(&lines, &rect);
                    if paint::total_length(&inn) < 0.01 {
                        continue;
                    }
                    let w = s.stroke_width * s.m.avg_scale();
                    let out = paint::stroke_pieces(&paint::lines_outside(&lines, &rect), c, w);
                    replace.push((s.id, out, paint::stroke_pieces(&inn, c, w)));
                }
            }
            // With "Edit Multiple Frames": items from other keyframes that lie inside, whole.
            if self.edit_multiple {
                let here: Vec<u32> = self.scope_shapes(li).iter().map(|s| s.id).collect();
                for s in self.pick_shapes(li).into_iter().filter(|s| !here.contains(&s.id)) {
                    if inside(&s) && !sel.contains(&s.id) {
                        sel.push(s.id);
                    }
                }
            }
            if replace.is_empty() {
                continue;
            }
            let k = self.doc.layers[li].ensure_key(self.frame);
            let scope = self.scope();
            let ids: Vec<u32> = replace.iter().map(|r| r.0).collect();
            self.doc.layers[li].keys.get_mut(&k).unwrap().shapes.retain(|s| !ids.contains(&s.id));
            for (id, out, inn) in replace {
                let mut out = out;
                if let Some(first) = out.first_mut() {
                    first.id = id;
                }
                self.put_shapes(li, k, scope, out);
                sel.extend(self.put_shapes(li, k, scope, inn));
            }
        }
        self.sel = sel;
        self.commit_from(base);
    }

    // ───────────────────────── Double click ─────────────────────────

    pub(crate) fn double_click_at(&mut self, p: Pt) {
        let Some((li, id)) = self.hit_top(p) else {
            if !self.doc.scopes.is_empty() {
                self.exit_group();
            } else if self.doc.sym_edit.last().map_or(false, |e| e.in_place.is_some()) {
                // Double click outside what is being edited in place: go up one level.
                self.exit_symbol();
            }
            return;
        };
        let shapes = self.scope_shapes(li);
        let Some(s) = shapes.iter().find(|s| s.id == id).cloned() else { return };
        self.layer = li;
        if s.is_instance() {
            self.edit_instance_in_place(li, id);
            return;
        }
        // Double click on text: edit it with the Text tool.
        if matches!(s.geom, Geometry::Text(_)) {
            self.begin_text_edit(li, id);
            return;
        }
        if s.is_group() || !s.raw {
            self.enter_group(li, id);
            return;
        }
        let tol = 1.5 * self.px();
        if s.fill.is_some() {
            // Fill + the strokes bordering it.
            let edges: Vec<Vec<Pt>> = flatten(&s.outline())
                .into_iter()
                .map(|(pts, _)| pts.iter().map(|q| s.m.apply(q.0, q.1)).collect())
                .collect();
            let mut sel = vec![id];
            for t in shapes.iter().filter(|t| t.raw && t.stroke.is_some()) {
                let w = t.stroke_width * t.m.avg_scale() / 2.0 + tol;
                let pts: Vec<Pt> = paint::stroke_lines(t).iter().flatten().map(|p| (p[0], p[1])).collect();
                let near = pts.iter().filter(|q| edges.iter().any(|e| dist_to_polyline(**q, e, true) <= w)).count();
                if !pts.is_empty() && near * 10 >= pts.len() * 8 {
                    sel.push(t.id);
                }
            }
            self.sel = sel;
        } else {
            // All connected strokes.
            let strokes: Vec<&Shape> = shapes.iter().filter(|t| t.raw && t.stroke.is_some() && t.fill.is_none()).collect();
            let ends = |t: &Shape| -> Vec<Pt> {
                paint::stroke_lines(t).iter().flat_map(|l| [(l[0][0], l[0][1]), (l[l.len() - 1][0], l[l.len() - 1][1])]).collect()
            };
            let mut sel = vec![id];
            let mut frontier = vec![s.clone()];
            while let Some(cur) = frontier.pop() {
                let ce = ends(&cur);
                for t in &strokes {
                    if sel.contains(&t.id) {
                        continue;
                    }
                    if ends(t).iter().any(|e| ce.iter().any(|c| dist(*c, *e) < 0.6)) {
                        sel.push(t.id);
                        frontier.push((*t).clone());
                    }
                }
            }
            self.sel = sel;
        }
    }

    // ───────────────────────── Groups and drawing objects ─────────────────────────

    /// Enters editing of a group or drawing object (double click): its content
    /// moves to the layer with the accumulated matrix and everything else is dimmed.
    pub(crate) fn enter_group(&mut self, li: usize, id: u32) {
        self.settle();
        let base = self.doc.clone();
        let k = self.doc.layers[li].ensure_key(self.frame);
        let tag = self.doc.alloc_id();
        let v = &mut self.doc.layers[li].keys.get_mut(&k).unwrap().shapes;
        let Some(idx) = v.iter().position(|s| s.id == id) else { return };
        let g = v.remove(idx);
        let (children, object) = match &g.geom {
            Geometry::Group { children, object } => (
                children
                    .iter()
                    .map(|c| {
                        let mut c = c.clone();
                        c.m = g.m.mul(&c.m);
                        c
                    })
                    .collect::<Vec<_>>(),
                *object,
            ),
            // A simple drawing object is edited as a raw shape.
            _ => (raw::to_raw_pieces(&g), true),
        };
        let mut children = children;
        for c in &mut children {
            c.tag = Some(tag);
        }
        let mut need_ids = Vec::new();
        for (n, c) in children.into_iter().enumerate() {
            if c.id == 0 {
                need_ids.push(idx + n);
            }
            v.insert(idx + n, c);
        }
        for i in need_ids {
            let nid = self.doc.next_id;
            self.doc.next_id += 1;
            self.doc.layers[li].keys.get_mut(&k).unwrap().shapes[i].id = nid;
        }
        normalize_order(&mut self.doc.layers[li].keys.get_mut(&k).unwrap().shapes, Some(tag), &[]);
        self.doc.scopes.push(Scope { tag, layer: li, group_id: g.id, object });
        self.layer = li;
        self.sel.clear();
        self.anchors.clear();
        self.commit_from(base);
    }

    pub(crate) fn exit_group(&mut self) {
        self.settle();
        let Some(sc) = self.doc.scopes.last().cloned() else { return };
        let base = self.doc.clone();
        let li = sc.layer.min(self.doc.layers.len() - 1);
        self.doc.scopes.pop();
        let parent = self.scope();
        let k = self.doc.layers[li].ensure_key(self.frame);
        let v = &mut self.doc.layers[li].keys.get_mut(&k).unwrap().shapes;
        let first = v.iter().position(|s| s.tag == Some(sc.tag));
        let children: Vec<Shape> = v.iter().filter(|s| s.tag == Some(sc.tag)).cloned().collect();
        v.retain(|s| s.tag != Some(sc.tag));
        if let (Some(first), false) = (first, children.is_empty()) {
            let g = make_group(sc.group_id, children, sc.object, parent);
            v.insert(first.min(v.len()), g);
            normalize_order(v, parent, &[]);
            self.sel = vec![sc.group_id];
        }
        self.layer = li;
        self.anchors.clear();
        self.commit_from(base);
    }

    pub(crate) fn exit_all_groups(&mut self) {
        while !self.doc.scopes.is_empty() {
            self.exit_group();
        }
    }

    /// Ctrl+G
    pub(crate) fn group_selection(&mut self) {
        self.settle();
        let li = self.layer;
        let ids: Vec<u32> = self.selected_shapes().into_iter().filter(|(l, _)| *l == li).map(|(_, s)| s.id).collect();
        if ids.is_empty() {
            return;
        }
        let base = self.doc.clone();
        let scope = self.scope();
        let gid = self.doc.alloc_id();
        let k = self.doc.layers[li].ensure_key(self.frame);
        let v = &mut self.doc.layers[li].keys.get_mut(&k).unwrap().shapes;
        let idxs: Vec<usize> = (0..v.len()).filter(|&i| ids.contains(&v[i].id)).collect();
        let top = *idxs.last().unwrap();
        let children: Vec<Shape> = idxs.iter().map(|&i| v[i].clone()).collect();
        v.retain(|s| !ids.contains(&s.id));
        let pos = (top + 1 - idxs.len()).min(v.len());
        v.insert(pos, make_group(gid, children, false, scope));
        normalize_order(v, scope, &[]);
        self.sel = vec![gid];
        self.floating.retain(|f| !ids.contains(f));
        self.commit_from(base);
    }

    /// Ctrl+Shift+G (and Ctrl+B, which also turns objects into raw shapes).
    pub(crate) fn ungroup_selection(&mut self, explode_objects: bool) {
        let li = self.layer;
        let targets: Vec<Shape> = self.selected_shapes().into_iter().filter(|(l, _)| *l == li).map(|(_, s)| s).collect();
        if targets.is_empty() {
            return;
        }
        let base = self.doc.clone();
        let scope = self.scope();
        let k = self.doc.layers[li].ensure_key(self.frame);
        let mut new_sel = Vec::new();
        let mut floating = Vec::new();
        for t in targets {
            let mut replacement: Vec<Shape> = match &t.geom {
                Geometry::Group { children, .. } => children
                    .iter()
                    .map(|c| {
                        let mut c = c.clone();
                        c.m = t.m.mul(&c.m);
                        c
                    })
                    .collect(),
                // Break apart an instance: its content, with the color effect applied.
                Geometry::Instance(inst) => crate::symbols::instance_leaves(&t, inst, &Matrix::IDENTITY, 0, true)
                    .into_iter()
                    .map(|mut c| {
                        c.id = 0;
                        c
                    })
                    .collect(),
                Geometry::Text(_) if explode_objects => self.break_text(&t),
                _ if explode_objects && !t.raw => raw::to_raw_pieces(&t),
                _ => {
                    new_sel.push(t.id);
                    continue;
                }
            };
            for (n, c) in replacement.iter_mut().enumerate() {
                if c.id == 0 || (n > 0 && c.id == t.id) {
                    c.id = self.doc.alloc_id();
                }
                c.tag = scope;
                new_sel.push(c.id);
                if c.raw {
                    floating.push(c.id);
                }
            }
            let v = &mut self.doc.layers[li].keys.get_mut(&k).unwrap().shapes;
            let Some(idx) = v.iter().position(|s| s.id == t.id) else { continue };
            v.remove(idx);
            for (n, c) in replacement.into_iter().enumerate() {
                v.insert((idx + n).min(v.len()), c);
            }
        }
        // What comes out stays selected; raw pieces float until deselected.
        self.floating.extend(floating);
        let fl = self.floating.clone();
        normalize_order(&mut self.doc.layers[li].keys.get_mut(&k).unwrap().shapes, scope, &fl);
        self.sel = new_sel;
        self.commit_from(base);
    }

    // ───────────────────────── Bending lines and moving corners (Selection) ─────────────────────────

    /// Corner of an unselected raw shape under the pointer.
    pub(crate) fn raw_corner_at(&self, x: f32, y: f32) -> Option<(usize, Pt)> {
        let vm = self.view_matrix();
        let r = 5.0 * self.dpr;
        for li in self.editable_layers() {
            for s in self.scope_shapes(li).iter().rev().filter(|s| s.raw && !self.sel.contains(&s.id)) {
                for bp in world_paths(s) {
                    for (ni, n) in bp.nodes.iter().enumerate() {
                        let end = !bp.closed && (ni == 0 || ni + 1 == bp.nodes.len());
                        if (!n.smooth || end) && dist(vm.apply(n.p.0, n.p.1), (x, y)) <= r {
                            return Some((li, n.p));
                        }
                    }
                }
            }
        }
        None
    }

    /// Edge of an unselected raw shape under the pointer: (layer, shape, subpath, a, b, t).
    pub(crate) fn raw_edge_at(&self, x: f32, y: f32) -> Option<(usize, u32, usize, usize, usize, f32)> {
        let vm = self.view_matrix();
        let r = 4.0 * self.dpr;
        for li in self.editable_layers() {
            let mut best: Option<(f32, (usize, u32, usize, usize, usize, f32))> = None;
            for s in self.scope_shapes(li).iter().rev().filter(|s| s.raw && !self.sel.contains(&s.id)) {
                for (pi, bp) in world_paths(s).iter().enumerate() {
                    for (a, b) in bp.segments() {
                        let (na, nb) = (bp.nodes[a], bp.nodes[b]);
                        for k in 1..32 {
                            let t = k as f32 / 32.0;
                            let w = cubic_point(na.p, na.o, nb.i, nb.p, t);
                            let d = dist(vm.apply(w.0, w.1), (x, y));
                            if d <= r && best.map_or(true, |(bd, _)| d < bd) {
                                best = Some((d, (li, s.id, pi, a, b, t)));
                            }
                        }
                    }
                }
            }
            if let Some((_, r)) = best {
                return Some(r);
            }
        }
        None
    }

    /// All raw-shape segments that match the grabbed one
    /// (the stroke and the fill edge bend together).
    /// Spans that run along the same curve as the dragged one (a fill's
    /// edge and its line, even if they are split at different places).
    /// Also returns the parameter `t` of the dragged point on the chosen curve.
    pub(crate) fn bend_targets(&self, li: usize, id: u32, pi: usize, a: usize, b: usize, t: f32) -> (Vec<SegRef>, f32) {
        let shapes = self.scope_shapes(li);
        let Some(s) = shapes.iter().find(|s| s.id == id) else { return (Vec::new(), t) };
        let paths = world_paths(s);
        let (mut id, mut pi, mut a, mut b) = (id, pi, a, b);
        let mut master = cubic_of(&paths[pi].nodes[a], &paths[pi].nodes[b]);
        let grab = cubic_point(master[0], master[1], master[2], master[3], t);
        // The longest curve containing the grabbed span is bent (the whole
        // line even if the fill edge is split, or vice versa).
        loop {
            let mut longer = None;
            for sh in shapes.iter().filter(|x| x.raw) {
                for (qi, bp) in world_paths(sh).iter().enumerate() {
                    for (x, y) in bp.segments() {
                        let seg = cubic_of(&bp.nodes[x], &bp.nodes[y]);
                        let (u0, d0) = project_cubic(&seg, master[0]);
                        let (u1, d1) = project_cubic(&seg, master[3]);
                        let mid = cubic_point(master[0], master[1], master[2], master[3], 0.5);
                        if d0 < 0.6 && d1 < 0.6 && (u1 - u0).abs() < 0.98 && project_cubic(&seg, mid).1 < 1.0 && dist(seg[0], seg[3]) > dist(master[0], master[3]) + 0.5 {
                            longer = Some((sh.id, qi, x, y, seg));
                        }
                    }
                }
            }
            match longer {
                Some((i, q, x, y, seg)) => {
                    (id, pi, a, b) = (i, q, x, y);
                    master = seg;
                }
                None => break,
            }
        }
        let t = project_cubic(&master, grab).0;
        let mut out = vec![(id, pi, a, b, 0.0, 1.0)];
        for sh in shapes.iter().filter(|x| x.raw) {
            for (qi, bp) in world_paths(sh).iter().enumerate() {
                for (x, y) in bp.segments() {
                    if sh.id == id && qi == pi && x == a && y == b {
                        continue;
                    }
                    let seg = cubic_of(&bp.nodes[x], &bp.nodes[y]);
                    let (u0, d0) = project_cubic(&master, seg[0]);
                    let (u1, d1) = project_cubic(&master, seg[3]);
                    if d0 > 0.6 || d1 > 0.6 || (u1 - u0).abs() < 0.01 {
                        continue;
                    }
                    // The whole span runs along the curve (not just its endpoints).
                    let on = [0.25, 0.5, 0.75].iter().all(|&k| project_cubic(&master, cubic_point(seg[0], seg[1], seg[2], seg[3], k)).1 < 1.0);
                    if on {
                        out.push((sh.id, qi, x, y, u0, u1));
                    }
                }
            }
        }
        (out, t)
    }

    pub(crate) fn corner_targets(&self, li: usize, p: Pt) -> Vec<AnchorRef> {
        let mut out = Vec::new();
        for s in self.scope_shapes(li).iter().filter(|s| s.raw) {
            for (pi, bp) in world_paths(s).iter().enumerate() {
                for (ni, n) in bp.nodes.iter().enumerate() {
                    if dist(n.p, p) < MATCH_EPS {
                        out.push((s.id, pi, ni));
                    }
                }
            }
        }
        out
    }

    pub(crate) fn apply_bend(&mut self, targets: &[SegRef], t: f32, d: Pt) {
        let Some(&(mid, mpi, ma, mb, _, _)) = targets.first() else { return };
        // The dragged curve, in the document, and how it ends up bent.
        let Some(ms) = self.doc.frame_shapes(self.layer, self.frame).into_iter().chain(self.selected_shapes().into_iter().map(|(_, s)| s)).find(|s| s.id == mid) else {
            return;
        };
        let mp = world_paths(&ms);
        let Some(bp) = mp.get(mpi) else { return };
        let c = cubic_of(&bp.nodes[ma], &bp.nodes[mb]);
        let tt = t.clamp(0.1, 0.9);
        let k = 1.0 / (3.0 * tt * (1.0 - tt) * ((1.0 - tt).powi(2) + tt * tt));
        let bent = [c[0], (c[1].0 + d.0 * (1.0 - tt) * k, c[1].1 + d.1 * (1.0 - tt) * k), (c[2].0 + d.0 * tt * k, c[2].1 + d.1 * tt * k), c[3]];
        // Each span takes its piece of the bent curve (including its interior endpoints).
        let mut by_shape: Vec<(u32, Vec<(usize, usize, usize, [Pt; 4])>)> = Vec::new();
        for &(id, pi, a, b, u0, u1) in targets {
            let sub = sub_cubic(&bent, u0, u1);
            match by_shape.iter_mut().find(|(i, _)| *i == id) {
                Some((_, v)) => v.push((pi, a, b, sub)),
                None => by_shape.push((id, vec![(pi, a, b, sub)])),
            }
        }
        for (id, segs) in by_shape {
            self.modify_shapes(&[id], |s| {
                let Some(inv) = s.m.invert() else { return };
                let l = |q: Pt| inv.apply(q.0, q.1);
                s.ensure_bezier();
                let Geometry::Bezier { paths } = &mut s.geom else { return };
                // Handles pinned by some span (the rest move with their anchor).
                let set_out: Vec<(usize, usize)> = segs.iter().map(|x| (x.0, x.1)).collect();
                let set_in: Vec<(usize, usize)> = segs.iter().map(|x| (x.0, x.2)).collect();
                for (pi, a, b, sub) in &segs {
                    let Some(bp) = paths.get_mut(*pi) else { continue };
                    for (n, p) in [(*a, sub[0]), (*b, sub[3])] {
                        let np = l(p);
                        let old = bp.nodes[n].p;
                        let dl = (np.0 - old.0, np.1 - old.1);
                        if dl.0.abs() + dl.1.abs() > 1e-6 {
                            bp.nodes[n].p = np;
                            if !set_in.contains(&(*pi, n)) {
                                bp.nodes[n].i = (bp.nodes[n].i.0 + dl.0, bp.nodes[n].i.1 + dl.1);
                            }
                            if !set_out.contains(&(*pi, n)) {
                                bp.nodes[n].o = (bp.nodes[n].o.0 + dl.0, bp.nodes[n].o.1 + dl.1);
                            }
                        }
                    }
                    bp.nodes[*a].o = l(sub[1]);
                    bp.nodes[*b].i = l(sub[2]);
                    bp.nodes[*a].smooth = false;
                    bp.nodes[*b].smooth = false;
                }
            });
        }
    }

    pub(crate) fn apply_corner(&mut self, targets: &[AnchorRef], d: Pt) {
        for &(id, pi, ni) in targets {
            self.modify_shapes(&[id], |s| {
                let d = local_vec(&s.m, d);
                s.ensure_bezier();
                if let Geometry::Bezier { paths } = &mut s.geom {
                    if let Some(n) = paths.get_mut(pi).and_then(|bp| bp.nodes.get_mut(ni)) {
                        n.translate(d);
                    }
                }
            });
        }
    }

    // ───────────────────────── Modify > Shape, Smooth, Straighten ─────────────────────────

    fn rework_paths(&mut self, f: impl Fn(&BezPath, f32) -> BezPath) {
        let ids: Vec<u32> = self.selected_shapes().into_iter().filter(|(_, s)| !s.is_container()).map(|(_, s)| s.id).collect();
        if ids.is_empty() {
            return;
        }
        let base = self.doc.clone();
        self.modify_shapes(&ids, |s| {
            let scale = s.m.avg_scale();
            let paths: Vec<BezPath> = s.bez_paths().iter().map(|p| f(p, scale)).collect();
            s.geom = Geometry::Bezier { paths };
        });
        self.mark_floating();
        self.commit_from(base);
    }

    pub(crate) fn smooth_sel(&mut self) {
        self.rework_paths(|p, scale| {
            let (pts, closed) = path_polyline(p);
            let sm = chaikin(&pts, closed);
            let r = fit::fit_path(&sm, closed, 0.6 / scale, 100.0);
            if r.nodes.len() >= 2 { r } else { p.clone() }
        });
    }

    pub(crate) fn straighten_sel(&mut self) {
        self.rework_paths(|p, scale| {
            let (mut pts, closed) = path_polyline(p);
            if closed && !pts.is_empty() {
                pts.push(pts[0]);
            }
            let mut s = simplify(&pts, 2.0 / scale);
            if closed {
                s.pop();
            }
            if s.len() < 2 {
                return p.clone();
            }
            BezPath { nodes: s.into_iter().map(Node::corner).collect(), closed: closed && p.closed }
        });
    }

    /// Modify > Shape > Optimize Curves. Returns the summary for the notice.
    pub(crate) fn optimize_sel(&mut self, amount: f32) -> String {
        let segs = |sh: &[(usize, Shape)]| -> usize { sh.iter().map(|(_, s)| s.bez_paths().iter().map(|p| p.segments().len()).sum::<usize>()).sum() };
        let before = segs(&self.selected_shapes());
        let err = 0.1 + amount.clamp(0.0, 100.0) * 0.04;
        self.rework_paths(|p, scale| {
            let (pts, closed) = path_polyline(p);
            let r = fit::fit_path(&pts, closed, err / scale, 40.0);
            if r.nodes.len() >= 2 { r } else { p.clone() }
        });
        let after = segs(&self.selected_shapes());
        let pct = if before > 0 { 100.0 * (before as f32 - after as f32) / before as f32 } else { 0.0 };
        format!("Curvas originales: {before} · optimizadas: {after} · reducción: {pct:.0}%")
    }

    /// Modify > Shape > Convert Lines to Fills.
    pub(crate) fn lines_to_fills_sel(&mut self) {
        let sel = self.selected_shapes();
        if sel.is_empty() {
            return;
        }
        let base = self.doc.clone();
        let mut new_sel = Vec::new();
        for (li, s) in sel {
            let Some(c) = s.stroke else {
                new_sel.push(s.id);
                continue;
            };
            let r = paint::stroke_region(&s);
            let k = self.doc.layers[li].ensure_key(self.frame);
            let v = &mut self.doc.layers[li].keys.get_mut(&k).unwrap().shapes;
            let Some(i) = v.iter().position(|x| x.id == s.id) else { continue };
            if s.raw || s.fill.is_none() || !s.is_closed() {
                v.remove(i);
            } else {
                v[i].stroke = None;
                new_sel.push(s.id);
            }
            let scope = self.scope();
            let pieces = if s.raw { paint::fill_pieces(&r, c) } else { paint::region_shape(&r, c).into_iter().collect() };
            new_sel.extend(self.put_shapes(li, k, scope, pieces));
        }
        self.sel = new_sel;
        self.mark_floating();
        self.commit_from(base);
    }

    fn offset_region(r: &Region, d: f32) -> Region {
        if d.abs() < 1e-3 {
            return r.clone();
        }
        let tip = paint::stamp(paint::Tip::Round, 2.0 * d.abs());
        let ring = paint::union_all(
            r.iter()
                .map(|c| {
                    let mut pts: Vec<Pt> = c.iter().map(|p| (p[0], p[1])).collect();
                    pts.push(pts[0]);
                    paint::sweep(&pts, &tip)
                })
                .collect(),
        );
        if d > 0.0 {
            paint::union_all(vec![r.clone(), ring])
        } else {
            paint::difference(r, &ring)
        }
    }

    fn replace_fill(&mut self, li: usize, s: &Shape, pieces: Vec<Shape>) -> Vec<u32> {
        let k = self.doc.layers[li].ensure_key(self.frame);
        let scope = self.scope();
        let v = &mut self.doc.layers[li].keys.get_mut(&k).unwrap().shapes;
        let keep_stroke = !s.raw && s.stroke.is_some();
        if let Some(i) = v.iter().position(|x| x.id == s.id) {
            v.remove(i);
        }
        let mut pieces = pieces;
        for p in &mut pieces {
            p.raw = s.raw;
            if keep_stroke {
                p.stroke = s.stroke;
                p.stroke_width = s.stroke_width * s.m.avg_scale();
            }
        }
        if let Some(first) = pieces.first_mut() {
            first.id = s.id;
        }
        let mut ids = vec![s.id];
        ids.extend(self.put_shapes(li, k, scope, pieces));
        ids
    }

    /// Modify > Shape > Expand Fill (distance > 0 expands, < 0 insets).
    pub(crate) fn expand_fill_sel(&mut self, d: f32) {
        let sel: Vec<(usize, Shape)> = self.selected_shapes().into_iter().filter(|(_, s)| s.fill.is_some() && s.is_closed()).collect();
        if sel.is_empty() {
            self.message = "Selecciona un relleno".into();
            return;
        }
        let base = self.doc.clone();
        let mut new_sel = Vec::new();
        for (li, s) in sel {
            let r = Self::offset_region(&paint::fill_region(&s), d);
            let pieces = if s.raw { paint::fill_pieces_like(&r, &s) } else { paint::region_shape_like(&r, &s).into_iter().collect() };
            new_sel.extend(self.replace_fill(li, &s, pieces));
        }
        self.sel = new_sel;
        self.mark_floating();
        self.commit_from(base);
    }

    /// Modify > Shape > Soften Fill Edges.
    pub(crate) fn soften_sel(&mut self, d: f32, steps: u32, expand: bool) {
        let sel: Vec<(usize, Shape)> = self.selected_shapes().into_iter().filter(|(_, s)| s.fill.is_some() && s.is_closed()).collect();
        if sel.is_empty() {
            self.message = "Selecciona un relleno".into();
            return;
        }
        let steps = steps.clamp(1, 50);
        let base = self.doc.clone();
        let mut new_sel = Vec::new();
        for (li, s) in sel {
            let r = paint::fill_region(&s);
            let c = s.fill.unwrap();
            let core = if expand { r.clone() } else { Self::offset_region(&r, -d) };
            let mut pieces: Vec<Shape> = if s.raw { paint::fill_pieces_like(&core, &s) } else { paint::region_shape_like(&core, &s).into_iter().collect() };
            let mut prev = core.clone();
            for i in 1..=steps {
                let off = if expand { d * i as f32 / steps as f32 } else { -d + d * i as f32 / steps as f32 };
                let next = Self::offset_region(&r, off);
                let ring = paint::difference(&next, &prev);
                let a = (c.a as f32 * (1.0 - i as f32 / (steps as f32 + 1.0))).round() as u8;
                let rc = Rgba { a, ..c };
                if s.raw {
                    pieces.extend(paint::fill_pieces(&ring, rc));
                } else {
                    pieces.extend(paint::region_shape(&ring, rc));
                }
                prev = next;
            }
            new_sel.extend(self.replace_fill(li, &s, pieces));
        }
        self.sel = new_sel;
        self.commit_from(base);
    }

    // ───────────────────────── Snapping (magnet, grid, pixels) ─────────────────────────

    pub(crate) fn build_snap_targets(&mut self, exclude: &[u32]) {
        let mut t = vec![
            (0.0, 0.0),
            (self.doc.width, 0.0),
            (self.doc.width, self.doc.height),
            (0.0, self.doc.height),
            (self.doc.width / 2.0, self.doc.height / 2.0),
        ];
        if self.snap_objects {
            for l in self.doc.layers.iter().filter(|l| l.visible) {
                for s in l.frame_shapes(self.frame) {
                    if exclude.contains(&s.id) {
                        continue;
                    }
                    for leaf in s.leaves(&Matrix::IDENTITY) {
                        for bp in world_paths(&leaf) {
                            t.extend(bp.nodes.iter().map(|n| n.p));
                        }
                        t.push(leaf.world_bounds().center());
                    }
                }
            }
        }
        if let Some(pen) = &self.pen {
            t.extend(pen.nodes.iter().map(|n| n.p));
        }
        self.snap_targets = t;
    }

    fn nearest_target(&self, p: Pt, tol: f32) -> Option<(Pt, f32)> {
        let mut best: Option<(Pt, f32)> = None;
        if self.snap_objects {
            for &t in &self.snap_targets {
                let d = dist(t, p);
                if d <= tol && best.map_or(true, |(_, bd)| d < bd) {
                    best = Some((t, d));
                }
            }
        }
        if best.is_none() {
            best = self.guide_snap(p);
        }
        if best.is_none() && self.snap_grid {
            let g = self.grid_size.max(1.0);
            let q = ((p.0 / g).round() * g, (p.1 / g).round() * g);
            let d = dist(q, p);
            if d <= tol {
                best = Some((q, d));
            }
        }
        best
    }

    pub(crate) fn snap(&mut self, p: Pt) -> Pt {
        self.snap_hit = None;
        if let Some((q, _)) = self.nearest_target(p, 8.0 * self.px()) {
            self.snap_hit = Some(q);
            return q;
        }
        if self.snap_pixels {
            return (p.0.round(), p.1.round());
        }
        p
    }

    /// Adjusts an offset so that a corner, edge or center of the
    /// selection lands on a snap point.
    pub(crate) fn snap_delta(&mut self, b: BBox, d: Pt) -> Pt {
        self.snap_hit = None;
        if b.is_empty() {
            return d;
        }
        let c = b.center();
        let cands = [(b.x0, b.y0), (b.x1, b.y0), (b.x1, b.y1), (b.x0, b.y1), c, (c.0, b.y0), (c.0, b.y1), (b.x0, c.1), (b.x1, c.1)];
        let tol = 8.0 * self.px();
        let mut best: Option<(Pt, Pt, f32)> = None;
        for q in cands {
            let moved = (q.0 + d.0, q.1 + d.1);
            if let Some((t, dd)) = self.nearest_target(moved, tol) {
                if best.map_or(true, |(_, _, bd)| dd < bd) {
                    best = Some((q, t, dd));
                }
            }
        }
        match best {
            Some((q, t, _)) => {
                self.snap_hit = Some(t);
                (t.0 - q.0, t.1 - q.1)
            }
            None if self.snap_pixels => {
                let moved = (b.x0 + d.0, b.y0 + d.1);
                (moved.0.round() - b.x0, moved.1.round() - b.y0)
            }
            None => d,
        }
    }

    // ───────────────────────── Helper drawing ─────────────────────────

    pub(crate) fn draw_grid(&self, pm: &mut Pixmap) {
        if !self.show_grid {
            return;
        }
        let g = self.grid_size.max(2.0);
        if g * self.zoom < 4.0 {
            return;
        }
        let vm = self.view_matrix();
        let (x0, y0) = vm.apply(0.0, 0.0);
        let (x1, y1) = vm.apply(self.doc.width, self.doc.height);
        let c = Rgba { a: 110, ..self.grid_color };
        let mut x = 0.0;
        while x <= self.doc.width + 0.01 {
            let sx = vm.apply(x, 0.0).0.floor();
            render::fill_rect(pm, sx, y0, self.dpr.round().max(1.0), y1 - y0, c);
            x += g;
        }
        let mut y = 0.0;
        while y <= self.doc.height + 0.01 {
            let sy = vm.apply(0.0, y).1.floor();
            render::fill_rect(pm, x0, sy, x1 - x0, self.dpr.round().max(1.0), c);
            y += g;
        }
    }

    /// Dot pattern over the selected raw shapes, as in Flash.
    pub(crate) fn draw_hatch(&self, pm: &mut Pixmap, s: &Shape, vm: &Matrix) {
        let Some(path) = render::build_path(&s.outline()) else { return };
        let m = vm.mul(&s.m);
        let Some(inv) = m.invert() else { return };
        let mut pat = Pixmap::new(4, 4).unwrap();
        {
            let data = pat.data_mut();
            let mut set = |x: usize, y: usize, v: [u8; 4]| data[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4].copy_from_slice(&v);
            set(0, 0, [255, 255, 255, 255]);
            set(2, 2, [255, 255, 255, 255]);
            set(1, 1, [30, 30, 30, 255]);
            set(3, 3, [30, 30, 30, 255]);
        }
        let shader = Pattern::new(pat.as_ref(), SpreadMode::Repeat, FilterQuality::Nearest, 0.9, inv.to_skia());
        let mut paint = Paint::default();
        paint.shader = shader;
        paint.anti_alias = false;
        if s.is_closed() && s.fill.is_some() {
            pm.fill_path(&path, &paint, tiny_skia::FillRule::EvenOdd, m.to_skia(), None);
        }
        if s.stroke.is_some() {
            let w = s.stroke_width + 3.0 * self.dpr / m.avg_scale();
            pm.stroke_path(&path, &paint, &Stroke { width: w, ..Default::default() }, m.to_skia(), None);
        }
        let _ = Transform::identity();
    }
}

fn make_group(id: u32, mut children: Vec<Shape>, object: bool, tag: Option<u32>) -> Shape {
    let b = children.iter().fold(BBox::EMPTY, |b, c| b.union(&c.world_bounds()));
    let c = if b.is_empty() { (0.0, 0.0) } else { b.center() };
    let back = Matrix::translate(-c.0, -c.1);
    for ch in &mut children {
        ch.m = back.mul(&ch.m);
        ch.tag = None;
    }
    let mut g = Shape::new(Geometry::Group { children, object }, None, None, 1.0, Matrix::translate(c.0, c.1));
    g.id = id;
    g.tag = tag;
    g
}
