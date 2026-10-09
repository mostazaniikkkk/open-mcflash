//! Open McFlash (OMF) — a vector animation editor inspired by Flash 8, with its core
//! in Rust compiled to WebAssembly. The interface (HTML/JS) only translates events
//! and paints panels; the document, the tools, rendering and
//! export live here.

// The UI state object is large for the `json!` macro.
#![recursion_limit = "512"]

pub mod fit;
pub mod geom;
pub mod bitmap;
pub mod grad;
pub mod model;
pub mod morph;
pub mod raw;
pub mod timeline;
pub mod paint;
pub mod render;
pub mod swf;
pub mod symbols;
pub mod text;
pub mod filters;
mod drawing;
mod fill_tools;
mod shape_tools;
mod text_editor;
pub mod guides;
pub mod as2;
mod words;
mod actions;
pub mod sound;
mod sym_editor;
mod tl_editor;
mod tools;

use geom::*;
use model::*;
use paint::Tip;
use serde_json::{json, Value};
use tiny_skia::Pixmap;
use wasm_bindgen::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tool {
    Select,
    Subselect,
    Transform,
    FillXform,
    Line,
    Lasso,
    Pen,
    Text,
    Pencil,
    Brush,
    Rect,
    Oval,
    PolyStar,
    Bucket,
    Ink,
    Eyedropper,
    Eraser,
    Hand,
    Zoom,
}

const TOOLS: [(Tool, &str); 19] = [
    (Tool::Select, "select"),
    (Tool::Subselect, "subselect"),
    (Tool::Transform, "transform"),
    (Tool::FillXform, "filltransform"),
    (Tool::Line, "line"),
    (Tool::Lasso, "lasso"),
    (Tool::Text, "text"),
    (Tool::PolyStar, "polystar"),
    (Tool::Pen, "pen"),
    (Tool::Pencil, "pencil"),
    (Tool::Brush, "brush"),
    (Tool::Rect, "rect"),
    (Tool::Oval, "oval"),
    (Tool::Bucket, "bucket"),
    (Tool::Ink, "ink"),
    (Tool::Eyedropper, "eyedropper"),
    (Tool::Eraser, "eraser"),
    (Tool::Hand, "hand"),
    (Tool::Zoom, "zoom"),
];

impl Tool {
    fn name(self) -> &'static str {
        TOOLS.iter().find(|(t, _)| *t == self).map(|(_, n)| *n).unwrap_or("select")
    }
    fn parse(s: &str) -> Option<Tool> {
        TOOLS.iter().find(|(_, n)| *n == s).map(|(t, _)| *t)
    }
}

/// Tool modes, with the same names as in Flash 8.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PencilMode {
    Straighten,
    Smooth,
    Ink,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BrushMode {
    Normal,
    Fills,
    Behind,
    Selection,
    Inside,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EraserMode {
    Normal,
    Fills,
    Lines,
    SelectedFills,
    Inside,
}

macro_rules! named_enum {
    ($t:ty, $($v:ident = $s:literal),*) => {
        impl $t {
            fn name(self) -> &'static str { match self { $(Self::$v => $s),* } }
            fn parse(s: &str) -> Option<Self> { match s { $($s => Some(Self::$v),)* _ => None } }
        }
    };
}
named_enum!(PencilMode, Straighten = "straighten", Smooth = "smooth", Ink = "ink");
named_enum!(BrushMode, Normal = "normal", Fills = "fills", Behind = "behind", Selection = "selection", Inside = "inside");
named_enum!(EraserMode, Normal = "normal", Fills = "fills", Lines = "lines", SelectedFills = "selected", Inside = "inside");

#[derive(Clone, Copy, Debug, PartialEq)]
enum Handle {
    Scale(i8, i8),
    /// Box edge: (horizontal, local coordinate of the edge).
    Skew(bool, f32),
    Rotate,
    Pivot,
}

/// Anchor selected with Direct Selection: (shape, subpath, node).
type AnchorRef = (u32, usize, usize);

/// Path being built with the Pen (document coordinates).
struct PenState {
    nodes: Vec<Node>,
    closed: bool,
}

enum Drag {
    None,
    Move { start: Pt, base: Box<Document>, delta: Pt, bbox: BBox },
    Marquee { start: Pt, cur: Pt, base_sel: Vec<u32> },
    Scale { base: Box<Document>, f: Matrix, lb: BBox, hx: i8, hy: i8, pivot: Pt },
    Rotate { base: Box<Document>, center: Pt, a0: f32 },
    Pivot { f: Matrix },
    Create { start: Pt, cur: Pt },
    Freehand { pts: Vec<Pt> },
    Pan { sx: f32, sy: f32, px: f32, py: f32 },
    PenNode { idx: usize, out_only: bool, closing: bool },
    Anchors { base: Box<Document>, start: Pt },
    BezHandle { base: Box<Document>, id: u32, path: usize, node: usize, out: bool },
    Segment { base: Box<Document>, id: u32, path: usize, a: usize, b: usize, t: f32, start: Pt },
    AnchorMarquee { start: Pt, cur: Pt, base: Vec<AnchorRef> },
    Bend { base: Box<Document>, targets: Vec<drawing::SegRef>, t: f32, start: Pt, click: u32, moved: bool },
    Corner { base: Box<Document>, targets: Vec<AnchorRef>, start: Pt, click: u32, moved: bool },
    /// Dragged shape hint: (keyframe, index).
    Hint { base: Box<Document>, key: u32, idx: usize },
    /// 9-slice scaling guide (0 left, 1 top, 2 right, 3 bottom).
    Scale9 { base: Box<Document>, which: usize },
    /// Fill Transform handle.
    Grad { base: Box<Document>, ids: Vec<u32>, handle: fill_tools::GradHandle, g0: fill_tools::PaintRef, start: Pt },
    Skew { base: Box<Document>, f: Matrix, horiz: bool, edge: f32, start_q: Pt, pivot: Pt },
    /// Box of a new text field.
    TextBox { start: Pt, cur: Pt },
    /// Dragged guide (new from a ruler, or an existing one).
    Guide { base: Box<Document>, idx: usize },
    Env { base: Box<Document>, st0: shape_tools::EnvState, handle: shape_tools::EnvHandle, start: Pt },
}

const ACCENT: Rgba = Rgba { r: 47, g: 140, b: 255, a: 255 };
const PASTEBOARD: Rgba = Rgba { r: 38, g: 38, b: 38, a: 255 };
const MAX_UNDO: usize = 200;
/// Paint Bucket gap closing, in screen pixels: don't close, small, medium, large.
const GAPS: [f32; 4] = [0.0, 2.0, 5.0, 10.0];

#[wasm_bindgen]
pub struct Editor {
    doc: Document,
    frame: u32,
    layer: usize,
    tool: Tool,
    temp_hand: bool,
    sel: Vec<u32>,
    fill: Rgba,
    fill_on: bool,
    /// Active gradient fill (no matrix: it fits each new shape).
    fill_grad: Option<grad::Gradient>,
    /// Active bitmap fill (image from the Library).
    fill_bmp: Option<u32>,
    lock_fill: bool,
    /// PolyStar: (star, sides, star point size).
    polystar: (bool, u32, f32),
    lasso_poly: bool,
    lasso_pts: Vec<Pt>,
    xform_mode: shape_tools::XMode,
    env: Option<shape_tools::EnvState>,
    /// Format for new text (Text tool).
    text_style: text::TextField,
    text_edit: Option<text_editor::TextEdit>,
    text_rgba: Rgba,
    /// Committed changes (for the History panel).
    commits: u32,
    /// Undo steps discarded by the limit, and the history "epoch"
    /// (changes when opening or creating a document), for the History panel.
    hist_base: u32,
    hist_epoch: u32,
    /// Last pointer position on screen.
    last_screen: Pt,
    stroke: Rgba,
    stroke_on: bool,
    stroke_width: f32,
    brush_size: f32,
    rect_radius: f32,
    pencil_mode: PencilMode,
    brush_mode: BrushMode,
    brush_tip: Tip,
    eraser_mode: EraserMode,
    eraser_tip: Tip,
    eraser_size: f32,
    faucet: bool,
    gap: usize,
    pen: Option<PenState>,
    anchors: Vec<AnchorRef>,
    pivot: Option<(Vec<u32>, Pt)>,
    hover: Option<Pt>,
    zoom: f32,
    pan: Pt,
    vw: u32,
    vh: u32,
    dpr: f32,
    pm: Pixmap,
    preview: Pixmap,
    drag: Drag,
    shift: bool,
    alt: bool,
    undo: Vec<Document>,
    redo: Vec<Document>,
    clipboard: Vec<Shape>,
    onion: bool,
    message: String,
    /// Object Drawing mode (J). Off = Flash's merge drawing model.
    object_mode: bool,
    /// Moved or pasted raw pieces that will merge when deselected.
    floating: Vec<u32>,
    snap_objects: bool,
    snap_grid: bool,
    snap_pixels: bool,
    show_grid: bool,
    grid_size: f32,
    grid_color: Rgba,
    snap_targets: Vec<Pt>,
    snap_hit: Option<Pt>,
    /// Frames selected in the timeline: (layer0, layer1, frame0, frame1).
    frame_sel: Option<(usize, usize, u32, u32)>,
    /// Copied frames: (source layer, cells).
    frame_clip: Vec<(usize, Vec<timeline::Cell>)>,
    onion_before: u32,
    onion_after: u32,
    onion_outlines: bool,
    edit_multiple: bool,
    show_hints: bool,
    /// Views saved when entering symbol editing.
    sym_views: Vec<(Pt, f32)>,
    /// Control > Enable Simple Buttons.
    simple_buttons: bool,
    /// Pointer over the Stage (for simple buttons), in document coordinates.
    mouse: Option<(Pt, bool)>,
    preview_mouse: Option<(Pt, bool)>,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl Editor {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Editor {
        Editor {
            doc: Document::new(550.0, 400.0),
            frame: 0,
            layer: 0,
            tool: Tool::Select,
            temp_hand: false,
            sel: Vec::new(),
            fill: Rgba { r: 0x33, g: 0x66, b: 0xcc, a: 255 },
            fill_on: true,
            fill_grad: None,
            fill_bmp: None,
            lock_fill: false,
            polystar: (false, 5, 0.5),
            lasso_poly: false,
            lasso_pts: Vec::new(),
            xform_mode: shape_tools::XMode::Free,
            env: None,
            text_style: text::TextField::default(),
            text_edit: None,
            text_rgba: Rgba::BLACK,
            commits: 0,
            hist_base: 0,
            hist_epoch: 0,
            last_screen: (0.0, 0.0),
            stroke: Rgba::BLACK,
            stroke_on: true,
            stroke_width: 1.0,
            brush_size: 10.0,
            rect_radius: 0.0,
            pencil_mode: PencilMode::Straighten,
            brush_mode: BrushMode::Normal,
            brush_tip: Tip::Round,
            eraser_mode: EraserMode::Normal,
            eraser_tip: Tip::Round,
            eraser_size: 14.0,
            faucet: false,
            gap: 1,
            pen: None,
            anchors: Vec::new(),
            pivot: None,
            hover: None,
            zoom: 1.0,
            pan: (0.0, 0.0),
            vw: 1,
            vh: 1,
            dpr: 1.0,
            pm: Pixmap::new(1, 1).unwrap(),
            preview: Pixmap::new(1, 1).unwrap(),
            drag: Drag::None,
            shift: false,
            alt: false,
            undo: Vec::new(),
            redo: Vec::new(),
            clipboard: Vec::new(),
            onion: false,
            message: String::new(),
            object_mode: false,
            floating: Vec::new(),
            snap_objects: true,
            snap_grid: false,
            snap_pixels: false,
            show_grid: false,
            grid_size: 18.0,
            grid_color: Rgba { r: 0x94, g: 0x94, b: 0x94, a: 255 },
            snap_targets: Vec::new(),
            snap_hit: None,
            frame_sel: None,
            frame_clip: Vec::new(),
            onion_before: 2,
            onion_after: 2,
            onion_outlines: false,
            edit_multiple: false,
            show_hints: false,
            sym_views: Vec::new(),
            simple_buttons: false,
            mouse: None,
            preview_mouse: None,
        }
    }

    // ───────────────────────── View ─────────────────────────

    pub fn set_viewport(&mut self, w: u32, h: u32, dpr: f32) {
        let (w, h) = (w.max(1), h.max(1));
        let first = self.vw == 1 && self.vh == 1;
        let old_dpr = self.dpr;
        if let Some(pm) = Pixmap::new(w, h) {
            let c = self.to_doc(self.vw as f32 / 2.0, self.vh as f32 / 2.0);
            self.pm = pm;
            self.vw = w;
            self.vh = h;
            self.dpr = dpr.max(0.5);
            if first {
                self.fit_stage();
            } else {
                self.zoom *= self.dpr / old_dpr;
                self.pan = (w as f32 / 2.0 - c.0 * self.zoom, h as f32 / 2.0 - c.1 * self.zoom);
            }
        }
    }

    pub fn fit_stage(&mut self) {
        let m = 60.0 * self.dpr;
        let z = ((self.vw as f32 - m) / self.doc.width).min((self.vh as f32 - m) / self.doc.height);
        self.zoom = z.clamp(0.05 * self.dpr, 32.0 * self.dpr);
        self.pan = (
            (self.vw as f32 - self.doc.width * self.zoom) / 2.0,
            (self.vh as f32 - self.doc.height * self.zoom) / 2.0,
        );
    }

    /// Zoom as a CSS percentage (100 = 1 document pixel per CSS pixel).
    pub fn set_zoom(&mut self, percent: f32) {
        let f = (percent / 100.0 * self.dpr) / self.zoom;
        self.zoom_at(self.vw as f32 / 2.0, self.vh as f32 / 2.0, f);
    }

    pub fn zoom_at(&mut self, x: f32, y: f32, factor: f32) {
        let p = self.to_doc(x, y);
        self.zoom = (self.zoom * factor).clamp(0.05 * self.dpr, 32.0 * self.dpr);
        self.pan = (x - p.0 * self.zoom, y - p.1 * self.zoom);
    }

    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        self.pan.0 += dx;
        self.pan.1 += dy;
    }

    pub fn set_temp_hand(&mut self, on: bool) {
        self.temp_hand = on;
    }

    pub fn pixels_ptr(&self) -> *const u8 {
        self.pm.data().as_ptr()
    }
    pub fn width(&self) -> u32 {
        self.vw
    }
    pub fn height(&self) -> u32 {
        self.vh
    }

    // ───────────────────────── Render ─────────────────────────

    pub fn render(&mut self) {
        let mut pixmap = std::mem::replace(&mut self.pm, Pixmap::new(1, 1).unwrap());
        let pm = &mut pixmap;
        let dpr = self.dpr;
        pm.fill(tiny_skia::Color::from_rgba8(PASTEBOARD.r, PASTEBOARD.g, PASTEBOARD.b, 255));
        let vm = self.view_matrix();
        // Editing a symbol on its own: the whole window is its "stage".
        let separate = self.doc.sym_edit.iter().rposition(|e| e.in_place.is_none());
        if separate.is_some() {
            let bg = self.doc.bg;
            pm.fill(tiny_skia::Color::from_rgba8(bg.r, bg.g, bg.b, 255));
        } else {
            let (sx, sy) = self.pan;
            let (sw, sh) = (self.doc.width * self.zoom, self.doc.height * self.zoom);
            render::fill_rect(pm, sx + 2.0 * dpr, sy + 3.0 * dpr, sw, sh, Rgba { r: 0, g: 0, b: 0, a: 110 });
            render::fill_rect(pm, sx, sy, sw, sh, self.doc.bg);
            self.draw_grid(pm);
        }
        // In place: whatever contains the symbol, dimmed and without the edited instance.
        let start = separate.map_or(0, |k| k + 1);
        let mut chain = self.stage_matrix();
        for e in self.doc.sym_edit.iter().skip(start) {
            let dim = |_: usize, _: &Shape| 0.35;
            let o = render::DrawOpts { skip: e.instance, ..render::DrawOpts::new(true, true, &dim) };
            render::draw_layers(pm, &e.parent_layers, e.frame, &chain, &o);
            chain = chain.mul(&e.in_place.unwrap_or(Matrix::IDENTITY));
        }

        let total = self.doc.total_frames() as i64;
        if self.edit_multiple {
            // Edit Multiple Frames: every keyframe in the range, opaque.
            let (a, b) = (self.frame as i64 - self.onion_before as i64, self.frame as i64 + self.onion_after as i64);
            for f in a.max(0)..=b.min(total - 1) {
                let is_key = self.doc.layers.iter().any(|l| l.keys.contains_key(&(f as u32)));
                if f != self.frame as i64 && is_key {
                    render::draw_frame(pm, &self.doc, f as u32, &vm, 1.0, true);
                }
            }
        } else if self.onion {
            // Onion skin: more transparent the farther from the current frame.
            let (nb, na) = (self.onion_before as i64, self.onion_after as i64);
            let mut frames: Vec<(i64, f32)> = (1..=nb).map(|d| (-d, d as f32 / (nb + 1) as f32)).chain((1..=na).map(|d| (d, d as f32 / (na + 1) as f32))).collect();
            frames.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
            for (df, far) in frames {
                let f = self.frame as i64 + df;
                if f < 0 || f >= total {
                    continue;
                }
                let alpha = 0.05 + 0.3 * (1.0 - far);
                let alpha_of = move |_: usize, _: &Shape| alpha;
                let o = render::DrawOpts { outlines: self.onion_outlines, ..render::DrawOpts::new(true, true, &alpha_of) };
                render::draw_layers(pm, &self.doc.layers, f as u32, &vm, &o);
            }
        }
        match self.doc.scopes.last() {
            Some(sc) => {
                let (tag, layer) = (Some(sc.tag), sc.layer);
                let dim = |li: usize, s: &Shape| if li == layer && s.tag == tag { 1.0 } else { 0.35 };
                // The text being typed is drawn on top by the interface.
                let o = render::DrawOpts { skip: self.text_edit.as_ref().map(|t| t.id), ..render::DrawOpts::new(true, self.doc.export_hidden, &dim) };
                render::draw_layers(pm, &self.doc.layers, self.frame, &vm, &o);
            }
            None => {
                let one = |_: usize, _: &Shape| 1.0;
                let mouse = if self.simple_buttons { self.mouse.map(|(p, d)| (vm.apply(p.0, p.1), d)) } else { None };
                let skip = self.text_edit.as_ref().map(|t| t.id);
                let o = render::DrawOpts { mouse, skip, ..render::DrawOpts::new(true, self.doc.export_hidden, &one) };
                render::draw_layers(pm, &self.doc.layers, self.frame, &vm, &o);
            }
        }
        if !self.doc.sym_edit.is_empty() {
            self.draw_symbol_guides(pm, &vm);
        }
        self.draw_guides(pm);
        self.draw_tool_preview(pm, &vm);
        self.draw_selection(pm, &vm);
        self.pm = pixmap;
    }

    // ───────────────────────── Pointer ─────────────────────────

    pub fn pointer_down(&mut self, x: f32, y: f32, button: u32, shift: bool, alt: bool) {
        let p = self.to_doc(x, y);
        self.shift = shift;
        self.alt = alt;
        if button == 1 || self.temp_hand || self.tool == Tool::Hand {
            self.drag = Drag::Pan { sx: x, sy: y, px: self.pan.0, py: self.pan.1 };
            return;
        }
        if button != 0 {
            return;
        }
        self.mouse = Some((p, true));
        if matches!(self.tool, Tool::Select | Tool::Transform) {
            if let Some(which) = self.scale9_at(x, y) {
                self.drag = Drag::Scale9 { base: Box::new(self.doc.clone()), which };
                return;
            }
        }
        if !matches!(self.tool, Tool::Select | Tool::Transform | Tool::Subselect | Tool::Zoom) {
            self.settle();
        }
        let exclude = if matches!(self.tool, Tool::Select | Tool::Transform | Tool::Subselect) { self.sel.clone() } else { Vec::new() };
        self.build_snap_targets(&exclude);
        self.snap_hit = None;
        match self.tool {
            // With the Arrow, guides are picked before shapes (as in Flash).
            Tool::Select if self.guide_at(x, y).is_some() => {
                let idx = self.guide_at(x, y).unwrap();
                self.drag = Drag::Guide { base: Box::new(self.doc.clone()), idx };
            }
            Tool::Select => self.select_down(p, shift),
            Tool::Transform if self.xform_mode.warps() => match self.env_handle_at(x, y) {
                Some(h) => {
                    let st0 = self.env_view().unwrap();
                    self.drag = Drag::Env { base: Box::new(self.doc.clone()), st0, handle: h, start: p };
                }
                None => self.select_down(p, shift),
            },
            Tool::Transform => match self.handle_at(x, y) {
                Some(Handle::Skew(horiz, edge)) => {
                    let (f, lb) = self.frame_box().unwrap();
                    let pivot = self.pivot_local(&lb);
                    let start_q = f.invert().map_or(p, |i| i.apply(p.0, p.1));
                    self.drag = Drag::Skew { base: Box::new(self.doc.clone()), f, horiz, edge, start_q, pivot };
                }
                Some(Handle::Scale(hx, hy)) => {
                    let (f, lb) = self.frame_box().unwrap();
                    let pivot = self.pivot_local(&lb);
                    self.drag = Drag::Scale { base: Box::new(self.doc.clone()), f, lb, hx, hy, pivot };
                }
                Some(Handle::Rotate) => {
                    let (f, lb) = self.frame_box().unwrap();
                    let center = f.apply(self.pivot_local(&lb).0, self.pivot_local(&lb).1);
                    let a0 = (p.1 - center.1).atan2(p.0 - center.0);
                    self.drag = Drag::Rotate { base: Box::new(self.doc.clone()), center, a0 };
                }
                Some(Handle::Pivot) => {
                    let (f, _) = self.frame_box().unwrap();
                    self.drag = Drag::Pivot { f };
                }
                None => self.select_down(p, shift),
            },
            Tool::Subselect => self.direct_down(x, y, p, shift, alt),
            Tool::FillXform => self.fill_xform_down(x, y, p),
            Tool::Pen => self.pen_down(x, y, p, shift, alt),
            Tool::Lasso => self.lasso_down(p, shift),
            Tool::Text => self.text_down(p),
            Tool::Rect | Tool::Oval | Tool::Line | Tool::PolyStar => {
                if self.can_draw() {
                    self.sel.clear();
                    let p = self.snap(p);
                    self.drag = Drag::Create { start: p, cur: p };
                }
            }
            Tool::Pencil | Tool::Brush => {
                if self.can_draw() {
                    self.sel.retain(|_| self.tool == Tool::Brush && self.brush_mode == BrushMode::Selection);
                    self.drag = Drag::Freehand { pts: vec![p] };
                }
            }
            Tool::Eraser => {
                if !self.can_draw() {
                } else if self.faucet {
                    self.faucet_at(p);
                } else {
                    self.drag = Drag::Freehand { pts: vec![p] };
                }
            }
            Tool::Bucket => self.bucket_at(p),
            Tool::Ink => self.ink_at(p),
            Tool::Eyedropper => self.eyedropper_at(p),
            Tool::Zoom => self.zoom_at(x, y, if alt { 0.5 } else { 2.0 }),
            Tool::Hand => {}
        }
    }

    pub fn pointer_move(&mut self, x: f32, y: f32, shift: bool, alt: bool) {
        let p = self.to_doc(x, y);
        self.last_screen = (x, y);
        self.shift = shift;
        self.alt = alt;
        self.hover = Some(p);
        self.mouse = Some((p, self.mouse.map_or(false, |m| m.1)));
        let mut drag = std::mem::replace(&mut self.drag, Drag::None);
        self.drag_move(&mut drag, x, y, p, shift, alt);
        self.drag = drag;
    }

    pub fn pointer_up(&mut self, x: f32, y: f32, shift: bool, alt: bool) {
        self.pointer_move(x, y, shift, alt);
        let drag = std::mem::replace(&mut self.drag, Drag::None);
        self.drag_end(drag);
        if let Some(m) = &mut self.mouse {
            m.1 = false;
        }
    }

    pub fn is_dragging(&self) -> bool {
        !matches!(self.drag, Drag::None)
    }

    /// The pointer left the canvas: stop showing the Pen's rubber band.
    pub fn pointer_leave(&mut self) {
        self.hover = None;
    }

    pub fn cursor_at(&self, x: f32, y: f32) -> String {
        self.cursor_name(x, y).into()
    }

    /// Document coordinates under the pointer, for the status bar.
    pub fn doc_coords(&self, x: f32, y: f32) -> Vec<f32> {
        let p = self.to_doc(x, y);
        vec![p.0, p.1]
    }

    // ───────────────────────── Tools and colors ─────────────────────────

    pub fn set_tool(&mut self, name: &str) {
        if let Some(t) = Tool::parse(name) {
            if t != Tool::Text {
                self.text_commit();
            }
            if t != Tool::Pen {
                self.pen_finish();
            }
            if !matches!(t, Tool::Select | Tool::Transform | Tool::Subselect) {
                self.settle();
            }
            if t != self.tool {
                self.anchors.clear();
            }
            self.tool = t;
            self.drag = Drag::None;
        }
    }

    pub fn set_fill(&mut self, hex: &str, alpha: u8, on: bool) {
        if let Some(c) = Rgba::parse(hex, alpha) {
            self.fill = c;
        }
        self.fill_on = on;
        // A solid color replaces the gradient or bitmap.
        self.fill_grad = None;
        self.fill_bmp = None;
        let f = on.then_some(self.fill);
        self.edit_selection(|s| {
            if s.is_closed() {
                s.fill = f;
                s.grad = None;
                s.bmp = None;
            }
        });
    }

    pub fn set_stroke(&mut self, hex: &str, alpha: u8, on: bool) {
        if let Some(c) = Rgba::parse(hex, alpha) {
            self.stroke = c;
        }
        self.stroke_on = on;
        let st = on.then_some(self.stroke);
        self.edit_selection(|s| {
            // A line with no stroke would vanish: the color is honored there, but not "no color".
            if st.is_some() || (s.is_closed() && s.fill.is_some()) {
                s.stroke = st
            }
        });
    }

    pub fn swap_colors(&mut self) {
        std::mem::swap(&mut self.fill, &mut self.stroke);
        self.fill_grad = None;
        self.fill_bmp = None;
        std::mem::swap(&mut self.fill_on, &mut self.stroke_on);
    }

    pub fn default_colors(&mut self) {
        self.fill = Rgba::BLACK;
        self.fill_grad = None;
        self.fill_bmp = None;
        self.stroke = Rgba::BLACK;
        self.fill_on = true;
        self.stroke_on = true;
    }

    pub fn set_stroke_width(&mut self, w: f32) {
        self.stroke_width = w.clamp(0.1, 200.0);
        let w = self.stroke_width;
        self.edit_selection(|s| {
            if s.stroke.is_some() {
                s.stroke_width = w
            }
        });
    }

    pub fn set_brush_size(&mut self, w: f32) {
        self.brush_size = w.clamp(1.0, 200.0);
    }

    pub fn set_rect_radius(&mut self, r: f32) {
        self.rect_radius = r.clamp(0.0, 1000.0);
        let r = self.rect_radius;
        self.edit_selection(|s| {
            if let Geometry::Rect { r: rr, .. } = &mut s.geom {
                *rr = r
            }
        });
    }

    pub fn set_pencil_mode(&mut self, m: &str) {
        if let Some(m) = PencilMode::parse(m) {
            self.pencil_mode = m;
        }
    }
    pub fn set_brush_mode(&mut self, m: &str) {
        if let Some(m) = BrushMode::parse(m) {
            self.brush_mode = m;
        }
    }
    pub fn set_brush_tip(&mut self, t: &str) {
        self.brush_tip = Tip::parse(t);
    }
    pub fn set_eraser_mode(&mut self, m: &str) {
        if let Some(m) = EraserMode::parse(m) {
            self.eraser_mode = m;
        }
    }
    pub fn set_eraser_tip(&mut self, t: &str) {
        self.eraser_tip = Tip::parse(t);
    }
    pub fn set_eraser_size(&mut self, s: f32) {
        self.eraser_size = s.clamp(1.0, 200.0);
    }
    pub fn set_faucet(&mut self, on: bool) {
        self.faucet = on;
    }
    pub fn set_gap(&mut self, g: u32) {
        self.gap = (g as usize).min(GAPS.len() - 1);
    }

    pub fn set_sel_bounds(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let Some(b) = self.sel_bbox() else { return };
        let sx = if b.w() > 1e-3 { w.max(0.1) / b.w() } else { 1.0 };
        let sy = if b.h() > 1e-3 { h.max(0.1) / b.h() } else { 1.0 };
        let m = Matrix::translate(x, y).mul(&Matrix::scale(sx, sy)).mul(&Matrix::translate(-b.x0, -b.y0));
        self.edit_selection(|s| s.m = m.mul(&s.m));
        self.mark_floating();
    }

    pub fn set_document(&mut self, w: f32, h: f32, bg: &str, fps: f32) {
        let base = self.doc.clone();
        self.doc.width = w.clamp(1.0, 2880.0);
        self.doc.height = h.clamp(1.0, 2880.0);
        if let Some(c) = Rgba::parse(bg, 255) {
            self.doc.bg = c;
        }
        self.doc.fps = fps.clamp(0.01, 120.0);
        self.commit_from(base);
    }

    pub fn set_export_hidden(&mut self, on: bool) {
        let base = self.doc.clone();
        self.doc.export_hidden = on;
        self.commit_from(base);
    }

    pub fn toggle_onion(&mut self) {
        self.onion = !self.onion;
    }

    // ───────────────────────── Pen ─────────────────────────

    /// Finishes the current path (Enter / Esc / switching tools).
    pub fn pen_finish(&mut self) {
        if let Some(pen) = self.pen.take() {
            self.commit_pen(pen);
        }
    }

    /// Ctrl+Z while drawing with the Pen removes the last point, as in Photoshop.
    pub fn pen_undo(&mut self) -> bool {
        match &mut self.pen {
            Some(pen) => {
                pen.nodes.pop();
                if pen.nodes.is_empty() {
                    self.pen = None;
                }
                true
            }
            None => false,
        }
    }

    // ───────────────────────── Drawing model, groups and shape ─────────────────────────

    /// J: toggles Object Drawing mode.
    pub fn set_object_mode(&mut self, on: bool) {
        self.object_mode = on;
    }

    pub fn double_click(&mut self, x: f32, y: f32) {
        if self.tool == Tool::Lasso {
            self.lasso_finish();
            return;
        }
        if matches!(self.tool, Tool::Select | Tool::Transform) {
            let p = self.to_doc(x, y);
            self.double_click_at(p);
        }
    }

    /// Ctrl+G
    pub fn group(&mut self) {
        self.group_selection();
    }

    /// Ctrl+Shift+G
    pub fn ungroup(&mut self) {
        self.ungroup_selection(false);
    }

    /// Ctrl+B: breaks apart groups and turns drawing objects into raw shapes.
    pub fn break_apart(&mut self) {
        self.ungroup_selection(true);
    }

    /// Edit > Edit Selected: enters the selected group or object.
    pub fn edit_selected(&mut self) {
        if let Some((li, s)) = self.selected_shapes().into_iter().find(|(_, s)| s.is_group() || !s.raw) {
            if s.is_instance() {
                self.edit_instance_in_place(li, s.id);
                return;
            }
            self.enter_group(li, s.id);
        }
    }

    /// Leaves the group being edited (the "←" arrow in the edit bar).
    pub fn leave_group(&mut self) {
        self.exit_group();
    }

    pub fn leave_all_groups(&mut self) {
        self.exit_all_groups();
    }

    pub fn smooth_selection(&mut self) {
        self.smooth_sel();
    }

    pub fn straighten_selection(&mut self) {
        self.straighten_sel();
    }

    pub fn optimize_curves(&mut self, amount: f32) -> String {
        self.optimize_sel(amount)
    }

    pub fn lines_to_fills(&mut self) {
        self.lines_to_fills_sel();
    }

    pub fn expand_fill(&mut self, distance: f32, expand: bool) {
        self.expand_fill_sel(if expand { distance.abs() } else { -distance.abs() });
    }

    pub fn soften_edges(&mut self, distance: f32, steps: u32, expand: bool) {
        self.soften_sel(distance.abs(), steps, expand);
    }

    /// Snapping: "objects" | "grid" | "pixels".
    pub fn set_snap(&mut self, kind: &str, on: bool) {
        match kind {
            "objects" => self.snap_objects = on,
            "grid" => self.snap_grid = on,
            "pixels" => self.snap_pixels = on,
            _ => {}
        }
    }

    pub fn set_grid(&mut self, show: bool, size: f32, color: &str) {
        self.show_grid = show;
        self.grid_size = size.clamp(1.0, 500.0);
        if let Some(c) = Rgba::parse(color, 255) {
            self.grid_color = c;
        }
    }

    // ───────────────────────── Timeline ─────────────────────────

    pub fn set_frame(&mut self, f: u32) {
        if f != self.frame {
            self.pen_finish();
            self.settle();
            self.exit_all_groups();
            self.frame = f;
            self.sel.clear();
            self.anchors.clear();
        }
    }

    pub fn select_cell(&mut self, layer: usize, frame: u32) {
        if layer < self.doc.layers.len() {
            self.layer = layer;
        }
        self.set_frame(frame);
        self.frame_sel = Some((self.layer, self.layer, frame, frame));
    }

    pub fn step_frame(&mut self, delta: i32, wrap: bool) {
        self.frame_sel = None;
        let total = self.doc.total_frames() as i64;
        let mut f = self.frame as i64 + delta as i64;
        if wrap {
            f = f.rem_euclid(total);
        } else {
            f = f.clamp(0, total - 1);
        }
        self.set_frame(f as u32);
    }

    pub fn insert_frame(&mut self) {
        let (li, f) = (self.layer, self.frame);
        self.timeline_edit(|d| d.layers[li].insert_frame(f));
    }

    pub fn remove_frame(&mut self) {
        let (li, f) = (self.layer, self.frame);
        self.timeline_edit(|d| d.layers[li].remove_frame(f));
    }

    pub fn insert_keyframe(&mut self, blank: bool) {
        let (li, f) = (self.layer, self.frame);
        self.timeline_edit(|d| d.layers[li].insert_keyframe(f, blank));
    }

    pub fn clear_keyframe(&mut self) {
        let (li, f) = (self.layer, self.frame);
        self.timeline_edit(|d| d.layers[li].clear_keyframe(f));
    }

    pub fn set_tween(&mut self, on: bool) {
        let (li, f) = (self.layer, self.frame);
        self.timeline_edit(|d| {
            let l = &mut d.layers[li];
            if let Some(k) = l.key_at_or_before(f) {
                l.keys.get_mut(&k).unwrap().tween = on;
            }
        });
    }

    pub fn set_ease(&mut self, ease: f32) {
        let (li, f) = (self.layer, self.frame);
        self.timeline_edit(|d| {
            let l = &mut d.layers[li];
            if let Some(k) = l.key_at_or_before(f) {
                l.keys.get_mut(&k).unwrap().ease = ease.clamp(-100.0, 100.0);
            }
        });
    }

    pub fn add_layer(&mut self) {
        let at = self.layer;
        let total = self.doc.total_frames();
        let base = self.doc.clone();
        let parent = self.doc.layers.get(at).and_then(|l| l.parent);
        let i = self.doc.add_layer(at);
        self.doc.layers[i].length = total;
        // Inside a folder, mask or guide, the new layer stays in the same place.
        self.doc.layers[i].parent = parent;
        self.layer = i;
        self.commit_from(base);
    }

    pub fn delete_layer(&mut self) {
        if self.doc.layers.len() <= 1 {
            self.message = "No se puede eliminar la única capa".into();
            return;
        }
        let base = self.doc.clone();
        self.doc.layers.remove(self.layer);
        self.layer = self.layer.min(self.doc.layers.len() - 1);
        self.sel.clear();
        self.commit_from(base);
    }

    pub fn move_layer(&mut self, from: usize, to: usize) {
        let n = self.doc.layers.len();
        if from >= n || to >= n || from == to {
            return;
        }
        let base = self.doc.clone();
        let l = self.doc.layers.remove(from);
        self.doc.layers.insert(to, l);
        self.layer = to;
        self.commit_from(base);
    }

    pub fn rename_layer(&mut self, i: usize, name: &str) {
        if i < self.doc.layers.len() && !name.trim().is_empty() {
            let base = self.doc.clone();
            self.doc.layers[i].name = name.trim().to_string();
            self.commit_from(base);
        }
    }

    pub fn toggle_visible(&mut self, i: usize) {
        if let Some(l) = self.doc.layers.get_mut(i) {
            l.visible = !l.visible;
            self.sel.clear();
        }
    }

    pub fn toggle_lock(&mut self, i: usize) {
        if let Some(l) = self.doc.layers.get_mut(i) {
            l.locked = !l.locked;
            self.sel.clear();
        }
    }

    pub fn toggle_outline(&mut self, i: usize) {
        if let Some(l) = self.doc.layers.get_mut(i) {
            l.outline = !l.outline;
        }
    }

    pub fn total_frames(&self) -> u32 {
        self.doc.total_frames()
    }

    pub fn fps(&self) -> f32 {
        self.doc.fps
    }

    // ───────────────────────── Edit ─────────────────────────

    pub fn undo(&mut self) {
        self.text_commit();
        if self.pen_undo() {
            return;
        }
        if let Some(d) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.doc, d));
            self.after_history();
        }
    }

    pub fn redo(&mut self) {
        self.text_commit();
        if let Some(d) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.doc, d));
            self.after_history();
        }
    }

    pub fn delete_selection(&mut self) {
        if self.tool == Tool::Subselect && !self.anchors.is_empty() {
            self.delete_anchors();
            return;
        }
        if self.sel.is_empty() {
            return;
        }
        let base = self.doc.clone();
        let ids = std::mem::take(&mut self.sel);
        for li in self.editable_layers() {
            if self.doc.frame_shapes(li, self.frame).iter().any(|s| ids.contains(&s.id)) {
                let k = self.doc.layers[li].ensure_key(self.frame);
                self.doc.layers[li].keys.get_mut(&k).unwrap().shapes.retain(|s| !ids.contains(&s.id));
            }
        }
        self.anchors.clear();
        self.floating.retain(|f| !ids.contains(f));
        self.commit_from(base);
    }

    pub fn copy(&mut self) {
        let shapes: Vec<Shape> = self.selected_shapes().into_iter().map(|(_, s)| s).collect();
        if !shapes.is_empty() {
            self.clipboard = shapes;
        }
    }

    pub fn cut(&mut self) {
        self.copy();
        self.delete_selection();
    }

    /// Ctrl+V pastes at the center of the view (like Flash); Ctrl+Shift+V, in place.
    pub fn paste(&mut self, in_place: bool) {
        if self.clipboard.is_empty() {
            return;
        }
        let off = if in_place {
            (0.0, 0.0)
        } else {
            let b = self.clipboard.iter().fold(BBox::EMPTY, |b, s| b.union(&s.world_bounds()));
            let c = self.to_doc(self.vw as f32 / 2.0, self.vh as f32 / 2.0);
            (c.0 - b.center().0, c.1 - b.center().1)
        };
        let clip = self.clipboard.clone();
        self.paste_shapes(clip, off);
    }

    /// Ctrl+D: duplicate, slightly offset.
    pub fn duplicate(&mut self) {
        let shapes: Vec<Shape> = self.selected_shapes().into_iter().map(|(_, s)| s).collect();
        if !shapes.is_empty() {
            self.paste_shapes(shapes, (10.0, 10.0));
        }
    }

    pub fn select_all(&mut self) {
        self.settle();
        let scope = self.scope();
        self.sel = self
            .editable_layers()
            .into_iter()
            .flat_map(|li| self.doc.frame_shapes(li, self.frame))
            .filter(|s| s.tag == scope)
            .map(|s| s.id)
            .collect();
        if self.tool == Tool::Subselect {
            self.anchors = self.all_anchors(|_| true);
        }
    }

    pub fn deselect(&mut self) {
        self.settle();
        self.sel.clear();
        self.anchors.clear();
    }

    pub fn nudge(&mut self, dx: f32, dy: f32) {
        if self.tool == Tool::Subselect && !self.anchors.is_empty() {
            let base = self.doc.clone();
            self.move_anchors((dx, dy));
            self.commit_from(base);
            return;
        }
        self.edit_selection(|s| {
            s.m.tx += dx;
            s.m.ty += dy;
        });
        self.shift_pivot((dx, dy));
        self.mark_floating();
    }

    /// "front" | "back" | "forward" | "backward"
    pub fn arrange(&mut self, op: &str) {
        if self.sel.is_empty() {
            return;
        }
        let base = self.doc.clone();
        let ids = self.sel.clone();
        for li in self.editable_layers() {
            if !self.doc.frame_shapes(li, self.frame).iter().any(|s| ids.contains(&s.id)) {
                continue;
            }
            let k = self.doc.layers[li].ensure_key(self.frame);
            let v = &mut self.doc.layers[li].keys.get_mut(&k).unwrap().shapes;
            let is_sel = |s: &Shape| ids.contains(&s.id);
            match op {
                "front" | "back" => {
                    let (sel, rest): (Vec<Shape>, Vec<Shape>) = v.drain(..).partition(|s| is_sel(s));
                    *v = if op == "front" { rest.into_iter().chain(sel).collect() } else { sel.into_iter().chain(rest).collect() };
                }
                "forward" => {
                    for i in (0..v.len().saturating_sub(1)).rev() {
                        if is_sel(&v[i]) && !is_sel(&v[i + 1]) {
                            v.swap(i, i + 1);
                        }
                    }
                }
                "backward" => {
                    for i in 1..v.len() {
                        if is_sel(&v[i]) && !is_sel(&v[i - 1]) {
                            v.swap(i, i - 1);
                        }
                    }
                }
                _ => {}
            }
        }
        self.commit_from(base);
    }

    pub fn flip(&mut self, horizontal: bool) {
        let Some(c) = self.transform_center() else { return };
        let m = Matrix::about(c, &if horizontal { Matrix::scale(-1.0, 1.0) } else { Matrix::scale(1.0, -1.0) });
        self.edit_selection(|s| s.m = m.mul(&s.m));
        self.mark_floating();
    }

    pub fn rotate90(&mut self, cw: bool) {
        let Some(c) = self.transform_center() else { return };
        let a = std::f32::consts::FRAC_PI_2 * if cw { 1.0 } else { -1.0 };
        let m = Matrix::about(c, &Matrix::rotate(a));
        self.edit_selection(|s| s.m = m.mul(&s.m));
        self.mark_floating();
    }

    /// Aligns the selection: "left" | "hcenter" | "right" | "top" | "vcenter" | "bottom".
    pub fn align(&mut self, op: &str, to_stage: bool) {
        let Some(all) = self.sel_bbox() else { return };
        let r = if to_stage { BBox { x0: 0.0, y0: 0.0, x1: self.doc.width, y1: self.doc.height } } else { all };
        let base = self.doc.clone();
        for (_, s) in self.selected_shapes() {
            let b = s.world_bounds();
            let (dx, dy) = match op {
                "left" => (r.x0 - b.x0, 0.0),
                "hcenter" => (r.center().0 - b.center().0, 0.0),
                "right" => (r.x1 - b.x1, 0.0),
                "top" => (0.0, r.y0 - b.y0),
                "vcenter" => (0.0, r.center().1 - b.center().1),
                "bottom" => (0.0, r.y1 - b.y1),
                _ => (0.0, 0.0),
            };
            self.modify_shapes(&[s.id], |s| {
                s.m.tx += dx;
                s.m.ty += dy;
            });
        }
        self.mark_floating();
        self.commit_from(base);
    }

    // ───────────────────────── File ─────────────────────────

    pub fn new_document(&mut self, w: f32, h: f32) {
        self.doc = Document::new(w.clamp(1.0, 2880.0), h.clamp(1.0, 2880.0));
        self.reset_session();
    }

    pub fn to_json(&self) -> String {
        // Only fonts used by some text travel with the file.
        let mut doc = self.doc.clone();
        let fonts = std::mem::take(&mut doc.fonts);
        let used = serde_json::to_string(&doc).unwrap_or_default();
        doc.fonts = fonts.into_iter().filter(|f| used.contains(&format!("\"font\":{}", serde_json::to_string(&f.family).unwrap_or_default()))).collect();
        serde_json::to_string(&json!({ "format": "omf", "version": 1, "document": doc })).unwrap_or_default()
    }

    pub fn load_json(&mut self, s: &str) -> Result<(), JsValue> {
        let v: Value = serde_json::from_str(s).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let d = v.get("document").cloned().unwrap_or(v);
        let doc: Document = serde_json::from_value(d).map_err(|e| JsValue::from_str(&e.to_string()))?;
        if doc.layers.is_empty() {
            return Err(JsValue::from_str("El documento no tiene capas"));
        }
        crate::text::sync_fonts(&doc.fonts);
        self.doc = doc.closed().into_owned();
        self.reset_session();
        // A file saved with a group open opens with everything closed.
        self.exit_all_groups();
        self.undo.clear();
        Ok(())
    }

    pub fn export_png(&self, scale: f32) -> Vec<u8> {
        render::stage_pixmap(&self.doc.closed(), self.scene_frame(), scale.clamp(0.1, 8.0)).and_then(|p| p.encode_png().ok()).unwrap_or_default()
    }

    /// Publish Settings (JSON).
    pub fn publish_settings(&self) -> String {
        serde_json::to_string(&self.doc.publish).unwrap_or_default()
    }

    pub fn set_publish_settings(&mut self, json: &str) {
        match serde_json::from_str::<Publish>(json) {
            Ok(p) => {
                let base = self.doc.clone();
                self.doc.publish = p;
                self.commit_from(base);
            }
            Err(e) => self.message = format!("Configuración no válida: {e}"),
        }
    }

    pub fn export_swf(&self) -> Vec<u8> {
        swf::export(&self.doc.closed())
    }

    /// Renders a Stage frame (without UI) for Test Movie.
    /// `frame` is global: scenes play one after another.
    pub fn render_preview(&mut self, frame: u32, scale: f32) {
        let mouse = self.preview_mouse;
        if let Some(p) = render::movie_pixmap(&self.doc.closed(), frame, scale.clamp(0.05, 8.0), mouse) {
            self.preview = p;
        }
    }
    /// Mouse over Test Movie, in preview pixels (x < 0: outside).
    pub fn set_preview_mouse(&mut self, x: f32, y: f32, down: bool) {
        self.preview_mouse = (x >= 0.0).then_some(((x, y), down));
    }
    pub fn preview_ptr(&self) -> *const u8 {
        self.preview.data().as_ptr()
    }
    pub fn preview_width(&self) -> u32 {
        self.preview.width()
    }
    pub fn preview_height(&self) -> u32 {
        self.preview.height()
    }

    // ───────────────────────── UI state ─────────────────────────

    pub fn state(&self) -> String {
        let d = &self.doc;
        let layers: Vec<Value> = d
            .layers
            .iter()
            .enumerate()
            .map(|(li, l)| {
                let mut v = json!({
                    "name": l.name, "visible": l.visible, "locked": l.locked, "length": l.length,
                    "outline": l.outline, "color": l.color.hex(),
                    "keys": l.keys.iter().map(|(f, k)| json!({
                        "f": f, "empty": k.shapes.is_empty(),
                        "tween": l.tween_target(*f).is_some(),
                        "shape": k.shape_tween,
                        "broken": k.tweens() && l.tween_target(*f).is_none(),
                        "label": k.label,
                        "actions": !k.actions.trim().is_empty(),
                        "sound": k.sound.as_ref().map(|s| json!(s)),
                        "labelKind": match k.label_kind { LabelKind::Name => "name", LabelKind::Comment => "comment", LabelKind::Anchor => "anchor" },
                    })).collect::<Vec<_>>(),
                });
                if let (Value::Object(m), Value::Object(extra)) = (&mut v, self.layer_state(li)) {
                    m.extend(extra);
                }
                v
            })
            .collect();

        let selected = self.selected_shapes();
        let sel = self.sel_bbox().map(|b| {
            let first = &selected[0].1;
            let same_kind = selected.iter().all(|(_, s)| std::mem::discriminant(&s.geom) == std::mem::discriminant(&first.geom));
            let radius = match (&first.geom, same_kind) {
                (Geometry::Rect { r, .. }, true) => json!(r),
                _ => Value::Null,
            };
            json!({
                "count": selected.len(),
                "kind": if selected.len() == 1 { first.kind_name() } else { "Varios" },
                "x": b.x0, "y": b.y0, "w": b.w(), "h": b.h(),
                "fill": first.fill.map(|c| c.hex()), "fillAlpha": first.fill.map(|c| c.a),
                "grad": first.grad.as_ref().map(|g| json!({ "kind": g.kind, "stops": g.stops, "spread": g.spread, "focal": g.focal })),
                "bmp": first.bmp.as_ref().map(|b| b.id),
                "closed": first.is_closed(),
                "stroke": first.stroke.map(|c| c.hex()), "strokeAlpha": first.stroke.map(|c| c.a),
                "strokeWidth": first.stroke_width, "radius": radius,
            })
        });

        let cur = &d.layers[self.layer.min(d.layers.len() - 1)];
        let frame_info = cur.key_at_or_before(self.frame).filter(|_| self.frame < cur.length).map(|k| {
            let kf = &cur.keys[&k];
            json!({ "key": k, "isKey": k == self.frame, "tween": kf.tween, "ease": kf.ease,
                    "sound": kf.sound.as_ref().map(|s| json!(s)),
                    "spanEnd": cur.span_end(k), "shapes": kf.shapes.len() })
        });

        let mut out = json!({
            "tool": self.tool.name(), "frame": self.frame, "layer": self.layer,
            "total": d.total_frames(), "fps": d.fps, "width": d.width, "height": d.height, "bg": d.bg.hex(),
            "exportHidden": d.export_hidden,
            "zoom": self.zoom / self.dpr * 100.0,
            "canUndo": !self.undo.is_empty() || self.pen.is_some(), "canRedo": !self.redo.is_empty(),
            "fill": self.fill.hex(), "fillAlpha": self.fill.a, "fillOn": self.fill_on,
            "fillGrad": self.fill_grad, "fillBmp": self.fill_bmp, "lockFill": self.lock_fill,
            "guides": { "show": d.guide_opts.show, "lock": d.guide_opts.lock, "snap": d.guide_opts.snap, "color": d.guide_opts.color.hex(), "accuracy": d.guide_opts.accuracy, "count": self.guides_here().len() },
            "polystar": { "style": if self.polystar.0 { "star" } else { "polygon" }, "sides": self.polystar.1, "point": self.polystar.2 },
            "lassoPoly": self.lasso_poly, "lassoActive": !self.lasso_pts.is_empty(),
            "xformMode": self.xform_mode.name(), "warpable": self.warpable(),
            "stroke": self.stroke.hex(), "strokeAlpha": self.stroke.a, "strokeOn": self.stroke_on,
            "strokeWidth": self.stroke_width, "brushSize": self.brush_size, "rectRadius": self.rect_radius,
            "pencilMode": self.pencil_mode.name(), "brushMode": self.brush_mode.name(), "brushTip": self.brush_tip.name(),
            "eraserMode": self.eraser_mode.name(), "eraserTip": self.eraser_tip.name(), "eraserSize": self.eraser_size,
            "faucet": self.faucet, "gap": self.gap,
            "penActive": self.pen.is_some(), "anchors": self.anchors.len(),
            "objectMode": self.object_mode, "floating": self.floating.len(),
            "snapObjects": self.snap_objects, "snapGrid": self.snap_grid, "snapPixels": self.snap_pixels,
            "showGrid": self.show_grid, "gridSize": self.grid_size, "gridColor": self.grid_color.hex(),
            "scopes": d.scopes.iter().map(|s| if s.object { "Objeto de dibujo" } else { "Grupo" }).collect::<Vec<_>>(),
            "selGroup": selected.iter().any(|(_, s)| s.is_group()), "selRaw": selected.iter().any(|(_, s)| s.raw),
            "selObject": selected.iter().any(|(_, s)| !s.raw && !s.is_group()),
            "selInstance": selected.iter().any(|(_, s)| s.is_instance()),
            "onion": self.onion, "clipboard": self.clipboard.len(),
            "layers": layers, "sel": sel, "frameInfo": frame_info,
        });
        for extra in [self.tl_state(), self.sym_state(), self.text_state()] {
            if let (Value::Object(m), Value::Object(extra)) = (&mut out, extra) {
                m.extend(extra);
            }
        }
        out.to_string()
    }

    /// For automated tests: visible shapes of a layer in a frame
    /// (tweened if applicable), with their bounds in the document.
    pub fn debug_frame(&self, layer: usize, frame: u32) -> String {
        if layer >= self.doc.layers.len() {
            return "[]".into();
        }
        let v: Vec<Value> = self
            .doc
            .frame_shapes(layer, frame)
            .iter()
            .map(|s| {
                let b = s.world_bounds();
                let d = s.m.decompose();
                json!({
                    "id": s.id, "kind": s.kind_name(), "raw": s.raw, "group": s.is_group(), "instance": s.inst().map(|i| i.symbol), "tag": s.tag,
                    "fill": s.fill.map(|c| c.hex()), "fillAlpha": s.fill.map(|c| c.a),
                    "grad": s.grad_world().map(|g| json!({ "kind": g.kind, "stops": g.stops, "m": g.m, "focal": g.focal, "spread": g.spread })),
                    "bmp": s.bmp_world().map(|b| json!({ "id": b.id, "m": b.m, "clip": b.clip })),
                    "stroke": s.stroke.map(|c| c.hex()), "strokeWidth": s.stroke_width,
                    "x0": b.x0, "y0": b.y0, "x1": b.x1, "y1": b.y1,
                    "rot": d.rot, "sx": d.sx, "sy": d.sy,
                    "area": paint::region_area(&paint::fill_region(s)),
                    "nodes": s.bez_paths().iter().map(|p| p.nodes.len()).sum::<usize>(),
                    "paths": s.bez_paths().len(),
                    "closed": s.is_closed(),
                    "children": match &s.geom { Geometry::Group { children, .. } => children.len(), _ => 0 },
                })
            })
            .collect();
        Value::Array(v).to_string()
    }

    /// Position in the history: steps that can be undone (counting those
    /// discarded by the limit), for the History panel slider.
    pub fn undo_depth(&self) -> u32 {
        self.hist_base + self.undo.len() as u32
    }

    pub fn undo_floor(&self) -> u32 {
        self.hist_base
    }

    pub fn redo_count(&self) -> u32 {
        self.redo.len() as u32
    }

    pub fn history_epoch(&self) -> u32 {
        self.hist_epoch
    }

    /// Undoes or redoes up to position `depth` (the History slider).
    pub fn history_go(&mut self, depth: u32) {
        self.text_commit();
        let target = depth.clamp(self.hist_base, self.undo_depth() + self.redo.len() as u32);
        while self.undo_depth() > target && !self.undo.is_empty() {
            let d = self.undo.pop().unwrap();
            self.redo.push(std::mem::replace(&mut self.doc, d));
        }
        while self.undo_depth() < target && !self.redo.is_empty() {
            let d = self.redo.pop().unwrap();
            self.undo.push(std::mem::replace(&mut self.doc, d));
        }
        self.after_history();
    }

    /// Number of changes committed to the document (grows with every step).
    pub fn commit_count(&self) -> u32 {
        self.commits
    }

    pub fn take_message(&mut self) -> String {
        std::mem::take(&mut self.message)
    }
}

// ───────────────────────── Shared internals ─────────────────────────

impl Editor {
    fn to_doc(&self, x: f32, y: f32) -> Pt {
        let p = ((x - self.pan.0) / self.zoom, (y - self.pan.1) / self.zoom);
        if self.doc.sym_edit.is_empty() {
            return p;
        }
        self.edit_matrix().invert().map_or(p, |m| m.apply(p.0, p.1))
    }

    /// Stage view without in-place editing (screen ← scene).
    fn stage_matrix(&self) -> Matrix {
        Matrix::translate(self.pan.0, self.pan.1).mul(&Matrix::scale(self.zoom, self.zoom))
    }

    /// Screen ← coordinates of whatever is being edited.
    fn view_matrix(&self) -> Matrix {
        if self.doc.sym_edit.is_empty() {
            return self.stage_matrix();
        }
        self.stage_matrix().mul(&self.edit_matrix())
    }

    /// Size of a screen pixel in document units.
    fn px(&self) -> f32 {
        self.dpr / (self.zoom * self.edit_matrix().avg_scale().max(1e-3))
    }

    fn reset_session(&mut self) {
        self.frame = 0;
        self.layer = 0;
        self.sel.clear();
        self.anchors.clear();
        self.floating.clear();
        self.pen = None;
        self.pivot = None;
        self.undo.clear();
        self.redo.clear();
        self.hist_base = 0;
        self.hist_epoch = self.hist_epoch.wrapping_add(1);
        self.drag = Drag::None;
        self.sym_views.clear();
        self.fit_stage();
        self.sync_lib();
    }

    fn after_history(&mut self) {
        self.env = None;
        self.sync_lib();
        let n = self.doc.sym_edit.len();
        self.sym_views.truncate(n);
        while self.sym_views.len() < n {
            self.sym_views.push((self.pan, self.zoom));
        }
        self.layer = self.layer.min(self.doc.layers.len() - 1);
        let present: Vec<u32> = self.doc.layers.iter().flat_map(|l| l.frame_shapes(self.frame)).map(|s| s.id).collect();
        self.sel.retain(|id| present.contains(id));
        self.floating.retain(|id| present.contains(id));
        self.anchors.clear();
        if let Some(sc) = self.doc.scopes.last() {
            self.layer = sc.layer.min(self.doc.layers.len() - 1);
        }
    }

    fn commit_from(&mut self, base: Document) {
        if base != self.doc {
            self.commits = self.commits.wrapping_add(1);
            self.undo.push(base);
            if self.undo.len() > MAX_UNDO {
                self.undo.remove(0);
                self.hist_base += 1;
            }
            self.redo.clear();
        }
        self.sync_lib();
    }

    fn timeline_edit(&mut self, f: impl FnOnce(&mut Document)) {
        if self.doc.layers[self.layer].locked {
            self.message = "La capa está bloqueada".into();
            return;
        }
        self.pen_finish();
        self.settle();
        self.exit_all_groups();
        let base = self.doc.clone();
        f(&mut self.doc);
        self.sel.clear();
        self.anchors.clear();
        self.commit_from(base);
    }

    fn editable_layers(&self) -> Vec<usize> {
        // Inside a group, only its layer is edited.
        let only = self.doc.scopes.last().map(|s| s.layer);
        let ls = &self.doc.layers;
        (0..ls.len())
            .filter(|&i| {
                ls[i].kind != LayerKind::Folder
                    && timeline::eff_visible(ls, i)
                    && !timeline::eff_locked(ls, i)
                    && only.map_or(true, |l| l == i)
            })
            .collect()
    }

    fn can_draw(&mut self) -> bool {
        let l = &self.doc.layers[self.layer];
        if l.kind == LayerKind::Folder {
            self.message = "Las carpetas no tienen contenido: elige una capa".into();
            false
        } else if timeline::eff_locked(&self.doc.layers, self.layer) {
            self.message = format!("La capa «{}» está bloqueada", l.name);
            false
        } else if !timeline::eff_visible(&self.doc.layers, self.layer) {
            self.message = format!("La capa «{}» está oculta", l.name);
            false
        } else if l.locked {
            self.message = format!("La capa «{}» está bloqueada", l.name);
            false
        } else if !l.visible {
            self.message = format!("La capa «{}» está oculta", l.name);
            false
        } else {
            true
        }
    }

    /// Shapes that can be picked: those of the current frame and, with "Edit
    /// Multiple Frames", also those of the other keyframes in the range.
    fn pick_shapes(&self, li: usize) -> Vec<Shape> {
        let mut v = self.scope_shapes(li);
        if self.edit_multiple {
            let scope = self.scope();
            let (a, b) = (self.frame.saturating_sub(self.onion_before), self.frame + self.onion_after);
            let l = &self.doc.layers[li];
            for (_, k) in l.keys.range(a..=b.min(l.length.saturating_sub(1))) {
                for s in &k.shapes {
                    if s.tag == scope && !v.iter().any(|x| x.id == s.id) {
                        v.push(s.clone());
                    }
                }
            }
        }
        v
    }

    fn hit_top(&self, p: Pt) -> Option<(usize, u32)> {
        let tol = 3.0 * self.px();
        for li in self.editable_layers() {
            for s in self.pick_shapes(li).iter().rev() {
                if s.hit(p.0, p.1, tol) {
                    return Some((li, s.id));
                }
            }
        }
        None
    }

    fn selected_shapes(&self) -> Vec<(usize, Shape)> {
        if self.sel.is_empty() {
            return Vec::new();
        }
        let mut v = Vec::new();
        for li in self.editable_layers() {
            for s in self.pick_shapes(li) {
                if self.sel.contains(&s.id) {
                    v.push((li, s));
                }
            }
        }
        v
    }

    fn sel_bbox(&self) -> Option<BBox> {
        let v = self.selected_shapes();
        if v.is_empty() {
            return None;
        }
        Some(v.iter().fold(BBox::EMPTY, |b, (_, s)| b.union(&s.world_bounds())))
    }

    /// Modifies the given shapes in the current frame, creating the
    /// necessary keyframe in each affected layer.
    fn modify_shapes(&mut self, ids: &[u32], f: impl Fn(&mut Shape)) {
        for li in self.editable_layers() {
            let here = self.doc.frame_shapes(li, self.frame).iter().any(|s| ids.contains(&s.id));
            if !here && !self.edit_multiple {
                continue;
            }
            // With Edit Multiple Frames it may only be in other keyframes of the range.
            let k = if here { self.doc.layers[li].ensure_key(self.frame) } else { u32::MAX };
            if here {
                for s in self.doc.layers[li].keys.get_mut(&k).unwrap().shapes.iter_mut() {
                    if ids.contains(&s.id) {
                        f(s);
                    }
                }
            }
            // Edit Multiple Frames: the change reaches the keyframes in the range.
            if self.edit_multiple {
                let (a, b) = (self.frame.saturating_sub(self.onion_before), self.frame + self.onion_after);
                let others: Vec<u32> = self.doc.layers[li].keys.range(a..=b).map(|(x, _)| *x).filter(|x| *x != k).collect();
                for o in others {
                    for s in self.doc.layers[li].keys.get_mut(&o).unwrap().shapes.iter_mut() {
                        if ids.contains(&s.id) {
                            f(s);
                        }
                    }
                }
            }
        }
    }

    fn edit_selection(&mut self, f: impl Fn(&mut Shape)) {
        if self.sel.is_empty() {
            return;
        }
        let base = self.doc.clone();
        let ids = self.sel.clone();
        self.modify_shapes(&ids, f);
        self.commit_from(base);
    }

    fn paste_shapes(&mut self, shapes: Vec<Shape>, off: Pt) {
        if !self.can_draw() {
            return;
        }
        self.settle();
        let base = self.doc.clone();
        let raw_ids: Vec<bool> = shapes.iter().map(|s| s.raw).collect();
        let moved: Vec<Shape> = shapes
            .into_iter()
            .map(|mut s| {
                s.m.tx += off.0;
                s.m.ty += off.1;
                s
            })
            .collect();
        let ids = self.insert_shapes(moved, None);
        // Pasted raw pieces float above the rest until deselected.
        for (id, raw) in ids.iter().zip(raw_ids) {
            if raw {
                self.floating.push(*id);
            }
        }
        let fl = self.floating.clone();
        let scope = self.scope();
        if let Some(k) = self.doc.layers[self.layer].key_at_or_before(self.frame) {
            normalize_order(&mut self.doc.layers[self.layer].keys.get_mut(&k).unwrap().shapes, scope, &fl);
        }
        self.sel = ids;
        self.anchors.clear();
        self.commit_from(base);
    }

    /// Inserts new shapes into the current layer (`at` = stack position; None = on top).
    fn insert_shapes(&mut self, shapes: Vec<Shape>, at: Option<usize>) -> Vec<u32> {
        let k = self.doc.layers[self.layer].ensure_key(self.frame);
        let scope = self.scope();
        let mut ids = Vec::new();
        // Inside a group, "bottom of everything" is the start of its block.
        let mut pos = at.map(|_| {
            let v = &self.doc.layers[self.layer].keys[&k].shapes;
            v.iter().position(|s| s.tag == scope).unwrap_or(if scope.is_none() { 0 } else { v.len() })
        });
        for mut s in shapes {
            s.id = self.doc.alloc_id();
            s.tag = scope;
            ids.push(s.id);
            let v = &mut self.doc.layers[self.layer].keys.get_mut(&k).unwrap().shapes;
            match pos {
                Some(i) => {
                    v.insert(i.min(v.len()), s);
                    pos = Some(i + 1);
                }
                None => {
                    // On top of everything within the current context.
                    let last = v.iter().rposition(|x| x.tag == scope).map(|i| i + 1);
                    let at = if scope.is_none() { v.len() } else { last.unwrap_or(v.len()) };
                    v.insert(at, s);
                }
            }
        }
        let fl = self.floating.clone();
        normalize_order(&mut self.doc.layers[self.layer].keys.get_mut(&k).unwrap().shapes, scope, &fl);
        ids
    }

}
