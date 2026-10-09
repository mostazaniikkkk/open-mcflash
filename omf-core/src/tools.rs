//! Behavior of each tool on the canvas: drags, previews,
//! selection overlays and paint operations.

use super::*;
use crate::paint::{self, Recognized};
use crate::render;
use tiny_skia::{PathBuilder, Stroke, Transform};

const ANCHOR_PX: f32 = 6.0;

fn constrain45(o: Pt, p: Pt) -> Pt {
    let (dx, dy) = (p.0 - o.0, p.1 - o.1);
    let step = std::f32::consts::FRAC_PI_4;
    let a = (dy.atan2(dx) / step).round() * step;
    let len = (dx * dx + dy * dy).sqrt();
    (o.0 + len * a.cos(), o.1 + len * a.sin())
}

fn mirror(node: &Node, h: Pt, len: f32) -> Pt {
    let (dx, dy) = (node.p.0 - h.0, node.p.1 - h.1);
    let l = (dx * dx + dy * dy).sqrt();
    if l < 1e-6 {
        return node.p;
    }
    (node.p.0 + dx / l * len, node.p.1 + dy / l * len)
}

/// Vector `d` (world) expressed in the local space of `m` (without translation).
fn local_vec(m: &Matrix, d: Pt) -> Pt {
    match m.invert() {
        Some(inv) => (inv.a * d.0 + inv.c * d.1, inv.b * d.0 + inv.d * d.1),
        None => d,
    }
}

fn world_paths(s: &Shape) -> Vec<BezPath> {
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

fn bez_mut(s: &mut Shape) -> &mut Vec<BezPath> {
    s.ensure_bezier();
    match &mut s.geom {
        Geometry::Bezier { paths } => paths,
        _ => unreachable!(),
    }
}

fn quad_path(pts: &[Pt; 4]) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].0, pts[0].1);
    for p in &pts[1..] {
        pb.line_to(p.0, p.1);
    }
    pb.close();
    pb.finish()
}

impl Editor {
    // ───────────────────────── Transform box ─────────────────────────

    /// Box frame: for a single object, its local axes (rotates with it,
    /// as in Flash); for several, the rectangle that contains them.
    pub(crate) fn frame_box(&self) -> Option<(Matrix, BBox)> {
        let v = self.selected_shapes();
        match v.len() {
            0 => None,
            1 => {
                let lb = v[0].1.local_bounds();
                (!lb.is_empty()).then_some((v[0].1.m, lb))
            }
            _ => Some((Matrix::IDENTITY, self.sel_bbox()?)),
        }
    }

    pub(crate) fn pivot_local(&self, lb: &BBox) -> Pt {
        match &self.pivot {
            Some((ids, p)) if *ids == self.sel => *p,
            _ => lb.center(),
        }
    }

    pub(crate) fn transform_center(&self) -> Option<Pt> {
        let (f, lb) = self.frame_box()?;
        let p = self.pivot_local(&lb);
        Some(f.apply(p.0, p.1))
    }

    /// With several objects the transformation point is in document
    /// coordinates and has to be moved along with them.
    pub(crate) fn shift_pivot(&mut self, d: Pt) {
        if self.sel.len() > 1 {
            if let Some((ids, p)) = &mut self.pivot {
                if *ids == self.sel {
                    p.0 += d.0;
                    p.1 += d.1;
                }
            }
        }
    }

    fn handles(&self) -> Option<(Matrix, BBox, Vec<(i8, i8, Pt)>, Pt)> {
        let (f, lb) = self.frame_box()?;
        let vf = self.view_matrix().mul(&f);
        let c = lb.center();
        let mut hs = Vec::new();
        for hy in -1i8..=1 {
            for hx in -1i8..=1 {
                if hx == 0 && hy == 0 {
                    continue;
                }
                let x = [lb.x0, c.0, lb.x1][(hx + 1) as usize];
                let y = [lb.y0, c.1, lb.y1][(hy + 1) as usize];
                hs.push((hx, hy, vf.apply(x, y)));
            }
        }
        let pv = self.pivot_local(&lb);
        Some((vf, lb, hs, vf.apply(pv.0, pv.1)))
    }

    pub(crate) fn handle_at(&self, x: f32, y: f32) -> Option<Handle> {
        use shape_tools::XMode;
        let mode = self.xform_mode;
        let (vf, lb, hs, pv) = self.handles()?;
        let r = 6.0 * self.dpr;
        for (hx, hy, p) in &hs {
            if (x - p.0).abs() <= r && (y - p.1).abs() <= r {
                // "Rotate and Skew": corners rotate and sides skew.
                if mode == XMode::RotateSkew {
                    if *hx != 0 && *hy != 0 {
                        return Some(Handle::Rotate);
                    }
                    let horiz = *hy != 0;
                    let e = if horiz { [lb.y0, 0.0, lb.y1][(*hy + 1) as usize] } else { [lb.x0, 0.0, lb.x1][(*hx + 1) as usize] };
                    return Some(Handle::Skew(horiz, e));
                }
                return Some(Handle::Scale(*hx, *hy));
            }
        }
        if dist((x, y), pv) <= 6.0 * self.dpr {
            return Some(Handle::Pivot);
        }
        if mode != XMode::Scale {
            if let Some((horiz, e)) = self.skew_edge_at(x, y) {
                return Some(Handle::Skew(horiz, e));
            }
        }
        if mode == XMode::Scale {
            return None;
        }
        let q = vf.invert()?.apply(x, y);
        let inside = q.0 >= lb.x0 && q.0 <= lb.x1 && q.1 >= lb.y0 && q.1 <= lb.y1;
        if !inside {
            for (hx, hy, p) in &hs {
                if *hx != 0 && *hy != 0 && dist((x, y), *p) <= 20.0 * self.dpr {
                    return Some(Handle::Rotate);
                }
            }
        }
        None
    }

    // ───────────────────────── Selection ─────────────────────────

    pub(crate) fn select_down(&mut self, p: Pt, shift: bool) {
        let hit = self.hit_top(p);
        let on_selection = hit.map_or(false, |(_, id)| self.sel.contains(&id));
        if !on_selection && !shift {
            // When floating pieces are dropped onto something else, Flash merges them.
            self.settle();
        }
        let hit = if on_selection { hit } else { self.hit_top(p) };
        let (sx, sy) = { let vm = self.view_matrix(); vm.apply(p.0, p.1) };
        // Shape hints: dragged with the Arrow.
        if self.tool == Tool::Select {
            if let Some((key, idx)) = self.hint_at(sx, sy) {
                self.drag = Drag::Hint { base: Box::new(self.doc.clone()), key, idx };
                return;
            }
        }
        // Corners and edges of unselected raw shapes: they move and bend.
        if !on_selection && !shift && self.tool == Tool::Select {
            let click = hit.map(|h| h.1).unwrap_or(0);
            if let Some((li, corner)) = self.raw_corner_at(sx, sy) {
                let targets = self.corner_targets(li, corner);
                self.layer = li;
                self.drag = Drag::Corner { base: Box::new(self.doc.clone()), targets, start: corner, click, moved: false };
                return;
            }
            if let Some((li, id, pi, a, b, t)) = self.raw_edge_at(sx, sy) {
                let (targets, t) = self.bend_targets(li, id, pi, a, b, t);
                self.layer = li;
                self.drag = Drag::Bend { base: Box::new(self.doc.clone()), targets, t, start: p, click, moved: false };
                return;
            }
        }
        match hit {
            Some((li, id)) => {
                self.layer = li;
                if shift {
                    if let Some(i) = self.sel.iter().position(|s| *s == id) {
                        self.sel.remove(i);
                    } else {
                        self.sel.push(id);
                    }
                } else if !self.sel.contains(&id) {
                    self.sel = vec![id];
                }
                if self.sel.contains(&id) {
                    let bbox = self.sel_bbox().unwrap_or(BBox::EMPTY);
                    self.drag = Drag::Move { start: p, base: Box::new(self.doc.clone()), delta: (0.0, 0.0), bbox };
                }
            }
            None => {
                if !shift {
                    self.sel.clear();
                }
                self.drag = Drag::Marquee { start: p, cur: p, base_sel: self.sel.clone() };
            }
        }
    }

    // ───────────────────────── Drags ─────────────────────────

    pub(crate) fn drag_move(&mut self, drag: &mut Drag, x: f32, y: f32, p: Pt, shift: bool, alt: bool) {
        match drag {
            Drag::Move { start, base, delta, bbox } => {
                let (mut dx, mut dy) = (p.0 - start.0, p.1 - start.1);
                if shift {
                    if dx.abs() > dy.abs() {
                        dy = 0.0
                    } else {
                        dx = 0.0
                    }
                }
                if dx != 0.0 || dy != 0.0 {
                    (dx, dy) = self.snap_delta(*bbox, (dx, dy));
                }
                *delta = (dx, dy);
                self.doc = (**base).clone();
                // A click without movement must not create keyframes.
                if dx != 0.0 || dy != 0.0 {
                    let ids = self.sel.clone();
                    self.modify_shapes(&ids, |s| {
                        s.m.tx += dx;
                        s.m.ty += dy;
                    });
                }
            }
            Drag::Marquee { start, cur, base_sel } => {
                *cur = p;
                // Preview: whatever is fully inside. On release, partially covered raw
                // shapes are cut by the rectangle (as in Flash).
                let r = BBox::from_pts(*start, p);
                let mut sel = base_sel.clone();
                for li in self.editable_layers() {
                    for s in self.pick_shapes(li) {
                        let b = s.world_bounds();
                        let inside = b.x0 >= r.x0 && b.x1 <= r.x1 && b.y0 >= r.y0 && b.y1 <= r.y1;
                        if inside && !sel.contains(&s.id) {
                            sel.push(s.id);
                        }
                    }
                }
                self.sel = sel;
            }
            Drag::Bend { base, targets, t, start, moved, .. } => {
                let d = (p.0 - start.0, p.1 - start.1);
                if !*moved && dist(p, *start) < 2.0 * self.px() {
                    return;
                }
                *moved = true;
                self.doc = (**base).clone();
                let (targets, t) = (targets.clone(), *t);
                self.apply_bend(&targets, t, d);
            }
            Drag::Corner { base, targets, start, moved, .. } => {
                if !*moved && dist(p, *start) < 2.0 * self.px() {
                    return;
                }
                *moved = true;
                self.build_snap_targets(&[]);
                let q = self.snap(p);
                self.doc = (**base).clone();
                let targets = targets.clone();
                self.apply_corner(&targets, (q.0 - start.0, q.1 - start.1));
            }
            Drag::Scale { base, f, lb, hx, hy, pivot } => {
                let Some(inv) = f.invert() else { return };
                let q = inv.apply(p.0, p.1);
                let edge = |h: i8, lo: f32, hi: f32| if h < 0 { lo } else { hi };
                let anchor = if alt { *pivot } else { (edge(-*hx, lb.x0, lb.x1), edge(-*hy, lb.y0, lb.y1)) };
                let factor = |h: i8, qv: f32, a: f32, lo: f32, hi: f32| {
                    if h == 0 {
                        return 1.0;
                    }
                    let e = edge(h, lo, hi);
                    if (e - a).abs() < 1e-6 { 1.0 } else { (qv - a) / (e - a) }
                };
                let mut sx = factor(*hx, q.0, anchor.0, lb.x0, lb.x1);
                let mut sy = factor(*hy, q.1, anchor.1, lb.y0, lb.y1);
                if shift && *hx != 0 && *hy != 0 {
                    let s = if sx.abs() > sy.abs() { sx } else { sy };
                    sx = s;
                    sy = s;
                }
                let fix = |v: f32| if v.abs() < 0.01 { 0.01f32.copysign(v) } else { v };
                let w = f.mul(&Matrix::about(anchor, &Matrix::scale(fix(sx), fix(sy)))).mul(&inv);
                self.doc = (**base).clone();
                if sx != 1.0 || sy != 1.0 {
                    let ids = self.sel.clone();
                    self.modify_shapes(&ids, |s| s.m = w.mul(&s.m));
                }
            }
            Drag::Rotate { base, center, a0 } => {
                let mut a = (p.1 - center.1).atan2(p.0 - center.0) - *a0;
                if shift {
                    let step = std::f32::consts::PI / 12.0;
                    a = (a / step).round() * step;
                }
                let m = Matrix::about(*center, &Matrix::rotate(a));
                self.doc = (**base).clone();
                if a != 0.0 {
                    let ids = self.sel.clone();
                    self.modify_shapes(&ids, |s| s.m = m.mul(&s.m));
                }
            }
            Drag::Pivot { f } => {
                if let Some(inv) = f.invert() {
                    self.pivot = Some((self.sel.clone(), inv.apply(p.0, p.1)));
                }
            }
            Drag::Create { cur, .. } => *cur = self.snap(p),
            Drag::TextBox { cur, .. } => *cur = p,
            Drag::Guide { idx, .. } => self.guide_drag(*idx, p),
            Drag::Scale9 { which, .. } => self.move_scale9(*which, p),
            Drag::Skew { base, f, horiz, edge, start_q, pivot } => {
                self.doc = (**base).clone();
                let (f, horiz, edge, start_q, pivot) = (*f, *horiz, *edge, *start_q, *pivot);
                self.skew_drag(&f, horiz, edge, start_q, pivot, p);
            }
            Drag::Env { base, st0, handle, start } => {
                self.doc = (**base).clone();
                let (st0, h, start) = (st0.clone(), *handle, *start);
                self.env_drag(&st0, h, start, p, alt);
            }
            Drag::Grad { base, ids, handle, g0, start } => {
                self.doc = (**base).clone();
                let (ids, h, g0, start) = (ids.clone(), *handle, g0.clone(), *start);
                self.grad_drag(&ids, h, &g0, start, p, shift);
            }
            Drag::Hint { base, key, idx } => {
                self.doc = (**base).clone();
                let q = self.snap_hint(*key, p);
                let li = self.layer;
                if let Some(h) = self.doc.layers[li].keys.get_mut(key).and_then(|k| k.hints.get_mut(*idx)) {
                    *h = q;
                }
            }
            Drag::Freehand { pts } => {
                let l = *pts.last().unwrap();
                if dist(p, l) > 0.5 * self.px() {
                    pts.push(p);
                }
            }
            Drag::Pan { sx, sy, px, py } => self.pan = (*px + x - *sx, *py + y - *sy),
            Drag::PenNode { idx, out_only, .. } => {
                if let Some(n) = self.pen.as_mut().and_then(|pen| pen.nodes.get_mut(*idx)) {
                    let h = if shift { constrain45(n.p, p) } else { p };
                    n.o = h;
                    if alt {
                        n.smooth = false;
                    } else if !*out_only {
                        n.i = (2.0 * n.p.0 - h.0, 2.0 * n.p.1 - h.1);
                        n.smooth = true;
                    }
                }
            }
            Drag::Anchors { base, start } => {
                self.doc = (**base).clone();
                let mut d = (p.0 - start.0, p.1 - start.1);
                if shift {
                    if d.0.abs() > d.1.abs() {
                        d.1 = 0.0
                    } else {
                        d.0 = 0.0
                    }
                }
                if d != (0.0, 0.0) {
                    self.move_anchors(d);
                }
            }
            Drag::BezHandle { base, id, path, node, out } => {
                self.doc = (**base).clone();
                let (pi, ni, out) = (*path, *node, *out);
                self.modify_shapes(&[*id], |s| {
                    let Some(inv) = s.m.invert() else { return };
                    let h = inv.apply(p.0, p.1);
                    let Some(n) = bez_mut(s).get_mut(pi).and_then(|bp| bp.nodes.get_mut(ni)) else { return };
                    if alt {
                        n.smooth = false;
                    }
                    if out {
                        n.o = h;
                        if n.smooth && n.has_in() {
                            n.i = mirror(n, h, dist(n.i, n.p));
                        }
                    } else {
                        n.i = h;
                        if n.smooth && n.has_out() {
                            n.o = mirror(n, h, dist(n.o, n.p));
                        }
                    }
                });
            }
            Drag::Segment { base, id, path, a, b, t, start } => {
                self.doc = (**base).clone();
                let d = (p.0 - start.0, p.1 - start.1);
                let (pi, ia, ib, t) = (*path, *a, *b, *t);
                self.modify_shapes(&[*id], |s| {
                    let d = local_vec(&s.m, d);
                    let Some(bp) = bez_mut(s).get_mut(pi) else { return };
                    let (na, nb) = (bp.nodes[ia], bp.nodes[ib]);
                    if !na.has_out() && !nb.has_in() {
                        // Straight segment: it moves as a whole, as in Photoshop.
                        bp.nodes[ia].translate(d);
                        bp.nodes[ib].translate(d);
                        return;
                    }
                    let tt = t.clamp(0.15, 0.85);
                    let k = 1.0 / (3.0 * tt * (1.0 - tt) * ((1.0 - tt).powi(2) + tt * tt));
                    let na_m = &mut bp.nodes[ia];
                    na_m.o = (na.o.0 + d.0 * (1.0 - tt) * k, na.o.1 + d.1 * (1.0 - tt) * k);
                    if na_m.smooth && na_m.has_in() {
                        let l = dist(na_m.i, na_m.p);
                        na_m.i = mirror(na_m, na_m.o, l);
                    }
                    let nb_m = &mut bp.nodes[ib];
                    nb_m.i = (nb.i.0 + d.0 * tt * k, nb.i.1 + d.1 * tt * k);
                    if nb_m.smooth && nb_m.has_out() {
                        let l = dist(nb_m.o, nb_m.p);
                        nb_m.o = mirror(nb_m, nb_m.i, l);
                    }
                });
            }
            Drag::AnchorMarquee { start, cur, base } => {
                *cur = p;
                let r = BBox::from_pts(*start, p);
                let mut a = base.clone();
                for x in self.all_anchors(|w| w.0 >= r.x0 && w.0 <= r.x1 && w.1 >= r.y0 && w.1 <= r.y1) {
                    if !a.contains(&x) {
                        a.push(x);
                    }
                }
                let mut ids: Vec<u32> = a.iter().map(|x| x.0).collect();
                ids.dedup();
                for id in ids {
                    if !self.sel.contains(&id) {
                        self.sel.push(id);
                    }
                }
                self.anchors = a;
            }
            Drag::None => {}
        }
    }

    pub(crate) fn drag_end(&mut self, drag: Drag) {
        self.snap_hit = None;
        // Any other edit leaves the bounding envelope straight around the result.
        if !matches!(drag, Drag::Env { .. } | Drag::None | Drag::Pan { .. }) {
            self.env = None;
        }
        match drag {
            Drag::Move { base, delta, .. } => {
                self.commit_from(*base);
                if delta != (0.0, 0.0) {
                    self.shift_pivot(delta);
                    self.mark_floating();
                }
            }
            Drag::Scale { base, .. } | Drag::Rotate { base, .. } => {
                let changed = *base != self.doc;
                self.commit_from(*base);
                if changed {
                    self.mark_floating();
                }
            }
            Drag::Scale9 { base, .. } | Drag::Grad { base, .. } | Drag::Skew { base, .. } | Drag::Env { base, .. } => self.commit_from(*base),
            Drag::Anchors { base, .. } | Drag::BezHandle { base, .. } | Drag::Segment { base, .. } => self.commit_from(*base),
            Drag::Bend { base, click, moved, .. } | Drag::Corner { base, click, moved, .. } => {
                if moved {
                    self.commit_from(*base);
                } else {
                    // Click without dragging: selects the piece, like a normal click.
                    self.doc = *base;
                    self.sel = if click != 0 { vec![click] } else { Vec::new() };
                }
            }
            Drag::Hint { base, key, idx } => {
                // Dropped outside the Stage: removed (in both keyframes).
                let li = self.layer;
                let h = self.doc.layers[li].keys.get(&key).and_then(|k| k.hints.get(idx)).copied();
                if let Some(h) = h {
                    let out = h.0 < 0.0 || h.1 < 0.0 || h.0 > self.doc.width || h.1 > self.doc.height;
                    if out {
                        if let Some((k, n)) = self.shape_tween_keys() {
                            for kk in [k, n] {
                                let hs = &mut self.doc.layers[li].keys.get_mut(&kk).unwrap().hints;
                                if idx < hs.len() {
                                    hs.remove(idx);
                                }
                            }
                        }
                    }
                }
                self.commit_from(*base);
            }
            Drag::Marquee { start, cur, base_sel } => {
                if self.tool == Tool::Select && dist(start, cur) > 0.5 * self.px() {
                    self.marquee_select(BBox::from_pts(start, cur), base_sel);
                }
            }
            Drag::Create { start, cur } => {
                if let Some(s) = self.preview_shape(start, cur) {
                    self.add_art(s);
                }
            }
            Drag::TextBox { start, cur } => self.create_text(start, cur),
            Drag::Guide { base, idx } => {
                let (x, y) = self.last_screen;
                self.guide_drop(idx, *base, x, y);
            }
            Drag::Freehand { pts } => match self.tool {
                Tool::Lasso => {
                    let keep = self.sel.clone();
                    self.lasso_select(&pts, keep);
                }
                Tool::Pencil => self.pencil_finish(pts),
                Tool::Brush => self.brush_finish(pts),
                Tool::Eraser => self.eraser_finish(pts),
                _ => {}
            },
            Drag::PenNode { closing: true, .. } => self.pen_finish(),
            _ => {}
        }
    }

    // ───────────────────────── Cursors ─────────────────────────

    pub(crate) fn cursor_name(&self, x: f32, y: f32) -> &'static str {
        if let Drag::Pan { .. } = self.drag {
            return "grabbing";
        }
        if self.temp_hand || self.tool == Tool::Hand {
            return "grab";
        }
        let over = || self.hit_top(self.to_doc(x, y)).is_some();
        match self.tool {
            Tool::FillXform => match self.grad_handle_at(x, y) {
                Some(fill_tools::GradHandle::Center) => "move",
                Some(fill_tools::GradHandle::Rotate) => "rotate",
                Some(fill_tools::GradHandle::Width) => "ew-resize",
                Some(fill_tools::GradHandle::Height) => "ns-resize",
                Some(fill_tools::GradHandle::Size) => "nwse-resize",
                Some(fill_tools::GradHandle::Focal) => "pointer",
                None => "default",
            },
            Tool::Transform if self.xform_mode.warps() => {
                if self.env_handle_at(x, y).is_some() {
                    "pointer"
                } else if over() {
                    "move"
                } else {
                    "default"
                }
            }
            Tool::Lasso => "lasso",
            Tool::Text => {
                if over() {
                    "text"
                } else {
                    "crosshair"
                }
            }
            Tool::Zoom => {
                if self.alt {
                    "zoom-out"
                } else {
                    "zoom-in"
                }
            }
            Tool::Transform => match self.handle_at(x, y) {
                Some(Handle::Scale(hx, hy)) => {
                    let (vf, lb, _, _) = self.handles().unwrap();
                    let c = vf.apply(lb.center().0, lb.center().1);
                    let x = [lb.x0, lb.center().0, lb.x1][(hx + 1) as usize];
                    let y = [lb.y0, lb.center().1, lb.y1][(hy + 1) as usize];
                    let h = vf.apply(x, y);
                    let deg = ((h.1 - c.1).atan2(h.0 - c.0).to_degrees() + 360.0) % 180.0;
                    match deg {
                        d if !(22.5..157.5).contains(&d) => "ew-resize",
                        d if d < 67.5 => "nwse-resize",
                        d if d < 112.5 => "ns-resize",
                        _ => "nesw-resize",
                    }
                }
                Some(Handle::Rotate) => "rotate",
                Some(Handle::Skew(horiz, _)) => {
                    if horiz {
                        "skew-x"
                    } else {
                        "skew-y"
                    }
                }
                Some(Handle::Pivot) => "pivot",
                None => {
                    if over() {
                        "move"
                    } else {
                        "default"
                    }
                }
            },
            Tool::Select if self.guide_at(x, y).is_some() || matches!(self.drag, Drag::Guide { .. }) => {
                let vertical = match &self.drag {
                    Drag::Guide { idx, .. } => self.doc.guides.get(*idx).map_or(true, |g| g.vertical),
                    _ => self.doc.guides[self.guide_at(x, y).unwrap()].vertical,
                };
                if vertical {
                    "ew-resize"
                } else {
                    "ns-resize"
                }
            }
            Tool::Select => {
                let p = self.to_doc(x, y);
                let on_sel = self.hit_top(p).map_or(false, |(_, id)| self.sel.contains(&id));
                if on_sel {
                    "move"
                } else if self.raw_corner_at(x, y).is_some() {
                    "corner"
                } else if self.raw_edge_at(x, y).is_some() {
                    "bend"
                } else if over() {
                    "move"
                } else {
                    "default"
                }
            }
            Tool::Subselect => {
                if self.handle_hit(x, y).is_some() || self.anchor_at(x, y).is_some() {
                    "direct-point"
                } else {
                    "direct"
                }
            }
            Tool::Pen => self.pen_cursor(x, y),
            Tool::Bucket => "bucket",
            Tool::Ink => "ink",
            Tool::Eyedropper => "eyedropper",
            Tool::Eraser => {
                if self.faucet {
                    "faucet"
                } else {
                    "eraser"
                }
            }
            _ => "crosshair",
        }
    }

    fn pen_cursor(&self, x: f32, y: f32) -> &'static str {
        let p = self.to_doc(x, y);
        let tol = ANCHOR_PX * self.px();
        if let Some(pen) = &self.pen {
            if pen.nodes.len() >= 2 && dist(p, pen.nodes[0].p) <= tol {
                return "pen-close";
            }
            return "pen";
        }
        if self.sel.len() == 1 && !self.alt {
            if self.anchor_at(x, y).is_some() {
                return "pen-remove";
            }
            if self.segment_at(x, y).is_some() {
                return "pen-add";
            }
        }
        "pen-new"
    }

    // ───────────────────────── Pen ─────────────────────────

    pub(crate) fn pen_down(&mut self, x: f32, y: f32, p: Pt, shift: bool, alt: bool) {
        let tol = ANCHOR_PX * self.px();
        let p = if shift { p } else { self.snap(p) };
        if let Some(pen) = &mut self.pen {
            let n = pen.nodes.len();
            if n >= 2 && dist(p, pen.nodes[0].p) <= tol {
                pen.closed = true;
                self.drag = Drag::PenNode { idx: 0, out_only: false, closing: true };
                return;
            }
            if dist(p, pen.nodes[n - 1].p) <= tol {
                // Click on the last point: removes its outgoing handle; dragging creates a new one.
                let last = &mut pen.nodes[n - 1];
                last.o = last.p;
                last.smooth = false;
                self.drag = Drag::PenNode { idx: n - 1, out_only: true, closing: false };
                return;
            }
            let q = if shift { constrain45(pen.nodes[n - 1].p, p) } else { p };
            pen.nodes.push(Node::corner(q));
            self.drag = Drag::PenNode { idx: n, out_only: false, closing: false };
            return;
        }
        if !self.can_draw() {
            return;
        }
        // Auto add/delete point on the selected path.
        if !alt && self.sel.len() == 1 {
            if let Some(a) = self.anchor_at(x, y) {
                let base = self.doc.clone();
                self.delete_anchor_refs(&[a]);
                self.commit_from(base);
                return;
            }
            if let Some((id, pi, a, b, t)) = self.segment_at(x, y) {
                let base = self.doc.clone();
                self.insert_anchor(id, pi, a, b, t);
                self.commit_from(base);
                return;
            }
        }
        self.sel.clear();
        self.anchors.clear();
        self.pen = Some(PenState { nodes: vec![Node::corner(p)], closed: false });
        self.drag = Drag::PenNode { idx: 0, out_only: false, closing: false };
    }

    pub(crate) fn commit_pen(&mut self, pen: PenState) {
        if pen.nodes.len() < 2 || !self.can_draw() {
            return;
        }
        let mut s = Shape::new(
            Geometry::Bezier { paths: vec![BezPath { nodes: pen.nodes, closed: pen.closed }] },
            (pen.closed && self.fill_on).then_some(self.fill),
            self.stroke_on.then_some(self.stroke),
            self.stroke_width,
            Matrix::IDENTITY,
        );
        if s.fill.is_none() && s.stroke.is_none() {
            s.stroke = Some(self.stroke);
        }
        let c = s.local_bounds().center();
        for n in &mut bez_mut(&mut s)[0].nodes {
            n.translate((-c.0, -c.1));
        }
        s.m = Matrix::translate(c.0, c.1);
        self.anchors.clear();
        self.add_art(s);
    }

    fn insert_anchor(&mut self, id: u32, pi: usize, a: usize, b: usize, t: f32) {
        let mut new_idx = 0;
        let paths = self.selected_shapes().into_iter().find(|(_, s)| s.id == id).map(|(_, s)| s.bez_paths());
        if let Some(paths) = paths {
            new_idx = if b == 0 { paths[pi].nodes.len() } else { b };
        }
        self.modify_shapes(&[id], |s| {
            let Some(bp) = bez_mut(s).get_mut(pi) else { return };
            let (na, nb) = (bp.nodes[a], bp.nodes[b]);
            let node = if !na.has_out() && !nb.has_in() {
                Node::corner((lerp(na.p.0, nb.p.0, t), lerp(na.p.1, nb.p.1, t)))
            } else {
                let (c1a, c2a, m, c1b, c2b) = split_cubic(na.p, na.o, nb.i, nb.p, t);
                bp.nodes[a].o = c1a;
                bp.nodes[b].i = c2b;
                Node { p: m, i: c2a, o: c1b, smooth: true }
            };
            if b == 0 {
                bp.nodes.push(node);
            } else {
                bp.nodes.insert(b, node);
            }
        });
        self.anchors = vec![(id, pi, new_idx)];
    }

    // ───────────────────────── Direct Selection ─────────────────────────

    pub(crate) fn direct_down(&mut self, x: f32, y: f32, p: Pt, shift: bool, alt: bool) {
        if let Some((id, path, node, out)) = self.handle_hit(x, y) {
            self.drag = Drag::BezHandle { base: Box::new(self.doc.clone()), id, path, node, out };
            return;
        }
        if let Some(a) = self.anchor_at(x, y) {
            if alt {
                let base = self.doc.clone();
                self.convert_point(a);
                self.commit_from(base);
                self.anchors = vec![a];
                return;
            }
            if shift {
                if let Some(i) = self.anchors.iter().position(|x| *x == a) {
                    self.anchors.remove(i);
                } else {
                    self.anchors.push(a);
                }
            } else if !self.anchors.contains(&a) {
                self.anchors = vec![a];
            }
            self.drag = Drag::Anchors { base: Box::new(self.doc.clone()), start: p };
            return;
        }
        if let Some((id, path, a, b, t)) = self.segment_at(x, y) {
            self.anchors = vec![(id, path, a), (id, path, b)];
            self.drag = Drag::Segment { base: Box::new(self.doc.clone()), id, path, a, b, t, start: p };
            return;
        }
        if let Some((li, id)) = self.hit_top(p) {
            self.layer = li;
            if shift {
                if !self.sel.contains(&id) {
                    self.sel.push(id);
                }
            } else {
                // As in Photoshop: the path stays visible with unselected anchors;
                // dragging from the fill moves the whole shape.
                self.sel = vec![id];
                self.anchors.clear();
            }
            let bbox = self.sel_bbox().unwrap_or(BBox::EMPTY);
            self.drag = Drag::Move { start: p, base: Box::new(self.doc.clone()), delta: (0.0, 0.0), bbox };
            return;
        }
        if !shift {
            self.sel.clear();
            self.anchors.clear();
        }
        self.drag = Drag::AnchorMarquee { start: p, cur: p, base: self.anchors.clone() };
    }

    /// Anchors of the editable shapes that satisfy `pred` (in document coordinates).
    pub(crate) fn all_anchors(&self, pred: impl Fn(Pt) -> bool) -> Vec<AnchorRef> {
        let mut v = Vec::new();
        for li in self.editable_layers() {
            for s in self.doc.frame_shapes(li, self.frame) {
                for (pi, bp) in world_paths(&s).iter().enumerate() {
                    for (ni, n) in bp.nodes.iter().enumerate() {
                        if pred(n.p) {
                            v.push((s.id, pi, ni));
                        }
                    }
                }
            }
        }
        v
    }

    fn anchor_at(&self, x: f32, y: f32) -> Option<AnchorRef> {
        let vm = self.view_matrix();
        let r = ANCHOR_PX * self.dpr;
        for (_, s) in self.selected_shapes().iter().rev() {
            for (pi, bp) in world_paths(s).iter().enumerate() {
                for (ni, n) in bp.nodes.iter().enumerate() {
                    if dist(vm.apply(n.p.0, n.p.1), (x, y)) <= r {
                        return Some((s.id, pi, ni));
                    }
                }
            }
        }
        None
    }

    fn handle_hit(&self, x: f32, y: f32) -> Option<(u32, usize, usize, bool)> {
        if self.tool != Tool::Subselect {
            return None;
        }
        let vm = self.view_matrix();
        let r = 5.0 * self.dpr;
        for (_, s) in self.selected_shapes() {
            let paths = world_paths(&s);
            for &(id, pi, ni) in &self.anchors {
                if id != s.id {
                    continue;
                }
                let Some(n) = paths.get(pi).and_then(|bp| bp.nodes.get(ni)) else { continue };
                if n.has_out() && dist(vm.apply(n.o.0, n.o.1), (x, y)) <= r {
                    return Some((id, pi, ni, true));
                }
                if n.has_in() && dist(vm.apply(n.i.0, n.i.1), (x, y)) <= r {
                    return Some((id, pi, ni, false));
                }
            }
        }
        None
    }

    /// Segment of the selected path under the pointer: (shape, subpath, a, b, t).
    fn segment_at(&self, x: f32, y: f32) -> Option<(u32, usize, usize, usize, f32)> {
        let vm = self.view_matrix();
        let mut best: Option<(f32, (u32, usize, usize, usize, f32))> = None;
        for (_, s) in self.selected_shapes() {
            for (pi, bp) in world_paths(&s).iter().enumerate() {
                for (a, b) in bp.segments() {
                    let (na, nb) = (bp.nodes[a], bp.nodes[b]);
                    const N: usize = 40;
                    for k in 1..N {
                        let t = k as f32 / N as f32;
                        let w = cubic_point(na.p, na.o, nb.i, nb.p, t);
                        let d = dist(vm.apply(w.0, w.1), (x, y));
                        if d <= 5.0 * self.dpr && best.map_or(true, |(bd, _)| d < bd) {
                            best = Some((d, (s.id, pi, a, b, t)));
                        }
                    }
                }
            }
        }
        best.map(|(_, r)| r)
    }

    pub(crate) fn move_anchors(&mut self, d: Pt) {
        let mut ids: Vec<u32> = self.anchors.iter().map(|a| a.0).collect();
        ids.sort();
        ids.dedup();
        for id in ids {
            let idx: Vec<(usize, usize)> = self.anchors.iter().filter(|a| a.0 == id).map(|a| (a.1, a.2)).collect();
            self.modify_shapes(&[id], |s| {
                let d = local_vec(&s.m, d);
                let paths = bez_mut(s);
                for &(pi, ni) in &idx {
                    if let Some(n) = paths.get_mut(pi).and_then(|bp| bp.nodes.get_mut(ni)) {
                        n.translate(d);
                    }
                }
            });
        }
    }

    fn convert_point(&mut self, (id, pi, ni): AnchorRef) {
        self.modify_shapes(&[id], |s| {
            let Some(bp) = bez_mut(s).get_mut(pi) else { return };
            let len = bp.nodes.len();
            let n = bp.nodes[ni];
            if n.has_in() || n.has_out() {
                bp.nodes[ni] = Node::corner(n.p);
                return;
            }
            let prev = if ni > 0 { Some(bp.nodes[ni - 1].p) } else if bp.closed { Some(bp.nodes[len - 1].p) } else { None };
            let next = if ni + 1 < len { Some(bp.nodes[ni + 1].p) } else if bp.closed { Some(bp.nodes[0].p) } else { None };
            let (a, b) = (prev.unwrap_or(n.p), next.unwrap_or(n.p));
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let l = (dx * dx + dy * dy).sqrt();
            if l < 1e-6 {
                return;
            }
            let (ux, uy) = (dx / l, dy / l);
            let (li, lo) = (dist(n.p, a) / 3.0, dist(n.p, b) / 3.0);
            let node = &mut bp.nodes[ni];
            node.i = (n.p.0 - ux * li, n.p.1 - uy * li);
            node.o = (n.p.0 + ux * lo, n.p.1 + uy * lo);
            node.smooth = true;
        });
    }

    /// Deletes anchors by joining their neighbors; removes subpaths and shapes left empty.
    pub(crate) fn delete_anchor_refs(&mut self, refs: &[AnchorRef]) {
        let mut ids: Vec<u32> = refs.iter().map(|a| a.0).collect();
        ids.sort();
        ids.dedup();
        for id in ids {
            let mut idx: Vec<(usize, usize)> = refs.iter().filter(|a| a.0 == id).map(|a| (a.1, a.2)).collect();
            idx.sort_by(|a, b| b.cmp(a));
            self.modify_shapes(&[id], |s| {
                let paths = bez_mut(s);
                for &(pi, ni) in &idx {
                    if let Some(bp) = paths.get_mut(pi) {
                        if ni < bp.nodes.len() {
                            bp.nodes.remove(ni);
                        }
                        if bp.nodes.len() < 3 {
                            bp.closed = false;
                        }
                    }
                }
                paths.retain(|bp| bp.nodes.len() >= 2);
            });
        }
        let frame = self.frame;
        for li in self.editable_layers() {
            if let Some(k) = self.doc.layers[li].key_at_or_before(frame) {
                let kf = self.doc.layers[li].keys.get_mut(&k).unwrap();
                kf.shapes.retain(|s| !matches!(&s.geom, Geometry::Bezier { paths } if paths.is_empty()));
            }
        }
        self.anchors.clear();
    }

    pub(crate) fn delete_anchors(&mut self) {
        let base = self.doc.clone();
        let refs = std::mem::take(&mut self.anchors);
        self.delete_anchor_refs(&refs);
        self.commit_from(base);
    }

    // ───────────────────────── Shapes, Pencil, Brush, Eraser, Paint Bucket ─────────────────────────

    pub(crate) fn preview_shape(&self, start: Pt, cur: Pt) -> Option<Shape> {
        if self.tool == Tool::PolyStar {
            return self.polystar_shape(start, cur);
        }
        let (mut dx, mut dy) = (cur.0 - start.0, cur.1 - start.1);
        let tool = self.tool;
        if self.shift {
            if tool == Tool::Line {
                let c = constrain45(start, cur);
                dx = c.0 - start.0;
                dy = c.1 - start.1;
            } else {
                let s = dx.abs().max(dy.abs());
                dx = s.copysign(dx);
                dy = s.copysign(dy);
            }
        }
        let (a, b) = if self.alt && tool != Tool::Line {
            ((start.0 - dx, start.1 - dy), (start.0 + dx, start.1 + dy))
        } else {
            (start, (start.0 + dx, start.1 + dy))
        };
        let (w, h) = ((b.0 - a.0).abs(), (b.1 - a.1).abs());
        if w.max(h) < 0.5 {
            return None;
        }
        let center = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        let (mut fill, mut stroke) = (self.fill_on.then_some(self.fill), self.stroke_on.then_some(self.stroke));
        let geom = match tool {
            Tool::Rect => Geometry::Rect { w, h, r: self.rect_radius },
            Tool::Oval => Geometry::Ellipse { w, h },
            _ => {
                fill = None;
                stroke = Some(self.stroke);
                Geometry::Line { dx: b.0 - a.0, dy: b.1 - a.1 }
            }
        };
        if fill.is_none() && stroke.is_none() {
            stroke = Some(self.stroke);
        }
        Some(Shape::new(geom, fill, stroke, self.stroke_width, Matrix::translate(center.0, center.1)))
    }

    fn path_shape(&self, mut pts: Vec<Pt>, closed: bool, smooth: bool, color: Rgba, width: f32) -> Shape {
        if pts.len() == 1 {
            pts.push((pts[0].0 + 0.1, pts[0].1));
        }
        let mut b = BBox::EMPTY;
        pts.iter().for_each(|p| b.add(*p));
        let c = b.center();
        Shape::new(
            Geometry::Path { pts: pts.iter().map(|p| (p.0 - c.0, p.1 - c.1)).collect(), closed, smooth },
            None,
            Some(color),
            width,
            Matrix::translate(c.0, c.1),
        )
    }

    fn pencil_finish(&mut self, pts: Vec<Pt>) {
        let px = self.px();
        let (color, w) = (self.stroke, self.stroke_width);
        let line = |g: Geometry, c: Pt| Shape::new(g, None, Some(color), w, Matrix::translate(c.0, c.1));
        let shape = match self.pencil_mode {
            PencilMode::Ink => self.path_shape(simplify(&pts, 0.35 * px), false, true, color, w),
            PencilMode::Smooth => self.path_shape(simplify(&pts, 1.8 * px), false, true, color, w),
            PencilMode::Straighten => match paint::straighten(&pts, px) {
                Recognized::Line(a, b) if dist(a, b) > 0.5 => {
                    line(Geometry::Line { dx: b.0 - a.0, dy: b.1 - a.1 }, ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0))
                }
                Recognized::Line(a, _) => self.path_shape(vec![a], false, false, color, w),
                Recognized::Rect(bb) => line(Geometry::Rect { w: bb.w(), h: bb.h(), r: 0.0 }, bb.center()),
                Recognized::Ellipse(bb) => line(Geometry::Ellipse { w: bb.w(), h: bb.h() }, bb.center()),
                Recognized::Poly(v, closed) => self.path_shape(v, closed, false, color, w),
            },
        };
        self.add_art(shape);
    }

    fn brush_finish(&mut self, pts: Vec<Pt>) {
        if !self.fill_on {
            self.message = "El pincel pinta con el color de relleno: elige uno".into();
            return;
        }
        let tip = paint::stamp(self.brush_tip, self.brush_size * self.px());
        let mut r = paint::sweep(&pts, &tip);
        let shapes = self.scope_shapes(self.layer);
        let fills_of = |v: &[Shape]| paint::union_all(v.iter().flat_map(|s| s.leaves(&Matrix::IDENTITY)).map(|s| paint::fill_region(&s)).collect());
        let mut behind = false;
        match self.brush_mode {
            BrushMode::Normal => {}
            BrushMode::Behind => {
                // Only where nothing is painted.
                behind = true;
                let lines = paint::union_all(shapes.iter().map(paint::stroke_region).collect());
                r = paint::difference(&paint::difference(&r, &fills_of(&shapes)), &lines);
            }
            BrushMode::Fills => {
                let lines = paint::union_all(shapes.iter().map(paint::stroke_region).collect());
                r = paint::difference(&r, &lines);
            }
            BrushMode::Selection => {
                let sel: Vec<Shape> = shapes.iter().filter(|s| self.sel.contains(&s.id)).cloned().collect();
                let fills = fills_of(&sel);
                if fills.is_empty() {
                    self.message = "«Pintar selección» necesita un relleno seleccionado en esta capa".into();
                    return;
                }
                r = paint::intersect(&r, &fills);
            }
            BrushMode::Inside => {
                let start = pts[0];
                match shapes.iter().rev().find(|s| s.fill.is_some() && s.encloses(start.0, start.1)) {
                    Some(s) => r = paint::intersect(&r, &paint::fill_region(s)),
                    None => r = paint::difference(&r, &fills_of(&shapes)),
                }
            }
        }
        self.add_region(&r, self.fill, behind);
    }

    fn eraser_finish(&mut self, pts: Vec<Pt>) {
        let e = paint::sweep(&pts, &paint::stamp(self.eraser_tip, self.eraser_size * self.px()));
        if e.is_empty() {
            return;
        }
        let eb = paint::region_bbox(&e);
        let mode = self.eraser_mode;
        let scope = self.scope();
        let all = self.doc.frame_shapes(self.layer, self.frame);
        let inside_id = (mode == EraserMode::Inside)
            .then(|| all.iter().rev().filter(|s| s.tag == scope).find(|s| s.fill.is_some() && s.encloses(pts[0].0, pts[0].1)).map(|s| s.id))
            .flatten();
        let mut out: Vec<Shape> = Vec::new();
        let mut changed = false;
        for s in all {
            // The Eraser does not affect groups or anything outside the group being edited.
            if s.tag != scope || s.is_container() {
                out.push(s);
                continue;
            }
            let pad = s.stroke_width * s.m.avg_scale();
            let mut b = s.world_bounds();
            b = BBox { x0: b.x0 - pad, y0: b.y0 - pad, x1: b.x1 + pad, y1: b.y1 + pad };
            let erase_fill = s.is_closed()
                && s.fill.is_some()
                && match mode {
                    EraserMode::Normal | EraserMode::Fills => true,
                    EraserMode::SelectedFills => self.sel.contains(&s.id),
                    EraserMode::Inside => inside_id == Some(s.id),
                    EraserMode::Lines => false,
                };
            let erase_lines = s.stroke.is_some() && matches!(mode, EraserMode::Normal | EraserMode::Lines);
            if !b.intersects(&eb) || (!erase_fill && !erase_lines) {
                out.push(s);
                continue;
            }
            let fill_r = if erase_fill { paint::fill_region(&s) } else { Vec::new() };
            let fill_hit = erase_fill && paint::region_area(&paint::intersect(&fill_r, &e)) > 0.01;
            let lines = if erase_lines { paint::stroke_lines(&s) } else { Vec::new() };
            let kept_lines = if erase_lines { paint::lines_outside(&lines, &e) } else { Vec::new() };
            let lines_hit = erase_lines && (paint::total_length(&lines) - paint::total_length(&kept_lines)).abs() > 0.01;
            if !fill_hit && !lines_hit {
                out.push(s);
                continue;
            }
            changed = true;
            // Fill and stroke become independent pieces; raw content, one piece per fragment.
            let mut parts: Vec<Shape> = Vec::new();
            let w = s.stroke_width * s.m.avg_scale();
            if let (true, Some(_)) = (s.is_closed(), s.fill) {
                if fill_hit {
                    let rest = paint::difference(&fill_r, &e);
                    if s.raw {
                        parts.extend(paint::fill_pieces_like(&rest, &s));
                    } else {
                        parts.extend(paint::region_shape_like(&rest, &s));
                    }
                } else {
                    let mut c = s.clone();
                    c.stroke = None;
                    parts.push(c);
                }
            }
            if let Some(sc) = s.stroke {
                if lines_hit {
                    if s.raw {
                        parts.extend(paint::stroke_pieces(&kept_lines, sc, w));
                    } else {
                        parts.extend(paint::lines_shape(&kept_lines, sc, w));
                    }
                } else {
                    let mut c = s.clone();
                    c.fill = None;
                    parts.push(c);
                }
            }
            let mut keep = Some(s.id);
            for mut part in parts {
                part.id = keep.take().unwrap_or(0);
                part.raw = s.raw;
                part.tag = s.tag;
                out.push(part);
            }
        }
        if !changed {
            return;
        }
        let base = self.doc.clone();
        for s in &mut out {
            if s.id == 0 {
                s.id = self.doc.alloc_id();
            }
        }
        let present: Vec<u32> = out.iter().map(|s| s.id).collect();
        let k = self.doc.layers[self.layer].ensure_key(self.frame);
        let fl = self.floating.clone();
        let v = &mut self.doc.layers[self.layer].keys.get_mut(&k).unwrap().shapes;
        *v = out;
        normalize_order(v, scope, &fl);
        self.sel.retain(|id| present.contains(id));
        self.commit_from(base);
    }

    /// Faucet: erases with one click the whole fill or the whole stroke under the pointer.
    pub(crate) fn faucet_at(&mut self, p: Pt) {
        let tol = 3.0 * self.px();
        let shapes = self.scope_shapes(self.layer);
        let Some(s) = shapes.iter().rev().find(|s| !s.is_container() && s.hit(p.0, p.1, tol)) else { return };
        let mut line = s.clone();
        line.fill = None;
        let on_stroke = s.stroke.is_some() && line.hit(p.0, p.1, tol);
        let id = s.id;
        // In raw shapes, the whole stroke is the chain of connected segments (like double-click).
        let targets: Vec<u32> = if s.raw && on_stroke {
            self.double_click_at(p);
            self.sel.clone()
        } else {
            vec![id]
        };
        let base = self.doc.clone();
        let k = self.doc.layers[self.layer].ensure_key(self.frame);
        let v = &mut self.doc.layers[self.layer].keys.get_mut(&k).unwrap().shapes;
        for tid in &targets {
            if let Some(i) = v.iter().position(|x| x.id == *tid) {
                let sh = &mut v[i];
                if on_stroke {
                    sh.stroke = None;
                } else {
                    sh.fill = None;
                }
                if sh.stroke.is_none() && (sh.fill.is_none() || !sh.is_closed()) {
                    v.remove(i);
                }
            }
        }
        self.sel.retain(|x| !targets.contains(x));
        self.commit_from(base);
    }

    pub(crate) fn bucket_at(&mut self, p: Pt) {
        if !self.fill_on {
            self.message = "El cubo usa el color de relleno: elige uno".into();
            return;
        }
        let fill = self.fill;
        // 1) Over a fill or a closed object: change its color (and, if it is
        //    raw, merge it with neighbors of the same color).
        for li in self.editable_layers() {
            let found = self.scope_shapes(li).into_iter().rev().find(|s| s.is_closed() && s.encloses(p.0, p.1) && (!s.raw || s.fill.is_some()));
            if let Some(s) = found {
                self.layer = li;
                let base = self.doc.clone();
                if s.raw {
                    let k = self.doc.layers[li].ensure_key(self.frame);
                    self.doc.layers[li].keys.get_mut(&k).unwrap().shapes.retain(|x| x.id != s.id);
                    let mut piece = s.clone();
                    piece.fill = Some(fill);
                    self.paint_shape(&mut piece, &s.world_bounds());
                    self.merge_raw(li, vec![piece]);
                } else {
                    let b = s.world_bounds();
                    let text = matches!(s.geom, Geometry::Text(_));
                    let (g, bm) = if text { (None, None) } else { (self.new_grad(&b), self.new_bmp(&b)) };
                    self.modify_shapes(&[s.id], |s| {
                        s.fill = Some(fill);
                        s.set_grad_world(g.clone());
                        s.set_bmp_world(bm.clone());
                    });
                }
                self.commit_from(base);
                return;
            }
        }
        // 2) Area enclosed by lines on the current layer: flood fill.
        if !self.can_draw() {
            return;
        }
        let shapes = self.scope_shapes(self.layer);
        let area = BBox::from_pts(self.to_doc(0.0, 0.0), self.to_doc(self.vw as f32, self.vh as f32));
        let mut scale = self.zoom;
        let max = area.w().max(area.h()) * scale;
        if max > 2048.0 {
            scale *= 2048.0 / max;
        }
        let gap = (GAPS[self.gap] * self.dpr * scale / self.zoom).round() as i32;
        match paint::flood_region(&shapes, area, scale, p, gap) {
            Ok(r) => self.add_region(&r, fill, true),
            Err(m) => self.message = m.into(),
        }
    }

    /// Ink Bottle: on a raw fill, adds its outline as a stroke.
    pub(crate) fn ink_at(&mut self, p: Pt) {
        let Some((li, id)) = self.hit_top(p) else { return };
        self.layer = li;
        let (stroke, w) = (self.stroke, self.stroke_width);
        let base = self.doc.clone();
        let s = self.scope_shapes(li).into_iter().find(|s| s.id == id);
        match s {
            Some(s) if s.raw && s.fill.is_some() => {
                let lines: Vec<Vec<[f32; 2]>> = flatten(&s.outline())
                    .into_iter()
                    .map(|(pts, closed)| {
                        let mut v: Vec<[f32; 2]> = pts.iter().map(|q| {
                            let w = s.m.apply(q.0, q.1);
                            [w.0, w.1]
                        }).collect();
                        if closed && !v.is_empty() {
                            v.push(v[0]);
                        }
                        v
                    })
                    .collect();
                let pieces = paint::stroke_pieces(&lines, stroke, w);
                self.merge_raw(li, pieces);
            }
            Some(s) if s.raw => {
                // Raw stroke segment: the whole connected line changes.
                self.double_click_at(p);
                let ids = self.sel.clone();
                self.modify_shapes(&ids, |s| {
                    s.stroke = Some(stroke);
                    s.stroke_width = w / s.m.avg_scale();
                });
            }
            _ => self.modify_shapes(&[id], |s| {
                if !s.is_container() {
                    s.stroke = Some(stroke);
                    s.stroke_width = w;
                }
            }),
        }
        self.commit_from(base);
    }

    pub(crate) fn eyedropper_at(&mut self, p: Pt) {
        let Some((li, id)) = self.hit_top(p) else { return };
        let Some(s) = self.doc.frame_shapes(li, self.frame).into_iter().find(|s| s.id == id) else { return };
        if let (Some(f), true) = (s.fill, s.is_closed()) {
            self.fill = f;
            self.fill_on = true;
            // A gradient is picked up whole (like Flash's Eyedropper).
            self.fill_grad = s.grad.clone().map(|g| grad::Gradient { m: Matrix::IDENTITY, ..g });
            self.fill_bmp = s.bmp.as_ref().map(|b| b.id);
        }
        if let Some(c) = s.stroke {
            self.stroke = c;
            self.stroke_on = true;
            self.stroke_width = s.stroke_width;
            if !s.is_closed() {
                self.fill = c;
            }
        }
    }

    // ───────────────────────── Overlays ─────────────────────────

    pub(crate) fn draw_tool_preview(&self, pm: &mut Pixmap, vm: &Matrix) {
        match &self.drag {
            Drag::Create { start, cur } => {
                if let Some(s) = self.preview_shape(*start, *cur) {
                    render::draw_shape(pm, &s, vm, 1.0);
                }
            }
            Drag::Freehand { .. } if self.tool == Tool::Lasso => {}
            Drag::TextBox { start, cur } => {
                let (a, b) = (vm.apply(start.0, start.1), vm.apply(cur.0, cur.1));
                if (b.0 - a.0).abs() > 4.0 {
                    let bb = BBox::from_pts(a, (b.0, a.1 + self.text_style.size * 1.2 * self.zoom));
                    render::stroke_rect(pm, bb.x0, bb.y0, bb.w(), bb.h(), Rgba::BLACK, self.dpr, true);
                }
            }
            Drag::Freehand { pts } => {
                let (color, w, alpha) = match self.tool {
                    Tool::Brush => (self.fill, self.brush_size * self.px(), 1.0),
                    Tool::Eraser => (self.doc.bg, self.eraser_size * self.px(), 1.0),
                    _ => (self.stroke, self.stroke_width, 1.0),
                };
                let s = self.path_shape(pts.clone(), false, false, color, w);
                render::draw_shape(pm, &s, vm, alpha);
            }
            _ => {}
        }
        if let Some(pen) = &self.pen {
            let mut nodes = pen.nodes.clone();
            let rubber = !matches!(self.drag, Drag::PenNode { .. }) && !pen.closed;
            if let (true, Some(h)) = (rubber, self.hover) {
                nodes.push(Node::corner(h));
            }
            let s = Shape::new(
                Geometry::Bezier { paths: vec![BezPath { nodes, closed: pen.closed }] },
                (pen.closed && self.fill_on).then_some(self.fill),
                Some(if self.stroke_on { self.stroke } else { ACCENT }),
                self.stroke_width,
                Matrix::IDENTITY,
            );
            render::draw_shape(pm, &s, vm, 1.0);
            self.outline_px(pm, &s, vm);
            let last = pen.nodes.len() - 1;
            for (i, n) in pen.nodes.iter().enumerate() {
                if i == last {
                    self.draw_handles(pm, vm, n);
                }
                self.draw_anchor(pm, vm.apply(n.p.0, n.p.1), i == last);
            }
        }
    }

    pub(crate) fn draw_selection(&self, pm: &mut Pixmap, vm: &Matrix) {
        let dpr = self.dpr;
        let sel = self.selected_shapes();
        self.draw_lasso(pm, vm);
        match self.tool {
            Tool::Transform if self.xform_mode.warps() => {
                for (_, s) in sel.iter().filter(|(_, s)| s.raw) {
                    self.draw_hatch(pm, s, vm);
                }
                self.draw_env(pm, vm);
            }
            Tool::Transform => {
                for (_, s) in sel.iter().filter(|(_, s)| s.raw) {
                    self.draw_hatch(pm, s, vm);
                }
                if let Some((vf, lb, hs, pv)) = self.handles() {
                    let q = [vf.apply(lb.x0, lb.y0), vf.apply(lb.x1, lb.y0), vf.apply(lb.x1, lb.y1), vf.apply(lb.x0, lb.y1)];
                    self.stroke_screen(pm, quad_path(&q), ACCENT, dpr);
                    let r = 3.5 * dpr;
                    for (_, _, p) in hs {
                        render::fill_rect(pm, p.0 - r, p.1 - r, 2.0 * r, 2.0 * r, Rgba::WHITE);
                        render::stroke_rect(pm, p.0 - r, p.1 - r, 2.0 * r, 2.0 * r, ACCENT, dpr, false);
                    }
                    render::fill_circle(pm, pv.0, pv.1, 4.0 * dpr, Rgba::WHITE);
                    render::stroke_circle(pm, pv.0, pv.1, 4.0 * dpr, ACCENT, 1.5 * dpr);
                }
            }
            Tool::FillXform => self.draw_grad_handles(pm, vm),
            Tool::Subselect | Tool::Pen => {
                for (_, s) in &sel {
                    self.outline_px(pm, s, vm);
                    for (pi, bp) in world_paths(s).iter().enumerate() {
                        for (ni, n) in bp.nodes.iter().enumerate() {
                            let on = self.anchors.contains(&(s.id, pi, ni));
                            if on && self.tool == Tool::Subselect {
                                self.draw_handles(pm, vm, n);
                            }
                            self.draw_anchor(pm, vm.apply(n.p.0, n.p.1), on);
                        }
                    }
                }
            }
            _ => {
                for (_, s) in &sel {
                    // Raw content is marked with Flash's hatch pattern; objects and groups, with their box.
                    if s.raw {
                        self.draw_hatch(pm, s, vm);
                        continue;
                    }
                    let lb = s.local_bounds();
                    let m = vm.mul(&s.m);
                    let q = [m.apply(lb.x0, lb.y0), m.apply(lb.x1, lb.y0), m.apply(lb.x1, lb.y1), m.apply(lb.x0, lb.y1)];
                    self.stroke_screen(pm, quad_path(&q), ACCENT, dpr);
                }
            }
        }
        // Snap ring (the thick circle Flash shows when snapping).
        if let (Some(t), false) = (self.snap_hit, matches!(self.drag, Drag::None)) {
            let c = vm.apply(t.0, t.1);
            render::stroke_circle(pm, c.0, c.1, 6.0 * dpr, Rgba::WHITE, 3.5 * dpr);
            render::stroke_circle(pm, c.0, c.1, 6.0 * dpr, Rgba::BLACK, 1.5 * dpr);
        }
        let marquee = match &self.drag {
            Drag::Marquee { start, cur, .. } | Drag::AnchorMarquee { start, cur, .. } => Some((*start, *cur)),
            _ => None,
        };
        if let Some((a, b)) = marquee {
            let bb = BBox::from_pts(vm.apply(a.0, a.1), vm.apply(b.0, b.1));
            render::fill_rect(pm, bb.x0, bb.y0, bb.w(), bb.h(), Rgba { a: 30, ..ACCENT });
            render::stroke_rect(pm, bb.x0.floor() + 0.5, bb.y0.floor() + 0.5, bb.w().round(), bb.h().round(), ACCENT, dpr, true);
        }
    }

    fn stroke_screen(&self, pm: &mut Pixmap, path: Option<tiny_skia::Path>, c: Rgba, w: f32) {
        if let Some(p) = path {
            pm.stroke_path(&p, &render::paint(c, 1.0), &Stroke { width: w, ..Default::default() }, Transform::identity(), None);
        }
    }

    /// Thin outline (1 screen px) of a shape's path.
    fn outline_px(&self, pm: &mut Pixmap, s: &Shape, vm: &Matrix) {
        let Some(path) = render::build_path(&s.outline()) else { return };
        let m = vm.mul(&s.m);
        let st = Stroke { width: self.dpr / m.avg_scale(), ..Default::default() };
        pm.stroke_path(&path, &render::paint(ACCENT, 1.0), &st, m.to_skia(), None);
    }

    fn draw_anchor(&self, pm: &mut Pixmap, p: Pt, on: bool) {
        let r = 3.0 * self.dpr;
        render::fill_rect(pm, p.0 - r, p.1 - r, 2.0 * r, 2.0 * r, if on { ACCENT } else { Rgba::WHITE });
        render::stroke_rect(pm, p.0 - r, p.1 - r, 2.0 * r, 2.0 * r, ACCENT, self.dpr, false);
    }

    fn draw_handles(&self, pm: &mut Pixmap, vm: &Matrix, n: &Node) {
        let p = vm.apply(n.p.0, n.p.1);
        for (has, h) in [(n.has_in(), n.i), (n.has_out(), n.o)] {
            if !has {
                continue;
            }
            let h = vm.apply(h.0, h.1);
            let mut pb = PathBuilder::new();
            pb.move_to(p.0, p.1);
            pb.line_to(h.0, h.1);
            self.stroke_screen(pm, pb.finish(), ACCENT, self.dpr);
            render::fill_circle(pm, h.0, h.1, 3.0 * self.dpr, ACCENT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Editor {
        let mut e = Editor::new();
        e.set_viewport(800, 600, 1.0);
        e.zoom = 1.0;
        e.pan = (0.0, 0.0);
        // These tests are for Object Drawing mode; the merge drawing model ones are in merge_tests.
        e.object_mode = true;
        e.snap_objects = false;
        e
    }
    fn click(e: &mut Editor, x: f32, y: f32) {
        e.pointer_down(x, y, 0, false, false);
        e.pointer_up(x, y, false, false);
    }
    fn drag(e: &mut Editor, a: Pt, b: Pt) {
        e.pointer_down(a.0, a.1, 0, false, false);
        e.pointer_move((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0, false, false);
        e.pointer_up(b.0, b.1, false, false);
    }
    fn shapes(e: &Editor) -> Vec<Shape> {
        e.doc.layers[0].frame_shapes(0)
    }

    #[test]
    fn pen_builds_closed_curve() {
        let mut e = editor();
        e.set_tool("pen");
        click(&mut e, 100.0, 100.0);
        drag(&mut e, (200.0, 100.0), (240.0, 140.0)); // smooth point
        click(&mut e, 150.0, 200.0);
        click(&mut e, 101.0, 101.0); // close on the first one
        let s = shapes(&e);
        assert_eq!(s.len(), 1);
        let Geometry::Bezier { paths } = &s[0].geom else { panic!() };
        assert!(paths[0].closed);
        assert_eq!(paths[0].nodes.len(), 3);
        assert!(paths[0].nodes[1].smooth && paths[0].nodes[1].has_in());
        assert!(s[0].fill.is_some());
    }

    #[test]
    fn pen_adds_and_removes_anchor_on_selected_path() {
        let mut e = editor();
        e.set_tool("pen");
        click(&mut e, 100.0, 100.0);
        click(&mut e, 300.0, 100.0);
        e.pen_finish();
        assert_eq!(e.sel.len(), 1);
        click(&mut e, 200.0, 100.0); // on the segment: adds
        let Geometry::Bezier { paths } = &shapes(&e)[0].geom else { panic!() };
        assert_eq!(paths[0].nodes.len(), 3);
        click(&mut e, 200.0, 100.0); // on the anchor: removes
        let Geometry::Bezier { paths } = &shapes(&e)[0].geom else { panic!() };
        assert_eq!(paths[0].nodes.len(), 2);
    }

    #[test]
    fn direct_selection_moves_anchor_of_rect() {
        let mut e = editor();
        e.set_tool("rect");
        drag(&mut e, (100.0, 100.0), (200.0, 200.0));
        e.set_tool("subselect");
        click(&mut e, 150.0, 150.0); // selects the shape
        e.anchors.clear();
        drag(&mut e, (200.0, 200.0), (260.0, 240.0)); // drags the corner
        let b = shapes(&e)[0].world_bounds();
        assert!((b.x1 - 260.0).abs() < 0.5 && (b.y1 - 240.0).abs() < 0.5, "{b:?}");
        assert!(matches!(shapes(&e)[0].geom, Geometry::Bezier { .. }));
    }

    #[test]
    fn transform_box_scales_along_rotated_axes() {
        let mut e = editor();
        e.set_tool("rect");
        drag(&mut e, (100.0, 100.0), (300.0, 150.0));
        e.set_tool("transform");
        click(&mut e, 200.0, 125.0);
        e.rotate90(true);
        // After rotating 90°, the local right side (hx = 1) ends up at the bottom: pulling it down lengthens the rectangle.
        let (vf, lb, hs, _) = e.handles().unwrap();
        let h = hs.iter().find(|(hx, hy, _)| *hx == 1 && *hy == 0).unwrap().2;
        let _ = (vf, lb);
        drag(&mut e, h, (h.0, h.1 + 100.0));
        let d = shapes(&e)[0].m.decompose();
        assert!((d.rot.abs() - std::f32::consts::FRAC_PI_2).abs() < 1e-3);
        assert!((d.sx - 1.5).abs() < 0.02, "sx {}", d.sx);
    }

    #[test]
    fn eraser_cuts_rect_in_two() {
        let mut e = editor();
        e.stroke_on = false;
        e.set_tool("rect");
        drag(&mut e, (100.0, 100.0), (300.0, 150.0));
        e.set_tool("eraser");
        e.set_eraser_size(20.0);
        drag(&mut e, (200.0, 60.0), (200.0, 190.0));
        let s = shapes(&e);
        assert_eq!(s.len(), 1);
        let Geometry::Bezier { paths } = &s[0].geom else { panic!() };
        assert_eq!(paths.len(), 2, "el rectángulo debe quedar en dos piezas");
        assert!(!s[0].hit(200.0, 125.0, 0.1));
        assert!(s[0].hit(120.0, 125.0, 0.1));
    }

    #[test]
    fn bucket_fills_area_closed_by_lines() {
        let mut e = editor();
        e.set_tool("line");
        drag(&mut e, (100.0, 100.0), (300.0, 100.0));
        drag(&mut e, (300.0, 100.0), (200.0, 260.0));
        drag(&mut e, (200.0, 260.0), (100.0, 100.0));
        e.set_tool("bucket");
        click(&mut e, 200.0, 150.0);
        let s = shapes(&e);
        assert_eq!(s.len(), 4);
        assert!(s[0].fill.is_some() && s[0].hit(200.0, 150.0, 0.1));
        click(&mut e, 600.0, 500.0);
        assert!(!e.take_message().is_empty());
    }

    #[test]
    fn brush_behind_goes_under() {
        let mut e = editor();
        e.set_tool("rect");
        drag(&mut e, (100.0, 100.0), (200.0, 200.0));
        e.set_tool("brush");
        e.set_brush_mode("behind");
        drag(&mut e, (50.0, 150.0), (250.0, 150.0));
        let s = shapes(&e);
        assert_eq!(s.len(), 2);
        assert!(matches!(s[0].geom, Geometry::Bezier { .. }));
    }

    #[test]
    fn moved_pivot_is_rotation_center() {
        let mut e = editor();
        e.set_tool("rect");
        drag(&mut e, (100.0, 100.0), (200.0, 200.0));
        e.set_tool("transform");
        click(&mut e, 150.0, 150.0);
        drag(&mut e, (150.0, 150.0), (100.0, 100.0)); // center circle → corner
        assert!(e.pivot.is_some());
        e.rotate90(true);
        let b = shapes(&e)[0].world_bounds();
        // Rotate 90° around (100,100): the square moves to x ∈ [0,100].
        assert!((b.x0 - 0.0).abs() < 0.5 && (b.x1 - 100.0).abs() < 0.5, "{b:?}");
    }

    #[test]
    fn marquee_needs_full_containment() {
        let mut e = editor();
        e.set_tool("rect");
        drag(&mut e, (100.0, 100.0), (200.0, 200.0));
        e.set_tool("select");
        drag(&mut e, (50.0, 50.0), (150.0, 150.0));
        assert!(e.sel.is_empty());
        drag(&mut e, (50.0, 50.0), (250.0, 250.0));
        assert_eq!(e.sel.len(), 1);
    }
}
#[cfg(test)]
mod merge_tests {
    use super::*;
    use crate::paint;

    fn editor() -> Editor {
        let mut e = Editor::new();
        e.set_viewport(800, 600, 1.0);
        e.zoom = 1.0;
        e.pan = (0.0, 0.0);
        e.snap_objects = false;
        e
    }
    fn click(e: &mut Editor, x: f32, y: f32) {
        e.pointer_down(x, y, 0, false, false);
        e.pointer_up(x, y, false, false);
    }
    fn drag(e: &mut Editor, a: Pt, b: Pt) {
        e.pointer_down(a.0, a.1, 0, false, false);
        for k in 1..=4 {
            let t = k as f32 / 4.0;
            e.pointer_move(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, false, false);
        }
        e.pointer_up(b.0, b.1, false, false);
    }
    fn shapes(e: &Editor) -> Vec<Shape> {
        e.doc.layers[0].frame_shapes(0)
    }
    fn fills(e: &Editor) -> Vec<Shape> {
        shapes(e).into_iter().filter(|s| s.fill.is_some()).collect()
    }
    fn strokes(e: &Editor) -> Vec<Shape> {
        shapes(e).into_iter().filter(|s| s.fill.is_none()).collect()
    }
    fn rect(e: &mut Editor, a: Pt, b: Pt) {
        e.set_tool("rect");
        drag(e, a, b);
    }

    #[test]
    fn rect_is_raw_fill_plus_four_sides() {
        let mut e = editor();
        rect(&mut e, (100.0, 100.0), (200.0, 200.0));
        assert!(shapes(&e).iter().all(|s| s.raw));
        assert_eq!(fills(&e).len(), 1);
        assert_eq!(strokes(&e).len(), 4);
    }

    #[test]
    fn overlapping_same_color_fills_merge() {
        let mut e = editor();
        e.stroke_on = false;
        rect(&mut e, (100.0, 100.0), (200.0, 200.0));
        rect(&mut e, (150.0, 150.0), (250.0, 250.0));
        assert_eq!(fills(&e).len(), 1);
        let a = paint::region_area(&paint::fill_region(&fills(&e)[0]));
        assert!((a - 17500.0).abs() < 40.0, "área {a}");
    }

    #[test]
    fn line_across_fill_splits_it() {
        let mut e = editor();
        e.stroke_on = false;
        rect(&mut e, (100.0, 100.0), (200.0, 200.0));
        e.set_tool("line");
        drag(&mut e, (150.0, 50.0), (150.0, 250.0));
        assert_eq!(fills(&e).len(), 2);
    }

    #[test]
    fn click_selects_fill_double_click_adds_outline() {
        let mut e = editor();
        rect(&mut e, (100.0, 100.0), (200.0, 200.0));
        e.set_tool("select");
        click(&mut e, 150.0, 150.0);
        assert_eq!(e.sel.len(), 1);
        e.double_click(150.0, 150.0);
        assert_eq!(e.sel.len(), 5, "relleno + 4 lados");
        e.deselect();
        e.double_click(150.0, 100.0);
        assert_eq!(e.sel.len(), 4, "doble clic en un lado: todos los lados conectados");
    }

    #[test]
    fn moved_fill_floats_then_cuts_on_deselect() {
        let mut e = editor();
        e.stroke_on = false;
        rect(&mut e, (100.0, 100.0), (200.0, 200.0));
        e.fill = Rgba { r: 255, g: 0, b: 0, a: 255 };
        rect(&mut e, (300.0, 100.0), (400.0, 200.0));
        e.set_tool("select");
        click(&mut e, 350.0, 150.0);
        drag(&mut e, (350.0, 150.0), (200.0, 150.0)); // the red one covers the right half of the blue one
        assert_eq!(e.floating.len(), 1);
        assert_eq!(fills(&e).len(), 2, "flotando todavía no corta");
        e.deselect();
        assert!(e.floating.is_empty());
        let blue = fills(&e).into_iter().find(|s| s.fill.unwrap().r != 255).unwrap();
        let a = paint::region_area(&paint::fill_region(&blue));
        assert!((a - 5000.0).abs() < 40.0, "el rojo recorta al azul al soltarlo: {a}");
    }

    #[test]
    fn marquee_cuts_raw_shape() {
        let mut e = editor();
        e.stroke_on = false;
        rect(&mut e, (100.0, 100.0), (200.0, 200.0));
        e.set_tool("select");
        drag(&mut e, (50.0, 50.0), (150.0, 250.0));
        assert_eq!(fills(&e).len(), 2);
        assert_eq!(e.sel.len(), 1);
        let s = fills(&e).into_iter().find(|s| e.sel.contains(&s.id)).unwrap();
        assert!((s.world_bounds().x1 - 150.0).abs() < 0.5);
    }

    #[test]
    fn bending_an_edge_bends_fill_and_line_together() {
        let mut e = editor();
        rect(&mut e, (100.0, 100.0), (200.0, 200.0));
        e.set_tool("select");
        drag(&mut e, (150.0, 100.0), (150.0, 60.0));
        let top_fill = fills(&e)[0].world_bounds().y0;
        let top_line = strokes(&e).iter().map(|s| s.world_bounds().y0).fold(f32::INFINITY, f32::min);
        assert!(top_fill < 80.0, "relleno doblado: {top_fill}");
        assert!((top_fill - top_line).abs() < 1.0, "línea y relleno siguen juntos");
    }

    #[test]
    fn dragging_a_corner_moves_it_for_all_pieces() {
        let mut e = editor();
        rect(&mut e, (100.0, 100.0), (200.0, 200.0));
        e.set_tool("select");
        drag(&mut e, (200.0, 200.0), (240.0, 230.0));
        let b = fills(&e)[0].world_bounds();
        assert!((b.x1 - 240.0).abs() < 0.5 && (b.y1 - 230.0).abs() < 0.5, "{b:?}");
        let lines_b = strokes(&e).iter().fold(BBox::EMPTY, |acc, s| acc.union(&s.world_bounds()));
        assert!((lines_b.x1 - 240.0).abs() < 0.5);
    }

    #[test]
    fn group_edit_and_ungroup() {
        let mut e = editor();
        rect(&mut e, (100.0, 100.0), (200.0, 200.0));
        e.set_tool("select");
        e.select_all();
        e.group();
        assert_eq!(shapes(&e).len(), 1);
        assert!(shapes(&e)[0].is_group());
        e.double_click(150.0, 150.0);
        assert_eq!(e.doc.scopes.len(), 1);
        rect(&mut e, (300.0, 300.0), (350.0, 350.0));
        e.set_tool("select");
        e.leave_group();
        assert!(e.doc.scopes.is_empty());
        assert_eq!(shapes(&e).len(), 1);
        let Geometry::Group { children, .. } = &shapes(&e)[0].geom else { panic!() };
        assert_eq!(children.len(), 10);
        e.undo();
        assert_eq!(e.doc.scopes.len(), 1);
        e.redo();
        e.select_all();
        e.ungroup();
        assert_eq!(shapes(&e).len(), 10);
    }

    #[test]
    fn object_mode_and_break_apart() {
        let mut e = editor();
        e.set_object_mode(true);
        rect(&mut e, (100.0, 100.0), (200.0, 200.0));
        assert_eq!(shapes(&e).len(), 1);
        assert!(!shapes(&e)[0].raw);
        e.set_tool("select");
        e.select_all();
        e.break_apart();
        e.deselect();
        assert_eq!(shapes(&e).len(), 5);
        assert!(shapes(&e).iter().all(|s| s.raw));
    }

    #[test]
    fn snapping_to_object_corner() {
        let mut e = editor();
        e.snap_objects = true;
        rect(&mut e, (100.0, 100.0), (200.0, 200.0));
        rect(&mut e, (203.0, 196.0), (300.0, 300.0));
        let b = shapes(&e).iter().filter(|s| s.fill.is_some()).map(|s| s.world_bounds()).find(|b| b.x1 > 250.0).unwrap();
        assert!((b.x0 - 200.0).abs() < 0.01 && (b.y0 - 200.0).abs() < 0.01, "{b:?}");
    }

    #[test]
    fn shape_commands() {
        let mut e = editor();
        e.stroke_on = false;
        e.set_tool("oval");
        drag(&mut e, (100.0, 100.0), (200.0, 200.0));
        e.set_tool("select");
        click(&mut e, 150.0, 150.0);
        let a0 = paint::region_area(&paint::fill_region(&fills(&e)[0]));
        e.expand_fill(10.0, true);
        let a1 = paint::region_area(&paint::fill_region(&fills(&e)[0]));
        assert!(a1 > a0 * 1.3, "{a0} -> {a1}");
        let msg = e.optimize_curves(50.0);
        assert!(msg.contains("optimizadas"));
        e.straighten_selection();
        e.smooth_selection();
        e.soften_edges(8.0, 4, true);
        assert!(fills(&e).len() >= 4);
        e.set_tool("line");
        drag(&mut e, (300.0, 300.0), (400.0, 300.0));
        e.set_tool("select");
        click(&mut e, 350.0, 300.0);
        e.lines_to_fills();
        assert!(strokes(&e).is_empty());
    }
}
