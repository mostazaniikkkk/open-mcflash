//! Rasterization with tiny-skia (all in Rust, the browser only copies pixels).

use crate::geom::*;
use crate::model::*;
use crate::symbols::{symbol, symbol_frame, symbol_shapes, CxForm, Inst, Symbol, SymbolKind};
use crate::timeline::{eff_locked, eff_visible, layer_shapes, mask_of};
use tiny_skia::{FillRule, LineCap, LineJoin, Mask, Paint, PathBuilder, Pixmap, Stroke, StrokeDash, Transform};

pub fn build_path(segs: &[Seg]) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for s in segs {
        match *s {
            Seg::Move(x, y) => pb.move_to(x, y),
            Seg::Line(x, y) => pb.line_to(x, y),
            Seg::Quad(cx, cy, x, y) => pb.quad_to(cx, cy, x, y),
            Seg::Cubic(a, b, c, d, x, y) => pb.cubic_to(a, b, c, d, x, y),
            Seg::Close => pb.close(),
        }
    }
    pb.finish()
}

pub fn paint(c: Rgba, alpha_mul: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c.r, c.g, c.b, (c.a as f32 * alpha_mul).round().clamp(0.0, 255.0) as u8);
    p.anti_alias = true;
    p
}

pub fn draw_shape(pm: &mut Pixmap, s: &Shape, base: &Matrix, alpha: f32) {
    draw_shape_masked(pm, s, base, alpha, None);
}

pub fn draw_shape_masked(pm: &mut Pixmap, s: &Shape, base: &Matrix, alpha: f32, mask: Option<&Mask>) {
    if let Geometry::Group { children, .. } = &s.geom {
        let m = base.mul(&s.m);
        for c in children {
            draw_shape_masked(pm, c, &m, alpha, mask);
        }
        return;
    }
    if let Geometry::Text(tf) = &s.geom {
        // Dynamic or input with "Show border": white background and black border.
        if tf.border && tf.kind != crate::text::TextKind::Static {
            let b = tf.bounds();
            if let Some(r) = rect_path(b.x0, b.y0, b.w(), b.h()) {
                let t = base.mul(&s.m).to_skia();
                pm.fill_path(&r, &paint(Rgba::WHITE, alpha), FillRule::Winding, t, mask);
                let st = Stroke { width: 1.0 / base.mul(&s.m).avg_scale(), ..Default::default() };
                pm.stroke_path(&r, &paint(Rgba::BLACK, alpha), &st, t, mask);
            }
        }
    }
    // Text with several formats: each group with its color (and its simulated bold).
    if let (Geometry::Text(tf), Some(f)) = (&s.geom, s.fill) {
        let laid = tf.laid();
        if laid.groups.len() > 1 || laid.groups.iter().any(|g| g.color.is_some()) {
            let t = base.mul(&s.m).to_skia();
            for g in &laid.groups {
                let Some(path) = build_path(&g.outline) else { continue };
                let c = g.color.unwrap_or(f);
                pm.fill_path(&path, &paint(c, alpha), FillRule::EvenOdd, t, mask);
                if g.fake_bold {
                    let st = Stroke { width: g.size * 0.045, line_join: LineJoin::Round, ..Default::default() };
                    pm.stroke_path(&path, &paint(c, alpha), &st, t, mask);
                }
            }
            return;
        }
    }
    let segs = s.outline();
    let Some(path) = build_path(&segs) else { return };
    let t = base.mul(&s.m).to_skia();
    if s.is_closed() {
        if let Some(f) = s.fill {
            let shader = s.grad.as_ref().and_then(|g| g.shader(alpha));
            let bmp = s.bmp.as_ref().and_then(|b| crate::bitmap::pixmap_cx(b.id, b.cx.as_ref()).map(|p| (b, p)));
            if let Some((b, img)) = bmp {
                let pat = tiny_skia::Pattern::new(
                    (*img).as_ref(),
                    if b.clip { tiny_skia::SpreadMode::Pad } else { tiny_skia::SpreadMode::Repeat },
                    if b.smooth { tiny_skia::FilterQuality::Bicubic } else { tiny_skia::FilterQuality::Nearest },
                    alpha.clamp(0.0, 1.0),
                    b.m.to_skia(),
                );
                let p = Paint { shader: pat, anti_alias: true, ..Default::default() };
                pm.fill_path(&path, &p, FillRule::EvenOdd, t, mask);
            } else {
            match shader {
                Some(sh) => {
                    // The gradient is in local coordinates: the same space as the path.
                    let p = Paint { shader: sh, anti_alias: true, ..Default::default() };
                    pm.fill_path(&path, &p, FillRule::EvenOdd, t, mask);
                }
                None => pm.fill_path(&path, &paint(f, alpha), FillRule::EvenOdd, t, mask),
            }
            }
            // Simulated bold (the font lacks that variant).
            if let Geometry::Text(tf) = &s.geom {
                if tf.laid().fake_bold {
                    let st = Stroke { width: tf.size * 0.045, line_join: LineJoin::Round, ..Default::default() };
                    pm.stroke_path(&path, &paint(f, alpha), &st, t, mask);
                }
            }
        }
    }
    if let Some(c) = s.stroke {
        let st = Stroke { width: s.stroke_width.max(0.05), line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
        pm.stroke_path(&path, &paint(c, alpha), &st, t, mask);
    }
}

/// Outline mode: only the shape's outline, in the layer's color.
pub fn draw_outline(pm: &mut Pixmap, s: &Shape, base: &Matrix, color: Rgba, alpha: f32) {
    if let Geometry::Group { children, .. } = &s.geom {
        let m = base.mul(&s.m);
        for c in children {
            draw_outline(pm, c, &m, color, alpha);
        }
        return;
    }
    let Some(path) = build_path(&s.outline()) else { return };
    let m = base.mul(&s.m);
    let st = Stroke { width: 1.0 / m.avg_scale(), ..Default::default() };
    pm.stroke_path(&path, &paint(color, alpha), &st, m.to_skia(), None);
}

/// How to draw the layers.
#[derive(Clone, Copy)]
pub struct DrawOpts<'a> {
    /// In the editor: hidden layers left out, guides visible, layer outlines,
    /// and masks only take effect if mask and masked layers are locked.
    pub editor: bool,
    pub export_hidden: bool,
    /// Everything as outlines (onion skin in outlines).
    pub outlines: bool,
    pub alpha_of: &'a dyn Fn(usize, &Shape) -> f32,
    /// Color effect inherited from the instances containing what is being drawn.
    pub cx: CxForm,
    /// Mouse (in output coordinates) and whether it is pressed: button states.
    pub mouse: Option<(Pt, bool)>,
    /// Instance not drawn (the one being edited in place).
    pub skip: Option<u32>,
    /// Inherited clip (mask of a layer containing the instance).
    pub clip: Option<&'a Mask>,
}

impl<'a> DrawOpts<'a> {
    pub fn new(editor: bool, export_hidden: bool, alpha_of: &'a dyn Fn(usize, &Shape) -> f32) -> DrawOpts<'a> {
        DrawOpts { editor, export_hidden, outlines: false, alpha_of, cx: CxForm::ID, mouse: None, skip: None, clip: None }
    }
}

fn mask_from(pm: &Pixmap, shapes: &[Shape], base: &Matrix) -> Mask {
    let mut m = Mask::new(pm.width(), pm.height()).unwrap();
    for s in shapes.iter().flat_map(|s| s.leaves(&Matrix::IDENTITY)) {
        if !s.is_closed() || s.fill.is_none() {
            continue;
        }
        if let Some(path) = build_path(&s.outline()) {
            m.fill_path(&path, FillRule::EvenOdd, true, base.mul(&s.m).to_skia());
        }
    }
    m
}

/// Draws the layers of a timeline (bottom to top) at frame `frame`.
pub fn draw_layers(pm: &mut Pixmap, layers: &[Layer], frame: u32, base: &Matrix, o: &DrawOpts) {
    for li in (0..layers.len()).rev() {
        let l = &layers[li];
        if l.kind == LayerKind::Folder {
            continue;
        }
        let visible = eff_visible(layers, li);
        if !visible && (o.editor || !o.export_hidden) {
            continue;
        }
        if !o.editor && matches!(l.kind, LayerKind::Guide | LayerKind::MotionGuide) {
            continue;
        }
        // The mask itself is not visible while masking.
        if l.kind == LayerKind::Mask && (!o.editor || eff_locked(layers, li)) {
            continue;
        }
        let mask = mask_of(layers, li).filter(|&mi| !o.editor || (eff_locked(layers, mi) && eff_locked(layers, li)));
        let own_clip = mask.map(|mi| mask_from(pm, &layer_shapes(layers, mi, frame), base));
        let clip = own_clip.as_ref().or(o.clip);
        // Frames elapsed since the keyframe (for graphics and clips).
        let offset = l.anim_offset(frame);
        for s in layer_shapes(layers, li, frame) {
            let alpha = (o.alpha_of)(li, &s);
            if o.outlines || (l.outline && o.editor) {
                draw_outline(pm, &s, base, l.color, alpha);
            } else {
                draw_item(pm, &s, base, alpha, clip, offset, o);
            }
        }
    }
}

/// Draws a shape, group or instance.
pub fn draw_item(pm: &mut Pixmap, s: &Shape, base: &Matrix, alpha: f32, clip: Option<&Mask>, offset: u32, o: &DrawOpts) {
    if o.skip == Some(s.id) && !s.is_instance() {
        return;
    }
    match &s.geom {
        Geometry::Group { children, .. } => {
            let m = base.mul(&s.m);
            for c in children {
                draw_item(pm, c, &m, alpha, clip, offset, o);
            }
        }
        Geometry::Instance(inst) => draw_instance(pm, s, inst, base, alpha, clip, offset, o),
        _ => {
            if o.cx.is_identity() {
                draw_shape_masked(pm, s, base, alpha, clip);
            } else {
                let mut c = s.clone();
                c.apply_cx(&o.cx);
                draw_shape_masked(pm, &c, base, alpha, clip);
            }
        }
    }
}

/// Button state according to the mouse: 0 up, 1 over, 2 down.
fn button_state(sym: &Symbol, s: &Shape, base: &Matrix, mouse: Option<(Pt, bool)>) -> u32 {
    let Some((p, down)) = mouse else { return 0 };
    let len = sym.total();
    let hit_frame = if len > 3 && !symbol_shapes(sym, 3).is_empty() { 3 } else { 0 };
    let m = base.mul(&s.m);
    let over = symbol_shapes(sym, hit_frame).iter().flat_map(|c| c.leaves(&m)).any(|l| l.hit(p.0, p.1, 0.5));
    match (over, down) {
        (false, _) => 0,
        (true, false) => 1.min(len - 1),
        (true, true) => 2.min(len - 1),
    }
}

thread_local! {
    static DRAW_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

#[allow(clippy::too_many_arguments)]
fn draw_instance(pm: &mut Pixmap, s: &Shape, inst: &Inst, base: &Matrix, alpha: f32, clip: Option<&Mask>, offset: u32, o: &DrawOpts) {
    if o.skip == Some(s.id) {
        return;
    }
    let Some(sym) = symbol(inst.symbol) else { return };
    if DRAW_DEPTH.with(|d| d.get()) > 24 {
        return;
    }
    DRAW_DEPTH.with(|d| d.set(d.get() + 1));
    let frame = match inst.kind {
        SymbolKind::Button => button_state(&sym, s, base, o.mouse),
        _ => symbol_frame(&sym, inst, offset, o.editor),
    };
    let m = base.mul(&s.m);
    let cx = o.cx.compose(&inst.color.cxform());
    let a = move |_: usize, _: &Shape| alpha;
    let scaled9 = inst.kind == SymbolKind::Movieclip && sym.scale9.is_some() && {
        let d = s.m.decompose();
        (d.sx - 1.0).abs() > 1e-3 || (d.sy - 1.0).abs() > 1e-3
    };
    let paint_into = |target: &mut Pixmap, clip: Option<&Mask>| {
        let sub = DrawOpts { cx, alpha_of: &a, clip, skip: o.skip, ..*o };
        if scaled9 {
            // 9-slice scaling: the already-deformed shapes are drawn.
            let mut plain = inst.clone();
            plain.color = crate::symbols::ColorFx::None;
            for l in crate::symbols::instance_leaves(s, &plain, base, offset, o.editor) {
                draw_item(target, &l, &Matrix::IDENTITY, alpha, clip, 0, &DrawOpts { cx, ..sub });
            }
        } else {
            draw_layers(target, &sym.layers, frame, &m, &sub);
        }
    };
    if !inst.needs_layer() {
        paint_into(pm, clip);
    } else if let Some(mut tmp) = Pixmap::new(pm.width(), pm.height()) {
        paint_into(&mut tmp, None);
        let filters: Vec<&crate::symbols::Filter> = inst.active_filters().collect();
        crate::filters::apply(&mut tmp, &filters, m.avg_scale());
        crate::filters::composite(pm, &tmp, inst.blend, clip);
    }
    DRAW_DEPTH.with(|d| d.set(d.get() - 1));
}

/// Draws the whole stage of a frame (active scene) with matrix `base`.
pub fn draw_frame(pm: &mut Pixmap, doc: &Document, frame: u32, base: &Matrix, alpha: f32, editor: bool) {
    draw_frame_with(pm, doc, frame, base, editor, &|_, _| alpha);
}

/// Like `draw_frame`, with per-shape opacity (to dim what lies outside
/// the group being edited).
pub fn draw_frame_with(pm: &mut Pixmap, doc: &Document, frame: u32, base: &Matrix, editor: bool, alpha_of: &dyn Fn(usize, &Shape) -> f32) {
    let o = DrawOpts::new(editor, doc.export_hidden, alpha_of);
    draw_layers(pm, &doc.layers, frame, base, &o);
}

pub fn rect_path(x: f32, y: f32, w: f32, h: f32) -> Option<tiny_skia::Path> {
    tiny_skia::Rect::from_xywh(x, y, w.max(0.01), h.max(0.01)).map(PathBuilder::from_rect)
}

pub fn fill_rect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, c: Rgba) {
    if let Some(r) = tiny_skia::Rect::from_xywh(x, y, w.max(0.01), h.max(0.01)) {
        pm.fill_rect(r, &paint(c, 1.0), Transform::identity(), None);
    }
}

pub fn stroke_rect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, c: Rgba, width: f32, dashed: bool) {
    if let Some(p) = rect_path(x, y, w, h) {
        let mut st = Stroke { width, ..Default::default() };
        if dashed {
            st.dash = StrokeDash::new(vec![4.0 * width, 3.0 * width], 0.0);
        }
        pm.stroke_path(&p, &paint(c, 1.0), &st, Transform::identity(), None);
    }
}

pub fn fill_circle(pm: &mut Pixmap, x: f32, y: f32, r: f32, c: Rgba) {
    if let Some(p) = PathBuilder::from_circle(x, y, r) {
        pm.fill_path(&p, &paint(c, 1.0), FillRule::Winding, Transform::identity(), None);
    }
}

pub fn stroke_circle(pm: &mut Pixmap, x: f32, y: f32, r: f32, c: Rgba, w: f32) {
    if let Some(p) = PathBuilder::from_circle(x, y, r) {
        let st = Stroke { width: w, ..Default::default() };
        pm.stroke_path(&p, &paint(c, 1.0), &st, Transform::identity(), None);
    }
}

fn scene_pixmap(doc: &Document, layers: &[Layer], frame: u32, scale: f32, mouse: Option<(Pt, bool)>) -> Option<Pixmap> {
    let w = (doc.width * scale).round().max(1.0) as u32;
    let h = (doc.height * scale).round().max(1.0) as u32;
    let mut pm = Pixmap::new(w, h)?;
    let bg = doc.bg;
    pm.fill(tiny_skia::Color::from_rgba8(bg.r, bg.g, bg.b, 255));
    let o = DrawOpts { mouse, ..DrawOpts::new(false, doc.export_hidden, &|_, _| 1.0) };
    draw_layers(&mut pm, layers, frame, &Matrix::scale(scale, scale), &o);
    Some(pm)
}

/// PNG of the stage at the given frame of the active scene.
pub fn stage_pixmap(doc: &Document, frame: u32, scale: f32) -> Option<Pixmap> {
    scene_pixmap(doc, &doc.layers, frame, scale, None)
}

/// Global movie frame (all scenes in sequence), as in "Test Movie".
/// `mouse` in output pixels: buttons respond.
pub fn movie_pixmap(doc: &Document, global: u32, scale: f32, mouse: Option<(Pt, bool)>) -> Option<Pixmap> {
    let (s, f) = doc.movie_frame(global);
    scene_pixmap(doc, doc.scene_layers(s), f, scale, mouse)
}

/// Thumbnail of a layer at a frame (timeline preview).
pub fn layer_thumb(doc: &Document, li: usize, frame: u32, w: u32, h: u32) -> Option<Pixmap> {
    let mut pm = Pixmap::new(w.max(1), h.max(1))?;
    pm.fill(tiny_skia::Color::from_rgba8(255, 255, 255, 255));
    let s = (w as f32 / doc.width).min(h as f32 / doc.height);
    let base = Matrix::translate((w as f32 - doc.width * s) / 2.0, (h as f32 - doc.height * s) / 2.0).mul(&Matrix::scale(s, s));
    for sh in doc.frame_shapes(li, frame) {
        draw_shape(&mut pm, &sh, &base, 1.0);
    }
    Some(pm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_clips_masked_layer_on_export() {
        let mut doc = Document::new(100.0, 100.0);
        let mut mask = Layer::new("máscara");
        mask.uid = 50;
        mask.kind = LayerKind::Mask;
        let mut hole = Shape::new(Geometry::Rect { w: 20.0, h: 20.0, r: 0.0 }, Some(Rgba::BLACK), None, 1.0, Matrix::translate(50.0, 50.0));
        hole.id = 2;
        mask.keys.get_mut(&0).unwrap().shapes.push(hole);
        let mut big = Shape::new(Geometry::Rect { w: 100.0, h: 100.0, r: 0.0 }, Some(Rgba { r: 255, g: 0, b: 0, a: 255 }), None, 1.0, Matrix::translate(50.0, 50.0));
        big.id = 1;
        doc.layers[0].keys.get_mut(&0).unwrap().shapes.push(big);
        doc.layers[0].parent = Some(50);
        doc.layers.insert(0, mask);
        let pm = stage_pixmap(&doc, 0, 1.0).unwrap();
        let px = |x: u32, y: u32| pm.pixel(x, y).unwrap();
        assert_eq!((px(50, 50).red(), px(50, 50).green()), (255, 0), "dentro de la máscara se ve el rojo");
        assert_eq!(px(10, 10).green(), 255, "fuera de la máscara, el fondo blanco");
    }
}
