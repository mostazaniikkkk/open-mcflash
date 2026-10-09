//! Block 4 shape tools: PolyStar, Lasso (freehand and polygon) and
//! the Free Transform modes (rotate and skew, scale,
//! distort and envelope).

use super::*;
use crate::render;
use tiny_skia::{PathBuilder, Stroke, Transform};

/// Free Transform modifiers ("Options" buttons in Flash 8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum XMode {
    /// No modifier: scale, rotate and skew.
    Free,
    RotateSkew,
    Scale,
    Distort,
    Envelope,
}

impl XMode {
    pub(crate) fn parse(s: &str) -> XMode {
        match s {
            "rotate" => XMode::RotateSkew,
            "scale" => XMode::Scale,
            "distort" => XMode::Distort,
            "envelope" => XMode::Envelope,
            _ => XMode::Free,
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            XMode::Free => "free",
            XMode::RotateSkew => "rotate",
            XMode::Scale => "scale",
            XMode::Distort => "distort",
            XMode::Envelope => "envelope",
        }
    }
    pub(crate) fn warps(self) -> bool {
        matches!(self, XMode::Distort | XMode::Envelope)
    }
}

/// Envelope mesh: 8 anchors (corners and midpoints, clockwise
/// from the top-left) with their handles.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EnvState {
    pub ids: Vec<u32>,
    /// Shapes as they were at the start (always deformed from here).
    pub base: Vec<Shape>,
    pub bb: BBox,
    pub a: [Pt; 8],
    pub hin: [Pt; 8],
    pub hout: [Pt; 8],
}

fn lerp_pt(a: Pt, b: Pt, t: f32) -> Pt {
    (lerp(a.0, b.0, t), lerp(a.1, b.1, t))
}

fn cubic(p0: Pt, p1: Pt, p2: Pt, p3: Pt, t: f32) -> Pt {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0, a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1)
}

impl EnvState {
    pub(crate) fn straight(ids: Vec<u32>, base: Vec<Shape>, bb: BBox) -> EnvState {
        let c = bb.center();
        let a = [(bb.x0, bb.y0), (c.0, bb.y0), (bb.x1, bb.y0), (bb.x1, c.1), (bb.x1, bb.y1), (c.0, bb.y1), (bb.x0, bb.y1), (bb.x0, c.1)];
        let mut e = EnvState { ids, base, bb, a, hin: a, hout: a };
        e.relink();
        e
    }

    /// Straight sides between the anchors (handles at one third).
    pub(crate) fn relink(&mut self) {
        for k in 0..8 {
            let n = (k + 1) % 8;
            self.hout[k] = lerp_pt(self.a[k], self.a[n], 1.0 / 3.0);
            self.hin[n] = lerp_pt(self.a[n], self.a[k], 1.0 / 3.0);
        }
    }

    /// Distort: the midpoints follow the corners.
    pub(crate) fn from_corners(&mut self) {
        for k in [1usize, 3, 5, 7] {
            self.a[k] = lerp_pt(self.a[k - 1], self.a[(k + 1) % 8], 0.5);
        }
        self.relink();
    }

    fn seg(&self, k: usize, t: f32) -> Pt {
        let n = (k + 1) % 8;
        cubic(self.a[k], self.hout[k], self.hin[n], self.a[n], t)
    }

    fn side(&self, k0: usize, s: f32) -> Pt {
        if s < 0.5 {
            self.seg(k0, s * 2.0)
        } else {
            self.seg(k0 + 1, s * 2.0 - 1.0)
        }
    }

    /// Coons patch: (u, v) of the original box → deformed point.
    pub(crate) fn patch(&self, u: f32, v: f32) -> Pt {
        let top = self.side(0, u);
        let right = self.side(2, v);
        let bottom = self.side(4, 1.0 - u);
        let left = self.side(6, 1.0 - v);
        let (c00, c10, c11, c01) = (self.a[0], self.a[2], self.a[4], self.a[6]);
        let f = |i: usize| {
            let g = |p: Pt| if i == 0 { p.0 } else { p.1 };
            (1.0 - v) * g(top) + v * g(bottom) + (1.0 - u) * g(left) + u * g(right)
                - ((1.0 - u) * (1.0 - v) * g(c00) + u * (1.0 - v) * g(c10) + (1.0 - u) * v * g(c01) + u * v * g(c11))
        };
        (f(0), f(1))
    }

    pub(crate) fn map(&self, p: Pt) -> Pt {
        let (w, h) = (self.bb.w().max(1e-3), self.bb.h().max(1e-3));
        self.patch((p.0 - self.bb.x0) / w, (p.1 - self.bb.y0) / h)
    }

    /// Outline of the mesh (for drawing it).
    pub(crate) fn outline(&self) -> Vec<Pt> {
        (0..8).flat_map(|k| (0..8).map(move |i| (k, i as f32 / 8.0))).map(|(k, t)| self.seg(k, t)).collect()
    }
}

/// Splits each segment into `parts` so the deformation follows the curves well.
fn subdivide(bp: &BezPath, parts: usize) -> BezPath {
    let nodes = &bp.nodes;
    if nodes.len() < 2 {
        return bp.clone();
    }
    let mut out: Vec<Node> = vec![nodes[0]];
    for (a, b) in bp.segments() {
        let mut cur = (nodes[a].p, nodes[a].o, nodes[b].i, nodes[b].p);
        for k in (1..=parts).rev() {
            if k == 1 {
                out.last_mut().unwrap().o = cur.1;
                out.push(Node { p: cur.3, i: cur.2, o: nodes[b].o, smooth: nodes[b].smooth });
            } else {
                let (c1a, c2a, m, c1b, c2b) = split_cubic(cur.0, cur.1, cur.2, cur.3, 1.0 / k as f32);
                out.last_mut().unwrap().o = c1a;
                out.push(Node { p: m, i: c2a, o: m, smooth: true });
                cur = (m, c1b, c2b, cur.3);
            }
        }
    }
    if bp.closed && out.len() > 1 {
        let last = out.pop().unwrap();
        out[0].i = last.i;
    }
    BezPath { nodes: out, closed: bp.closed }
}

/// Deforms a shape (or a drawing object) with `f`, in parent coordinates.
pub(crate) fn warp_shape(s: &Shape, f: &dyn Fn(Pt) -> Pt) -> Shape {
    if let Geometry::Group { children, object } = &s.geom {
        let children = children
            .iter()
            .map(|c| {
                let mut c2 = c.clone();
                c2.m = s.m.mul(&c.m);
                warp_shape(&c2, f)
            })
            .collect();
        return Shape { geom: Geometry::Group { children, object: *object }, m: Matrix::IDENTITY, ..s.clone() };
    }
    let grad = s.grad_world();
    let mut paths: Vec<BezPath> = s
        .bez_paths()
        .iter()
        .map(|bp| {
            let mut bp = subdivide(bp, 4);
            for n in &mut bp.nodes {
                for q in [&mut n.p, &mut n.i, &mut n.o] {
                    *q = f(s.m.apply(q.0, q.1));
                }
            }
            bp
        })
        .collect();
    let tmp = Shape::new(Geometry::Bezier { paths: paths.clone() }, None, None, 1.0, Matrix::IDENTITY);
    let c = tmp.local_bounds().center();
    for p in &mut paths {
        for n in &mut p.nodes {
            n.translate((-c.0, -c.1));
        }
    }
    let mut out = Shape {
        geom: Geometry::Bezier { paths },
        m: Matrix::translate(c.0, c.1),
        stroke_width: s.stroke_width * s.m.avg_scale(),
        ..s.clone()
    };
    out.set_grad_world(grad);
    out
}

/// Envelope grip: (0 anchor, 1 in handle, 2 out handle; index).
pub(crate) type EnvHandle = (u8, usize);

impl Editor {
    // ───────── PolyStar ─────────

    /// Polygon or star from the center; the first vertex points to the pointer.
    pub(crate) fn polystar_shape(&self, start: Pt, cur: Pt) -> Option<Shape> {
        let r = dist(start, cur);
        if r < 0.5 {
            return None;
        }
        let mut a0 = (cur.1 - start.1).atan2(cur.0 - start.0);
        if self.shift {
            let step = std::f32::consts::FRAC_PI_4;
            a0 = (a0 / step).round() * step;
        }
        let (star, n, depth) = self.polystar;
        let n = n.clamp(3, 32) as usize;
        let count = if star { 2 * n } else { n };
        let inner = r * depth.clamp(0.0, 1.0);
        let nodes: Vec<Node> = (0..count)
            .map(|i| {
                let a = a0 + i as f32 * std::f32::consts::TAU / count as f32;
                let rr = if star && i % 2 == 1 { inner } else { r };
                Node::corner((rr * a.cos(), rr * a.sin()))
            })
            .collect();
        let (fill, stroke) = (self.fill_on.then_some(self.fill), self.stroke_on.then_some(self.stroke));
        let stroke = if fill.is_none() && stroke.is_none() { Some(self.stroke) } else { stroke };
        Some(Shape::new(Geometry::Bezier { paths: vec![BezPath { nodes, closed: true }] }, fill, stroke, self.stroke_width, Matrix::translate(start.0, start.1)))
    }

    // ───────── Lasso ─────────

    pub(crate) fn lasso_down(&mut self, p: Pt, shift: bool) {
        self.settle();
        if self.lasso_poly {
            if self.lasso_pts.is_empty() && !shift {
                self.sel.clear();
            }
            self.lasso_pts.push(p);
            return;
        }
        if !shift {
            self.sel.clear();
        }
        self.drag = Drag::Freehand { pts: vec![p] };
    }

    pub(crate) fn lasso_finish_poly(&mut self) {
        let mut pts = std::mem::take(&mut self.lasso_pts);
        pts.dedup_by(|a, b| dist(*a, *b) < 0.01);
        let keep = self.sel.clone();
        self.lasso_select(&pts, keep);
    }

    pub(crate) fn draw_lasso(&self, pm: &mut Pixmap, vm: &Matrix) {
        let mut pts: Vec<Pt> = match &self.drag {
            Drag::Freehand { pts } if self.tool == Tool::Lasso => pts.clone(),
            _ => self.lasso_pts.clone(),
        };
        if pts.is_empty() {
            return;
        }
        if self.lasso_poly && matches!(self.drag, Drag::None) {
            if let Some(h) = self.hover {
                pts.push(h);
            }
        }
        let mut pb = PathBuilder::new();
        let s0 = vm.apply(pts[0].0, pts[0].1);
        pb.move_to(s0.0, s0.1);
        for q in &pts[1..] {
            let s = vm.apply(q.0, q.1);
            pb.line_to(s.0, s.1);
        }
        pb.close();
        if let Some(path) = pb.finish() {
            pm.fill_path(&path, &render::paint(Rgba { a: 30, ..ACCENT }, 1.0), tiny_skia::FillRule::EvenOdd, Transform::identity(), None);
            let mut st = Stroke { width: self.dpr, ..Default::default() };
            st.dash = tiny_skia::StrokeDash::new(vec![4.0 * self.dpr, 3.0 * self.dpr], 0.0);
            pm.stroke_path(&path, &render::paint(ACCENT, 1.0), &st, Transform::identity(), None);
        }
    }

    // ───────── Distort and envelope ─────────

    /// Can the selection be deformed? (Flash doesn't deform symbols, groups or text.)
    pub(crate) fn warpable(&self) -> bool {
        let sel = self.selected_shapes();
        !sel.is_empty()
            && sel.iter().all(|(_, s)| match &s.geom {
                Geometry::Instance(_) | Geometry::Text(_) => false,
                Geometry::Group { object, .. } => *object,
                _ => true,
            })
    }

    /// Current mesh (the session's, or a new straight one around the selection).
    pub(crate) fn env_view(&self) -> Option<EnvState> {
        if !self.xform_mode.warps() || !self.warpable() {
            return None;
        }
        if let Some(e) = &self.env {
            if e.ids == self.sel {
                return Some(e.clone());
            }
        }
        let base: Vec<Shape> = self.selected_shapes().into_iter().map(|(_, s)| s).collect();
        let bb = self.sel_bbox()?;
        (bb.w() > 0.01 || bb.h() > 0.01).then(|| EnvState::straight(self.sel.clone(), base, bb))
    }

    fn env_points(&self, e: &EnvState) -> Vec<(EnvHandle, Pt)> {
        let vm = self.view_matrix();
        let mut v = Vec::new();
        let distort = self.xform_mode == XMode::Distort;
        for k in 0..8 {
            if distort && k % 2 == 1 {
                continue;
            }
            if !distort {
                v.push(((1, k), vm.apply(e.hin[k].0, e.hin[k].1)));
                v.push(((2, k), vm.apply(e.hout[k].0, e.hout[k].1)));
            }
            v.push(((0, k), vm.apply(e.a[k].0, e.a[k].1)));
        }
        v
    }

    pub(crate) fn env_handle_at(&self, x: f32, y: f32) -> Option<EnvHandle> {
        let e = self.env_view()?;
        let r = 5.0 * self.dpr;
        // Anchors take priority over handles that overlap them.
        let pts = self.env_points(&e);
        pts.iter().filter(|(h, _)| h.0 == 0).chain(pts.iter().filter(|(h, _)| h.0 != 0)).find(|(_, p)| dist(*p, (x, y)) <= r).map(|(h, _)| *h)
    }

    pub(crate) fn env_drag(&mut self, st0: &EnvState, h: EnvHandle, start: Pt, p: Pt, alt: bool) {
        let d = (p.0 - start.0, p.1 - start.1);
        let mut e = st0.clone();
        let k = h.1;
        let add = |q: Pt| (q.0 + d.0, q.1 + d.1);
        match h.0 {
            0 => {
                e.a[k] = add(st0.a[k]);
                e.hin[k] = add(st0.hin[k]);
                e.hout[k] = add(st0.hout[k]);
                if self.xform_mode == XMode::Distort {
                    e.from_corners();
                }
            }
            _ => {
                let (moved, other) = if h.0 == 1 { (&mut e.hin, &mut e.hout) } else { (&mut e.hout, &mut e.hin) };
                let src = if h.0 == 1 { st0.hin[k] } else { st0.hout[k] };
                moved[k] = add(src);
                if !alt {
                    // Aligned handles, as in Bézier curves.
                    let a = st0.a[k];
                    let len = dist(if h.0 == 1 { st0.hout[k] } else { st0.hin[k] }, a);
                    let v = (a.0 - moved[k].0, a.1 - moved[k].1);
                    let l = (v.0 * v.0 + v.1 * v.1).sqrt();
                    if l > 1e-4 {
                        other[k] = (a.0 + v.0 / l * len, a.1 + v.1 / l * len);
                    }
                }
            }
        }
        let warped: Vec<Shape> = e.base.iter().map(|s| warp_shape(s, &|q| e.map(q))).collect();
        self.modify_shapes(&e.ids.clone(), |s| {
            if let Some(w) = warped.iter().find(|w| w.id == s.id) {
                let (id, tag) = (s.id, s.tag);
                *s = w.clone();
                s.id = id;
                s.tag = tag;
            }
        });
        self.env = Some(e);
    }

    pub(crate) fn draw_env(&self, pm: &mut Pixmap, vm: &Matrix) {
        let Some(e) = self.env_view() else { return };
        let dpr = self.dpr;
        let mut pb = PathBuilder::new();
        let o = e.outline();
        let s0 = vm.apply(o[0].0, o[0].1);
        pb.move_to(s0.0, s0.1);
        for q in &o[1..] {
            let s = vm.apply(q.0, q.1);
            pb.line_to(s.0, s.1);
        }
        pb.close();
        if let Some(path) = pb.finish() {
            pm.stroke_path(&path, &render::paint(ACCENT, 1.0), &Stroke { width: dpr, ..Default::default() }, Transform::identity(), None);
        }
        let pts = self.env_points(&e);
        for ((kind, k), p) in &pts {
            if *kind != 0 {
                let a = vm.apply(e.a[*k].0, e.a[*k].1);
                let mut lb = PathBuilder::new();
                lb.move_to(a.0, a.1);
                lb.line_to(p.0, p.1);
                if let Some(path) = lb.finish() {
                    pm.stroke_path(&path, &render::paint(ACCENT, 1.0), &Stroke { width: dpr, ..Default::default() }, Transform::identity(), None);
                }
                render::fill_circle(pm, p.0, p.1, 3.0 * dpr, ACCENT);
            }
        }
        let r = 3.5 * dpr;
        for ((kind, _), p) in &pts {
            if *kind == 0 {
                render::fill_rect(pm, p.0 - r, p.1 - r, 2.0 * r, 2.0 * r, Rgba::WHITE);
                render::stroke_rect(pm, p.0 - r, p.1 - r, 2.0 * r, 2.0 * r, ACCENT, dpr, false);
            }
        }
    }

    // ───────── Skew ─────────

    /// Box edge under the pointer (outside the grips): true = horizontal
    /// edge (top/bottom, skews in x); and the local coordinate of that edge.
    pub(crate) fn skew_edge_at(&self, x: f32, y: f32) -> Option<(bool, f32)> {
        let (f, lb) = self.frame_box()?;
        let vf = self.view_matrix().mul(&f);
        let r = 4.0 * self.dpr;
        let edges = [
            (true, lb.y0, (lb.x0, lb.y0), (lb.x1, lb.y0)),
            (true, lb.y1, (lb.x0, lb.y1), (lb.x1, lb.y1)),
            (false, lb.x0, (lb.x0, lb.y0), (lb.x0, lb.y1)),
            (false, lb.x1, (lb.x1, lb.y0), (lb.x1, lb.y1)),
        ];
        for (horiz, e, a, b) in edges {
            let (sa, sb) = (vf.apply(a.0, a.1), vf.apply(b.0, b.1));
            if dist_to_polyline((x, y), &[sa, sb], false) <= r {
                return Some((horiz, e));
            }
        }
        None
    }

    pub(crate) fn skew_drag(&mut self, f: &Matrix, horiz: bool, edge: f32, start_q: Pt, pivot: Pt, p: Pt) {
        let Some(inv) = f.invert() else { return };
        let q = inv.apply(p.0, p.1);
        let shear = if horiz {
            let den = edge - pivot.1;
            if den.abs() < 1e-3 {
                return;
            }
            Matrix { a: 1.0, b: 0.0, c: (q.0 - start_q.0) / den, d: 1.0, tx: 0.0, ty: 0.0 }
        } else {
            let den = edge - pivot.0;
            if den.abs() < 1e-3 {
                return;
            }
            Matrix { a: 1.0, b: (q.1 - start_q.1) / den, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 }
        };
        let w = f.mul(&Matrix::about(pivot, &shear)).mul(&inv);
        let ids = self.sel.clone();
        self.modify_shapes(&ids, |s| s.m = w.mul(&s.m));
    }
}

#[wasm_bindgen]
impl Editor {
    /// PolyStar options: style ("polygon" | "star"), sides (3…32) and star point size (0…1).
    pub fn set_polystar(&mut self, style: &str, sides: u32, point: f32) {
        self.polystar = (style == "star", sides.clamp(3, 32), point.clamp(0.0, 1.0));
    }

    /// Lasso polygon mode.
    pub fn set_lasso_polygon(&mut self, on: bool) {
        self.lasso_poly = on;
        self.lasso_pts.clear();
    }

    /// Closes the polygon lasso (double-click or Enter).
    pub fn lasso_finish(&mut self) {
        if !self.lasso_pts.is_empty() {
            self.lasso_finish_poly();
        }
    }

    /// Free Transform modifier: "free" | "rotate" | "scale" | "distort" | "envelope".
    pub fn set_xform_mode(&mut self, mode: &str) {
        self.xform_mode = XMode::parse(mode);
        self.env = None;
        if self.xform_mode.warps() && !self.sel.is_empty() && !self.warpable() {
            self.message = "Distorsionar y Envoltura solo se aplican a formas (no a símbolos, grupos ni texto)".into();
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
        e.snap_objects = false;
        e
    }

    #[test]
    fn polystar_star_has_alternating_radii() {
        let mut e = editor();
        e.object_mode = true;
        e.set_polystar("star", 5, 0.5);
        e.set_tool("polystar");
        e.pointer_down(200.0, 200.0, 0, false, false);
        e.pointer_up(200.0, 100.0, false, false);
        let s = &e.doc.frame_shapes(0, 0)[0];
        let n = &s.bez_paths()[0].nodes;
        assert_eq!(n.len(), 10);
        let r = |k: usize| dist(s.m.apply(n[k].p.0, n[k].p.1), (200.0, 200.0));
        assert!((r(0) - 100.0).abs() < 0.01 && (r(1) - 50.0).abs() < 0.01);
        assert!((s.m.apply(n[0].p.0, n[0].p.1).1 - 100.0).abs() < 0.01, "el primer vértice apunta al puntero");
    }

    #[test]
    fn lasso_cuts_raw_fill() {
        let mut e = editor();
        e.stroke_on = false;
        e.set_tool("rect");
        e.pointer_down(100.0, 100.0, 0, false, false);
        e.pointer_up(300.0, 300.0, false, false);
        e.set_tool("lasso");
        e.set_lasso_polygon(true);
        for p in [(50.0, 50.0), (200.0, 50.0), (50.0, 200.0)] {
            e.pointer_down(p.0, p.1, 0, false, false);
            e.pointer_up(p.0, p.1, false, false);
        }
        e.lasso_finish();
        let fills = e.doc.frame_shapes(0, 0);
        assert_eq!(fills.len(), 2, "el triángulo corta el relleno");
        assert_eq!(e.sel.len(), 1);
        let a = paint::region_area(&paint::fill_region(&e.selected_shapes()[0].1));
        assert!((a - 1250.0).abs() < 10.0, "área del trozo seleccionado: {a}");
    }

    #[test]
    fn skew_and_distort() {
        let mut e = editor();
        e.object_mode = true;
        e.set_tool("rect");
        e.pointer_down(100.0, 100.0, 0, false, false);
        e.pointer_up(200.0, 200.0, false, false);
        e.set_tool("transform");
        // Skew by dragging the top edge.
        e.pointer_down(130.0, 100.0, 0, false, false);
        assert!(matches!(e.drag, Drag::Skew { .. }));
        e.pointer_up(180.0, 100.0, false, false);
        let b = e.doc.frame_shapes(0, 0)[0].world_bounds();
        assert!((b.w() - 200.0).abs() < 1.0, "sesgado: {b:?}");
        // Distort: move a corner.
        e.undo();
        e.set_xform_mode("distort");
        e.pointer_down(200.0, 200.0, 0, false, false);
        e.pointer_up(260.0, 230.0, false, false);
        let s = &e.doc.frame_shapes(0, 0)[0];
        let b = s.world_bounds();
        assert!((b.x1 - 260.0).abs() < 1.0 && (b.y1 - 230.0).abs() < 1.0, "{b:?}");
        assert!(s.hit(105.0, 105.0, 0.5), "la esquina opuesta no se mueve");
    }

    #[test]
    fn bending_follows_pieces_split_differently() {
        // Raw rectangle; a vertical line splits the top line in two
        // (and the fill too). Bending the top edge on one side moves
        // the line and the fill edge on that side, even if their nodes don't match.
        let mut e = editor();
        e.set_tool("rect");
        e.pointer_down(100.0, 100.0, 0, false, false);
        e.pointer_up(300.0, 200.0, false, false);
        e.set_tool("line");
        e.pointer_down(250.0, 50.0, 0, false, false);
        e.pointer_up(250.0, 250.0, false, false);
        e.set_tool("select");
        e.deselect();
        // Bend the top edge of the left part, at x = 150.
        e.pointer_down(150.0, 100.0, 0, false, false);
        e.pointer_move(150.0, 80.0, false, false);
        e.pointer_up(150.0, 70.0, false, false);
        let shapes = e.doc.frame_shapes(0, 0);
        let fill = shapes.iter().filter(|s| s.fill.is_some()).find(|s| s.world_bounds().x0 < 110.0).unwrap();
        let line_top = shapes.iter().filter(|s| s.stroke.is_some() && s.fill.is_none()).filter(|s| s.world_bounds().y0 < 95.0 && s.world_bounds().w() > 20.0).collect::<Vec<_>>();
        assert!(!line_top.is_empty(), "la línea de arriba se dobló");
        let fy = fill.world_bounds().y0;
        let ly = line_top.iter().map(|s| s.world_bounds().y0).fold(f32::INFINITY, f32::min);
        assert!(fy < 90.0, "el relleno se dobló: {fy}");
        assert!((fy - ly).abs() < 1.0, "relleno y línea siguen juntos: {fy} {ly}");
    }

    #[test]
    fn bending_a_line_moves_a_fill_edge_with_other_nodes() {
        let mut e = editor();
        let c = |x: f32, y: f32| Node::corner((x, y));
        // Fill with a node in the middle of the top edge; the line runs end to end.
        let mut fill = Shape::new(
            Geometry::Bezier { paths: vec![BezPath { nodes: vec![c(100.0, 100.0), c(200.0, 100.0), c(300.0, 100.0), c(300.0, 200.0), c(100.0, 200.0)], closed: true }] },
            Some(Rgba::BLACK),
            None,
            1.0,
            Matrix::IDENTITY,
        );
        fill.raw = true;
        fill.id = 1;
        let mut line = Shape::new(Geometry::Bezier { paths: vec![BezPath { nodes: vec![c(100.0, 100.0), c(300.0, 100.0)], closed: false }] }, None, Some(Rgba::BLACK), 1.0, Matrix::IDENTITY);
        line.raw = true;
        line.id = 2;
        e.doc.layers[0].keys.get_mut(&0).unwrap().shapes = vec![fill, line];
        e.doc.next_id = 10;
        e.set_tool("select");
        e.pointer_down(150.0, 100.0, 0, false, false);
        e.pointer_move(150.0, 80.0, false, false);
        e.pointer_up(150.0, 60.0, false, false);
        let v = e.doc.frame_shapes(0, 0);
        let (f, l) = (&v[0], &v[1]);
        assert!(l.world_bounds().y0 < 70.0, "la línea se dobló");
        // The fill edge follows the line along its whole length.
        for x in [130.0, 170.0, 230.0, 270.0] {
            let top = |s: &Shape| (0..400).map(|i| 40.0 + i as f32 * 0.25).find(|y| s.hit(x, *y, 0.3)).unwrap();
            assert!((top(f) - top(l)).abs() < 1.2, "x={x}: relleno {} línea {}", top(f), top(l));
        }
    }

    #[test]
    fn envelope_bends_a_side() {
        let mut e = editor();
        e.object_mode = true;
        e.set_tool("rect");
        e.pointer_down(100.0, 100.0, 0, false, false);
        e.pointer_up(300.0, 200.0, false, false);
        e.set_tool("transform");
        e.set_xform_mode("envelope");
        // Top-middle anchor upwards: the side curves.
        e.pointer_down(200.0, 100.0, 0, false, false);
        e.pointer_up(200.0, 60.0, false, false);
        let s = &e.doc.frame_shapes(0, 0)[0];
        let b = s.world_bounds();
        assert!((b.y0 - 60.0).abs() < 1.5, "{b:?}");
        assert!(!s.hit(110.0, 62.0, 0.5) && s.hit(200.0, 64.0, 0.5), "curva, no un rectángulo");
        // Again from the deformed mesh (the session is kept).
        e.pointer_down(200.0, 60.0, 0, false, false);
        e.pointer_up(200.0, 80.0, false, false);
        let b = e.doc.frame_shapes(0, 0)[0].world_bounds();
        assert!((b.y0 - 80.0).abs() < 1.5, "{b:?}");
    }
}
