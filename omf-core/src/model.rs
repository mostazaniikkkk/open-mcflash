//! Document model: shapes, keyframes, layers and the timeline
//! logic (insert frame, F6, F7, motion tween…).

use crate::geom::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Rgba = Rgba { r: 255, g: 255, b: 255, a: 255 };

    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    pub fn parse(hex: &str, alpha: u8) -> Option<Rgba> {
        let h = hex.trim().trim_start_matches('#');
        let h = if h.len() == 3 { h.chars().flat_map(|c| [c, c]).collect::<String>() } else { h.to_string() };
        if h.len() != 6 {
            return None;
        }
        let v = u32::from_str_radix(&h, 16).ok()?;
        Some(Rgba { r: (v >> 16) as u8, g: (v >> 8) as u8, b: v as u8, a: alpha })
    }

    pub fn lerp(&self, o: &Rgba, t: f32) -> Rgba {
        let l = |a: u8, b: u8| lerp(a as f32, b as f32, t).round().clamp(0.0, 255.0) as u8;
        Rgba { r: l(self.r, o.r), g: l(self.g, o.g), b: l(self.b, o.b), a: l(self.a, o.a) }
    }
}

/// Local geometry of a shape, centered on the origin so that rotating and
/// scaling act around its center.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Geometry {
    Rect { w: f32, h: f32, r: f32 },
    Ellipse { w: f32, h: f32 },
    Line { dx: f32, dy: f32 },
    // Freehand stroke (Pencil). `smooth` = midpoint curves; otherwise, straight lines.
    Path {
        pts: Vec<Pt>,
        closed: bool,
        #[serde(default = "yes")]
        smooth: bool,
    },
    // Cubic Bézier paths (Pen, and the result of Brush/Eraser/Paint Bucket).
    // Multiple closed subpaths are filled with the even-odd rule (holes).
    Bezier { paths: Vec<BezPath> },
    // Group (Ctrl+G) or drawing object: a container for other shapes.
    Group {
        children: Vec<Shape>,
        #[serde(default)]
        object: bool,
    },
    // Instance of a Library symbol.
    Instance(crate::symbols::Inst),
    // Text field (origin at the top-left corner of the box).
    Text(crate::text::TextField),
}

fn yes() -> bool {
    true
}

// Anchor point with its incoming (`i`) and outgoing (`o`) handles, in
// local coordinates. A handle equal to `p` is "retracted".
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub p: Pt,
    pub i: Pt,
    pub o: Pt,
    #[serde(default)]
    pub smooth: bool,
}

impl Node {
    pub fn corner(p: Pt) -> Node {
        Node { p, i: p, o: p, smooth: false }
    }
    pub fn has_in(&self) -> bool {
        dist(self.i, self.p) > 1e-3
    }
    pub fn has_out(&self) -> bool {
        dist(self.o, self.p) > 1e-3
    }
    pub fn translate(&mut self, d: Pt) {
        for q in [&mut self.p, &mut self.i, &mut self.o] {
            q.0 += d.0;
            q.1 += d.1;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BezPath {
    pub nodes: Vec<Node>,
    pub closed: bool,
}

impl BezPath {
    // Indices (a, b) of the nodes of each segment.
    pub fn segments(&self) -> Vec<(usize, usize)> {
        let n = self.nodes.len();
        let mut v: Vec<(usize, usize)> = (1..n).map(|k| (k - 1, k)).collect();
        if self.closed && n > 2 {
            v.push((n - 1, 0));
        }
        v
    }
}

pub fn dist(a: Pt, b: Pt) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

pub fn bez_segment(a: &Node, b: &Node) -> Seg {
    if !a.has_out() && !b.has_in() {
        Seg::Line(b.p.0, b.p.1)
    } else {
        Seg::Cubic(a.o.0, a.o.1, b.i.0, b.i.1, b.p.0, b.p.1)
    }
}

// Converts an outline of lines/quadratics/cubics into Bézier nodes.
pub fn segs_to_bez(segs: &[Seg]) -> Vec<BezPath> {
    let mut paths: Vec<BezPath> = Vec::new();
    let mut cur: Option<BezPath> = None;
    let finish = |cur: &mut Option<BezPath>, paths: &mut Vec<BezPath>| {
        if let Some(mut p) = cur.take() {
            if p.closed && p.nodes.len() > 1 && dist(p.nodes[0].p, p.nodes.last().unwrap().p) < 1e-3 {
                let last = p.nodes.pop().unwrap();
                p.nodes[0].i = last.i;
            }
            for n in &mut p.nodes {
                if n.has_in() && n.has_out() {
                    let (a, b) = ((n.p.0 - n.i.0, n.p.1 - n.i.1), (n.o.0 - n.p.0, n.o.1 - n.p.1));
                    let cross = a.0 * b.1 - a.1 * b.0;
                    let dot = a.0 * b.0 + a.1 * b.1;
                    n.smooth = dot > 0.0 && cross.abs() < 0.02 * (a.0.hypot(a.1) * b.0.hypot(b.1));
                }
            }
            if !p.nodes.is_empty() {
                paths.push(p);
            }
        }
    };
    for s in segs {
        match *s {
            Seg::Move(x, y) => {
                finish(&mut cur, &mut paths);
                cur = Some(BezPath { nodes: vec![Node::corner((x, y))], closed: false });
            }
            Seg::Line(x, y) => {
                if let Some(p) = &mut cur {
                    p.nodes.push(Node::corner((x, y)));
                }
            }
            Seg::Quad(cx, cy, x, y) => {
                if let Some(p) = &mut cur {
                    let prev = p.nodes.last_mut().unwrap();
                    let a = prev.p;
                    prev.o = (a.0 + 2.0 / 3.0 * (cx - a.0), a.1 + 2.0 / 3.0 * (cy - a.1));
                    let mut n = Node::corner((x, y));
                    n.i = (x + 2.0 / 3.0 * (cx - x), y + 2.0 / 3.0 * (cy - y));
                    p.nodes.push(n);
                }
            }
            Seg::Cubic(ax, ay, bx, by, x, y) => {
                if let Some(p) = &mut cur {
                    p.nodes.last_mut().unwrap().o = (ax, ay);
                    let mut n = Node::corner((x, y));
                    n.i = (bx, by);
                    p.nodes.push(n);
                }
            }
            Seg::Close => {
                if let Some(p) = &mut cur {
                    p.closed = true;
                }
                finish(&mut cur, &mut paths);
            }
        }
    }
    finish(&mut cur, &mut paths);
    paths
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub id: u32,
    pub geom: Geometry,
    pub fill: Option<Rgba>,
    pub stroke: Option<Rgba>,
    pub stroke_width: f32,
    pub m: Matrix,
    // "Raw" shape from Flash's merge drawing model: fills and strokes
    // are separate pieces that merge with and cut each other.
    #[serde(default, skip_serializing_if = "is_false")]
    pub raw: bool,
    // Group editing context it belongs to (while a group is being edited).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<u32>,
    /// Gradient fill (in local coordinates). `fill` then holds its
    /// first color, for everything that treats the fill as a color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grad: Option<crate::grad::Gradient>,
    /// Bitmap fill (in local coordinates).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bmp: Option<crate::bitmap::BitmapFill>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Default for Shape {
    fn default() -> Self {
        Shape {
            id: 0,
            geom: Geometry::Bezier { paths: Vec::new() },
            fill: None,
            stroke: None,
            stroke_width: 1.0,
            m: Matrix::IDENTITY,
            raw: false,
            tag: None,
            grad: None,
            bmp: None,
        }
    }
}

impl Shape {
    pub fn new(geom: Geometry, fill: Option<Rgba>, stroke: Option<Rgba>, stroke_width: f32, m: Matrix) -> Shape {
        Shape { geom, fill, stroke, stroke_width, m, ..Default::default() }
    }

    /// Fill gradient in parent coordinates.
    pub fn grad_world(&self) -> Option<crate::grad::Gradient> {
        self.grad.as_ref().filter(|_| self.fill.is_some()).map(|g| g.transformed(&self.m))
    }

    /// Sets a gradient given in parent coordinates (or removes it).
    pub fn set_grad_world(&mut self, g: Option<crate::grad::Gradient>) {
        self.grad = g.and_then(|g| self.m.invert().map(|inv| g.transformed(&inv)));
        if let Some(g) = &self.grad {
            self.fill = Some(g.base_color());
            self.bmp = None;
        }
    }

    /// Bitmap fill in parent coordinates.
    pub fn bmp_world(&self) -> Option<crate::bitmap::BitmapFill> {
        self.bmp.as_ref().filter(|_| self.fill.is_some()).map(|b| b.transformed(&self.m))
    }

    pub fn set_bmp_world(&mut self, b: Option<crate::bitmap::BitmapFill>) {
        self.bmp = b.and_then(|b| self.m.invert().map(|inv| b.transformed(&inv)));
        if self.bmp.is_some() {
            self.grad = None;
            self.fill = Some(self.fill.unwrap_or(Rgba { r: 128, g: 128, b: 128, a: 255 }));
        }
    }

    /// Same fill style (color, or the same gradient in the same place).
    pub fn same_fill(&self, o: &Shape) -> bool {
        self.fill == o.fill
            && match (self.grad_world(), o.grad_world()) {
                (None, None) => true,
                (Some(a), Some(b)) => a.approx_eq(&b),
                _ => false,
            }
            && match (self.bmp_world(), o.bmp_world()) {
                (None, None) => true,
                (Some(a), Some(b)) => a.approx_eq(&b),
                _ => false,
            }
    }

    /// Applies a color effect to the shape's colors (including its gradient).
    pub fn apply_cx(&mut self, cx: &crate::symbols::CxForm) {
        if cx.is_identity() {
            return;
        }
        self.fill = self.fill.map(|c| cx.apply(c));
        self.stroke = self.stroke.map(|c| cx.apply(c));
        self.grad = self.grad.as_ref().map(|g| g.with_cx(cx));
        if let Geometry::Text(tf) = &mut self.geom {
            for r in tf.runs.iter_mut() {
                r.color = r.color.map(|c| cx.apply(c));
            }
        }
        // The bitmap is tinted too (applied on top of any effects it already had).
        if let Some(b) = &mut self.bmp {
            b.cx = Some(cx.compose(&b.cx.unwrap_or(crate::symbols::CxForm::ID)));
        }
    }

    /// Copies the fill style of another shape (the gradient stays where it was).
    pub fn take_fill_of(&mut self, src: &Shape) {
        self.fill = src.fill;
        self.set_grad_world(src.grad_world());
        self.set_bmp_world(src.bmp_world());
    }

    pub fn is_group(&self) -> bool {
        matches!(self.geom, Geometry::Group { .. })
    }

    pub fn is_instance(&self) -> bool {
        matches!(self.geom, Geometry::Instance(_))
    }

    /// Groups and instances: containers with no geometry of their own.
    pub fn is_container(&self) -> bool {
        self.is_group() || self.is_instance()
    }

    pub fn inst(&self) -> Option<&crate::symbols::Inst> {
        match &self.geom {
            Geometry::Instance(i) => Some(i),
            _ => None,
        }
    }

    pub fn is_closed(&self) -> bool {
        match &self.geom {
            Geometry::Rect { .. } | Geometry::Ellipse { .. } | Geometry::Text(_) => true,
            Geometry::Line { .. } | Geometry::Group { .. } | Geometry::Instance(_) => false,
            Geometry::Path { closed, .. } => *closed,
            Geometry::Bezier { paths } => paths.iter().any(|p| p.closed),
        }
    }

    // Bézier nodes equivalent to the current geometry (without modifying it).
    pub fn bez_paths(&self) -> Vec<BezPath> {
        match &self.geom {
            Geometry::Bezier { paths } => paths.clone(),
            Geometry::Group { .. } | Geometry::Instance(_) | Geometry::Text(_) => Vec::new(),
            _ => segs_to_bez(&self.outline()),
        }
    }

    // Converts the shape into editable Bézier paths (like Photoshop's Direct
    // Selection on a shape).
    pub fn ensure_bezier(&mut self) {
        if !matches!(self.geom, Geometry::Bezier { .. } | Geometry::Group { .. } | Geometry::Instance(_) | Geometry::Text(_)) {
            self.geom = Geometry::Bezier { paths: self.bez_paths() };
        }
    }

    // Final shapes with the accumulated matrix (groups are expanded).
    pub fn leaves(&self, parent: &Matrix) -> Vec<Shape> {
        let m = parent.mul(&self.m);
        match &self.geom {
            Geometry::Group { children, .. } => children.iter().flat_map(|c| c.leaves(&m)).collect(),
            Geometry::Instance(inst) => crate::symbols::instance_leaves(self, inst, parent, 0, true),
            _ => {
                let mut s = self.clone();
                s.m = m;
                vec![s]
            }
        }
    }

    pub fn kind_name(&self) -> &'static str {
        if self.raw {
            return "Forma";
        }
        match &self.geom {
            Geometry::Instance(i) => i.kind.label(),
            Geometry::Text(t) => match t.kind {
                crate::text::TextKind::Static => "Texto estático",
                crate::text::TextKind::Dynamic => "Texto dinámico",
                crate::text::TextKind::Input => "Texto de entrada",
            },
            Geometry::Group { object: true, .. } => "Objeto de dibujo",
            Geometry::Group { .. } => "Grupo",
            Geometry::Rect { .. } => "Rectángulo",
            Geometry::Ellipse { .. } => "Óvalo",
            Geometry::Line { .. } => "Línea",
            Geometry::Path { .. } => "Trazo",
            Geometry::Bezier { .. } => "Trazado",
        }
    }

    /// Outline in local coordinates (lines and quadratics only).
    pub fn outline(&self) -> Vec<Seg> {
        let mut v = Vec::new();
        match &self.geom {
            Geometry::Group { .. } | Geometry::Instance(_) => {}
            Geometry::Text(t) => v = t.laid().outline.clone(),
            Geometry::Rect { w, h, r } => {
                let (x0, y0, x1, y1) = (-w / 2.0, -h / 2.0, w / 2.0, h / 2.0);
                let r = r.min(w.abs() / 2.0).min(h.abs() / 2.0).max(0.0);
                if r <= 0.01 {
                    v.extend([Seg::Move(x0, y0), Seg::Line(x1, y0), Seg::Line(x1, y1), Seg::Line(x0, y1), Seg::Close]);
                } else {
                    // Two quadratics per corner come quite close to an arc.
                    let k = r * (1.0 - std::f32::consts::FRAC_1_SQRT_2);
                    let q = r * 0.4142;
                    let (rx, ry) = (r, r);
                    v.push(Seg::Move(x0 + rx, y0));
                    v.push(Seg::Line(x1 - rx, y0));
                    v.push(Seg::Quad(x1 - rx + q, y0, x1 - k, y0 + k));
                    v.push(Seg::Quad(x1, y0 + ry - q, x1, y0 + ry));
                    v.push(Seg::Line(x1, y1 - ry));
                    v.push(Seg::Quad(x1, y1 - ry + q, x1 - k, y1 - k));
                    v.push(Seg::Quad(x1 - rx + q, y1, x1 - rx, y1));
                    v.push(Seg::Line(x0 + rx, y1));
                    v.push(Seg::Quad(x0 + rx - q, y1, x0 + k, y1 - k));
                    v.push(Seg::Quad(x0, y1 - ry + q, x0, y1 - ry));
                    v.push(Seg::Line(x0, y0 + ry));
                    v.push(Seg::Quad(x0, y0 + ry - q, x0 + k, y0 + k));
                    v.push(Seg::Quad(x0 + rx - q, y0, x0 + rx, y0));
                    v.push(Seg::Close);
                }
            }
            Geometry::Ellipse { w, h } => {
                // 8 quadratics, as Flash itself did when exporting.
                let (rx, ry) = (w / 2.0, h / 2.0);
                let n = 8;
                let step = std::f32::consts::TAU / n as f32;
                let cr = 1.0 / (step / 2.0).cos();
                v.push(Seg::Move(rx, 0.0));
                for i in 0..n {
                    let a1 = (i + 1) as f32 * step;
                    let am = (i as f32 + 0.5) * step;
                    v.push(Seg::Quad(rx * cr * am.cos(), ry * cr * am.sin(), rx * a1.cos(), ry * a1.sin()));
                }
                v.push(Seg::Close);
            }
            Geometry::Line { dx, dy } => {
                v.push(Seg::Move(-dx / 2.0, -dy / 2.0));
                v.push(Seg::Line(dx / 2.0, dy / 2.0));
            }
            Geometry::Bezier { paths } => {
                for p in paths {
                    if p.nodes.is_empty() {
                        continue;
                    }
                    v.push(Seg::Move(p.nodes[0].p.0, p.nodes[0].p.1));
                    for (a, b) in p.segments() {
                        v.push(bez_segment(&p.nodes[a], &p.nodes[b]));
                    }
                    if p.closed {
                        v.push(Seg::Close);
                    }
                }
            }
            Geometry::Path { pts, closed, smooth } => {
                if pts.is_empty() {
                    return v;
                }
                v.push(Seg::Move(pts[0].0, pts[0].1));
                if pts.len() < 3 || !*smooth {
                    for p in &pts[1..] {
                        v.push(Seg::Line(p.0, p.1));
                    }
                } else {
                    // Midpoint smoothing: each vertex is the control point of a quadratic.
                    for i in 1..pts.len() - 1 {
                        let (p, n) = (pts[i], pts[i + 1]);
                        v.push(Seg::Quad(p.0, p.1, (p.0 + n.0) / 2.0, (p.1 + n.1) / 2.0));
                    }
                    let l = pts[pts.len() - 1];
                    v.push(Seg::Line(l.0, l.1));
                }
                if *closed {
                    v.push(Seg::Close);
                }
            }
        }
        v
    }

    pub fn local_bounds(&self) -> BBox {
        let mut b = BBox::EMPTY;
        match &self.geom {
            Geometry::Rect { w, h, .. } | Geometry::Ellipse { w, h } => {
                b.add((-w / 2.0, -h / 2.0));
                b.add((w / 2.0, h / 2.0));
            }
            Geometry::Line { dx, dy } => {
                b.add((-dx / 2.0, -dy / 2.0));
                b.add((dx / 2.0, dy / 2.0));
            }
            Geometry::Text(t) => b = t.bounds(),
            Geometry::Path { .. } | Geometry::Bezier { .. } => {
                for (pts, _) in flatten(&self.outline()) {
                    pts.iter().for_each(|p| b.add(*p));
                }
            }
            Geometry::Group { children, .. } => {
                for c in children {
                    b = b.union(&c.world_bounds());
                }
            }
            Geometry::Instance(_) => {
                let mut me = self.clone();
                me.m = Matrix::IDENTITY;
                for l in me.leaves(&Matrix::IDENTITY) {
                    b = b.union(&l.world_bounds());
                }
            }
        }
        b
    }

    pub fn world_bounds(&self) -> BBox {
        let mut b = BBox::EMPTY;
        if self.is_container() {
            for l in self.leaves(&Matrix::IDENTITY) {
                b = b.union(&l.world_bounds());
            }
            if b.is_empty() {
                b.add(self.m.apply(0.0, 0.0));
            }
            return b;
        }
        if let Geometry::Text(t) = &self.geom {
            let l = t.bounds();
            for p in [(l.x0, l.y0), (l.x1, l.y0), (l.x1, l.y1), (l.x0, l.y1)] {
                b.add(self.m.apply(p.0, p.1));
            }
            return b;
        }
        for (pts, _) in flatten(&self.outline()) {
            for p in pts {
                b.add(self.m.apply(p.0, p.1));
            }
        }
        if b.is_empty() {
            let c = self.m.apply(0.0, 0.0);
            b.add(c);
        }
        b
    }

    /// `tol` in document units.
    pub fn hit(&self, wx: f32, wy: f32, tol: f32) -> bool {
        if self.is_container() {
            return self.leaves(&Matrix::IDENTITY).iter().any(|l| l.hit(wx, wy, tol));
        }
        let Some(inv) = self.m.invert() else { return false };
        let p = inv.apply(wx, wy);
        // A text field is picked by its whole box, as in Flash.
        if let Geometry::Text(t) = &self.geom {
            let b = t.bounds();
            let tl = tol / self.m.avg_scale();
            return p.0 >= b.x0 - tl && p.0 <= b.x1 + tl && p.1 >= b.y0 - tl && p.1 <= b.y1 + tl;
        }
        let s = self.m.avg_scale();
        let half = if self.stroke.is_some() { self.stroke_width / 2.0 } else { 0.0 };
        let ltol = tol / s + half;
        let mut inside = false;
        for (pts, closed) in flatten(&self.outline()) {
            if closed && self.fill.is_some() && point_in_polygon(p, &pts) {
                inside = !inside;
            }
            if (self.stroke.is_some() || self.fill.is_none() || !closed) && dist_to_polyline(p, &pts, closed) <= ltol {
                return true;
            }
        }
        inside
    }

    // Is the (world) point inside the closed outline, whether or not it has a fill?
    pub fn encloses(&self, wx: f32, wy: f32) -> bool {
        let Some(inv) = self.m.invert() else { return false };
        let p = inv.apply(wx, wy);
        if let Geometry::Text(t) = &self.geom {
            let b = t.bounds();
            return p.0 >= b.x0 && p.0 <= b.x1 && p.1 >= b.y0 && p.1 <= b.y1;
        }
        flatten(&self.outline()).iter().filter(|(pts, c)| *c && point_in_polygon(p, pts)).count() % 2 == 1
    }

    /// Motion tween with the keyframe's options.
    pub fn lerp_motion(&self, o: &Shape, t: f32, kf: &Keyframe) -> Shape {
        let mut s = self.lerp(o, t);
        let rot = match kf.rotate {
            RotateMode::Auto => 0,
            RotateMode::None => 1,
            RotateMode::Cw => 2,
            RotateMode::Ccw => 3,
        };
        s.m = self.m.lerp_opts(&o.m, t, rot, kf.spins, kf.scale);
        s
    }

    pub fn lerp(&self, o: &Shape, t: f32) -> Shape {
        let lp =|a: Pt, b: Pt| (lerp(a.0, b.0, t), lerp(a.1, b.1, t));
        let geom = match (&self.geom, &o.geom) {
            (Geometry::Rect { w, h, r }, Geometry::Rect { w: w2, h: h2, r: r2 }) => {
                Geometry::Rect { w: lerp(*w, *w2, t), h: lerp(*h, *h2, t), r: lerp(*r, *r2, t) }
            }
            (Geometry::Ellipse { w, h }, Geometry::Ellipse { w: w2, h: h2 }) => {
                Geometry::Ellipse { w: lerp(*w, *w2, t), h: lerp(*h, *h2, t) }
            }
            (Geometry::Line { dx, dy }, Geometry::Line { dx: dx2, dy: dy2 }) => {
                Geometry::Line { dx: lerp(*dx, *dx2, t), dy: lerp(*dy, *dy2, t) }
            }
            (Geometry::Path { pts, closed, smooth }, Geometry::Path { pts: p2, .. }) if pts.len() == p2.len() => Geometry::Path {
                pts: pts.iter().zip(p2).map(|(a, b)| lp(*a, *b)).collect(),
                closed: *closed,
                smooth: *smooth,
            },
            (Geometry::Bezier { paths }, Geometry::Bezier { paths: p2 })
                if paths.len() == p2.len() && paths.iter().zip(p2).all(|(a, b)| a.nodes.len() == b.nodes.len()) =>
            {
                Geometry::Bezier {
                    paths: paths
                        .iter()
                        .zip(p2)
                        .map(|(a, b)| BezPath {
                            closed: a.closed,
                            nodes: a
                                .nodes
                                .iter()
                                .zip(&b.nodes)
                                .map(|(x, y)| Node { p: lp(x.p, y.p), i: lp(x.i, y.i), o: lp(x.o, y.o), smooth: x.smooth })
                                .collect(),
                        })
                        .collect(),
                }
            }
            (Geometry::Instance(a), Geometry::Instance(b)) if a.symbol == b.symbol => Geometry::Instance(a.lerp(b, t)),
            (Geometry::Group { children, object }, Geometry::Group { children: c2, .. }) if children.len() == c2.len() => {
                Geometry::Group { children: children.iter().zip(c2).map(|(a, b)| a.lerp(b, t)).collect(), object: *object }
            }
            _ => self.geom.clone(),
        };
        let lc = |a: &Option<Rgba>, b: &Option<Rgba>| match (a, b) {
            (Some(x), Some(y)) => Some(x.lerp(y, t)),
            _ => *a,
        };
        let grad = match (&self.grad, &o.grad) {
            (Some(a), Some(b)) => Some(a.lerp(b, t)),
            (g, _) => g.clone(),
        };
        Shape {
            id: self.id,
            geom,
            fill: lc(&self.fill, &o.fill),
            stroke: lc(&self.stroke, &o.stroke),
            stroke_width: lerp(self.stroke_width, o.stroke_width, t),
            m: self.m.lerp(&o.m, t),
            raw: self.raw,
            tag: self.tag,
            grad,
            bmp: self.bmp.clone(),
        }
    }
}

/// Rotation of a motion tween (Flash Property inspector).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RotateMode {
    #[default]
    Auto,
    None,
    Cw,
    Ccw,
}

/// Frame label type: name, comment (//) or anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LabelKind {
    #[default]
    Name,
    Comment,
    Anchor,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub shapes: Vec<Shape>,
    /// Motion tween up to the next keyframe.
    #[serde(default)]
    pub tween: bool,
    /// Shape tween up to the next keyframe.
    #[serde(default)]
    pub shape_tween: bool,
    /// Flash-style easing: -100 (in) … 100 (out).
    #[serde(default)]
    pub ease: f32,
    /// Custom easing: Bézier curve (x1, y1, x2, y2) as in "Edit…".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ease_curve: Option<[f32; 4]>,
    #[serde(default)]
    pub rotate: RotateMode,
    /// Full turns with CW/CCW rotation.
    #[serde(default)]
    pub spins: u32,
    /// "Scale": when off, the tween does not change the size.
    #[serde(default = "yes")]
    pub scale: bool,
    /// "Orient to path" (with a motion guide).
    #[serde(default)]
    pub orient: bool,
    /// "Snap" to the motion guide.
    #[serde(default)]
    pub snap: bool,
    /// "Sync": graphics continue their animation across the tween's
    /// keyframes instead of restarting at each one.
    #[serde(default, skip_serializing_if = "is_false")]
    pub sync: bool,
    /// Shape hints (a, b, c…) in document coordinates.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<Pt>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default)]
    pub label_kind: LabelKind,
    /// Frame actions (ActionScript 2).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub actions: String,
    /// Keyframe sound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<crate::sound::FrameSound>,
}

impl Default for Keyframe {
    fn default() -> Self {
        Keyframe {
            shapes: Vec::new(),
            tween: false,
            shape_tween: false,
            ease: 0.0,
            ease_curve: None,
            rotate: RotateMode::Auto,
            spins: 0,
            scale: true,
            orient: false,
            snap: false,
            sync: false,
            hints: Vec::new(),
            label: String::new(),
            label_kind: LabelKind::Name,
            actions: String::new(),
            sound: None,
        }
    }
}

impl Keyframe {
    /// Copies the tween settings (not the shapes, the label or the hints).
    pub fn with_settings_of(shapes: Vec<Shape>, o: &Keyframe) -> Keyframe {
        Keyframe {
            shapes,
            tween: o.tween,
            shape_tween: o.shape_tween,
            ease: o.ease,
            ease_curve: o.ease_curve,
            rotate: o.rotate,
            spins: o.spins,
            scale: o.scale,
            orient: o.orient,
            snap: o.snap,
            sync: o.sync,
            ..Default::default()
        }
    }

    pub fn tweens(&self) -> bool {
        self.tween || self.shape_tween
    }

    /// Progress with easing applied.
    pub fn eased(&self, t: f32) -> f32 {
        if let Some(c) = self.ease_curve {
            return bezier_ease(c, t);
        }
        let e = (self.ease / 100.0).clamp(-1.0, 1.0);
        t + e * t * (1.0 - t)
    }
}

/// CSS-style easing curve: (0,0) → (x1,y1) → (x2,y2) → (1,1).
pub fn bezier_ease(c: [f32; 4], t: f32) -> f32 {
    let (x1, y1, x2, y2) = (c[0].clamp(0.0, 1.0), c[1], c[2].clamp(0.0, 1.0), c[3]);
    let bx = |u: f32| 3.0 * (1.0 - u).powi(2) * u * x1 + 3.0 * (1.0 - u) * u * u * x2 + u * u * u;
    let by = |u: f32| 3.0 * (1.0 - u).powi(2) * u * y1 + 3.0 * (1.0 - u) * u * u * y2 + u * u * u;
    // Bisection: x(u) is monotonic on [0,1].
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..30 {
        let mid = (lo + hi) / 2.0;
        if bx(mid) < t {
            lo = mid
        } else {
            hi = mid
        }
    }
    by((lo + hi) / 2.0)
}

/// Layer type (Flash Layer Properties).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LayerKind {
    #[default]
    Normal,
    /// Guide layer: visible while editing but not exported.
    Guide,
    /// Motion guide: guided layers follow its stroke.
    MotionGuide,
    /// Mask: its fills clip the masked layers.
    Mask,
    Folder,
}

fn one() -> u8 {
    1
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub keys: BTreeMap<u32, Keyframe>,
    /// Number of frames in the layer.
    pub length: u32,
    /// View as outlines (the colored square in the timeline).
    #[serde(default)]
    pub outline: bool,
    #[serde(default = "default_layer_color")]
    pub color: Rgba,
    /// Stable identifier (for parents of masks, guides and folders).
    #[serde(default)]
    pub uid: u32,
    #[serde(default)]
    pub kind: LayerKind,
    /// Mask layer, motion guide or folder it hangs from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<u32>,
    /// Expanded folder.
    #[serde(default = "yes")]
    pub open: bool,
    /// Row height: 1, 2 or 3 (100%, 200%, 300%).
    #[serde(default = "one")]
    pub height: u8,
}

/// Stored scene; the active one lives in `Document::layers`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneStore {
    pub name: String,
    #[serde(default)]
    pub layers: Vec<Layer>,
}

pub const LAYER_COLORS: [Rgba; 8] = [
    Rgba { r: 0x2e, g: 0xc8, b: 0x2e, a: 255 },
    Rgba { r: 0x40, g: 0x80, b: 0xff, a: 255 },
    Rgba { r: 0xff, g: 0x40, b: 0xff, a: 255 },
    Rgba { r: 0xff, g: 0x99, b: 0x00, a: 255 },
    Rgba { r: 0x00, g: 0xcc, b: 0xcc, a: 255 },
    Rgba { r: 0xff, g: 0x40, b: 0x40, a: 255 },
    Rgba { r: 0x99, g: 0x66, b: 0xff, a: 255 },
    Rgba { r: 0xcc, g: 0xcc, b: 0x00, a: 255 },
];

fn default_layer_color() -> Rgba {
    LAYER_COLORS[0]
}

impl Layer {
    pub fn new(name: &str) -> Layer {
        let mut keys = BTreeMap::new();
        keys.insert(0, Keyframe::default());
        Layer {
            name: name.to_string(),
            visible: true,
            locked: false,
            keys,
            length: 1,
            outline: false,
            color: LAYER_COLORS[0],
            uid: 0,
            kind: LayerKind::Normal,
            parent: None,
            open: true,
            height: 1,
        }
    }

    pub fn key_at_or_before(&self, f: u32) -> Option<u32> {
        self.keys.range(..=f).next_back().map(|(k, _)| *k)
    }

    /// Frames a graphic's animation has been playing at `f`: since
    /// its keyframe or, with "Sync", since the tween's first keyframe.
    pub fn anim_offset(&self, f: u32) -> u32 {
        let Some(mut start) = self.key_at_or_before(f) else { return 0 };
        while self.keys[&start].sync {
            match self.keys.range(..start).next_back() {
                Some((p, k)) if k.tween && !k.shape_tween && k.sync => start = *p,
                _ => break,
            }
        }
        f.saturating_sub(start)
    }

    pub fn next_key_after(&self, f: u32) -> Option<u32> {
        self.keys.range(f + 1..).next().map(|(k, _)| *k)
    }

    /// End (exclusive) of the span starting at keyframe `k`.
    pub fn span_end(&self, k: u32) -> u32 {
        self.next_key_after(k).unwrap_or(self.length).min(self.length)
    }

    /// If the span of `k` has a valid tween, returns the target keyframe.
    pub fn tween_target(&self, k: u32) -> Option<u32> {
        let kf = self.keys.get(&k)?;
        if !kf.tweens() {
            return None;
        }
        self.next_key_after(k).filter(|n| *n < self.length)
    }

    /// Tween progress at `f`: (start keyframe, end keyframe, eased t).
    pub fn tween_at(&self, f: u32) -> Option<(u32, u32, f32)> {
        if f >= self.length {
            return None;
        }
        let k = self.key_at_or_before(f)?;
        if f <= k {
            return None;
        }
        let n = self.tween_target(k)?;
        let t = (f - k) as f32 / (n - k) as f32;
        Some((k, n, self.keys[&k].eased(t)))
    }

    /// Visible shapes at frame `f` (tweened if applicable).
    pub fn frame_shapes(&self, f: u32) -> Vec<Shape> {
        if f >= self.length {
            return Vec::new();
        }
        let Some(k) = self.key_at_or_before(f) else { return Vec::new() };
        let kf = &self.keys[&k];
        if let Some((_, n, t)) = self.tween_at(f) {
            let nk = &self.keys[&n];
            if kf.shape_tween {
                return crate::morph::shape_tween(&kf.shapes, &nk.shapes, t, &kf.hints, &nk.hints);
            }
            return kf
                .shapes
                .iter()
                .map(|s| match nk.shapes.iter().find(|o| o.id == s.id) {
                    Some(o) => s.lerp_motion(o, t, kf),
                    None => s.clone(),
                })
                .collect();
        }
        kf.shapes.clone()
    }

    pub fn insert_frame(&mut self, f: u32) {
        if f >= self.length {
            self.length = f + 1;
            return;
        }
        self.keys = std::mem::take(&mut self.keys).into_iter().map(|(k, v)| (if k > f { k + 1 } else { k }, v)).collect();
        self.length += 1;
    }

    pub fn remove_frame(&mut self, f: u32) {
        if f >= self.length || self.length <= 1 {
            return;
        }
        let is_key = self.keys.contains_key(&f);
        let span_len = if is_key { self.span_end(f) - f } else { 2 };
        if is_key && span_len <= 1 {
            self.keys.remove(&f);
        }
        self.keys = std::mem::take(&mut self.keys).into_iter().map(|(k, v)| (if k > f { k - 1 } else { k }, v)).collect();
        self.length -= 1;
        if self.keys.is_empty() {
            self.keys.insert(0, Keyframe::default());
        }
    }

    /// F6 (with `blank == false`) or F7.
    pub fn insert_keyframe(&mut self, f: u32, blank: bool) {
        if self.keys.contains_key(&f) && f < self.length {
            return;
        }
        let prev = self.key_at_or_before(f);
        let shapes = if blank { Vec::new() } else { self.frame_shapes(f.min(self.length.saturating_sub(1))) };
        let mut kf = match prev {
            Some(k) => Keyframe::with_settings_of(shapes, &self.keys[&k]),
            None => Keyframe { shapes, ..Default::default() },
        };
        if blank {
            kf.tween = false;
            kf.shape_tween = false;
        }
        self.length = self.length.max(f + 1);
        self.keys.insert(f, kf);
    }

    pub fn clear_keyframe(&mut self, f: u32) {
        if f == 0 {
            if let Some(k) = self.keys.get_mut(&0) {
                k.shapes.clear();
            }
        } else {
            self.keys.remove(&f);
        }
    }

    /// Returns the keyframe to modify in order to edit `f`,
    /// creating it if needed (just like Flash inside a tween).
    pub fn ensure_key(&mut self, f: u32) -> u32 {
        if f >= self.length {
            self.length = f + 1;
            self.keys.insert(f, Keyframe::default());
            return f;
        }
        if self.keys.contains_key(&f) {
            return f;
        }
        match self.key_at_or_before(f) {
            None => {
                self.keys.insert(f, Keyframe::default());
                f
            }
            Some(k) => {
                if self.tween_target(k).is_some() {
                    self.insert_keyframe(f, false);
                    f
                } else {
                    k
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub width: f32,
    pub height: f32,
    pub bg: Rgba,
    pub fps: f32,
    /// `layers[0]` is the top layer, as in the Timeline panel.
    pub layers: Vec<Layer>,
    pub next_id: u32,
    #[serde(default)]
    pub layer_counter: u32,
    // Flash 8 exported hidden layers too.
    #[serde(default = "yes")]
    pub export_hidden: bool,
    // Stack of groups open for editing (double-click). It lives in the document
    // so that undo/redo restores it consistently.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<Scope>,
    /// Counter for layer identifiers.
    #[serde(default)]
    pub layer_uid: u32,
    /// The movie's scenes. The active one (`scene`) only keeps its name here:
    /// its layers are `layers`.
    #[serde(default)]
    pub scenes: Vec<SceneStore>,
    #[serde(default)]
    pub scene: usize,
    /// Library: symbols and folders.
    #[serde(default)]
    pub library: Vec<crate::symbols::Symbol>,
    #[serde(default)]
    pub lib_folders: Vec<crate::symbols::LibFolder>,
    /// Stack of symbols being edited (the active timeline is the last one's).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sym_edit: Vec<SymEdit>,
    #[serde(default)]
    pub symbol_counter: u32,
    /// Imported images.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bitmaps: Vec<crate::bitmap::BitmapAsset>,
    /// File > Publish Settings.
    #[serde(default)]
    pub publish: Publish,
    /// Imported sounds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sounds: Vec<crate::sound::SoundAsset>,
    /// Guides (View > Guides) and their options.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<crate::guides::Guide>,
    #[serde(default)]
    pub guide_opts: crate::guides::GuideOpts,
    /// Added fonts (from file or system) used by the text: they travel with the
    /// document so it looks the same on another computer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fonts: Vec<crate::text::FontAsset>,
    /// ActionScript (.as) files: classes and code for #include.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<Script>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Script {
    /// File path (e.g. "juego/Pelota.as").
    pub name: String,
    pub code: String,
}

impl Document {
    /// ActionScript compiler context: scenes, scripts and classes.
    pub fn as_ctx(&self) -> crate::as2::Ctx {
        let mut scenes = Vec::new();
        let mut start = 0;
        for i in 0..self.scenes.len().max(1) {
            scenes.push((self.scenes.get(i).map_or("Escena 1".to_string(), |s| s.name.clone()), start));
            start += self.scene_total(i);
        }
        let scripts: Vec<(String, String)> = self.scripts.iter().map(|s| (s.name.clone(), s.code.clone())).collect();
        let classes = crate::as2::class::class_names(&scripts);
        crate::as2::Ctx { scenes, scripts, classes }
    }
}

/// Publish Settings (Flash 8: Formats, Flash tab and HTML tab).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Publish {
    pub swf: bool,
    pub html: bool,
    pub png: bool,
    /// File name (empty: the document's).
    pub name: String,
    /// Compress movie (zlib-compressed SWF, "CWS").
    pub compress: bool,
    /// Protect from import.
    pub protect: bool,
    /// HTML: "match" (Match Movie), "pixels" or "percent".
    pub size: String,
    pub width: f32,
    pub height: f32,
    pub paused: bool,
    #[serde(rename = "loop")]
    pub looping: bool,
    pub menu: bool,
    /// HTML: load Ruffle to play the SWF in browsers without Flash.
    pub ruffle: bool,
}

impl Default for Publish {
    fn default() -> Self {
        Publish {
            swf: true,
            html: true,
            png: false,
            name: String::new(),
            compress: true,
            protect: false,
            size: "match".into(),
            width: 550.0,
            height: 400.0,
            paused: false,
            looping: true,
            menu: true,
            ruffle: true,
        }
    }
}

/// Symbol editing: its layers move into `Document::layers` and those of
/// whatever contained it are kept here until exiting.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SymEdit {
    pub symbol: u32,
    pub parent_layers: Vec<Layer>,
    /// Edit in place: matrix of the instance (otherwise it is edited separately).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_place: Option<Matrix>,
    /// Instance being edited in place (hidden in the parent's view).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<u32>,
    pub frame: u32,
    pub layer: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    pub tag: u32,
    pub layer: usize,
    pub group_id: u32,
    pub object: bool,
}

/// Keeps Flash's stacking order within a context: raw shapes
/// always below groups and objects (fills, then strokes, then
/// floating pieces), and everything else in its order.
pub fn normalize_order(v: &mut Vec<Shape>, tag: Option<u32>, floating: &[u32]) {
    let key = |s: &Shape| -> u8 {
        if !s.raw {
            return 4;
        }
        let fl = floating.contains(&s.id);
        match (s.fill.is_some(), fl) {
            (true, false) => 0,
            (false, false) => 1,
            (true, true) => 2,
            (false, true) => 3,
        }
    };
    if tag.is_none() {
        let mut raw: Vec<Shape> = v.iter().filter(|s| s.tag.is_none() && s.raw).cloned().collect();
        raw.sort_by_key(|s| key(s));
        let rest: Vec<Shape> = v.drain(..).filter(|s| !(s.tag.is_none() && s.raw)).collect();
        v.extend(raw);
        v.extend(rest);
        return;
    }
    let idx: Vec<usize> = (0..v.len()).filter(|&i| v[i].tag == tag).collect();
    let mut items: Vec<Shape> = idx.iter().map(|&i| v[i].clone()).collect();
    items.sort_by_key(|s| key(s));
    for (slot, s) in idx.into_iter().zip(items) {
        v[slot] = s;
    }
}

impl Document {
    pub fn new(width: f32, height: f32) -> Document {
        let mut d = Document {
            width,
            height,
            bg: Rgba::WHITE,
            fps: 12.0,
            layers: vec![Layer::new("Capa 1")],
            next_id: 1,
            layer_counter: 1,
            export_hidden: true,
            scopes: Vec::new(),
            layer_uid: 0,
            scenes: Vec::new(),
            scene: 0,
            library: Vec::new(),
            lib_folders: Vec::new(),
            sym_edit: Vec::new(),
            symbol_counter: 0,
            bitmaps: Vec::new(),
            publish: Publish::default(),
            guides: Vec::new(),
            sounds: Vec::new(),
            guide_opts: crate::guides::GuideOpts::default(),
            fonts: Vec::new(),
            scripts: Vec::new(),
        };
        d.fix_ids();
        d
    }

    /// Fills in data that may be missing from older files: layer ids and scenes.
    pub fn fix_ids(&mut self) {
        let mut used: Vec<u32> = Vec::new();
        let mut layers: Vec<&mut Layer> = self.layers.iter_mut().collect();
        for s in self.scenes.iter_mut() {
            layers.extend(s.layers.iter_mut());
        }
        for l in layers {
            if l.uid == 0 || used.contains(&l.uid) {
                self.layer_uid += 1;
                while used.contains(&self.layer_uid) {
                    self.layer_uid += 1;
                }
                l.uid = self.layer_uid;
            }
            self.layer_uid = self.layer_uid.max(l.uid);
            used.push(l.uid);
        }
        if self.scenes.is_empty() {
            self.scenes.push(SceneStore { name: "Escena 1".into(), layers: Vec::new() });
        }
        self.scene = self.scene.min(self.scenes.len() - 1);
    }

    pub fn new_layer_uid(&mut self) -> u32 {
        self.layer_uid += 1;
        self.layer_uid
    }

    pub fn total_frames(&self) -> u32 {
        self.layers.iter().map(|l| l.length).max().unwrap_or(1).max(1)
    }

    /// The document with symbol edits closed (the active scene in
    /// `layers`), to export or test the movie while a symbol is being edited.
    pub fn closed(&self) -> std::borrow::Cow<'_, Document> {
        if self.sym_edit.is_empty() {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut d = self.clone();
        while let Some(e) = d.sym_edit.pop() {
            let layers = std::mem::replace(&mut d.layers, e.parent_layers);
            if let Some(s) = d.library.iter_mut().find(|s| s.id == e.symbol) {
                s.layers = layers;
            }
        }
        std::borrow::Cow::Owned(d)
    }

    pub fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn add_layer(&mut self, at: usize) -> usize {
        self.layer_counter += 1;
        let mut l = Layer::new(&format!("Capa {}", self.layer_counter));
        l.color = LAYER_COLORS[(self.layer_counter as usize - 1) % LAYER_COLORS.len()];
        l.uid = self.new_layer_uid();
        let at = at.min(self.layers.len());
        self.layers.insert(at, l);
        at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(id: u32, x: f32) -> Shape {
        Shape {
            id,
            geom: Geometry::Rect { w: 10.0, h: 10.0, r: 0.0 },
            fill: Some(Rgba::BLACK),
            stroke: None,
            stroke_width: 1.0,
            m: Matrix::translate(x, 0.0),
            ..Default::default()
        }
    }

    #[test]
    fn motion_tween_interpolates() {
        let mut l = Layer::new("a");
        l.keys.get_mut(&0).unwrap().shapes.push(rect(1, 0.0));
        l.keys.get_mut(&0).unwrap().tween = true;
        l.insert_keyframe(10, false);
        l.keys.get_mut(&10).unwrap().shapes[0].m.tx = 100.0;
        let s = l.frame_shapes(5);
        assert!((s[0].m.tx - 50.0).abs() < 1e-3);
        assert_eq!(l.length, 11);
    }

    #[test]
    fn insert_and_remove_frames() {
        let mut l = Layer::new("a");
        l.insert_keyframe(4, true);
        l.insert_frame(2);
        assert!(l.keys.contains_key(&5));
        assert_eq!(l.length, 6);
        l.remove_frame(5);
        assert!(!l.keys.contains_key(&5));
        assert_eq!(l.length, 5);
    }

    #[test]
    fn ensure_key_inside_tween_creates_key() {
        let mut l = Layer::new("a");
        l.keys.get_mut(&0).unwrap().shapes.push(rect(1, 0.0));
        l.keys.get_mut(&0).unwrap().tween = true;
        l.insert_keyframe(10, false);
        assert_eq!(l.ensure_key(3), 3);
        assert!(l.keys.contains_key(&3));
    }

    #[test]
    fn hit_test_rect() {
        let s = rect(1, 50.0);
        assert!(s.hit(50.0, 0.0, 0.5));
        assert!(!s.hit(80.0, 0.0, 0.5));
    }
}
