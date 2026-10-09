//! Flash symbols: graphics, buttons and movie clips; their instances
//! with color effects, filters (Flash 8), blend modes, bitmap caching
//! and 9-slice scaling; and the Library.
//!
//! Shapes query the Library through a per-thread registry
//! (`set_library`) that the editor keeps in sync with the document; that way
//! bounds, hit-testing and group flattening work the same with
//! instances as with shapes.

use crate::geom::*;
use crate::model::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    #[default]
    Graphic,
    Button,
    Movieclip,
    /// Imported bitmap.
    Bitmap,
    /// Imported sound.
    Sound,
}

impl SymbolKind {
    pub fn parse(s: &str) -> SymbolKind {
        match s {
            "button" => SymbolKind::Button,
            "movieclip" => SymbolKind::Movieclip,
            "bitmap" => SymbolKind::Bitmap,
            "sound" => SymbolKind::Sound,
            _ => SymbolKind::Graphic,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            SymbolKind::Graphic => "graphic",
            SymbolKind::Button => "button",
            SymbolKind::Movieclip => "movieclip",
            SymbolKind::Bitmap => "bitmap",
            SymbolKind::Sound => "sound",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            SymbolKind::Graphic => "Gráfico",
            SymbolKind::Button => "Botón",
            SymbolKind::Movieclip => "Clip de película",
            SymbolKind::Bitmap => "Mapa de bits",
            SymbolKind::Sound => "Sonido",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Symbol {
    pub id: u32,
    pub name: String,
    pub kind: SymbolKind,
    pub layers: Vec<Layer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<u32>,
    /// 9-slice scaling guides (x0, y0, x1, y1) in symbol coordinates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale9: Option<[f32; 4]>,
    /// Linkage (Export for ActionScript): identifier and AS 2.0 class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linkage: Option<Linkage>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Linkage {
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub class: String,
}

impl Symbol {
    pub fn total(&self) -> u32 {
        self.layers.iter().map(|l| l.length).max().unwrap_or(1).max(1)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LibFolder {
    pub id: u32,
    pub name: String,
    #[serde(default = "yes_")]
    pub open: bool,
}

fn yes_() -> bool {
    true
}

/// Playback of a graphic (Flash Properties).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LoopMode {
    #[default]
    Loop,
    Once,
    Single,
}

/// Color effect of an instance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum ColorFx {
    #[default]
    None,
    /// -100 … 100
    Brightness { v: f32 },
    /// Color and amount 0 … 100
    Tint { color: Rgba, amount: f32 },
    /// 0 … 100
    Alpha { v: f32 },
    /// Red %, red +, green %, green +, blue %, blue +, alpha %, alpha +
    Advanced { m: [f32; 8] },
}

/// Color transform: `c' = c·mul + add` per channel (RGBA, 0…255).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CxForm {
    pub mul: [f32; 4],
    pub add: [f32; 4],
}

impl CxForm {
    pub const ID: CxForm = CxForm { mul: [1.0; 4], add: [0.0; 4] };

    pub fn is_identity(&self) -> bool {
        *self == CxForm::ID
    }

    /// Applies `inner` first and then `self`.
    pub fn compose(&self, inner: &CxForm) -> CxForm {
        let mut r = CxForm::ID;
        for i in 0..4 {
            r.mul[i] = inner.mul[i] * self.mul[i];
            r.add[i] = inner.add[i] * self.mul[i] + self.add[i];
        }
        r
    }

    pub fn apply(&self, c: Rgba) -> Rgba {
        let ch = |v: u8, i: usize| (v as f32 * self.mul[i] + self.add[i]).round().clamp(0.0, 255.0) as u8;
        Rgba { r: ch(c.r, 0), g: ch(c.g, 1), b: ch(c.b, 2), a: ch(c.a, 3) }
    }

    pub fn lerp(&self, o: &CxForm, t: f32) -> CxForm {
        let mut r = CxForm::ID;
        for i in 0..4 {
            r.mul[i] = lerp(self.mul[i], o.mul[i], t);
            r.add[i] = lerp(self.add[i], o.add[i], t);
        }
        r
    }
}

impl ColorFx {
    pub fn cxform(&self) -> CxForm {
        match self {
            ColorFx::None => CxForm::ID,
            ColorFx::Brightness { v } => {
                let b = (v / 100.0).clamp(-1.0, 1.0);
                if b >= 0.0 {
                    CxForm { mul: [1.0 - b, 1.0 - b, 1.0 - b, 1.0], add: [255.0 * b, 255.0 * b, 255.0 * b, 0.0] }
                } else {
                    CxForm { mul: [1.0 + b, 1.0 + b, 1.0 + b, 1.0], add: [0.0; 4] }
                }
            }
            ColorFx::Tint { color, amount } => {
                let a = (amount / 100.0).clamp(0.0, 1.0);
                CxForm { mul: [1.0 - a, 1.0 - a, 1.0 - a, 1.0], add: [color.r as f32 * a, color.g as f32 * a, color.b as f32 * a, 0.0] }
            }
            ColorFx::Alpha { v } => CxForm { mul: [1.0, 1.0, 1.0, (v / 100.0).clamp(0.0, 1.0)], add: [0.0; 4] },
            ColorFx::Advanced { m } => CxForm { mul: [m[0] / 100.0, m[2] / 100.0, m[4] / 100.0, m[6] / 100.0], add: [m[1], m[3], m[5], m[7]] },
        }
    }

    pub fn lerp(&self, o: &ColorFx, t: f32) -> ColorFx {
        if self == o {
            return self.clone();
        }
        let (a, b) = (self.cxform(), o.cxform());
        let c = a.lerp(&b, t);
        ColorFx::Advanced { m: [c.mul[0] * 100.0, c.add[0], c.mul[1] * 100.0, c.add[1], c.mul[2] * 100.0, c.add[2], c.mul[3] * 100.0, c.add[3]] }
    }
}

/// Flash 8 blend modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Blend {
    #[default]
    Normal,
    Layer,
    Darken,
    Multiply,
    Lighten,
    Screen,
    Overlay,
    Hardlight,
    Add,
    Subtract,
    Difference,
    Invert,
    Alpha,
    Erase,
}

impl Blend {
    pub fn parse(s: &str) -> Blend {
        serde_json::from_value(serde_json::Value::String(s.into())).unwrap_or_default()
    }
    /// Value of the SWF BlendMode field.
    pub fn swf(self) -> u8 {
        match self {
            Blend::Normal => 1,
            Blend::Layer => 2,
            Blend::Multiply => 3,
            Blend::Screen => 4,
            Blend::Lighten => 5,
            Blend::Darken => 6,
            Blend::Difference => 7,
            Blend::Add => 8,
            Blend::Subtract => 9,
            Blend::Invert => 10,
            Blend::Alpha => 11,
            Blend::Erase => 12,
            Blend::Overlay => 13,
            Blend::Hardlight => 14,
        }
    }
}

/// Flash 8 filters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Filter {
    Dropshadow {
        color: Rgba,
        blur_x: f32,
        blur_y: f32,
        /// Strength in % (100 = normal).
        strength: f32,
        /// Degrees.
        angle: f32,
        distance: f32,
        quality: u8,
        inner: bool,
        knockout: bool,
        hide: bool,
    },
    Blur {
        blur_x: f32,
        blur_y: f32,
        quality: u8,
    },
    Glow {
        color: Rgba,
        blur_x: f32,
        blur_y: f32,
        strength: f32,
        quality: u8,
        inner: bool,
        knockout: bool,
    },
    Bevel {
        shadow: Rgba,
        highlight: Rgba,
        blur_x: f32,
        blur_y: f32,
        strength: f32,
        angle: f32,
        distance: f32,
        quality: u8,
        knockout: bool,
        /// "inner" | "outer" | "full"
        bevel: String,
    },
    Adjustcolor {
        brightness: f32,
        contrast: f32,
        saturation: f32,
        hue: f32,
    },
}

impl Filter {
    /// Flash 8 default values when adding a filter.
    pub fn new(kind: &str) -> Option<Filter> {
        Some(match kind {
            "dropshadow" => Filter::Dropshadow {
                color: Rgba::BLACK,
                blur_x: 5.0,
                blur_y: 5.0,
                strength: 100.0,
                angle: 45.0,
                distance: 5.0,
                quality: 1,
                inner: false,
                knockout: false,
                hide: false,
            },
            "blur" => Filter::Blur { blur_x: 5.0, blur_y: 5.0, quality: 1 },
            "glow" => Filter::Glow {
                color: Rgba { r: 255, g: 0, b: 0, a: 255 },
                blur_x: 5.0,
                blur_y: 5.0,
                strength: 100.0,
                quality: 1,
                inner: false,
                knockout: false,
            },
            "bevel" => Filter::Bevel {
                shadow: Rgba::BLACK,
                highlight: Rgba::WHITE,
                blur_x: 5.0,
                blur_y: 5.0,
                strength: 100.0,
                angle: 45.0,
                distance: 5.0,
                quality: 1,
                knockout: false,
                bevel: "inner".into(),
            },
            "adjustcolor" => Filter::Adjustcolor { brightness: 0.0, contrast: 0.0, saturation: 0.0, hue: 0.0 },
            _ => return None,
        })
    }

    /// Margin (symbol pixels) the filter can take up outside the object.
    pub fn pad(&self) -> f32 {
        match self {
            Filter::Dropshadow { blur_x, blur_y, distance, .. } => blur_x.max(*blur_y) * 2.0 + distance.abs(),
            Filter::Blur { blur_x, blur_y, .. } | Filter::Glow { blur_x, blur_y, .. } => blur_x.max(*blur_y) * 2.0,
            Filter::Bevel { blur_x, blur_y, distance, .. } => blur_x.max(*blur_y) * 2.0 + distance.abs(),
            Filter::Adjustcolor { .. } => 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FilterEntry {
    #[serde(default = "yes_")]
    pub on: bool,
    #[serde(flatten)]
    pub f: Filter,
}

/// Instance of a symbol placed on a timeline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Inst {
    pub symbol: u32,
    /// Instance behavior (may differ from the symbol's type).
    #[serde(default)]
    pub kind: SymbolKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default)]
    pub color: ColorFx,
    #[serde(default)]
    pub loop_mode: LoopMode,
    /// First frame (graphics).
    #[serde(default)]
    pub first: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<FilterEntry>,
    #[serde(default)]
    pub blend: Blend,
    #[serde(default)]
    pub cache: bool,
    /// Instance actions: on (…) on buttons; onClipEvent (…) and on (…) on clips.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub actions: String,
}

impl Inst {
    pub fn new(symbol: u32, kind: SymbolKind) -> Inst {
        Inst {
            symbol,
            kind,
            name: String::new(),
            color: ColorFx::None,
            loop_mode: LoopMode::Loop,
            first: 0,
            filters: Vec::new(),
            blend: Blend::Normal,
            cache: false,
            actions: String::new(),
        }
    }

    pub fn active_filters(&self) -> impl Iterator<Item = &Filter> {
        self.filters.iter().filter(|f| f.on).map(|f| &f.f)
    }

    pub fn needs_layer(&self) -> bool {
        self.active_filters().next().is_some() || !matches!(self.blend, Blend::Normal | Blend::Layer)
    }

    pub fn lerp(&self, o: &Inst, t: f32) -> Inst {
        let mut r = self.clone();
        r.color = self.color.lerp(&o.color, t);
        // Filters matching in number and type: their parameters are interpolated (like Flash 8).
        if self.filters.len() == o.filters.len() {
            r.filters = self.filters.iter().zip(&o.filters).map(|(a, b)| FilterEntry { on: a.on, f: lerp_filter(&a.f, &b.f, t) }).collect();
        }
        r
    }
}

fn lerp_filter(a: &Filter, b: &Filter, t: f32) -> Filter {
    let l = |x: f32, y: f32| lerp(x, y, t);
    let c = |x: &Rgba, y: &Rgba| x.lerp(y, t);
    match (a, b) {
        (
            Filter::Dropshadow { color, blur_x, blur_y, strength, angle, distance, quality, inner, knockout, hide },
            Filter::Dropshadow { color: c2, blur_x: bx2, blur_y: by2, strength: s2, angle: a2, distance: d2, .. },
        ) => Filter::Dropshadow {
            color: c(color, c2),
            blur_x: l(*blur_x, *bx2),
            blur_y: l(*blur_y, *by2),
            strength: l(*strength, *s2),
            angle: l(*angle, *a2),
            distance: l(*distance, *d2),
            quality: *quality,
            inner: *inner,
            knockout: *knockout,
            hide: *hide,
        },
        (Filter::Blur { blur_x, blur_y, quality }, Filter::Blur { blur_x: bx2, blur_y: by2, .. }) => {
            Filter::Blur { blur_x: l(*blur_x, *bx2), blur_y: l(*blur_y, *by2), quality: *quality }
        }
        (
            Filter::Glow { color, blur_x, blur_y, strength, quality, inner, knockout },
            Filter::Glow { color: c2, blur_x: bx2, blur_y: by2, strength: s2, .. },
        ) => Filter::Glow {
            color: c(color, c2),
            blur_x: l(*blur_x, *bx2),
            blur_y: l(*blur_y, *by2),
            strength: l(*strength, *s2),
            quality: *quality,
            inner: *inner,
            knockout: *knockout,
        },
        (
            Filter::Adjustcolor { brightness, contrast, saturation, hue },
            Filter::Adjustcolor { brightness: b2, contrast: c2, saturation: s2, hue: h2 },
        ) => Filter::Adjustcolor { brightness: l(*brightness, *b2), contrast: l(*contrast, *c2), saturation: l(*saturation, *s2), hue: l(*hue, *h2) },
        _ => a.clone(),
    }
}

/// 4×5 color matrix of "Adjust Color" (brightness, contrast, saturation, hue),
/// in the format of the SWF ColorMatrixFilter.
pub fn adjust_color_matrix(brightness: f32, contrast: f32, saturation: f32, hue: f32) -> [f32; 20] {
    let mul = |a: [f32; 20], b: [f32; 20]| -> [f32; 20] {
        // Composition of 4×5 matrices (b after a).
        let mut r = [0.0; 20];
        for row in 0..4 {
            for col in 0..5 {
                let mut v = 0.0;
                for k in 0..4 {
                    v += b[row * 5 + k] * a[k * 5 + col];
                }
                if col == 4 {
                    v += b[row * 5 + 4];
                }
                r[row * 5 + col] = v;
            }
        }
        r
    };
    let ident = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let b = brightness.clamp(-100.0, 100.0) * 2.55;
    let bm = [1.0, 0.0, 0.0, 0.0, b, 0.0, 1.0, 0.0, 0.0, b, 0.0, 0.0, 1.0, 0.0, b, 0.0, 0.0, 0.0, 1.0, 0.0];
    let c = 1.0 + contrast.clamp(-100.0, 100.0) / 100.0;
    let o = 128.0 * (1.0 - c);
    let cm = [c, 0.0, 0.0, 0.0, o, 0.0, c, 0.0, 0.0, o, 0.0, 0.0, c, 0.0, o, 0.0, 0.0, 0.0, 1.0, 0.0];
    let s = 1.0 + saturation.clamp(-100.0, 100.0) / 100.0;
    let (lr, lg, lb) = (0.3086, 0.6094, 0.0820);
    let sm = [
        lr * (1.0 - s) + s, lg * (1.0 - s), lb * (1.0 - s), 0.0, 0.0,
        lr * (1.0 - s), lg * (1.0 - s) + s, lb * (1.0 - s), 0.0, 0.0,
        lr * (1.0 - s), lg * (1.0 - s), lb * (1.0 - s) + s, 0.0, 0.0,
        0.0, 0.0, 0.0, 1.0, 0.0,
    ];
    let h = hue.clamp(-180.0, 180.0).to_radians();
    let (cs, sn) = (h.cos(), h.sin());
    let hm = [
        lr + cs * (1.0 - lr) + sn * (-lr), lg + cs * (-lg) + sn * (-lg), lb + cs * (-lb) + sn * (1.0 - lb), 0.0, 0.0,
        lr + cs * (-lr) + sn * 0.143, lg + cs * (1.0 - lg) + sn * 0.14, lb + cs * (-lb) + sn * (-0.283), 0.0, 0.0,
        lr + cs * (-lr) + sn * (-(1.0 - lr)), lg + cs * (-lg) + sn * lg, lb + cs * (1.0 - lb) + sn * lb, 0.0, 0.0,
        0.0, 0.0, 0.0, 1.0, 0.0,
    ];
    let mut m = ident;
    for x in [bm, cm, sm, hm] {
        m = mul(m, x);
    }
    m
}

// ───────────────────────── Library registry ─────────────────────────

thread_local! {
    static LIBRARY: RefCell<Rc<HashMap<u32, Rc<Symbol>>>> = RefCell::new(Rc::new(HashMap::new()));
}

/// Publishes the document's Library for instance queries.
/// The symbol being edited has its layers in `doc.layers` (and those of
/// higher levels, in the edit stack).
pub fn set_library(doc: &Document) {
    crate::bitmap::sync(doc);
    let mut map: HashMap<u32, Rc<Symbol>> = HashMap::new();
    for s in &doc.library {
        map.insert(s.id, Rc::new(s.clone()));
    }
    for (i, e) in doc.sym_edit.iter().enumerate() {
        let layers = match doc.sym_edit.get(i + 1) {
            Some(next) => next.parent_layers.clone(),
            None => doc.layers.clone(),
        };
        if let Some(s) = map.get(&e.symbol).map(|s| (**s).clone()) {
            map.insert(e.symbol, Rc::new(Symbol { layers, ..s }));
        }
    }
    LIBRARY.with(|l| *l.borrow_mut() = Rc::new(map));
}

pub fn symbol(id: u32) -> Option<Rc<Symbol>> {
    LIBRARY.with(|l| l.borrow().get(&id).cloned())
}

/// Symbol frame shown by an instance, `offset` frames
/// after the keyframe where it was placed. In the editor, clips show their
/// first frame, as in Flash.
pub fn symbol_frame(sym: &Symbol, inst: &Inst, offset: u32, editor: bool) -> u32 {
    let len = sym.total();
    match inst.kind {
        SymbolKind::Graphic => match inst.loop_mode {
            LoopMode::Loop => (inst.first + offset) % len,
            LoopMode::Once => (inst.first + offset).min(len - 1),
            LoopMode::Single => inst.first.min(len - 1),
        },
        SymbolKind::Movieclip => {
            if editor {
                0
            } else {
                offset % len
            }
        }
        SymbolKind::Button | SymbolKind::Bitmap | SymbolKind::Sound => 0,
    }
}

/// Shapes seen by a symbol layer (bottom to top), without guides or folders.
pub fn symbol_shapes(sym: &Symbol, frame: u32) -> Vec<Shape> {
    let mut out = Vec::new();
    for li in (0..sym.layers.len()).rev() {
        if matches!(sym.layers[li].kind, LayerKind::Folder | LayerKind::Guide | LayerKind::MotionGuide | LayerKind::Mask) {
            continue;
        }
        out.extend(crate::timeline::layer_shapes(&sym.layers, li, frame));
    }
    out
}

thread_local! {
    static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Final shapes of an instance (with the accumulated matrix and color
/// effect), `offset` frames after its keyframe.
pub fn instance_leaves(s: &Shape, inst: &Inst, parent: &Matrix, offset: u32, editor: bool) -> Vec<Shape> {
    let Some(sym) = symbol(inst.symbol) else { return Vec::new() };
    // A symbol that contains itself must not hang the program.
    if DEPTH.with(|d| d.get()) > 24 {
        return Vec::new();
    }
    DEPTH.with(|d| d.set(d.get() + 1));
    let frame = symbol_frame(&sym, inst, offset, editor);
    let mut m = parent.mul(&s.m);
    let mut shapes = symbol_shapes(&sym, frame);
    if let (SymbolKind::Movieclip, Some(g)) = (inst.kind, sym.scale9) {
        let d = s.m.decompose();
        if (d.sx - 1.0).abs() > 1e-3 || (d.sy - 1.0).abs() > 1e-3 {
            let flat: Vec<Shape> = shapes.iter().flat_map(|c| c.leaves(&Matrix::IDENTITY)).collect();
            shapes = scale9_warp(&flat, g, d.sx.abs(), d.sy.abs());
            let unscaled = Matrix::compose(&Decomp { sx: d.sx.signum(), sy: d.sy.signum(), k: 0.0, ..d });
            m = parent.mul(&unscaled);
        }
    }
    let cx = inst.color.cxform();
    let out = shapes
        .iter()
        .flat_map(|c| c.leaves(&m))
        .map(|mut l| {
            l.apply_cx(&cx);
            l
        })
        .collect();
    DEPTH.with(|d| d.set(d.get() - 1));
    out
}

/// Applies 9-slice scaling: the corners are not distorted.
pub fn scale9_warp(shapes: &[Shape], grid: [f32; 4], sx: f32, sy: f32) -> Vec<Shape> {
    let (gx0, gy0, gx1, gy1) = (grid[0], grid[1], grid[2], grid[3]);
    let all = shapes.iter().fold(BBox::EMPTY, |b, s| b.union(&s.world_bounds()));
    if all.is_empty() {
        return shapes.to_vec();
    }
    // Unscaled result (the final matrix no longer carries it): the spans [lo, a] and
    // [b, hi] keep their size, the middle one absorbs the difference and the left
    // edge shifts as it would when scaling from the registration point.
    let warp = |v: f32, lo: f32, hi: f32, a: f32, b: f32, k: f32| -> f32 {
        let (a, b) = (a.clamp(lo, hi), b.clamp(lo, hi));
        let (left, right) = (a - lo, hi - b);
        let mid_new = ((hi - lo) * k - left - right).max(0.0);
        let mid_old = (b - a).max(1e-6);
        if v <= a {
            lo * k + (v - lo)
        } else if v >= b {
            lo * k + left + mid_new + (v - b)
        } else {
            lo * k + left + (v - a) / mid_old * mid_new
        }
    };
    shapes
        .iter()
        .map(|s| {
            let mut s = s.clone();
            let m = s.m;
            s.ensure_bezier();
            if let Geometry::Bezier { paths } = &mut s.geom {
                for p in paths.iter_mut() {
                    for n in p.nodes.iter_mut() {
                        for q in [&mut n.p, &mut n.i, &mut n.o] {
                            let w = m.apply(q.0, q.1);
                            *q = (warp(w.0, all.x0, all.x1, gx0, gx1, sx), warp(w.1, all.y0, all.y1, gy0, gy1, sy));
                        }
                    }
                }
            }
            s.m = Matrix::IDENTITY;
            s
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_effects() {
        let red = Rgba { r: 255, g: 0, b: 0, a: 255 };
        let t = ColorFx::Tint { color: Rgba { r: 0, g: 0, b: 255, a: 255 }, amount: 100.0 }.cxform().apply(red);
        assert_eq!((t.r, t.g, t.b), (0, 0, 255));
        let b = ColorFx::Brightness { v: 100.0 }.cxform().apply(red);
        assert_eq!((b.r, b.g, b.b), (255, 255, 255));
        let a = ColorFx::Alpha { v: 50.0 }.cxform().apply(red);
        assert_eq!(a.a, 128);
        let half = ColorFx::None.lerp(&ColorFx::Alpha { v: 0.0 }, 0.5).cxform().apply(red);
        assert_eq!(half.a, 128);
    }

    #[test]
    fn adjust_color_identity_and_desaturate() {
        let m = adjust_color_matrix(0.0, 0.0, 0.0, 0.0);
        for (i, v) in m.iter().enumerate() {
            let want = if [0, 6, 12, 18].contains(&i) { 1.0 } else { 0.0 };
            assert!((v - want).abs() < 1e-4, "{i}: {v}");
        }
        let g = adjust_color_matrix(0.0, 0.0, -100.0, 0.0);
        // Gray: the three rows are equal.
        assert!((g[0] - g[5]).abs() < 1e-4 && (g[1] - g[6]).abs() < 1e-4);
    }

    #[test]
    fn filters_serialize_with_kind() {
        let f = FilterEntry { on: true, f: Filter::new("glow").unwrap() };
        let s = serde_json::to_string(&f).unwrap();
        assert!(s.contains("\"kind\":\"glow\""));
        let back: FilterEntry = serde_json::from_str(&s).unwrap();
        assert_eq!(back, f);
    }
}
