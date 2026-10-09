//! Flash 8 text: static, dynamic and input text fields, with
//! real fonts (TrueType/OpenType read with ttf-parser) so that
//! text looks the same in the editor, in "Test Movie" and when exporting.
//!
//! Bundled fonts (SIL Open Font License, see `fonts/OFL.txt`): Arimo,
//! Tinos and Cousine, metric-compatible with Arial, Times and Courier. They are
//! also Flash's device fonts: `_sans`, `_serif` and `_typewriter`.
//! The UI can register more (system fonts or files).

use crate::geom::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextKind {
    #[default]
    Static,
    Dynamic,
    Input,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

/// Line type of dynamic and input fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LineType {
    Single,
    #[default]
    Multi,
    Nowrap,
}

fn def_font() -> String {
    "_sans".into()
}
fn two() -> f32 {
    2.0
}
fn yes() -> bool {
    true
}

/// Text field. Its origin (0, 0) is the top-left corner of the box.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextField {
    #[serde(default)]
    pub text: String,
    #[serde(default = "def_font")]
    pub font: String,
    pub size: f32,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub align: TextAlign,
    /// Letter spacing (px).
    #[serde(default)]
    pub letter: f32,
    /// Line spacing (px between lines, as in Flash).
    #[serde(default = "two")]
    pub leading: f32,
    /// Fixed width (dragging when creating) or automatic (`None`, a single click).
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub kind: TextKind,
    #[serde(default)]
    pub lines: LineType,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub var: String,
    #[serde(default = "yes")]
    pub selectable: bool,
    #[serde(default)]
    pub border: bool,
    #[serde(default)]
    pub max_chars: u32,
    /// "Auto kern" (the font's kerning).
    #[serde(default = "yes")]
    pub kerning: bool,
    /// Dynamic or input text: embed the font outlines in the SWF.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub embed: bool,
    /// Formats within the field (character runs). Empty: everything uses the
    /// field's format and the shape's color.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<Run>,
}

/// Character run with its format.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub len: u32,
    pub font: String,
    pub size: f32,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub letter: f32,
    /// Own color (otherwise, the shape's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<crate::model::Rgba>,
}

impl Run {
    fn same_style(&self, o: &Run) -> bool {
        self.font == o.font && self.size == o.size && self.bold == o.bold && self.italic == o.italic && self.letter == o.letter && self.color == o.color
    }
}

impl Default for TextField {
    fn default() -> Self {
        TextField {
            text: String::new(),
            font: def_font(),
            size: 24.0,
            bold: false,
            italic: false,
            align: TextAlign::Left,
            letter: 0.0,
            leading: 2.0,
            width: None,
            kind: TextKind::Static,
            lines: LineType::Multi,
            name: String::new(),
            var: String::new(),
            selectable: true,
            border: false,
            max_chars: 0,
            kerning: true,
            embed: false,
            runs: Vec::new(),
        }
    }
}

/// Inner padding of Flash text fields.
pub const GUTTER: f32 = 2.0;

// ───────────────────────── Fonts ─────────────────────────

struct FontEntry {
    family: String,
    bold: bool,
    italic: bool,
    data: &'static [u8],
}

macro_rules! font {
    ($fam:expr, $b:expr, $i:expr, $file:expr) => {
        FontEntry { family: $fam.into(), bold: $b, italic: $i, data: include_bytes!(concat!("../fonts/", $file)) }
    };
}

thread_local! {
    static FONTS: RefCell<Vec<FontEntry>> = RefCell::new(vec![
        font!("Arimo", false, false, "Arimo-Regular.ttf"),
        font!("Arimo", true, false, "Arimo-Bold.ttf"),
        font!("Arimo", false, true, "Arimo-Italic.ttf"),
        font!("Arimo", true, true, "Arimo-BoldItalic.ttf"),
        font!("Tinos", false, false, "Tinos-Regular.ttf"),
        font!("Tinos", true, false, "Tinos-Bold.ttf"),
        font!("Cousine", false, false, "Cousine-Regular.ttf"),
        font!("Cousine", true, false, "Cousine-Bold.ttf"),
    ]);
    static CACHE: RefCell<HashMap<String, Rc<Laid>>> = RefCell::new(HashMap::new());
}

/// Flash device fonts → bundled fonts.
pub fn real_family(f: &str) -> &str {
    match f {
        "_sans" => "Arimo",
        "_serif" => "Tinos",
        "_typewriter" => "Cousine",
        _ => f,
    }
}

/// Chosen font and what must be simulated (bold or italic that doesn't exist).
#[derive(Clone, Copy)]
pub struct FontPick {
    pub data: &'static [u8],
    pub fake_bold: bool,
    pub fake_italic: bool,
}

pub fn pick(family: &str, bold: bool, italic: bool) -> FontPick {
    let fam = real_family(family).to_string();
    FONTS.with(|f| {
        let f = f.borrow();
        let fam = if f.iter().any(|e| e.family.eq_ignore_ascii_case(&fam)) { fam } else { "Arimo".to_string() };
        let of: Vec<&FontEntry> = f.iter().filter(|e| e.family.eq_ignore_ascii_case(&fam)).collect();
        if let Some(e) = of.iter().find(|e| e.bold == bold && e.italic == italic) {
            return FontPick { data: e.data, fake_bold: false, fake_italic: false };
        }
        // Without the variant: the closest one, and the rest is simulated.
        let e = of.iter().find(|e| e.bold == bold).or_else(|| of.iter().find(|e| e.italic == italic)).unwrap_or(&of[0]);
        FontPick { data: e.data, fake_bold: bold && !e.bold, fake_italic: italic && !e.italic }
    })
}

/// Font stored in the document (TTF/OTF in base64).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FontAsset {
    pub family: String,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    pub data: String,
}

/// Is this variant already registered?
pub fn has(family: &str, bold: bool, italic: bool) -> bool {
    FONTS.with(|f| f.borrow().iter().any(|e| e.family == family && e.bold == bold && e.italic == italic))
}

/// Registers the fonts stored in a document that are not yet known.
pub fn sync_fonts(fonts: &[FontAsset]) {
    for a in fonts {
        if !has(&a.family, a.bold, a.italic) {
            let _ = register(crate::bitmap::b64_decode(&a.data), &a.family, a.bold, a.italic);
        }
    }
}

/// Registers a font (system or from a file). Returns its family.
pub fn register(data: Vec<u8>, family: &str, bold: bool, italic: bool) -> Result<String, String> {
    ttf_parser::Face::parse(&data, 0).map_err(|e| format!("No es una fuente válida: {e}"))?;
    let data: &'static [u8] = Box::leak(data.into_boxed_slice());
    let family = family.trim().to_string();
    FONTS.with(|f| {
        let mut f = f.borrow_mut();
        f.retain(|e| !(e.family == family && e.bold == bold && e.italic == italic));
        f.push(FontEntry { family: family.clone(), bold, italic, data });
    });
    CACHE.with(|c| c.borrow_mut().clear());
    Ok(family)
}

/// Available families (device fonts first, as in Flash).
pub fn families() -> Vec<String> {
    let mut v: Vec<String> = vec!["_sans".into(), "_serif".into(), "_typewriter".into()];
    FONTS.with(|f| {
        for e in f.borrow().iter() {
            if !v.contains(&e.family) {
                v.push(e.family.clone());
            }
        }
    });
    v
}

/// Data of the font that will be used (so the UI edits with the same typeface).
pub fn font_data(family: &str, bold: bool, italic: bool) -> &'static [u8] {
    pick(family, bold, italic).data
}

// ───────────────────────── Layout ─────────────────────────

#[derive(Clone, Debug)]
pub struct Glyph {
    pub id: u16,
    pub ch: char,
    pub x: f32,
    pub adv: f32,
    /// Format index (into `Laid::styles`).
    pub style: usize,
}

#[derive(Clone, Debug)]
pub struct Line {
    /// Baseline, in field coordinates.
    pub y: f32,
    pub glyphs: Vec<Glyph>,
    pub w: f32,
}

/// Outlines of a single color (a field with several colors has several).
#[derive(Debug)]
pub struct Group {
    /// None: the shape's color.
    pub color: Option<crate::model::Rgba>,
    pub fake_bold: bool,
    pub size: f32,
    pub outline: Vec<Seg>,
}

/// Laid-out text: lines, box and outlines.
#[derive(Debug)]
pub struct Laid {
    pub lines: Vec<Line>,
    pub w: f32,
    pub h: f32,
    pub ascent: f32,
    pub descent: f32,
    /// All outlines together (bounds, selection, export as shape).
    pub outline: Vec<Seg>,
    pub fake_bold: bool,
    pub styles: Vec<Run>,
    pub groups: Vec<Group>,
}

struct Builder<'a> {
    out: &'a mut Vec<Seg>,
    x: f32,
    y: f32,
    k: f32,
    skew: f32,
}

impl Builder<'_> {
    fn p(&self, x: f32, y: f32) -> (f32, f32) {
        let (gx, gy) = (x * self.k, y * self.k);
        (self.x + gx + gy * self.skew, self.y - gy)
    }
}

impl ttf_parser::OutlineBuilder for Builder<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.p(x, y);
        self.out.push(Seg::Move(p.0, p.1));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.p(x, y);
        self.out.push(Seg::Line(p.0, p.1));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (c, p) = (self.p(x1, y1), self.p(x, y));
        self.out.push(Seg::Quad(c.0, c.1, p.0, p.1));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (a, b, p) = (self.p(x1, y1), self.p(x2, y2), self.p(x, y));
        self.out.push(Seg::Cubic(a.0, a.1, b.0, b.1, p.0, p.1));
    }
    fn close(&mut self) {
        self.out.push(Seg::Close);
    }
}

/// Adjustment between two glyphs (font units): pairs from the GPOS
/// "kern" feature or, if absent, the classic "kern" table.
pub fn kerning(face: &ttf_parser::Face, l: u16, r: u16) -> i16 {
    use ttf_parser::gpos::{PairAdjustment, PositioningSubtable};
    let (l, r) = (ttf_parser::GlyphId(l), ttf_parser::GlyphId(r));
    if let Some(gpos) = face.tables().gpos {
        if let Some(feat) = gpos.features.find(ttf_parser::Tag::from_bytes(b"kern")) {
            for li in feat.lookup_indices {
                let Some(lookup) = gpos.lookups.get(li) else { continue };
                for si in 0..lookup.subtables.len() {
                    let Some(PositioningSubtable::Pair(p)) = lookup.subtables.get::<PositioningSubtable>(si) else { continue };
                    match p {
                        PairAdjustment::Format1 { coverage, sets } => {
                            if let Some(v) = coverage.get(l).and_then(|i| sets.get(i)).and_then(|set| set.get(r)) {
                                return v.0.x_advance;
                            }
                        }
                        PairAdjustment::Format2 { coverage, classes, matrix } => {
                            if coverage.get(l).is_some() {
                                if let Some(v) = matrix.get((classes.0.get(l), classes.1.get(r))) {
                                    if v.0.x_advance != 0 {
                                        return v.0.x_advance;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(k) = face.tables().kern {
        for st in k.subtables {
            if st.horizontal && !st.variable {
                if let Some(v) = st.glyphs_kerning(l, r) {
                    return v;
                }
            }
        }
    }
    0
}

struct StyleInfo {
    data: &'static [u8],
    k: f32,
    asc: f32,
    desc: f32,
    skew: f32,
}

impl TextField {
    /// Does it wrap lines when reaching the box width?
    fn wraps(&self) -> bool {
        self.width.is_some() && (self.kind == TextKind::Static || self.lines == LineType::Multi)
    }

    /// Text being laid out (single-line fields ignore line breaks).
    pub fn shown_text(&self) -> String {
        let t = self.text.replace("\r\n", "\n").replace('\r', "\n");
        if self.kind != TextKind::Static && self.lines == LineType::Single {
            t.replace('\n', " ")
        } else {
            t
        }
    }

    /// The field's format as a run.
    pub fn base_run(&self, len: u32) -> Run {
        Run { len, font: self.font.clone(), size: self.size, bold: self.bold, italic: self.italic, letter: self.letter, color: None }
    }

    /// Runs that cover the text exactly (the last one is extended or cut).
    pub fn full_runs(&self) -> Vec<Run> {
        let n = self.text.chars().count() as u32;
        if self.runs.is_empty() {
            return vec![self.base_run(n)];
        }
        let mut out = Vec::new();
        let mut left = n;
        for r in &self.runs {
            if left == 0 {
                break;
            }
            let len = r.len.min(left);
            if len > 0 {
                out.push(Run { len, ..r.clone() });
            }
            left -= len;
        }
        if left > 0 {
            match out.last_mut() {
                Some(r) => r.len += left,
                None => out.push(self.base_run(left)),
            }
        }
        if out.is_empty() {
            out.push(self.base_run(0));
        }
        out
    }

    /// Merges identical runs and, if everything has a single format, moves it to the field.
    pub fn normalize_runs(&mut self) {
        let mut v: Vec<Run> = Vec::new();
        for r in self.full_runs() {
            match v.last_mut() {
                Some(l) if l.same_style(&r) => l.len += r.len,
                _ => v.push(r),
            }
        }
        if v.len() <= 1 {
            if let Some(r) = v.pop() {
                self.font = r.font;
                self.size = r.size;
                self.bold = r.bold;
                self.italic = r.italic;
                self.letter = r.letter;
                if r.color.is_some() {
                    self.runs = vec![Run { len: self.text.chars().count() as u32, ..self.base_run(0) }];
                    self.runs[0].color = r.color;
                    return;
                }
            }
            self.runs.clear();
        } else {
            self.runs = v;
        }
    }

    /// Layout (cached: the editor requests it on every repaint).
    pub fn laid(&self) -> Rc<Laid> {
        let key = serde_json::to_string(self).unwrap_or_default();
        if let Some(l) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
            return l;
        }
        let l = Rc::new(self.compose());
        CACHE.with(|c| {
            let mut c = c.borrow_mut();
            if c.len() > 512 {
                c.clear();
            }
            c.insert(key, l.clone());
        });
        l
    }

    fn compose(&self) -> Laid {
        let text: Vec<char> = self.shown_text().chars().collect();
        let runs = self.full_runs();
        // Distinct formats and the format of each character.
        let mut styles: Vec<Run> = Vec::new();
        let mut style_of: Vec<usize> = Vec::with_capacity(text.len());
        for r in &runs {
            let idx = match styles.iter().position(|s| s.same_style(r)) {
                Some(i) => i,
                None => {
                    styles.push(Run { len: 0, ..r.clone() });
                    styles.len() - 1
                }
            };
            style_of.extend(std::iter::repeat(idx).take(r.len as usize));
        }
        style_of.resize(text.len(), *style_of.last().unwrap_or(&0));
        let info: Vec<StyleInfo> = styles
            .iter()
            .map(|st| {
                let pk = pick(&st.font, st.bold, st.italic);
                let face = ttf_parser::Face::parse(pk.data, 0).expect("fuente válida");
                let k = st.size.clamp(1.0, 2000.0) / face.units_per_em() as f32;
                StyleInfo { data: pk.data, k, asc: face.ascender() as f32 * k, desc: -(face.descender() as f32) * k, skew: if pk.fake_italic { 0.21 } else { 0.0 } }
            })
            .collect();
        let faces: Vec<ttf_parser::Face> = info.iter().map(|i| ttf_parser::Face::parse(i.data, 0).unwrap()).collect();
        let fake_bold: Vec<bool> = styles.iter().map(|st| pick(&st.font, st.bold, st.italic).fake_bold).collect();
        // Glyph and advance of each character (with kerning between pairs of the same format).
        let mut ids = Vec::with_capacity(text.len());
        let mut advs = Vec::with_capacity(text.len());
        for (i, &ch) in text.iter().enumerate() {
            let si = style_of[i];
            let f = &faces[si];
            let id = f.glyph_index(ch).or_else(|| f.glyph_index('?')).map_or(0, |g| g.0);
            ids.push(id);
            advs.push(f.glyph_hor_advance(ttf_parser::GlyphId(id)).unwrap_or(0) as f32 * info[si].k + styles[si].letter);
            if self.kerning && i > 0 && style_of[i - 1] == si && text[i - 1] != '\n' && ch != '\n' {
                let kp = kerning(f, ids[i - 1], id);
                if kp != 0 {
                    advs[i - 1] += kp as f32 * info[si].k;
                }
            }
        }
        let avail = self.width.map(|w| (w - 2.0 * GUTTER).max(1.0));
        // Lines: paragraphs wrapped by words if the field has a fixed width.
        let mut lines: Vec<(Vec<Glyph>, bool, usize)> = Vec::new(); // (glyphs, ends paragraph, format if empty)
        let mut start = 0;
        loop {
            let end = (start..text.len()).find(|&i| text[i] == '\n').unwrap_or(text.len());
            let empty_style = style_of.get(start.min(text.len().saturating_sub(1))).copied().unwrap_or(0);
            let mut cur: Vec<Glyph> = Vec::new();
            let mut x = 0.0;
            let mut i = start;
            while i < end {
                let mut j = i;
                while j < end && text[j] != ' ' {
                    j += 1;
                }
                while j < end && text[j] == ' ' {
                    j += 1;
                }
                let mut wx = 0.0;
                let word: Vec<Glyph> = (i..j)
                    .map(|c| {
                        let g = Glyph { id: ids[c], ch: text[c], x: wx, adv: advs[c], style: style_of[c] };
                        wx += advs[c];
                        g
                    })
                    .collect();
                let ink_w = word.iter().rev().skip_while(|g| g.ch == ' ').map(|g| g.x + g.adv).next().unwrap_or(0.0);
                if let (true, Some(av)) = (self.wraps(), avail) {
                    if !cur.is_empty() && x + ink_w > av {
                        lines.push((std::mem::take(&mut cur), false, empty_style));
                        x = 0.0;
                    }
                    if ink_w > av {
                        // Word longer than the box: it is broken by letters.
                        for g in word {
                            if !cur.is_empty() && x + g.adv > av && g.ch != ' ' {
                                lines.push((std::mem::take(&mut cur), false, empty_style));
                                x = 0.0;
                            }
                            cur.push(Glyph { x, ..g.clone() });
                            x += g.adv;
                        }
                        i = j;
                        continue;
                    }
                }
                for g in word {
                    cur.push(Glyph { x: x + g.x, ..g });
                }
                x += wx;
                i = j;
            }
            lines.push((cur, true, empty_style));
            if end >= text.len() {
                break;
            }
            start = end + 1;
        }
        let ink = |v: &[Glyph]| {
            v.iter().rev().skip_while(|g| g.ch == ' ').map(|g| g.x + g.adv - styles[g.style].letter).next().unwrap_or(0.0).max(0.0)
        };
        let max_w = lines.iter().map(|(v, _, _)| ink(v)).fold(0.0f32, f32::max);
        let inner_w = avail.unwrap_or(max_w);
        let mut group_out: Vec<Group> = Vec::new();
        let mut laid_lines = Vec::new();
        let mut top = GUTTER;
        let mut last_bottom = GUTTER;
        for (gl0, last, es) in lines.into_iter() {
            let mut gl = gl0;
            let asc = gl.iter().map(|g| info[g.style].asc).fold(f32::NAN, f32::max);
            let desc = gl.iter().map(|g| info[g.style].desc).fold(f32::NAN, f32::max);
            let (asc, desc) = if asc.is_nan() { (info[es].asc, info[es].desc) } else { (asc, desc) };
            let y = top + asc;
            let w = ink(&gl);
            let extra = inner_w - w;
            let x0 = GUTTER
                + match self.align {
                    TextAlign::Left | TextAlign::Justify => 0.0,
                    TextAlign::Center => extra / 2.0,
                    TextAlign::Right => extra,
                };
            // Justify: the leftover space is spread among the spaces (except on the paragraph's last line).
            let spaces = gl.iter().filter(|g| g.ch == ' ').count();
            let trailing = gl.iter().rev().take_while(|g| g.ch == ' ').count();
            let gaps = spaces - trailing;
            let per = if self.align == TextAlign::Justify && !last && gaps > 0 && extra > 0.0 { extra / gaps as f32 } else { 0.0 };
            let mut shift = 0.0;
            for g in gl.iter_mut() {
                g.x += x0 + shift;
                if g.ch == ' ' {
                    shift += per;
                }
                let st = &styles[g.style];
                let gi = match group_out.iter().position(|x| x.color == st.color && x.fake_bold == fake_bold[g.style] && x.size == st.size) {
                    Some(i) => i,
                    None => {
                        group_out.push(Group { color: st.color, fake_bold: fake_bold[g.style], size: st.size, outline: Vec::new() });
                        group_out.len() - 1
                    }
                };
                let inf = &info[g.style];
                let mut b = Builder { out: &mut group_out[gi].outline, x: g.x, y, k: inf.k, skew: inf.skew };
                faces[g.style].outline_glyph(ttf_parser::GlyphId(g.id), &mut b);
            }
            laid_lines.push(Line { y, glyphs: gl, w: w + shift });
            last_bottom = y + desc;
            top = y + desc + self.leading;
        }
        let outline: Vec<Seg> = group_out.iter().flat_map(|g| g.outline.iter().copied()).collect();
        let base = &info[0];
        Laid {
            lines: laid_lines,
            w: inner_w + 2.0 * GUTTER,
            h: last_bottom + GUTTER,
            ascent: base.asc,
            descent: base.desc,
            outline,
            fake_bold: fake_bold.iter().any(|b| *b),
            styles,
            groups: group_out,
        }
    }

    pub fn bounds(&self) -> BBox {
        let l = self.laid();
        BBox { x0: 0.0, y0: 0.0, x1: l.w, y1: l.h }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_width_grows_with_text() {
        let a = TextField { text: "Hola".into(), ..Default::default() };
        let b = TextField { text: "Hola mundo".into(), ..Default::default() };
        assert!(b.bounds().w() > a.bounds().w() + 40.0);
        assert!(!a.laid().outline.is_empty());
    }

    #[test]
    fn fixed_width_wraps_and_aligns() {
        let t = TextField { text: "uno dos tres cuatro cinco seis".into(), width: Some(120.0), ..Default::default() };
        let l = t.laid();
        assert!(l.lines.len() >= 3, "{}", l.lines.len());
        assert!(l.lines.iter().all(|ln| ln.w <= 116.5));
        let r = TextField { align: TextAlign::Right, ..t.clone() };
        let lr = r.laid();
        let end = |ln: &Line| ln.glyphs.iter().rev().find(|g| g.ch != ' ').map(|g| g.x + g.adv).unwrap();
        assert!((end(&lr.lines[0]) - 118.0).abs() < 1.0, "alineado a la derecha");
        let j = TextField { align: TextAlign::Justify, ..t };
        let lj = j.laid();
        assert!((end(&lj.lines[0]) - 118.0).abs() < 1.0, "justificado");
    }

    #[test]
    fn fonts_travel_with_the_document() {
        let data = font_data("Arimo", false, false).to_vec();
        let mut e = crate::Editor::new();
        e.register_font(data, "MiFuente", false, false).unwrap();
        // A text uses it: it is stored; if nobody uses it, it isn't.
        assert!(!e.to_json().contains("MiFuente"));
        e.text_style.font = "MiFuente".into();
        let mut s = crate::model::Shape::new(crate::model::Geometry::Text(TextField { text: "x".into(), font: "MiFuente".into(), ..Default::default() }), None, None, 1.0, crate::geom::Matrix::IDENTITY);
        s.id = 99;
        e.doc.layers[0].keys.get_mut(&0).unwrap().shapes.push(s);
        let j = e.to_json();
        assert!(j.contains("\"fonts\""));
        FONTS.with(|f| f.borrow_mut().retain(|e| e.family != "MiFuente"));
        assert!(!has("MiFuente", false, false));
        let mut e2 = crate::Editor::new();
        e2.load_json(&j).unwrap();
        assert!(has("MiFuente", false, false), "se vuelve a registrar al abrir");
    }

    #[test]
    fn kerning_tightens_pairs() {
        let k = TextField { text: "AVAVAVAV".into(), size: 40.0, ..Default::default() };
        let n = TextField { kerning: false, ..k.clone() };
        assert!(k.bounds().w() < n.bounds().w() - 2.0, "{} vs {}", k.bounds().w(), n.bounds().w());
    }

    #[test]
    fn runs_mix_sizes_and_colors() {
        let mut t = TextField { text: "abcdef".into(), ..Default::default() };
        let base = t.bounds();
        t.runs = vec![Run { len: 3, ..t.base_run(3) }, Run { len: 3, size: 40.0, color: Some(crate::model::Rgba::WHITE), ..t.base_run(3) }];
        let l = t.laid();
        assert!(l.h > base.h() + 10.0, "la línea crece con la letra mayor");
        assert_eq!(l.groups.len(), 2);
        assert!(l.groups.iter().any(|g| g.color == Some(crate::model::Rgba::WHITE)));
    }

    #[test]
    fn device_fonts_and_fake_styles() {
        assert!(std::ptr::eq(pick("_serif", false, false).data, pick("Tinos", false, false).data));
        assert!(pick("Tinos", false, true).fake_italic, "Tinos no trae cursiva: se simula");
        assert!(!pick("Arimo", true, true).fake_bold);
        assert!(std::ptr::eq(pick("NoExiste", false, false).data, pick("Arimo", false, false).data));
    }
}
