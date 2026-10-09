//! SWF exporter (version 8) following the public format specification:
//! DefineShape3 for shapes, DefineSprite for movie clips, DefineButton2
//! for buttons and PlaceObject2/3 per frame (color effects, instance names,
//! filters, blending and caching). The output can be played with Ruffle
//! or with any Flash Player.

use crate::geom::*;
use crate::model::*;
use crate::grad::{GradKind, Spread};
use crate::symbols::{adjust_color_matrix, symbol, symbol_frame, symbol_shapes, Blend, CxForm, Filter, Symbol, SymbolKind};
use std::collections::HashMap;

struct Bits {
    buf: Vec<u8>,
    cur: u8,
    n: u8,
}

impl Bits {
    fn new() -> Bits {
        Bits { buf: Vec::new(), cur: 0, n: 0 }
    }
    fn ub(&mut self, nbits: u32, v: u32) {
        for i in (0..nbits).rev() {
            self.cur = (self.cur << 1) | ((v >> i) & 1) as u8;
            self.n += 1;
            if self.n == 8 {
                self.buf.push(self.cur);
                self.cur = 0;
                self.n = 0;
            }
        }
    }
    fn sb(&mut self, nbits: u32, v: i32) {
        let mask = if nbits >= 32 { u32::MAX } else { (1u32 << nbits) - 1 };
        self.ub(nbits, (v as u32) & mask);
    }
    fn flush(&mut self) {
        if self.n > 0 {
            self.buf.push(self.cur << (8 - self.n));
            self.cur = 0;
            self.n = 0;
        }
    }
    fn u8(&mut self, v: u8) {
        self.flush();
        self.buf.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.flush();
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn rgba(&mut self, c: Rgba) {
        self.flush();
        self.buf.extend_from_slice(&[c.r, c.g, c.b, c.a]);
    }
}

fn sbits(v: i32) -> u32 {
    if v >= 0 {
        32 - (v as u32).leading_zeros() + 1
    } else {
        32 - ((!v) as u32).leading_zeros() + 1
    }
}

fn write_rect(b: &mut Bits, x0: i32, x1: i32, y0: i32, y1: i32) {
    let n = [x0, x1, y0, y1].iter().map(|v| sbits(*v)).max().unwrap();
    b.ub(5, n);
    for v in [x0, x1, y0, y1] {
        b.sb(n, v);
    }
    b.flush();
}

fn fixed(f: f32) -> i32 {
    (f as f64 * 65536.0).round() as i32
}

fn twips(f: f32) -> i32 {
    (f * 20.0).round() as i32
}

fn write_matrix(b: &mut Bits, m: &Matrix) {
    if m.a != 1.0 || m.d != 1.0 {
        let (sa, sd) = (fixed(m.a), fixed(m.d));
        let n = sbits(sa).max(sbits(sd));
        b.ub(1, 1);
        b.ub(5, n);
        b.sb(n, sa);
        b.sb(n, sd);
    } else {
        b.ub(1, 0);
    }
    if m.b != 0.0 || m.c != 0.0 {
        let (r0, r1) = (fixed(m.b), fixed(m.c));
        let n = sbits(r0).max(sbits(r1));
        b.ub(1, 1);
        b.ub(5, n);
        b.sb(n, r0);
        b.sb(n, r1);
    } else {
        b.ub(1, 0);
    }
    let (tx, ty) = (twips(m.tx), twips(m.ty));
    let n = if tx == 0 && ty == 0 { 0 } else { sbits(tx).max(sbits(ty)) };
    b.ub(5, n);
    b.sb(n, tx);
    b.sb(n, ty);
    b.flush();
}

fn tag(out: &mut Vec<u8>, code: u16, body: &[u8]) {
    if body.len() < 0x3f {
        out.extend_from_slice(&((code << 6) | body.len() as u16).to_le_bytes());
    } else {
        out.extend_from_slice(&((code << 6) | 0x3f).to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    }
    out.extend_from_slice(body);
}

struct EdgeWriter<'a> {
    b: &'a mut Bits,
    cur: (i32, i32),
}

impl EdgeWriter<'_> {
    fn line_to(&mut self, x: i32, y: i32) {
        let (dx, dy) = (x - self.cur.0, y - self.cur.1);
        if dx == 0 && dy == 0 {
            return;
        }
        let n = sbits(dx).max(sbits(dy)).max(2);
        if n > 17 {
            let mid = (self.cur.0 + dx / 2, self.cur.1 + dy / 2);
            self.line_to(mid.0, mid.1);
            self.line_to(x, y);
            return;
        }
        self.b.ub(1, 1); // edge
        self.b.ub(1, 1); // straight
        self.b.ub(4, n - 2);
        self.b.ub(1, 1); // general line
        self.b.sb(n, dx);
        self.b.sb(n, dy);
        self.cur = (x, y);
    }
    fn quad_to(&mut self, cx: i32, cy: i32, x: i32, y: i32) {
        let (cdx, cdy) = (cx - self.cur.0, cy - self.cur.1);
        let (adx, ady) = (x - cx, y - cy);
        if cdx == 0 && cdy == 0 && adx == 0 && ady == 0 {
            return;
        }
        let n = [cdx, cdy, adx, ady].iter().map(|v| sbits(*v)).max().unwrap().max(2);
        if n > 17 {
            self.line_to(x, y);
            return;
        }
        self.b.ub(1, 1); // edge
        self.b.ub(1, 0); // curved
        self.b.ub(4, n - 2);
        for v in [cdx, cdy, adx, ady] {
            self.b.sb(n, v);
        }
        self.cur = (x, y);
    }
}

/// Fill style: solid color or gradient (linear, radial or focal radial).
fn write_fill(b: &mut Bits, s: &Shape, c: Rgba, bmp_char: Option<u16>) {
    if let (Some(bf), Some(id)) = (&s.bmp, bmp_char) {
        // Bitmap fill: tiled or clipped, smoothed or not.
        b.u8(match (bf.clip, bf.smooth) {
            (false, true) => 0x40,
            (true, true) => 0x41,
            (false, false) => 0x42,
            (true, false) => 0x43,
        });
        b.u16(id);
        // One image pixel is 20 twips.
        let m = &bf.m;
        write_matrix(b, &Matrix { a: m.a * 20.0, b: m.b * 20.0, c: m.c * 20.0, d: m.d * 20.0, tx: m.tx, ty: m.ty });
        return;
    }
    let Some(g) = &s.grad else {
        b.u8(0x00);
        b.rgba(c);
        return;
    };
    let focal = g.kind == GradKind::Radial && g.focal.abs() > 1e-3;
    b.u8(match (g.kind, focal) {
        (GradKind::Linear, _) => 0x10,
        (GradKind::Radial, false) => 0x12,
        (GradKind::Radial, true) => 0x13,
    });
    // The SWF gradient square spans ±16384 twips; ours spans ±1.
    let k = 20.0 / 16384.0;
    write_matrix(b, &Matrix { a: g.m.a * k, b: g.m.b * k, c: g.m.c * k, d: g.m.d * k, tx: g.m.tx, ty: g.m.ty });
    let stops = &g.stops[..g.stops.len().min(crate::grad::MAX_STOPS)];
    b.ub(2, match g.spread { Spread::Pad => 0, Spread::Reflect => 1, Spread::Repeat => 2 });
    b.ub(2, g.linear_rgb as u32); // interpolation: normal RGB or linear RGB
    b.ub(4, stops.len() as u32);
    for st in stops {
        b.u8((st.t * 255.0).round().clamp(0.0, 255.0) as u8);
        b.rgba(st.c);
    }
    if focal {
        b.u16(((g.focal * 256.0).round() as i16) as u16);
    }
}

/// DefineShape3 (or DefineShape4 if there is a focal gradient) of a shape
/// in its local coordinates: (tag code, body).
fn define_shape(id: u16, s: &Shape, bmp_char: Option<u16>) -> (u16, Vec<u8>) {
    let v4 = s.grad.as_ref().map_or(false, |g| g.kind == GradKind::Radial && g.focal.abs() > 1e-3);
    let mut b = Bits::new();
    b.u16(id);
    let lb = s.local_bounds();
    let pad = if s.stroke.is_some() { s.stroke_width / 2.0 + 1.0 } else { 1.0 };
    write_rect(&mut b, twips(lb.x0 - pad), twips(lb.x1 + pad), twips(lb.y0 - pad), twips(lb.y1 + pad));
    if v4 {
        write_rect(&mut b, twips(lb.x0), twips(lb.x1), twips(lb.y0), twips(lb.y1));
        b.u8(s.stroke.is_some() as u8); // UsesScalingStrokes
    }

    let fill = if s.is_closed() { s.fill } else { None };
    match fill {
        Some(c) => {
            b.u8(1);
            write_fill(&mut b, s, c, bmp_char);
        }
        None => b.u8(0),
    }
    match s.stroke {
        Some(c) => {
            b.u8(1);
            b.u16(twips(s.stroke_width.max(0.05)).clamp(1, 65535) as u16);
            if v4 {
                // LINESTYLE2: round caps and joins, no fill.
                b.u16(0);
            }
            b.rgba(c);
        }
        None => b.u8(0),
    }
    let nfb = fill.is_some() as u32;
    let nlb = s.stroke.is_some() as u32;
    b.ub(4, nfb);
    b.ub(4, nlb);
    write_edges(&mut b, &s.outline(), nfb, nlb);
    b.flush();
    (if v4 { 83 } else { 32 }, b.buf)
}

/// Edge records of an outline (with fill and line style 1 set on the
/// first move) and the end of the shape.
fn write_edges(b: &mut Bits, segs: &[Seg], nfb: u32, nlb: u32) {
    let mut first = true;
    let mut ew = EdgeWriter { b, cur: (0, 0) };
    let mut start = (0, 0);
    for seg in segs.iter().copied() {
        match seg {
            Seg::Move(x, y) => {
                let (x, y) = (twips(x), twips(y));
                let w = &mut ew.b;
                w.ub(1, 0);
                w.ub(1, 0); // new styles
                w.ub(1, (first && nlb > 0) as u32);
                w.ub(1, 0); // fill style 1
                w.ub(1, (first && nfb > 0) as u32);
                w.ub(1, 1); // move to
                let n = sbits(x).max(sbits(y));
                w.ub(5, n);
                w.sb(n, x);
                w.sb(n, y);
                if first && nfb > 0 {
                    w.ub(nfb, 1);
                }
                if first && nlb > 0 {
                    w.ub(nlb, 1);
                }
                first = false;
                ew.cur = (x, y);
                start = (x, y);
            }
            Seg::Line(x, y) => ew.line_to(twips(x), twips(y)),
            Seg::Quad(cx, cy, x, y) => ew.quad_to(twips(cx), twips(cy), twips(x), twips(y)),
            Seg::Cubic(c1x, c1y, c2x, c2y, x, y) => {
                // SWF only has quadratics: each cubic is approximated with 4.
                let p0 = (ew.cur.0 as f32 / 20.0, ew.cur.1 as f32 / 20.0);
                let (mut a, mut b, mut c, d) = (p0, (c1x, c1y), (c2x, c2y), (x, y));
                for k in (1..=4).rev() {
                    let (h1, h2, m, r1, r2) = if k > 1 { split_cubic(a, b, c, d, 1.0 / k as f32) } else { (b, c, d, d, d) };
                    let q = ((3.0 * (h1.0 + h2.0) - a.0 - m.0) / 4.0, (3.0 * (h1.1 + h2.1) - a.1 - m.1) / 4.0);
                    ew.quad_to(twips(q.0), twips(q.1), twips(m.0), twips(m.1));
                    a = m;
                    b = r1;
                    c = r2;
                }
            }
            Seg::Close => ew.line_to(start.0, start.1),
        }
    }
    ew.b.ub(6, 0); // EndShapeRecord
}

/// Glyph outline for DefineFont3: EM of 1024 (×20 = 20480 units),
/// baseline at y = 0 and y pointing down.
struct GlyphOutline {
    k: f32,
    out: Vec<Seg>,
}

impl ttf_parser::OutlineBuilder for GlyphOutline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.out.push(Seg::Move(x * self.k, -y * self.k));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.out.push(Seg::Line(x * self.k, -y * self.k));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.out.push(Seg::Quad(x1 * self.k, -y1 * self.k, x * self.k, -y * self.k));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.out.push(Seg::Cubic(x1 * self.k, -y1 * self.k, x2 * self.k, -y2 * self.k, x * self.k, -y * self.k));
    }
    fn close(&mut self) {
        self.out.push(Seg::Close);
    }
}

/// Flash HTML text for a field with several formats.
fn html_of(tf: &crate::text::TextField, color: Rgba) -> String {
    use crate::text::TextAlign;
    let esc = |c: char| match c {
        '&' => "&amp;".to_string(),
        '<' => "&lt;".to_string(),
        '>' => "&gt;".to_string(),
        '"' => "&quot;".to_string(),
        c => c.to_string(),
    };
    let align = match tf.align {
        TextAlign::Left => "LEFT",
        TextAlign::Right => "RIGHT",
        TextAlign::Center => "CENTER",
        TextAlign::Justify => "JUSTIFY",
    };
    let chars: Vec<char> = tf.shown_text().chars().collect();
    let mut out = format!("<P ALIGN=\"{align}\">");
    let mut pos = 0usize;
    for r in tf.full_runs() {
        let end = (pos + r.len as usize).min(chars.len());
        let c = r.color.unwrap_or(color);
        let face = embed_name(&r.font, tf.embed);
        let open = format!(
            "<FONT FACE=\"{face}\" SIZE=\"{}\" COLOR=\"#{:02X}{:02X}{:02X}\" LETTERSPACING=\"{}\" KERNING=\"{}\">{}{}",
            r.size.round(),
            c.r,
            c.g,
            c.b,
            r.letter,
            tf.kerning as u8,
            if r.bold { "<B>" } else { "" },
            if r.italic { "<I>" } else { "" }
        );
        let close = format!("{}{}</FONT>", if r.italic { "</I>" } else { "" }, if r.bold { "</B>" } else { "" });
        out.push_str(&open);
        for &ch in &chars[pos..end] {
            if ch == '\n' {
                out.push_str(&close);
                out.push_str(&format!("</P><P ALIGN=\"{align}\">"));
                out.push_str(&open);
            } else {
                out.push_str(&esc(ch));
            }
        }
        out.push_str(&close);
        pos = end;
    }
    out.push_str("</P>");
    out
}

/// Font name in the SWF: the device font if it is not embedded.
fn embed_name(family: &str, embed: bool) -> String {
    let real = crate::text::real_family(family);
    if embed {
        return real.to_string();
    }
    match real {
        "Arimo" => "_sans",
        "Tinos" => "_serif",
        "Cousine" => "_typewriter",
        f => f,
    }
    .to_string()
}

fn shape_key(s: &Shape) -> String {
    let mut k = s.clone();
    k.id = 0;
    k.m = Matrix::IDENTITY;
    k.raw = false;
    k.tag = None;
    serde_json::to_string(&k).unwrap_or_default()
}

/// Display list item of a frame.
#[derive(Clone, PartialEq)]
struct Item {
    id: u16,
    m: Matrix,
    /// Depth it clips up to (if it is a mask).
    clip: u16,
    /// Only clip and button instances carry these properties.
    cx: Option<CxForm>,
    name: String,
    filters: Vec<Filter>,
    blend: Blend,
    cache: bool,
    /// Already encoded CLIPACTIONS (actions of a clip instance).
    actions: Vec<u8>,
}

impl Item {
    fn shape(id: u16, m: Matrix) -> Item {
        Item { id, m, clip: 0, cx: None, name: String::new(), filters: Vec::new(), blend: Blend::Normal, cache: false, actions: Vec::new() }
    }
    /// Needs PlaceObject3 (Flash 8).
    fn v3(&self) -> bool {
        !self.filters.is_empty() || self.blend != Blend::Normal || self.cache
    }
}

/// Single shape with all the fills of a mask layer (for ClipDepth).
fn mask_shape(shapes: &[Shape]) -> Option<Shape> {
    let r = crate::paint::union_all(shapes.iter().flat_map(|s| s.leaves(&Matrix::IDENTITY)).map(|s| crate::paint::fill_region(&s)).collect());
    crate::paint::region_shape(&r, Rgba::BLACK)
}

fn write_cxform(b: &mut Bits, cx: &CxForm) {
    let mul: Vec<i32> = cx.mul.iter().map(|v| (v * 256.0).round() as i32).collect();
    let add: Vec<i32> = cx.add.iter().map(|v| v.round() as i32).collect();
    let has_mul = mul.iter().any(|v| *v != 256);
    let has_add = add.iter().any(|v| *v != 0);
    let mut n = 1;
    if has_mul {
        n = mul.iter().map(|v| sbits(*v)).max().unwrap().max(n);
    }
    if has_add {
        n = add.iter().map(|v| sbits(*v)).max().unwrap().max(n);
    }
    b.ub(1, has_add as u32);
    b.ub(1, has_mul as u32);
    b.ub(4, n);
    if has_mul {
        mul.iter().for_each(|v| b.sb(n, *v));
    }
    if has_add {
        add.iter().for_each(|v| b.sb(n, *v));
    }
    b.flush();
}

fn put_fixed(b: &mut Bits, v: f32) {
    b.flush();
    b.buf.extend_from_slice(&fixed(v).to_le_bytes());
}

fn put_fixed8(b: &mut Bits, v: f32) {
    b.u16((v * 256.0).round().clamp(0.0, 65535.0) as u16);
}

fn write_filters(b: &mut Bits, filters: &[Filter]) {
    b.u8(filters.len().min(255) as u8);
    for f in filters.iter().take(255) {
        match f {
            Filter::Dropshadow { color, blur_x, blur_y, strength, angle, distance, quality, inner, knockout, hide } => {
                b.u8(0);
                b.rgba(*color);
                put_fixed(b, *blur_x);
                put_fixed(b, *blur_y);
                put_fixed(b, angle.to_radians());
                put_fixed(b, *distance);
                put_fixed8(b, strength / 100.0);
                b.ub(1, *inner as u32);
                b.ub(1, *knockout as u32);
                b.ub(1, !hide as u32);
                b.ub(5, (*quality).clamp(1, 15) as u32);
            }
            Filter::Blur { blur_x, blur_y, quality } => {
                b.u8(1);
                put_fixed(b, *blur_x);
                put_fixed(b, *blur_y);
                b.ub(5, (*quality).clamp(1, 15) as u32);
                b.ub(3, 0);
            }
            Filter::Glow { color, blur_x, blur_y, strength, quality, inner, knockout } => {
                b.u8(2);
                b.rgba(*color);
                put_fixed(b, *blur_x);
                put_fixed(b, *blur_y);
                put_fixed8(b, strength / 100.0);
                b.ub(1, *inner as u32);
                b.ub(1, *knockout as u32);
                b.ub(1, 1);
                b.ub(5, (*quality).clamp(1, 15) as u32);
            }
            Filter::Bevel { shadow, highlight, blur_x, blur_y, strength, angle, distance, quality, knockout, bevel } => {
                b.u8(3);
                b.rgba(*shadow);
                b.rgba(*highlight);
                put_fixed(b, *blur_x);
                put_fixed(b, *blur_y);
                put_fixed(b, angle.to_radians());
                put_fixed(b, *distance);
                put_fixed8(b, strength / 100.0);
                b.ub(1, (bevel == "inner") as u32);
                b.ub(1, *knockout as u32);
                b.ub(1, 1);
                b.ub(1, (bevel == "full") as u32);
                b.ub(4, (*quality).clamp(1, 15) as u32);
            }
            Filter::Adjustcolor { brightness, contrast, saturation, hue } => {
                b.u8(6);
                for v in adjust_color_matrix(*brightness, *contrast, *saturation, *hue) {
                    b.flush();
                    b.buf.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
    }
    b.flush();
}

fn put_string(b: &mut Bits, s: &str) {
    b.flush();
    b.buf.extend_from_slice(s.as_bytes());
    b.buf.push(0);
}

/// Definition tags (shared by the whole movie) and characters already emitted.
struct Ctx {
    tags: Vec<u8>,
    chars: HashMap<String, u16>,
    /// Symbols already defined (None while being defined: avoids recursion).
    symbols: HashMap<String, Option<u16>>,
    next: u16,
    export_hidden: bool,
    /// Scenes, for gotoAndPlay("Scene", n).
    as_ctx: crate::as2::Ctx,
    /// ActionScript compile errors (for the Output panel).
    errors: Vec<String>,
    /// Where we are exporting (to locate errors): «Escena=…» or «Símbolo=…».
    place: String,
    here: String,
    sounds: Vec<crate::sound::SoundAsset>,
    fps: f32,
    /// Images with "Photo (JPEG)" compression.
    jpegs: HashMap<u32, Vec<u8>>,
}

impl Ctx {
    fn alloc(&mut self) -> u16 {
        let id = self.next;
        self.next = self.next.wrapping_add(1).max(1);
        id
    }

    fn shape_char(&mut self, s: &Shape) -> u16 {
        let key = shape_key(s);
        if let Some(id) = self.chars.get(&key) {
            return *id;
        }
        let id = self.alloc();
        let bmp_char = s.bmp.as_ref().and_then(|b| self.bitmap_char(b.id, b.cx.as_ref()));
        let (code, body) = define_shape(id, s, bmp_char);
        tag(&mut self.tags, code, &body);
        self.chars.insert(key, id);
        id
    }

    /// Display list of a timeline at frame `f`.
    fn layer_items(&mut self, layers: &[Layer], f: u32, base: &Matrix, cx: CxForm, out: &mut Vec<Item>) {
        use crate::timeline::{eff_visible, layer_shapes, mask_of};
        let hidden = self.export_hidden;
        let shown = |li: usize| {
            !matches!(layers[li].kind, LayerKind::Folder | LayerKind::Guide | LayerKind::MotionGuide) && (eff_visible(layers, li) || hidden)
        };
        for li in (0..layers.len()).rev() {
            if !shown(li) || mask_of(layers, li).is_some() {
                continue;
            }
            if layers[li].kind == LayerKind::Mask {
                // Mask: a clip with ClipDepth over its masked layers (bottom to top).
                let kids: Vec<usize> = (0..layers.len()).rev().filter(|&k| mask_of(layers, k) == Some(li) && shown(k)).collect();
                let Some(ms) = mask_shape(&layer_shapes(layers, li, f)) else { continue };
                let clip_at = out.len();
                let id = self.shape_char(&ms);
                out.push(Item::shape(id, base.mul(&ms.m)));
                for k in kids {
                    self.layer_shape_items(layers, k, f, base, cx, out);
                }
                out[clip_at].clip = out.len() as u16;
                continue;
            }
            self.layer_shape_items(layers, li, f, base, cx, out);
        }
    }

    fn layer_shape_items(&mut self, layers: &[Layer], li: usize, f: u32, base: &Matrix, cx: CxForm, out: &mut Vec<Item>) {
        let key = layers[li].key_at_or_before(f).unwrap_or(0);
        self.here = format!("{}, capa={}, fotograma={}", self.place, layers[li].name, key + 1);
        let offset = layers[li].anim_offset(f);
        for s in crate::timeline::layer_shapes(layers, li, f) {
            self.collect(&s, base, cx, offset, out);
        }
    }

    fn collect(&mut self, s: &Shape, parent: &Matrix, cx: CxForm, offset: u32, out: &mut Vec<Item>) {
        let m = parent.mul(&s.m);
        match &s.geom {
            // Groups are exported as their shapes, with the accumulated matrix.
            Geometry::Group { children, .. } => {
                for c in children {
                    self.collect(c, &m, cx, offset, out);
                }
            }
            Geometry::Instance(inst) => {
                let Some(sym) = symbol(inst.symbol) else { return };
                let cx = cx.compose(&inst.color.cxform());
                match inst.kind {
                    // Graphics follow the timeline of their container: they are inlined.
                    SymbolKind::Graphic | SymbolKind::Bitmap => {
                        let key = format!("gr:{}", sym.id);
                        if out.len() > 16000 || self.symbols.contains_key(&key) {
                            return;
                        }
                        self.symbols.insert(key.clone(), None);
                        let frame = symbol_frame(&sym, inst, offset, false);
                        self.layer_items(&sym.layers, frame, &m, cx, out);
                        self.symbols.remove(&key);
                    }
                    kind => {
                        let Some(id) = self.symbol_char(&sym, kind, &inst.actions) else { return };
                        let actions = if kind == SymbolKind::Movieclip { self.clip_actions(&inst.actions) } else { Vec::new() };
                        out.push(Item {
                            id,
                            m,
                            clip: 0,
                            cx: (!cx.is_identity()).then_some(cx),
                            name: inst.name.clone(),
                            filters: inst.active_filters().cloned().collect(),
                            blend: inst.blend,
                            cache: inst.cache,
                            actions,
                        });
                    }
                }
            }
            // Dynamic or input text: a real text field (DefineEditText).
            Geometry::Text(tf) if tf.kind != crate::text::TextKind::Static => {
                let mut l = s.clone();
                l.apply_cx(&cx);
                let id = self.edit_text_char(tf, l.fill.unwrap_or(Rgba::BLACK));
                out.push(Item { name: tf.name.clone(), ..Item::shape(id, m) });
            }
            // Static text with several formats: one shape per color (and faux bold as a stroke).
            Geometry::Text(tf) if s.grad.is_none() && (tf.laid().groups.len() > 1 || tf.laid().groups.iter().any(|g| g.color.is_some() || g.fake_bold)) => {
                let mut l = s.clone();
                l.apply_cx(&cx);
                let Geometry::Text(tf) = &l.geom else { return };
                let base_fill = l.fill.unwrap_or(Rgba::BLACK);
                for g in &tf.laid().groups {
                    let c = g.color.unwrap_or(base_fill);
                    let mut piece = Shape::new(Geometry::Bezier { paths: crate::model::segs_to_bez(&g.outline) }, Some(c), None, 1.0, m);
                    piece.raw = true;
                    if g.fake_bold {
                        piece.stroke = Some(c);
                        piece.stroke_width = g.size * 0.045;
                    }
                    let id = self.shape_char(&piece);
                    out.push(Item::shape(id, m));
                }
            }
            _ => {
                let mut l = s.clone();
                l.m = m;
                l.apply_cx(&cx);
                let id = self.shape_char(&l);
                out.push(Item::shape(id, m));
            }
        }
    }

    /// The image in the SWF (once per image and color effect): JPEG if it
    /// has "Photo" compression, otherwise DefineBitsLossless2.
    fn bitmap_char(&mut self, asset: u32, cx: Option<&CxForm>) -> Option<u16> {
        let cx = cx.filter(|c| !c.is_identity());
        let key = format!("bmp:{asset}:{cx:?}");
        if let Some(id) = self.chars.get(&key) {
            return Some(*id);
        }
        let jpeg = self.jpegs.get(&asset).cloned().filter(|_| cx.is_none());
        let id = self.alloc();
        match jpeg {
            Some(jpg) => {
                let p = crate::bitmap::pixmap(asset)?;
                let alpha: Vec<u8> = p.pixels().iter().map(|c| c.alpha()).collect();
                let mut t = id.to_le_bytes().to_vec();
                if alpha.iter().all(|a| *a == 255) {
                    // DefineBitsJPEG2: just the JPEG.
                    t.extend_from_slice(&jpg);
                    tag(&mut self.tags, 21, &t);
                } else {
                    // DefineBitsJPEG3: JPEG + compressed alpha channel.
                    t.extend_from_slice(&(jpg.len() as u32).to_le_bytes());
                    t.extend_from_slice(&jpg);
                    t.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&alpha, 6));
                    tag(&mut self.tags, 35, &t);
                }
            }
            None => {
                let (w, h, data) = crate::bitmap::lossless2(asset, cx)?;
                let mut t = Vec::with_capacity(data.len() + 7);
                t.extend_from_slice(&id.to_le_bytes());
                t.push(5); // 32-bit ARGB
                t.extend_from_slice(&w.to_le_bytes());
                t.extend_from_slice(&h.to_le_bytes());
                t.extend_from_slice(&data);
                tag(&mut self.tags, 36, &t);
            }
        }
        self.chars.insert(key, id);
        Some(id)
    }

    /// Embedded font (DefineFont3 with the character outlines):
    /// dynamic text looks the same on any machine and can be rotated.
    fn embedded_font(&mut self, family: &str, bold: bool, italic: bool, chars: &str) -> u16 {
        let name = embed_name(family, true);
        let key = format!("efont:{name}:{bold}:{italic}");
        if let Some(id) = self.chars.get(&key) {
            return *id;
        }
        let pick = crate::text::pick(family, bold, italic);
        let Ok(face) = ttf_parser::Face::parse(pick.data, 0) else { return self.device_font(family, bold, italic) };
        // The field's characters plus printable ASCII (what code usually writes).
        let mut set: Vec<char> = (32u8..127).map(|c| c as char).chain(chars.chars().filter(|c| *c >= ' ')).collect();
        set.sort();
        set.dedup();
        let k = 1024.0 / face.units_per_em() as f32;
        let mut glyphs = Vec::new();
        for ch in set {
            let Some(gid) = face.glyph_index(ch) else { continue };
            let mut o = GlyphOutline { k, out: Vec::new() };
            face.outline_glyph(gid, &mut o);
            let mut g = Bits::new();
            g.ub(4, 1);
            g.ub(4, 0);
            write_edges(&mut g, &o.out, 1, 0);
            g.flush();
            let adv = face.glyph_hor_advance(gid).unwrap_or(0) as f32 * k;
            let bb = face.glyph_bounding_box(gid).map(|r| (r.x_min as f32 * k, r.x_max as f32 * k, -r.y_max as f32 * k, -r.y_min as f32 * k)).unwrap_or_default();
            glyphs.push((ch, g.buf, adv, bb));
        }
        let id = self.alloc();
        let mut b = Bits::new();
        b.u16(id);
        b.u8(0x80 | 0x08 | 0x04 | if italic { 0x02 } else { 0 } | if bold { 0x01 } else { 0 }); // with metrics, offsets and wide codes
        b.u8(1);
        b.u8(name.len() as u8);
        b.buf.extend_from_slice(name.as_bytes());
        b.u16(glyphs.len() as u16);
        // Offset table (from its own start) and the offset of the code table.
        let mut off = 4 * (glyphs.len() as u32 + 1);
        for g in &glyphs {
            b.buf.extend_from_slice(&off.to_le_bytes());
            off += g.1.len() as u32;
        }
        b.buf.extend_from_slice(&off.to_le_bytes());
        for g in &glyphs {
            b.buf.extend_from_slice(&g.1);
        }
        for g in &glyphs {
            b.u16(g.0 as u32 as u16);
        }
        b.u16(twips(face.ascender() as f32 * k).clamp(0, 65535) as u16);
        b.u16(twips(-face.descender() as f32 * k).clamp(0, 65535) as u16);
        b.u16(twips(face.line_gap() as f32 * k) as i16 as u16);
        for g in &glyphs {
            b.u16(twips(g.2) as i16 as u16);
        }
        for g in &glyphs {
            let (x0, x1, y0, y1) = g.3;
            write_rect(&mut b, twips(x0), twips(x1), twips(y0), twips(y1));
            b.flush();
        }
        b.u16(0); // no kerning pairs
        tag(&mut self.tags, 75, &b.buf);
        self.chars.insert(key, id);
        id
    }

    /// Device font (DefineFont2 without glyphs): the player uses its own.
    fn device_font(&mut self, family: &str, bold: bool, italic: bool) -> u16 {
        let name = embed_name(family, false);
        let key = format!("font:{name}:{bold}:{italic}");
        if let Some(id) = self.chars.get(&key) {
            return *id;
        }
        let id = self.alloc();
        let mut b = Bits::new();
        b.u16(id);
        b.u8(0x04 | if italic { 0x02 } else { 0 } | if bold { 0x01 } else { 0 }); // WideCodes, italic, bold
        b.u8(1); // language: Latin
        b.u8(name.len() as u8);
        b.buf.extend_from_slice(name.as_bytes());
        b.u16(0); // no glyphs
        tag(&mut self.tags, 48, &b.buf);
        self.chars.insert(key, id);
        id
    }

    fn edit_text_char(&mut self, tf: &crate::text::TextField, color: Rgba) -> u16 {
        use crate::text::{LineType, TextAlign, TextKind};
        let key = format!("edit:{}:{}", serde_json::to_string(tf).unwrap_or_default(), color.hex());
        if let Some(id) = self.chars.get(&key) {
            return *id;
        }
        let html = !tf.runs.is_empty();
        let font = if tf.embed {
            // Each variant used (bold, italic) gets its own font.
            for r in tf.full_runs() {
                if r.bold != tf.bold || r.italic != tf.italic || r.font != tf.font {
                    self.embedded_font(&r.font, r.bold, r.italic, &tf.text);
                }
            }
            self.embedded_font(&tf.font, tf.bold, tf.italic, &tf.text)
        } else {
            self.device_font(&tf.font, tf.bold, tf.italic)
        };
        let id = self.alloc();
        let bb = tf.bounds();
        let mut b = Bits::new();
        b.u16(id);
        write_rect(&mut b, twips(bb.x0), twips(bb.x1), twips(bb.y0), twips(bb.y1));
        let multi = tf.lines != LineType::Single;
        let wrap = tf.lines == LineType::Multi && tf.width.is_some();
        let mut f1 = 0x80 | 0x04 | 0x01; // text, color, font
        if wrap {
            f1 |= 0x40;
        }
        if multi {
            f1 |= 0x20;
        }
        if tf.kind == TextKind::Dynamic {
            f1 |= 0x08; // read-only
        }
        if tf.max_chars > 0 {
            f1 |= 0x02;
        }
        let mut f2 = 0x20; // with paragraph format
        if tf.width.is_none() {
            f2 |= 0x40; // auto size
        }
        if !tf.selectable {
            f2 |= 0x10;
        }
        if tf.border {
            f2 |= 0x08;
        }
        if html {
            f2 |= 0x02;
        }
        if tf.embed {
            f2 |= 0x01; // embedded font outlines
        }
        b.u8(f1);
        b.u8(f2);
        b.u16(font);
        b.u16(twips(tf.size).clamp(1, 65535) as u16);
        b.rgba(color);
        if tf.max_chars > 0 {
            b.u16(tf.max_chars.min(65535) as u16);
        }
        b.u8(match tf.align {
            TextAlign::Left => 0,
            TextAlign::Right => 1,
            TextAlign::Center => 2,
            TextAlign::Justify => 3,
        });
        b.u16(0);
        b.u16(0);
        b.u16(0);
        b.u16((twips(tf.leading) as i16) as u16);
        put_string(&mut b, &tf.var);
        let text = if html { html_of(tf, color) } else { tf.shown_text().replace('\n', "\r") };
        put_string(&mut b, &text);
        tag(&mut self.tags, 37, &b.buf);
        self.chars.insert(key, id);
        id
    }

    fn script_error(&mut self, what: &str, errs: Vec<crate::as2::AsError>) {
        for e in errs {
            self.errors.push(format!("**Error** {}{what}: {e}", self.here));
        }
    }

    /// CLIPACTIONS of a clip instance (empty if it has no actions).
    fn clip_actions(&mut self, src: &str) -> Vec<u8> {
        if src.trim().is_empty() {
            return Vec::new();
        }
        let recs = match crate::as2::compile_clip(src, &self.as_ctx) {
            Ok(r) => r,
            Err(e) => {
                self.script_error(", acciones del clip", e);
                return Vec::new();
            }
        };
        if recs.is_empty() {
            return Vec::new();
        }
        let mut b = Vec::new();
        b.extend_from_slice(&[0, 0]);
        let all = recs.iter().fold(0u32, |a, r| a | r.0);
        b.extend_from_slice(&all.to_be_bytes());
        for (flags, key, code) in recs {
            b.extend_from_slice(&flags.to_be_bytes());
            let size = code.len() + key.is_some() as usize;
            b.extend_from_slice(&(size as u32).to_le_bytes());
            if let Some(k) = key {
                b.push(k);
            }
            b.extend_from_slice(&code);
        }
        b.extend_from_slice(&[0, 0, 0, 0]);
        b
    }

    /// Actions of the keyframes starting at `f` (DoAction, top to bottom).
    fn frame_actions(&mut self, layers: &[Layer], f: u32) -> Vec<Vec<u8>> {
        let mut v = Vec::new();
        for l in layers {
            if matches!(l.kind, LayerKind::Folder | LayerKind::Guide | LayerKind::MotionGuide) || f >= l.length {
                continue;
            }
            let Some(k) = l.keys.get(&f) else { continue };
            if k.actions.trim().is_empty() {
                continue;
            }
            match crate::as2::compile_frame(&k.actions, &self.as_ctx) {
                Ok(code) => v.push(code),
                Err(e) => {
                    self.here = format!("{}, capa={}, fotograma={}", self.place, l.name, f + 1);
                    self.script_error("", e);
                }
            }
        }
        v
    }

    /// DefineSprite or DefineButton2 of a symbol (once per symbol, type and
    /// actions: a button's actions go in its definition, as Flash does).
    fn symbol_char(&mut self, sym: &Symbol, kind: SymbolKind, actions: &str) -> Option<u16> {
        let key = if kind == SymbolKind::Button { format!("btn:{}:{}", sym.id, actions) } else { format!("spr:{}", sym.id) };
        if let Some(c) = self.symbols.get(&key) {
            return *c; // None: being defined (it contains itself).
        }
        self.symbols.insert(key.clone(), None);
        let place = std::mem::replace(&mut self.place, format!("Símbolo={}", sym.name));
        let here = self.here.clone();
        let id = if kind == SymbolKind::Button {
            let conds = if actions.trim().is_empty() {
                Vec::new()
            } else {
                match crate::as2::compile_button(actions, &self.as_ctx) {
                    Ok(c) => c,
                    Err(e) => {
                        self.here = here.clone();
                        self.script_error(", acciones del botón", e);
                        Vec::new()
                    }
                }
            };
            self.define_button(sym, &conds)
        } else {
            self.define_sprite(sym)
        };
        self.place = place;
        self.here = here;
        self.symbols.insert(key, Some(id));
        Some(id)
    }

    /// One-frame clip with an already built display list.
    fn sprite_of(&mut self, items: Vec<Item>) -> u16 {
        let body = self.timeline_with(1, move |_, _, out| {
            *out = items.clone();
            Vec::new()
        });
        let id = self.alloc();
        let mut t = Vec::new();
        t.extend_from_slice(&id.to_le_bytes());
        t.extend_from_slice(&1u16.to_le_bytes());
        t.extend_from_slice(&body);
        tag(&mut self.tags, 39, &t);
        id
    }

    fn define_sprite(&mut self, sym: &Symbol) -> u16 {
        let frames = sym.total();
        let body = self.timeline(&sym.layers, frames);
        let id = self.alloc();
        let mut t = Vec::new();
        t.extend_from_slice(&id.to_le_bytes());
        t.extend_from_slice(&(frames.min(u16::MAX as u32) as u16).to_le_bytes());
        t.extend_from_slice(&body);
        tag(&mut self.tags, 39, &t);
        if let Some(g) = sym.scale9 {
            // DefineScalingGrid: 9-slice scaling.
            let mut b = Bits::new();
            b.u16(id);
            write_rect(&mut b, twips(g[0]), twips(g[2]), twips(g[1]), twips(g[3]));
            tag(&mut self.tags, 78, &b.buf);
        }
        id
    }

    fn define_button(&mut self, sym: &Symbol, conds: &[(u16, Vec<u8>)]) -> u16 {
        let len = sym.total();
        let hit = if len > 3 && !symbol_shapes(sym, 3).is_empty() { 3 } else { 0 };
        // Up, Over, Down, Hit.
        let states = [(0u32, 0x01u8), (1.min(len - 1), 0x02), (2.min(len - 1), 0x04), (hit, 0x08)];
        let mut recs: Vec<(u8, u16, Item)> = Vec::new();
        for (frame, bit) in states {
            let mut items = Vec::new();
            self.layer_items(&sym.layers, frame, &Matrix::IDENTITY, CxForm::ID, &mut items);
            if items.iter().any(|it| it.clip > 0) {
                // DefineButton2 does not clip: a masked state goes inside a one-frame clip.
                let sid = self.sprite_of(items);
                recs.push((bit, 1, Item::shape(sid, Matrix::IDENTITY)));
            } else {
                for (d, it) in items.into_iter().enumerate() {
                    recs.push((bit, d as u16 + 1, it));
                }
            }
        }
        let id = self.alloc();
        let mut b = Bits::new();
        b.u16(id);
        b.u8(0); // TrackAsMenu = 0
        let action_offset_at = b.buf.len();
        b.u16(0); // ActionOffset (filled in if there are actions)
        for (bit, depth, it) in recs {
            let mut flags = bit;
            if !it.filters.is_empty() {
                flags |= 0x10;
            }
            if it.blend != Blend::Normal {
                flags |= 0x20;
            }
            b.u8(flags);
            b.u16(it.id);
            b.u16(depth);
            write_matrix(&mut b, &it.m);
            write_cxform(&mut b, &it.cx.unwrap_or(CxForm::ID));
            if !it.filters.is_empty() {
                write_filters(&mut b, &it.filters);
            }
            if it.blend != Blend::Normal {
                b.u8(it.blend.swf());
            }
        }
        b.u8(0); // CharacterEndFlag
        if !conds.is_empty() {
            let start = b.buf.len();
            let off = (start - action_offset_at) as u16;
            b.buf[action_offset_at..action_offset_at + 2].copy_from_slice(&off.to_le_bytes());
            for (i, (cond, code)) in conds.iter().enumerate() {
                let size = if i + 1 < conds.len() { (4 + code.len()) as u16 } else { 0 };
                b.buf.extend_from_slice(&size.to_le_bytes());
                b.buf.extend_from_slice(&cond.to_be_bytes());
                b.buf.extend_from_slice(code);
            }
        }
        tag(&mut self.tags, 34, &b.buf);
        id
    }

    /// Control tags (PlaceObject/RemoveObject/ShowFrame) of a timeline.
    fn timeline(&mut self, layers: &[Layer], frames: u32) -> Vec<u8> {
        let stream = self.stream_format(&[layers]);
        self.timeline_with(frames, |c, f, out| {
            c.layer_items(layers, f, &Matrix::IDENTITY, CxForm::ID, out);
            c.frame_tags(layers, f, stream, f == 0)
        })
    }

    // ───────── Sound ─────────

    fn sound(&self, id: u32) -> Option<&crate::sound::SoundAsset> {
        self.sounds.iter().find(|s| s.id == id)
    }

    /// DefineSound (once per sound): uncompressed 16-bit PCM.
    fn sound_char(&mut self, id: u32) -> Option<u16> {
        let key = format!("snd:{id}");
        if let Some(c) = self.chars.get(&key) {
            return Some(*c);
        }
        let s = self.sound(id)?.clone();
        let cid = self.alloc();
        let mut t = Vec::new();
        t.extend_from_slice(&cid.to_le_bytes());
        let comp = if s.adpcm > 0 { 1 } else { 3 };
        t.push(comp << 4 | s.rate_code() << 2 | 1 << 1 | s.stereo as u8);
        t.extend_from_slice(&s.samples.to_le_bytes());
        if s.adpcm > 0 {
            t.extend_from_slice(&crate::sound::adpcm_encode(&crate::sound::pcm_samples(&s.pcm), s.stereo, s.adpcm));
        } else {
            t.extend_from_slice(&crate::bitmap::b64_decode(&s.pcm));
        }
        tag(&mut self.tags, 14, &t);
        self.chars.insert(key, cid);
        Some(cid)
    }

    /// ADPCM bits of a timeline's stream (the largest of its sounds; 0 = uncompressed).
    fn stream_adpcm(&self, layers: &[Layer]) -> u8 {
        let mut bits = 0;
        for l in layers {
            for k in l.keys.values() {
                if let Some(fs) = k.sound.as_ref().filter(|s| s.sync == crate::sound::Sync::Stream) {
                    match self.sound(fs.id).map(|a| a.adpcm) {
                        Some(0) | None => return 0,
                        Some(b) => bits = bits.max(b),
                    }
                }
            }
        }
        bits
    }

    /// Stream format of a timeline (rate, stereo), if it has stream sounds.
    fn stream_format(&self, sets: &[&[Layer]]) -> Option<(u32, bool)> {
        let mut fmt: Option<(u32, bool)> = None;
        for ls in sets {
            for l in ls.iter() {
                for k in l.keys.values() {
                    if let Some(fs) = k.sound.as_ref().filter(|s| s.sync == crate::sound::Sync::Stream) {
                        if let Some(a) = self.sound(fs.id) {
                            fmt = Some(match fmt {
                                Some((r, st)) => (r.max(a.rate), st || a.stereo),
                                None => (a.rate, a.stereo),
                            });
                        }
                    }
                }
            }
        }
        fmt
    }

    /// Tags of a frame besides the display list:
    /// stream header, actions (DoAction), sounds (StartSound) and stream block.
    fn frame_tags(&mut self, layers: &[Layer], f: u32, stream: Option<(u32, bool)>, first: bool) -> Vec<(u16, Vec<u8>)> {
        use crate::sound::Sync;
        let mut out = Vec::new();
        if let (true, Some((rate, stereo))) = (first, stream) {
            let code = crate::sound::SoundAsset { id: 0, rate, stereo, samples: 0, pcm: String::new(), adpcm: 0 }.rate_code();
            let fmt = code << 2 | 1 << 1 | stereo as u8;
            let comp = if self.stream_adpcm(layers) > 0 { 1 } else { 3 };
            let mut t = vec![fmt, comp << 4 | fmt];
            t.extend_from_slice(&((rate as f32 / self.fps.max(0.01)).round().min(65535.0) as u16).to_le_bytes());
            out.push((45, t));
        }
        for a in self.frame_actions(layers, f) {
            out.push((12, a));
        }
        let mut mix: Vec<i32> = Vec::new();
        for l in layers {
            if matches!(l.kind, LayerKind::Folder | LayerKind::Guide | LayerKind::MotionGuide) || f >= l.length {
                continue;
            }
            let Some(key) = l.key_at_or_before(f) else { continue };
            let Some(fs) = l.keys[&key].sound.clone() else { continue };
            match fs.sync {
                Sync::Stream => {
                    let (Some((rate, stereo)), Some(a)) = (stream, self.sound(fs.id).cloned()) else { continue };
                    if a.rate != rate {
                        continue;
                    }
                    let spf = rate as f32 / self.fps.max(0.01);
                    let (s0, s1) = (((f - key) as f32 * spf).round() as usize, ((f - key + 1) as f32 * spf).round() as usize);
                    let to_rate = |p44: u32| (p44 as u64 * a.rate as u64 / 44100) as usize;
                    let first = to_rate(fs.inp).min(a.samples as usize);
                    let last = if fs.out > 0 { to_rate(fs.out).clamp(first + 1, a.samples as usize) } else { a.samples as usize };
                    let len = (last - first).max(1);
                    let total = len * fs.loops.max(1) as usize;
                    if s0 >= total {
                        continue;
                    }
                    let pcm = crate::bitmap::b64_decode(&a.pcm);
                    let ch_in = if a.stereo { 2 } else { 1 };
                    let ch_out = if stereo { 2 } else { 1 };
                    if mix.is_empty() {
                        mix = vec![0; (s1 - s0) * ch_out];
                    }
                    for (n, smp) in (s0..s1.min(total)).enumerate() {
                        let frame = first + smp % len;
                        // The envelope counts from the start of the sound (in 44 kHz samples).
                        let (gl, gr) = fs.gain_at((frame as u64 * 44100 / a.rate.max(1) as u64) as u32);
                        for c in 0..ch_out {
                            let i = (frame * ch_in + c.min(ch_in - 1)) * 2;
                            if i + 1 < pcm.len() && n * ch_out + c < mix.len() {
                                let g = if ch_out == 1 { (gl + gr) / 2.0 } else if c == 0 { gl } else { gr };
                                mix[n * ch_out + c] += (i16::from_le_bytes([pcm[i], pcm[i + 1]]) as f32 * g) as i32;
                            }
                        }
                    }
                }
                _ if key != f => {}
                sync => {
                    let Some(cid) = self.sound_char(fs.id) else { continue };
                    let mut t = cid.to_le_bytes().to_vec();
                    let loops = fs.loops.max(1);
                    let stop = sync == Sync::Stop;
                    let env = fs.has_envelope() && !stop;
                    let flags = match sync {
                        Sync::Stop => 0x20,
                        Sync::Start => 0x10,
                        _ => 0,
                    } | if loops > 1 { 0x04 } else { 0 }
                        | if env { 0x08 } else { 0 }
                        | if fs.out > 0 && !stop { 0x02 } else { 0 }
                        | if fs.inp > 0 && !stop { 0x01 } else { 0 };
                    t.push(flags);
                    if flags & 0x01 != 0 {
                        t.extend_from_slice(&fs.inp.to_le_bytes());
                    }
                    if flags & 0x02 != 0 {
                        t.extend_from_slice(&fs.out.to_le_bytes());
                    }
                    if loops > 1 {
                        t.extend_from_slice(&(loops.min(65535) as u16).to_le_bytes());
                    }
                    if env {
                        t.push(fs.env.len() as u8);
                        for p in &fs.env {
                            t.extend_from_slice(&p.pos.to_le_bytes());
                            t.extend_from_slice(&p.l.to_le_bytes());
                            t.extend_from_slice(&p.r.to_le_bytes());
                        }
                    }
                    out.push((15, t));
                }
            }
        }
        if !mix.is_empty() {
            let bits = self.stream_adpcm(layers);
            let block: Vec<u8> = if bits > 0 {
                let s: Vec<i16> = mix.iter().map(|v| (*v).clamp(-32768, 32767) as i16).collect();
                crate::sound::adpcm_encode(&s, stream.map_or(false, |s| s.1), bits)
            } else {
                mix.iter().flat_map(|v| (*v).clamp(-32768, 32767).to_le_bytes()[..2].to_vec()).collect()
            };
            out.push((19, block));
        }
        out
    }

    /// `list_of` fills the frame's display list and returns its other tags.
    fn timeline_with(&mut self, frames: u32, mut list_of: impl FnMut(&mut Ctx, u32, &mut Vec<Item>) -> Vec<(u16, Vec<u8>)>) -> Vec<u8> {
        let mut ctl = Vec::new();
        let mut prev: Vec<Item> = Vec::new();
        for f in 0..frames {
            let mut list = Vec::new();
            let actions = list_of(self, f, &mut list);
            for (i, it) in list.iter().enumerate() {
                let old = prev.get(i);
                if old == Some(it) {
                    continue;
                }
                place(&mut ctl, (i + 1) as u16, it, old);
            }
            for i in list.len()..prev.len() {
                tag(&mut ctl, 28, &((i + 1) as u16).to_le_bytes());
            }
            for (code, body) in actions {
                tag(&mut ctl, code, &body);
            }
            tag(&mut ctl, 1, &[]);
            prev = list;
        }
        tag(&mut ctl, 0, &[]);
        ctl
    }
}

/// PlaceObject2 (or PlaceObject3 if needed) of an item at a depth.
fn place(ctl: &mut Vec<u8>, depth: u16, it: &Item, old: Option<&Item>) {
    let moving = matches!(old, Some(o) if o.id == it.id && o.clip == it.clip && o.name == it.name);
    if old.is_some() && !moving {
        // The character or clipping changes: it is replaced at that depth.
        tag(ctl, 28, &depth.to_le_bytes());
    }
    let mut flags = 0x04u8; // HasMatrix
    if moving {
        flags |= 0x01;
    } else {
        flags |= 0x02;
        if it.clip > 0 {
            flags |= 0x40;
        }
        if !it.name.is_empty() {
            flags |= 0x20;
        }
        if !it.actions.is_empty() {
            flags |= 0x80;
        }
    }
    if it.cx.is_some() || (moving && old.map_or(false, |o| o.cx.is_some())) {
        flags |= 0x08;
    }
    let v3 = it.v3() || (moving && old.map_or(false, |o| o.v3()));
    let mut b = Bits::new();
    b.u8(flags);
    if v3 {
        b.u8(0x01 | 0x02 | 0x04); // filters, blending and caching
    }
    b.u16(depth);
    if flags & 0x02 != 0 {
        b.u16(it.id);
    }
    write_matrix(&mut b, &it.m);
    if flags & 0x08 != 0 {
        write_cxform(&mut b, &it.cx.unwrap_or(CxForm::ID));
    }
    if flags & 0x20 != 0 {
        put_string(&mut b, &it.name);
    }
    if flags & 0x40 != 0 {
        b.u16(it.clip);
    }
    if v3 {
        write_filters(&mut b, &it.filters);
        b.u8(it.blend.swf());
        b.u8(it.cache as u8);
    }
    if flags & 0x80 != 0 {
        b.flush();
        b.buf.extend_from_slice(&it.actions);
    }
    tag(ctl, if v3 { 70 } else { 26 }, &b.buf);
}

#[cfg(test)]
pub mod tests_support {
    /// Codes of the top-level tags of an uncompressed SWF.
    pub fn tag_codes(swf: &[u8]) -> Vec<u16> {
        let nbits = (swf[8] >> 3) as usize;
        let mut i = 8 + (5 + nbits * 4).div_ceil(8) + 4;
        let mut out = Vec::new();
        while i + 2 <= swf.len() {
            let h = u16::from_le_bytes([swf[i], swf[i + 1]]);
            i += 2;
            let mut len = (h & 0x3f) as usize;
            if len == 0x3f {
                len = u32::from_le_bytes(swf[i..i + 4].try_into().unwrap()) as usize;
                i += 4;
            }
            out.push(h >> 6);
            i += len;
        }
        out
    }
}

thread_local! {
    static LAST_ERRORS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// ActionScript errors from the last export (Output panel).
pub fn last_errors() -> Vec<String> {
    LAST_ERRORS.with(|e| e.borrow().clone())
}

/// Uncompressed SWF (for reading): decompresses a "CWS" movie.
pub fn uncompressed(swf: &[u8]) -> Vec<u8> {
    if swf.len() > 8 && &swf[..3] == b"CWS" {
        let mut out = b"FWS".to_vec();
        out.extend_from_slice(&swf[3..8]);
        out.extend_from_slice(&miniz_oxide::inflate::decompress_to_vec_zlib(&swf[8..]).unwrap_or_default());
        return out;
    }
    swf.to_vec()
}

pub fn export(doc: &Document) -> Vec<u8> {
    let doc = doc.closed();
    crate::symbols::set_library(&doc);
    // Scenes: name and first global frame (for gotoAndPlay("Scene 2", 1)).
    let mut scenes = Vec::new();
    let mut start = 0;
    for i in 0..doc.scenes.len().max(1) {
        let name = doc.scenes.get(i).map_or("Escena 1".to_string(), |s| s.name.clone());
        scenes.push((name, start));
        start += doc.scene_total(i);
    }
    let mut cx = Ctx {
        tags: Vec::new(),
        chars: HashMap::new(),
        symbols: HashMap::new(),
        next: 1,
        export_hidden: doc.export_hidden,
        as_ctx: doc.as_ctx(),
        errors: Vec::new(),
        place: String::new(),
        here: String::new(),
        sounds: doc.sounds.clone(),
        fps: doc.fps,
        jpegs: doc.bitmaps.iter().filter_map(|a| a.jpeg.as_ref().map(|j| (a.id, crate::bitmap::b64_decode(j)))).collect(),
    };
    // FileAttributes: mandatory from SWF 8 on (AS2, no network).
    tag(&mut cx.tags, 69, &0u32.to_le_bytes());
    tag(&mut cx.tags, 9, &[doc.bg.r, doc.bg.g, doc.bg.b]);
    let total = doc.movie_total();
    // Linkage: symbols exported for ActionScript are defined right in
    // the first frame (even if they are not on the Stage) with their name.
    let mut exports = Vec::new();
    let mut register = Vec::new();
    for sym in &doc.library {
        let Some(link) = &sym.linkage else { continue };
        if !matches!(sym.kind, SymbolKind::Movieclip | SymbolKind::Button | SymbolKind::Graphic) {
            continue;
        }
        let kind = if sym.kind == SymbolKind::Graphic { SymbolKind::Movieclip } else { sym.kind };
        if let Some(id) = cx.symbol_char(sym, kind, "") {
            exports.push((id, link.id.clone()));
            if !link.class.is_empty() && kind == SymbolKind::Movieclip {
                register.push((link.id.clone(), link.class.clone()));
            }
        }
    }
    // Classes (.as) and registerClass: init actions of the first frame.
    let (init, class_errs) = crate::as2::class::compile_classes(&cx.as_ctx, &register);
    for (file, e) in class_errs {
        cx.errors.push(format!("**Error** {file}: {e}"));
    }
    if !init.is_empty() {
        // An empty clip they are attached to (like "__Packages" in Flash).
        let pid = cx.alloc();
        let mut spr = pid.to_le_bytes().to_vec();
        spr.extend_from_slice(&1u16.to_le_bytes());
        tag(&mut spr, 1, &[]);
        tag(&mut spr, 0, &[]);
        tag(&mut cx.tags, 39, &spr);
        exports.push((pid, "__Packages.OMF".into()));
        let mut t = pid.to_le_bytes().to_vec();
        t.extend_from_slice(&init);
        // ExportAssets goes before the actions (registerClass looks up the names).
        let mut ex = Vec::new();
        ex.extend_from_slice(&(exports.len() as u16).to_le_bytes());
        for (id, name) in &exports {
            ex.extend_from_slice(&id.to_le_bytes());
            ex.extend_from_slice(name.as_bytes());
            ex.push(0);
        }
        tag(&mut cx.tags, 56, &ex);
        tag(&mut cx.tags, 59, &t);
    } else if !exports.is_empty() {
        let mut ex = Vec::new();
        ex.extend_from_slice(&(exports.len() as u16).to_le_bytes());
        for (id, name) in &exports {
            ex.extend_from_slice(&id.to_le_bytes());
            ex.extend_from_slice(name.as_bytes());
            ex.push(0);
        }
        tag(&mut cx.tags, 56, &ex);
    }
    // Scenes are exported one after another, as Flash did.
    let sets: Vec<&[Layer]> = (0..doc.scenes.len().max(1)).map(|i| doc.scene_layers(i)).collect();
    let stream = cx.stream_format(&sets);
    let ctl = cx.timeline_with(total, |c, g, out| {
        let (sc, f) = doc.movie_frame(g);
        c.place = format!("Escena={}", scenes.get(sc).map_or("Escena 1", |s| s.0.as_str()));
        let layers = doc.scene_layers(sc);
        c.layer_items(layers, f, &Matrix::IDENTITY, CxForm::ID, out);
        c.frame_tags(layers, f, stream, g == 0)
    });
    LAST_ERRORS.with(|e| *e.borrow_mut() = std::mem::take(&mut cx.errors));
    // Definitions go before any use; control tags after.
    let mut tags = Vec::new();
    if doc.publish.protect {
        // Protect with no password: Flash refuses to import the movie.
        tag(&mut tags, 24, &[]);
    }
    tags.extend_from_slice(&cx.tags);
    tags.extend_from_slice(&ctl);

    let mut hdr = Bits::new();
    write_rect(&mut hdr, 0, twips(doc.width), 0, twips(doc.height));
    hdr.u16((doc.fps.clamp(0.01, 255.0) * 256.0) as u16);
    hdr.u16(total.min(u16::MAX as u32) as u16);

    let len = 8 + hdr.buf.len() + tags.len();
    let mut body = hdr.buf;
    body.extend_from_slice(&tags);
    // "Compress movie": everything after the 8-byte header is zlib-compressed.
    let mut out = if doc.publish.compress { b"CWS".to_vec() } else { b"FWS".to_vec() };
    out.push(8);
    out.extend_from_slice(&(len as u32).to_le_bytes());
    if doc.publish.compress {
        out.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&body, 9));
    } else {
        out.extend_from_slice(&body);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_valid() {
        let mut doc = Document::new(550.0, 400.0);
        doc.layers[0].keys.get_mut(&0).unwrap().shapes.push(Shape {
            id: 1,
            geom: Geometry::Ellipse { w: 50.0, h: 30.0 },
            fill: Some(Rgba::BLACK),
            stroke: Some(Rgba::WHITE),
            stroke_width: 2.0,
            m: Matrix::translate(100.0, 100.0),
            ..Default::default()
        });
        let swf = uncompressed(&export(&doc));
        assert_eq!(&swf[0..3], b"FWS");
        assert_eq!(u32::from_le_bytes(swf[4..8].try_into().unwrap()) as usize, swf.len());
    }

    /// (code, body) of the top-level tags.
    fn tags_of(swf: &[u8]) -> Vec<(u16, Vec<u8>)> {
        let nbits = (swf[8] >> 3) as usize;
        let mut i = 8 + (5 + nbits * 4).div_ceil(8) + 4;
        let mut out = Vec::new();
        while i + 2 <= swf.len() {
            let h = u16::from_le_bytes([swf[i], swf[i + 1]]);
            i += 2;
            let mut len = (h & 0x3f) as usize;
            if len == 0x3f {
                len = u32::from_le_bytes(swf[i..i + 4].try_into().unwrap()) as usize;
                i += 4;
            }
            out.push((h >> 6, swf[i..i + len].to_vec()));
            i += len;
        }
        out
    }

    #[test]
    fn symbols_become_sprites_and_buttons() {
        use crate::symbols::*;
        let mut doc = Document::new(200.0, 200.0);
        let sq = Shape { id: 5, geom: Geometry::Rect { w: 20.0, h: 20.0, r: 0.0 }, fill: Some(Rgba::BLACK), ..Default::default() };
        let mut lay = Layer::new("Capa 1");
        lay.keys.get_mut(&0).unwrap().shapes.push(sq);
        lay.length = 3;
        let mut mc = Symbol { id: 100, name: "mc".into(), kind: SymbolKind::Movieclip, layers: vec![lay.clone()], folder: None, scale9: Some([2.0, 2.0, 8.0, 8.0]), linkage: None };
        mc.layers[0].length = 3;
        let bt = Symbol { id: 101, name: "bt".into(), kind: SymbolKind::Button, layers: vec![lay.clone()], folder: None, scale9: None, linkage: None };
        let gr = Symbol { id: 102, name: "gr".into(), kind: SymbolKind::Graphic, layers: vec![lay], folder: None, scale9: None, linkage: None };
        doc.library = vec![mc, bt, gr];
        let mut i1 = Inst::new(100, SymbolKind::Movieclip);
        i1.name = "caja".into();
        i1.filters.push(FilterEntry { on: true, f: Filter::new("dropshadow").unwrap() });
        i1.blend = Blend::Multiply;
        i1.color = ColorFx::Alpha { v: 50.0 };
        let shapes = &mut doc.layers[0].keys.get_mut(&0).unwrap().shapes;
        for (k, i) in [i1, Inst::new(101, SymbolKind::Button), Inst::new(102, SymbolKind::Graphic)].into_iter().enumerate() {
            shapes.push(Shape { id: 10 + k as u32, geom: Geometry::Instance(i), m: Matrix::translate(50.0 * k as f32, 0.0), ..Default::default() });
        }
        let swf = uncompressed(&export(&doc));
        assert_eq!(u32::from_le_bytes(swf[4..8].try_into().unwrap()) as usize, swf.len());
        let tags = tags_of(&swf);
        let codes: Vec<u16> = tags.iter().map(|t| t.0).collect();
        assert_eq!(codes.iter().filter(|c| **c == 39).count(), 1, "un DefineSprite");
        assert_eq!(codes.iter().filter(|c| **c == 34).count(), 1, "un DefineButton2");
        assert_eq!(codes.iter().filter(|c| **c == 78).count(), 1, "DefineScalingGrid");
        assert_eq!(codes.iter().filter(|c| **c == 70).count(), 1, "PlaceObject3 para el clip con filtro");
        // The graphic is inlined: its shape is placed directly.
        assert!(codes.iter().filter(|c| **c == 26).count() >= 2);
        let po3 = &tags.iter().find(|t| t.0 == 70).unwrap().1;
        assert_eq!(po3[0] & 0x2a, 0x2a, "carácter, nombre y efecto de color");
        assert!(po3.windows(5).any(|w| w == b"caja\0"));
        // The sprite has 3 frames and ends with End.
        let sp = &tags.iter().find(|t| t.0 == 39).unwrap().1;
        assert_eq!(u16::from_le_bytes([sp[2], sp[3]]), 3);
        assert_eq!(&sp[sp.len() - 2..], &[0, 0]);
    }

    /// Sample movie to inspect by eye in Ruffle: `OMF_SWF_OUT=path cargo test muestra -- --ignored`.
    #[test]
    #[ignore]
    fn muestra_simbolos() {
        use crate::symbols::*;
        let mut doc = Document::new(550.0, 400.0);
        let mk = |w: f32, h: f32, c: Rgba| Shape { id: 1, geom: Geometry::Rect { w, h, r: 0.0 }, fill: Some(c), stroke: Some(Rgba::BLACK), stroke_width: 2.0, ..Default::default() };
        let red = Rgba { r: 220, g: 40, b: 40, a: 255 };
        let mut spin = Layer::new("Capa 1");
        spin.keys.get_mut(&0).unwrap().shapes.push(mk(60.0, 60.0, red));
        spin.keys.get_mut(&0).unwrap().tween = true;
        let mut k = spin.keys[&0].clone();
        k.shapes[0].m = Matrix::translate(80.0, 0.0);
        k.tween = false;
        spin.keys.insert(23, k);
        spin.length = 24;
        let mut panel = Layer::new("Capa 1");
        panel.keys.get_mut(&0).unwrap().shapes.push(Shape { geom: Geometry::Rect { w: 100.0, h: 60.0, r: 20.0 }, ..mk(0.0, 0.0, Rgba { r: 60, g: 160, b: 90, a: 255 }) });
        let mut btn = Layer::new("Capa 1");
        for (f, c) in [(0u32, Rgba { r: 40, g: 90, b: 220, a: 255 }), (1, Rgba { r: 250, g: 200, b: 40, a: 255 }), (2, red)] {
            let mut kf = Keyframe::default();
            kf.shapes.push(Shape { geom: Geometry::Ellipse { w: 80.0, h: 80.0 }, ..mk(0.0, 0.0, c) });
            btn.keys.insert(f, kf);
        }
        btn.length = 3;
        doc.library = vec![
            Symbol { id: 100, name: "mc".into(), kind: SymbolKind::Movieclip, layers: vec![spin], folder: None, scale9: None, linkage: None },
            Symbol { id: 101, name: "panel".into(), kind: SymbolKind::Movieclip, layers: vec![panel], folder: None, scale9: Some([-30.0, -10.0, 30.0, 10.0]), linkage: None },
            Symbol { id: 102, name: "bt".into(), kind: SymbolKind::Button, layers: vec![btn], folder: None, scale9: None, linkage: None },
        ];
        let mut a = Inst::new(100, SymbolKind::Movieclip);
        a.filters.push(FilterEntry { on: true, f: Filter::new("dropshadow").unwrap() });
        let mut b = Inst::new(100, SymbolKind::Movieclip);
        b.color = ColorFx::Tint { color: Rgba { r: 0, g: 0, b: 255, a: 255 }, amount: 60.0 };
        b.filters.push(FilterEntry { on: true, f: Filter::new("blur").unwrap() });
        let mut g = Inst::new(100, SymbolKind::Graphic);
        g.color = ColorFx::Alpha { v: 40.0 };
        let p = Inst::new(101, SymbolKind::Movieclip);
        let shapes = &mut doc.layers[0].keys.get_mut(&0).unwrap().shapes;
        let at = |i: Inst, m: Matrix| Shape { id: 0, geom: Geometry::Instance(i), m, ..Default::default() };
        shapes.push(at(a, Matrix::translate(80.0, 80.0)));
        shapes.push(at(b, Matrix::translate(80.0, 180.0)));
        shapes.push(at(g, Matrix::translate(80.0, 280.0)));
        shapes.push(at(p, Matrix::translate(380.0, 100.0).mul(&Matrix::scale(2.5, 1.5))));
        shapes.push(at(Inst::new(102, SymbolKind::Button), Matrix::translate(400.0, 280.0)));
        // Block 4: gradients and text.
        use crate::grad::{GradKind, Gradient, Stop};
        let stops = vec![Stop { t: 0.0, c: Rgba { r: 255, g: 220, b: 0, a: 255 } }, Stop { t: 1.0, c: Rgba { r: 200, g: 0, b: 80, a: 255 } }];
        let mut lin = Shape::new(Geometry::Rect { w: 120.0, h: 40.0, r: 0.0 }, Some(stops[0].c), None, 1.0, Matrix::translate(250.0, 40.0));
        lin.grad = Some(Gradient { kind: GradKind::Linear, stops: stops.clone(), m: Matrix::scale(60.0, 20.0), ..Gradient::default_of(GradKind::Linear) });
        shapes.push(lin);
        let mut rad = Shape::new(Geometry::Ellipse { w: 80.0, h: 80.0 }, Some(Rgba::WHITE), None, 1.0, Matrix::translate(250.0, 140.0));
        rad.grad = Some(Gradient { kind: GradKind::Radial, m: Matrix::scale(40.0, 40.0), focal: 0.6, ..Gradient::default_of(GradKind::Radial) });
        shapes.push(rad);
        let tx = |t: crate::text::TextField, x: f32, y: f32| Shape::new(Geometry::Text(t), Some(Rgba { r: 20, g: 40, b: 160, a: 255 }), None, 1.0, Matrix::translate(x, y));
        shapes.push(tx(crate::text::TextField { text: "Estático ñ".into(), size: 28.0, ..Default::default() }, 180.0, 330.0));
        shapes.push(tx(
            crate::text::TextField { text: "Dinámico".into(), size: 20.0, kind: crate::text::TextKind::Dynamic, border: true, width: Some(140.0), ..Default::default() },
            380.0,
            360.0,
        ));
        // Bitmap: scaled instance and tiled fill.
        let mut img = tiny_skia::Pixmap::new(16, 16).unwrap();
        img.fill(tiny_skia::Color::from_rgba8(255, 140, 0, 255));
        img.fill_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, 8.0, 8.0).unwrap(), &crate::render::paint(Rgba { r: 0, g: 120, b: 255, a: 255 }, 1.0), tiny_skia::Transform::identity(), None);
        let png = img.encode_png().unwrap();
        doc.bitmaps.push(crate::bitmap::BitmapAsset { id: 900, w: 16, h: 16, png: crate::bitmap::b64_encode(&png), jpeg: None, quality: None });
        let layers = crate::bitmap::bitmap_layers(&mut doc, 900, 16, 16);
        doc.library.push(Symbol { id: 103, name: "img".into(), kind: SymbolKind::Bitmap, layers, folder: None, scale9: None, linkage: None });
        let shapes = &mut doc.layers[0].keys.get_mut(&0).unwrap().shapes;
        shapes.push(at(Inst::new(103, SymbolKind::Bitmap), Matrix::translate(20.0, 340.0).mul(&Matrix::scale(3.0, 3.0))));
        let mut tile = Shape::new(Geometry::Rect { w: 100.0, h: 50.0, r: 0.0 }, Some(Rgba::WHITE), Some(Rgba::BLACK), 1.0, Matrix::translate(440.0, 40.0));
        tile.bmp = Some(crate::bitmap::BitmapFill { id: 900, m: Matrix::translate(-50.0, -25.0), clip: false, smooth: false, cx: None });
        shapes.push(tile);
        doc.fix_ids();
        doc.layers[0].length = 24;
        let out = std::env::var("OMF_SWF_OUT").unwrap_or_else(|_| "muestra.swf".into());
        std::fs::write(&out, export(&doc)).unwrap();
        crate::symbols::set_library(&doc);
        crate::render::stage_pixmap(&doc, 0, 1.0).unwrap().save_png(out.replace(".swf", ".png")).unwrap();
    }

    #[test]
    fn signed_bits() {
        assert_eq!(sbits(0), 1);
        assert_eq!(sbits(1), 2);
        assert_eq!(sbits(-1), 1);
        assert_eq!(sbits(-2), 2);
        assert_eq!(sbits(11000), 15);
    }
}
