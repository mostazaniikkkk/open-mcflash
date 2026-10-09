//! Special fills in the editor: the active fill (color, gradient or
//! bitmap), "Lock Fill" and the Fill Transform
//! tool (F), which edits gradients and bitmap fills.

use super::*;
use crate::bitmap::BitmapFill;
use crate::grad::{GradKind, Gradient};
use crate::render;

/// Fill Transform handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GradHandle {
    Center,
    Width,
    Height,
    Size,
    Rotate,
    Focal,
}

/// Fill being edited by Fill Transform (in document coordinates).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PaintRef {
    Grad(Gradient),
    /// Bitmap with the image size.
    Bmp(BitmapFill, f32, f32),
}

impl PaintRef {
    /// "Normalized" matrix: maps the square [−1, 1]² to the fill's box.
    fn norm(&self) -> Matrix {
        match self {
            PaintRef::Grad(g) => g.m,
            PaintRef::Bmp(b, w, h) => b.m.mul(&Matrix::translate(w / 2.0, h / 2.0)).mul(&Matrix::scale(w / 2.0, h / 2.0)),
        }
    }

    fn with_norm(&self, n: Matrix) -> PaintRef {
        match self {
            PaintRef::Grad(g) => PaintRef::Grad(Gradient { m: n, ..g.clone() }),
            PaintRef::Bmp(b, w, h) => {
                let m = n.mul(&Matrix::scale(2.0 / w, 2.0 / h)).mul(&Matrix::translate(-w / 2.0, -h / 2.0));
                PaintRef::Bmp(BitmapFill { m, ..b.clone() }, *w, *h)
            }
        }
    }

    fn apply_to(&self, s: &mut Shape) {
        match self {
            PaintRef::Grad(g) => s.set_grad_world(Some(g.clone())),
            PaintRef::Bmp(b, _, _) => s.set_bmp_world(Some(b.clone())),
        }
    }
}

fn paint_of(s: &Shape) -> Option<PaintRef> {
    if let Some(g) = s.grad_world() {
        return Some(PaintRef::Grad(g));
    }
    let b = s.bmp_world()?;
    let (w, h) = b.size();
    Some(PaintRef::Bmp(b, w, h))
}

fn same_paint(a: &PaintRef, b: &PaintRef) -> bool {
    match (a, b) {
        (PaintRef::Grad(x), PaintRef::Grad(y)) => x.approx_eq(y),
        (PaintRef::Bmp(x, ..), PaintRef::Bmp(y, ..)) => x.approx_eq(y),
        _ => false,
    }
}

impl Editor {
    /// Gradient for a new fill occupying `b` (document), based on the
    /// active fill; with "Lock Fill", fitted to the stage.
    pub(crate) fn new_grad(&self, b: &BBox) -> Option<Gradient> {
        let g = self.fill_grad.clone()?;
        Some(Gradient { m: Gradient::fit(g.kind, &self.fill_area(b)), ..g })
    }

    /// New bitmap fill: the image at its size, tiled from the
    /// shape's corner (or the stage's, with "Lock Fill").
    pub(crate) fn new_bmp(&self, b: &BBox) -> Option<BitmapFill> {
        let id = self.fill_bmp?;
        let a = self.fill_area(b);
        Some(BitmapFill { id, m: Matrix::translate(a.x0, a.y0), clip: false, smooth: true, cx: None })
    }

    fn fill_area(&self, b: &BBox) -> BBox {
        if self.lock_fill {
            BBox { x0: 0.0, y0: 0.0, x1: self.doc.width, y1: self.doc.height }
        } else {
            *b
        }
    }

    /// Applies the active fill (color, gradient or bitmap) to a shape occupying `b`.
    pub(crate) fn paint_shape(&self, s: &mut Shape, b: &BBox) {
        s.set_grad_world(self.new_grad(b));
        s.set_bmp_world(self.new_bmp(b));
    }

    /// Gives the active fill to a newly drawn shape.
    pub(crate) fn style_new_fill(&self, s: &mut Shape) {
        if s.fill.is_some() && s.is_closed() {
            let b = s.world_bounds();
            self.paint_shape(s, &b);
        }
    }

    /// Same for several pieces of a single fill (brush, paint bucket): the
    /// fill is fitted to all of them together.
    pub(crate) fn style_new_fills(&self, v: &mut [Shape]) {
        let b = v.iter().fold(BBox::EMPTY, |acc, s| acc.union(&s.world_bounds()));
        for s in v.iter_mut().filter(|s| s.fill.is_some()) {
            self.paint_shape(s, &b);
        }
    }

    // ───────── Fill Transform ─────────

    /// Selected shape with a gradient or bitmap, and all pieces that
    /// share that same fill (a split fill is still a single one).
    pub(crate) fn grad_target(&self) -> Option<(Vec<u32>, PaintRef)> {
        let (li, s) = self.selected_shapes().into_iter().find(|(_, s)| paint_of(s).is_some())?;
        let p = paint_of(&s)?;
        let ids = self
            .scope_shapes(li)
            .into_iter()
            .filter(|o| paint_of(o).map_or(false, |op| same_paint(&op, &p)))
            .map(|o| o.id)
            .collect();
        Some((ids, p))
    }

    /// Screen positions of the selected fill's handles.
    pub(crate) fn grad_handles(&self) -> Option<Vec<(GradHandle, Pt)>> {
        let (_, p) = self.grad_target()?;
        let m = self.view_matrix().mul(&p.norm());
        let k = std::f32::consts::FRAC_1_SQRT_2;
        let mut v = vec![(GradHandle::Center, m.apply(0.0, 0.0)), (GradHandle::Width, m.apply(1.0, 0.0))];
        match &p {
            PaintRef::Grad(g) if g.kind == GradKind::Linear => v.push((GradHandle::Rotate, m.apply(1.0, -1.0))),
            PaintRef::Grad(g) => {
                v.push((GradHandle::Size, m.apply(k, k)));
                v.push((GradHandle::Rotate, m.apply(k, -k)));
                // The focal point is drawn a bit above the center so it can be grabbed.
                let f = m.apply(g.focal, 0.0);
                v.push((GradHandle::Focal, (f.0, f.1 - 12.0 * self.dpr)));
            }
            PaintRef::Bmp(..) => {
                v.push((GradHandle::Height, m.apply(0.0, 1.0)));
                v.push((GradHandle::Size, m.apply(1.0, 1.0)));
                v.push((GradHandle::Rotate, m.apply(1.0, -1.0)));
            }
        }
        Some(v)
    }

    pub(crate) fn grad_handle_at(&self, x: f32, y: f32) -> Option<GradHandle> {
        let hs = self.grad_handles()?;
        let r = 7.0 * self.dpr;
        // The focal point and edge handles take priority over the center.
        hs.iter().rev().find(|(_, p)| dist(*p, (x, y)) <= r).map(|(h, _)| *h)
    }

    pub(crate) fn fill_xform_down(&mut self, x: f32, y: f32, p: Pt) {
        if let Some(h) = self.grad_handle_at(x, y) {
            let (ids, g) = self.grad_target().unwrap();
            self.drag = Drag::Grad { base: Box::new(self.doc.clone()), ids, handle: h, g0: g, start: p };
            return;
        }
        match self.hit_top(p) {
            Some((li, id)) => {
                let s = self.scope_shapes(li).into_iter().find(|s| s.id == id);
                match s {
                    Some(s) if paint_of(&s).is_some() => {
                        self.layer = li;
                        self.sel = vec![id];
                    }
                    _ => {
                        self.sel.clear();
                        self.message = "Haz clic en un relleno degradado o de mapa de bits para transformarlo".into();
                    }
                }
            }
            None => self.sel.clear(),
        }
    }

    pub(crate) fn grad_drag(&mut self, ids: &[u32], h: GradHandle, p0: &PaintRef, start: Pt, p: Pt, shift: bool) {
        let n0 = p0.norm();
        let c = n0.apply(0.0, 0.0);
        let axis = |v: Pt| {
            let l2 = v.0 * v.0 + v.1 * v.1;
            let k = if l2 > 1e-9 { ((p.0 - c.0) * v.0 + (p.1 - c.1) * v.1) / l2 } else { 1.0 };
            if k.abs() < 0.02 { 0.02f32.copysign(k) } else { k }
        };
        let mut out = p0.clone();
        match h {
            GradHandle::Center => out = p0.with_norm(Matrix::translate(p.0 - start.0, p.1 - start.1).mul(&n0)),
            GradHandle::Width => out = p0.with_norm(n0.mul(&Matrix::scale(axis((n0.a, n0.b)), 1.0))),
            GradHandle::Height => out = p0.with_norm(n0.mul(&Matrix::scale(1.0, axis((n0.c, n0.d))))),
            GradHandle::Size => {
                let k = (dist(p, c) / dist(start, c).max(1e-3)).max(0.02);
                out = p0.with_norm(n0.mul(&Matrix::scale(k, k)));
            }
            GradHandle::Rotate => {
                let mut a = (p.1 - c.1).atan2(p.0 - c.0) - (start.1 - c.1).atan2(start.0 - c.0);
                if shift {
                    let step = std::f32::consts::PI / 12.0;
                    a = (a / step).round() * step;
                }
                out = p0.with_norm(Matrix::about(c, &Matrix::rotate(a)).mul(&n0));
            }
            GradHandle::Focal => {
                if let (PaintRef::Grad(g), Some(inv)) = (&mut out, n0.invert()) {
                    g.focal = inv.apply(p.0, p.1).0.clamp(-0.98, 0.98);
                }
            }
        }
        self.modify_shapes(ids, |s| out.apply_to(s));
    }

    pub(crate) fn draw_grad_handles(&self, pm: &mut Pixmap, vm: &Matrix) {
        let Some((_, p)) = self.grad_target() else { return };
        let Some(hs) = self.grad_handles() else { return };
        let m = vm.mul(&p.norm());
        let dpr = self.dpr;
        let line = |pm: &mut Pixmap, pts: &[Pt], closed: bool| {
            let mut pb = tiny_skia::PathBuilder::new();
            pb.move_to(pts[0].0, pts[0].1);
            for q in &pts[1..] {
                pb.line_to(q.0, q.1);
            }
            if closed {
                pb.close();
            }
            if let Some(path) = pb.finish() {
                for (c, w) in [(Rgba::WHITE, 3.0 * dpr), (Rgba::BLACK, dpr)] {
                    pm.stroke_path(&path, &render::paint(c, 1.0), &tiny_skia::Stroke { width: w, ..Default::default() }, tiny_skia::Transform::identity(), None);
                }
            }
        };
        match &p {
            PaintRef::Grad(g) if g.kind == GradKind::Linear => {
                line(pm, &[m.apply(-1.0, -1.0), m.apply(-1.0, 1.0)], false);
                line(pm, &[m.apply(1.0, -1.0), m.apply(1.0, 1.0)], false);
            }
            PaintRef::Grad(_) => {
                let ring: Vec<Pt> = (0..64)
                    .map(|i| {
                        let a = i as f32 / 64.0 * std::f32::consts::TAU;
                        m.apply(a.cos(), a.sin())
                    })
                    .collect();
                line(pm, &ring, true);
            }
            PaintRef::Bmp(..) => line(pm, &[m.apply(-1.0, -1.0), m.apply(1.0, -1.0), m.apply(1.0, 1.0), m.apply(-1.0, 1.0)], true),
        }
        let r = 4.0 * dpr;
        for (h, p) in hs {
            match h {
                GradHandle::Center => {
                    render::fill_circle(pm, p.0, p.1, r, Rgba::WHITE);
                    render::stroke_circle(pm, p.0, p.1, r, Rgba::BLACK, dpr);
                }
                GradHandle::Width | GradHandle::Height => {
                    render::fill_rect(pm, p.0 - r, p.1 - r, 2.0 * r, 2.0 * r, Rgba::WHITE);
                    render::stroke_rect(pm, p.0 - r, p.1 - r, 2.0 * r, 2.0 * r, Rgba::BLACK, dpr, false);
                }
                GradHandle::Size | GradHandle::Rotate => {
                    render::fill_circle(pm, p.0, p.1, r, Rgba::WHITE);
                    render::stroke_circle(pm, p.0, p.1, r, Rgba::BLACK, dpr);
                    render::fill_circle(pm, p.0, p.1, 1.5 * dpr, Rgba::BLACK);
                }
                GradHandle::Focal => {
                    let mut pb = tiny_skia::PathBuilder::new();
                    pb.move_to(p.0, p.1 - r);
                    pb.line_to(p.0 + r, p.1 + r);
                    pb.line_to(p.0 - r, p.1 + r);
                    pb.close();
                    if let Some(path) = pb.finish() {
                        pm.fill_path(&path, &render::paint(Rgba::WHITE, 1.0), tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
                        pm.stroke_path(&path, &render::paint(Rgba::BLACK, 1.0), &tiny_skia::Stroke { width: dpr, ..Default::default() }, tiny_skia::Transform::identity(), None);
                    }
                }
            }
        }
    }
}

#[wasm_bindgen]
impl Editor {
    /// Active gradient fill, as JSON: {"kind":"linear","stops":[{"t":0,"c":{…}},…],"spread":"pad","focal":0}.
    /// Also applied to the selection (keeping its transform if it already had a gradient).
    pub fn set_fill_gradient(&mut self, json: &str) {
        let g = match serde_json::from_str::<Gradient>(json) {
            Ok(g) => g.normalized(),
            Err(e) => {
                self.message = format!("Degradado no válido: {e}");
                return;
            }
        };
        self.fill = g.base_color();
        self.fill_on = true;
        self.fill_bmp = None;
        self.fill_grad = Some(Gradient { m: Matrix::IDENTITY, ..g.clone() });
        let stage = BBox { x0: 0.0, y0: 0.0, x1: self.doc.width, y1: self.doc.height };
        let lock = self.lock_fill;
        self.edit_selection(|s| {
            if !s.is_closed() {
                return;
            }
            let m = match s.grad_world() {
                Some(old) if old.kind == g.kind => old.m,
                _ => Gradient::fit(g.kind, &if lock { stage } else { s.world_bounds() }),
            };
            s.fill = Some(g.base_color());
            s.set_grad_world(Some(Gradient { m, ..g.clone() }));
        });
    }

    /// Active bitmap fill (an image from the Library). Also
    /// applied to the selection.
    pub fn set_fill_bitmap(&mut self, asset: u32) {
        if !self.doc.bitmaps.iter().any(|a| a.id == asset) {
            return;
        }
        self.fill_on = true;
        self.fill_grad = None;
        self.fill_bmp = Some(asset);
        let bm = |s: &Shape, lock: bool, w: f32, h: f32| {
            let b = if lock { BBox { x0: 0.0, y0: 0.0, x1: w, y1: h } } else { s.world_bounds() };
            BitmapFill { id: asset, m: Matrix::translate(b.x0, b.y0), clip: false, smooth: true, cx: None }
        };
        let (lock, w, h) = (self.lock_fill, self.doc.width, self.doc.height);
        self.edit_selection(|s| {
            if s.is_closed() && !matches!(s.geom, Geometry::Text(_)) {
                let b = bm(s, lock, w, h);
                s.set_bmp_world(Some(b));
            }
        });
    }

    /// "Lock Fill" option of the paint bucket and brush.
    pub fn set_lock_fill(&mut self, on: bool) {
        self.lock_fill = on;
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
    const LIN: &str = r#"{"kind":"linear","stops":[{"t":0,"c":{"r":255,"g":0,"b":0,"a":255}},{"t":1,"c":{"r":0,"g":0,"b":255,"a":255}}]}"#;

    #[test]
    fn rect_with_gradient_renders_both_ends() {
        let mut e = editor();
        e.set_fill_gradient(LIN);
        e.stroke_on = false;
        e.set_tool("rect");
        e.pointer_down(100.0, 100.0, 0, false, false);
        e.pointer_up(300.0, 200.0, false, false);
        let s = &e.doc.frame_shapes(0, 0)[0];
        assert!(s.grad.is_some());
        let pm = render::stage_pixmap(&e.doc, 0, 1.0).unwrap();
        let px = |x: u32, y: u32| pm.pixel(x, y).unwrap().demultiply();
        assert!(px(102, 150).red() > 230 && px(298, 150).blue() > 230, "rojo a la izquierda, azul a la derecha");
    }

    #[test]
    fn cut_pieces_keep_the_same_gradient() {
        let mut e = editor();
        e.set_fill_gradient(LIN);
        e.stroke_on = false;
        e.set_tool("rect");
        e.pointer_down(100.0, 100.0, 0, false, false);
        e.pointer_up(300.0, 200.0, false, false);
        let g0 = e.doc.frame_shapes(0, 0)[0].grad_world().unwrap();
        // A line splits it into two pieces with the same gradient.
        e.stroke_on = true;
        e.set_tool("line");
        e.pointer_down(200.0, 50.0, 0, false, false);
        e.pointer_up(200.0, 250.0, false, false);
        let fills: Vec<Shape> = e.doc.frame_shapes(0, 0).into_iter().filter(|s| s.fill.is_some()).collect();
        assert_eq!(fills.len(), 2);
        assert!(fills.iter().all(|s| s.grad_world().unwrap().approx_eq(&g0)));
        // Fill Transform moves both pieces at once.
        e.set_tool("filltransform");
        e.pointer_down(150.0, 150.0, 0, false, false);
        e.pointer_up(150.0, 150.0, false, false);
        assert_eq!(e.grad_target().unwrap().0.len(), 2);
        let c = e.grad_handles().unwrap()[0].1;
        e.pointer_down(c.0, c.1, 0, false, false);
        e.pointer_up(c.0 + 40.0, c.1, false, false);
        let fills: Vec<Shape> = e.doc.frame_shapes(0, 0).into_iter().filter(|s| s.fill.is_some()).collect();
        assert!(fills.iter().all(|s| (s.grad_world().unwrap().m.tx - 240.0).abs() < 0.5));
    }

    #[test]
    fn lock_fill_spans_the_stage() {
        let mut e = editor();
        e.set_fill_gradient(LIN);
        e.set_lock_fill(true);
        e.object_mode = true;
        e.set_tool("rect");
        e.pointer_down(10.0, 10.0, 0, false, false);
        e.pointer_up(60.0, 60.0, false, false);
        let g = e.doc.frame_shapes(0, 0)[0].grad_world().unwrap();
        assert!((g.m.tx - 275.0).abs() < 0.01 && (g.m.a - 275.0).abs() < 0.01);
    }
}
