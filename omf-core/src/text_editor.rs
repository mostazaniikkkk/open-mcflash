//! Text tool (T): create fields (click = automatic width, drag =
//! fixed width), edit them, change their formatting and break them apart (Ctrl+B).
//!
//! While typing, the UI overlays an editable box with the
//! same font (cursor, selection, clipboard and accents are handled by the
//! browser) and the core stops drawing that field; when done, the text
//! goes back to the document in a single undo step.

use super::*;
use crate::text::{LineType, TextAlign, TextField, TextKind};

pub(crate) struct TextEdit {
    pub id: u32,
    pub base: Box<Document>,
    /// Selection within the text (characters).
    pub sel: (u32, u32),
}

/// Runs after changing the text from `old` to `new`: deleted characters leave their
/// run and typed ones take the formatting of the preceding character.
pub(crate) fn retarget_runs(runs: &[crate::text::Run], old: &str, new: &str) -> Vec<crate::text::Run> {
    let (o, n): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
    let pre = o.iter().zip(&n).take_while(|(a, b)| a == b).count();
    let max_suf = o.len().min(n.len()) - pre;
    let suf = o.iter().rev().zip(n.iter().rev()).take(max_suf).take_while(|(a, b)| a == b).count();
    let (del, ins) = ((o.len() - pre - suf) as u32, (n.len() - pre - suf) as u32);
    let mut out: Vec<crate::text::Run> = Vec::new();
    let mut pos = 0u32;
    let mut inserted = false;
    for r in runs {
        let (a, b) = (pos, pos + r.len);
        pos = b;
        // Characters of the run that are not deleted.
        let cut_a = (pre as u32).clamp(a, b);
        let cut_b = (pre as u32 + del).clamp(a, b);
        let mut len = r.len - (cut_b - cut_a);
        // Typed text goes into the run holding the preceding character (or into the first one).
        if !inserted && (pre as u32 <= b) && (pre == 0 || pre as u32 > a) {
            len += ins;
            inserted = true;
        }
        out.push(crate::text::Run { len, ..r.clone() });
    }
    if !inserted {
        if let Some(l) = out.last_mut() {
            l.len += ins;
        }
    }
    out.retain(|r| r.len > 0);
    out
}

/// Applies `f` to the formatting of characters [a, b) of a field.
pub(crate) fn format_range(tf: &mut TextField, a: u32, b: u32, f: impl Fn(&mut crate::text::Run)) {
    let mut out = Vec::new();
    let mut pos = 0u32;
    for r in tf.full_runs() {
        let (s, e) = (pos, pos + r.len);
        pos = e;
        let (ia, ib) = (a.clamp(s, e), b.clamp(s, e));
        for (x, y, inside) in [(s, ia, false), (ia, ib, true), (ib, e, false)] {
            if y > x {
                let mut piece = crate::text::Run { len: y - x, ..r.clone() };
                if inside {
                    f(&mut piece);
                }
                out.push(piece);
            }
        }
    }
    tf.runs = out;
    tf.normalize_runs();
}

fn text_of(s: &Shape) -> Option<&TextField> {
    match &s.geom {
        Geometry::Text(t) => Some(t),
        _ => None,
    }
}

impl Editor {
    pub(crate) fn text_down(&mut self, p: Pt) {
        if self.text_edit.is_some() {
            self.text_commit();
        }
        // Click on a field: edit it.
        let hit = self.hit_top(p).filter(|(li, id)| self.scope_shapes(*li).iter().any(|s| s.id == *id && text_of(s).is_some()));
        if let Some((li, id)) = hit {
            self.begin_text_edit(li, id);
            return;
        }
        if self.can_draw() {
            self.sel.clear();
            self.drag = Drag::TextBox { start: p, cur: p };
        }
    }

    pub(crate) fn begin_text_edit(&mut self, li: usize, id: u32) {
        self.settle();
        self.layer = li;
        self.sel = vec![id];
        self.tool = Tool::Text;
        let n = self.doc.frame_shapes(li, self.frame).iter().find(|s| s.id == id).and_then(|s| text_of(s).map(|t| t.text.chars().count() as u32)).unwrap_or(0);
        self.text_edit = Some(TextEdit { id, base: Box::new(self.doc.clone()), sel: (n, n) });
    }

    /// Creates an empty field and leaves it in editing mode.
    pub(crate) fn create_text(&mut self, start: Pt, cur: Pt) {
        let base = self.doc.clone();
        let w = (cur.0 - start.0).abs();
        let mut tf = TextField { text: String::new(), ..self.text_style.clone() };
        tf.width = (w > 4.0).then_some(w);
        let at = (start.0.min(cur.0), start.1);
        let mut s = Shape::new(Geometry::Text(tf), Some(self.text_color()), None, 1.0, Matrix::translate(at.0, at.1));
        s.raw = false;
        let ids = self.insert_shapes(vec![s], None);
        self.sel = ids.clone();
        self.text_edit = Some(TextEdit { id: ids[0], base: Box::new(base), sel: (0, 0) });
    }

    fn text_color(&self) -> Rgba {
        self.text_rgba
    }

    /// Text fields affected by a format change: the one being edited or the selected ones.
    fn text_targets(&self) -> Vec<u32> {
        if let Some(t) = &self.text_edit {
            return vec![t.id];
        }
        self.selected_shapes().into_iter().filter(|(_, s)| text_of(s).is_some()).map(|(_, s)| s.id).collect()
    }

    fn edited_shape(&self) -> Option<Shape> {
        let id = self.text_edit.as_ref()?.id;
        self.doc.frame_shapes(self.layer, self.frame).into_iter().find(|s| s.id == id)
    }

    /// Format shown by the panel: that of the edited or selected field, or the tool's.
    pub(crate) fn text_state(&self) -> Value {
        let shown = self
            .edited_shape()
            .or_else(|| self.selected_shapes().into_iter().map(|(_, s)| s).find(|s| text_of(s).is_some()));
        let (mut tf, mut color) = match &shown {
            Some(s) => (text_of(s).unwrap().clone(), s.fill.unwrap_or(Rgba::BLACK)),
            None => (self.text_style.clone(), self.text_color()),
        };
        if let (Some(te), false) = (&self.text_edit, tf.runs.is_empty()) {
            let at = te.sel.0;
            let mut acc = 0;
            for r in tf.full_runs() {
                if at < acc + r.len.max(1) {
                    tf.font = r.font.clone();
                    tf.size = r.size;
                    tf.bold = r.bold;
                    tf.italic = r.italic;
                    tf.letter = r.letter;
                    if let Some(c) = r.color {
                        color = c;
                    }
                    break;
                }
                acc += r.len;
            }
        }
        json!({
            "textEdit": self.text_edit.is_some(),
            "textSel": shown.is_some(),
            "textStyle": tf, "textColor": color.hex(), "textAlpha": color.a,
            "fonts": crate::text::families(),
        })
    }

    /// Ctrl+B on text: several letters → one field per letter; one letter → shapes.
    pub(crate) fn break_text(&self, s: &Shape) -> Vec<Shape> {
        let Some(tf) = text_of(s) else { return Vec::new() };
        let laid = tf.laid();
        let chars: Vec<(&crate::text::Line, &crate::text::Glyph)> = laid.lines.iter().flat_map(|l| l.glyphs.iter().map(move |g| (l, g))).filter(|(_, g)| !g.ch.is_whitespace()).collect();
        if chars.len() > 1 {
            return chars
                .into_iter()
                .map(|(l, g)| {
                    let st = &laid.styles[g.style];
                    let one = TextField {
                        text: g.ch.to_string(),
                        width: None,
                        align: TextAlign::Left,
                        kind: TextKind::Static,
                        name: String::new(),
                        var: String::new(),
                        runs: Vec::new(),
                        font: st.font.clone(),
                        size: st.size,
                        bold: st.bold,
                        italic: st.italic,
                        letter: st.letter,
                        ..tf.clone()
                    };
                    // The letter stays where it was: its origin moves to the glyph position.
                    let asc = one.laid().ascent;
                    let off = Matrix::translate(g.x - crate::text::GUTTER, l.y - asc - crate::text::GUTTER);
                    Shape { id: 0, geom: Geometry::Text(one), m: s.m.mul(&off), fill: st.color.or(s.fill), ..s.clone() }
                })
                .collect();
        }
        // A single letter: raw shapes with their outline.
        let r = paint::fill_region(s);
        let mut v = paint::fill_pieces(&r, s.fill.unwrap_or(Rgba::BLACK));
        for p in &mut v {
            p.id = 0;
        }
        v
    }
}

#[wasm_bindgen]
impl Editor {
    /// Text of the field being edited (every keystroke; creates no undo steps).
    pub fn text_set(&mut self, text: &str) {
        let Some(id) = self.text_edit.as_ref().map(|t| t.id) else { return };
        let t = text.replace("\r\n", "\n").replace('\r', "\n");
        self.modify_shapes(&[id], |s| {
            if let Geometry::Text(tf) = &mut s.geom {
                let mut new = t.clone();
                if tf.max_chars > 0 && tf.kind == TextKind::Input {
                    new = new.chars().take(tf.max_chars as usize).collect();
                }
                if !tf.runs.is_empty() {
                    tf.runs = retarget_runs(&tf.full_runs(), &tf.text, &new);
                }
                tf.text = new;
                tf.normalize_runs();
            }
        });
    }

    /// Selection within the text being typed (in characters), so that
    /// formatting applies to it and the panel shows the format at its start.
    pub fn text_select(&mut self, start: u32, end: u32) {
        if let Some(te) = &mut self.text_edit {
            te.sel = (start.min(end), start.max(end));
        }
    }

    /// Finishes typing: an empty field disappears (as in Flash).
    pub fn text_commit(&mut self) {
        let Some(te) = self.text_edit.take() else { return };
        let empty = self.edited_shape_by(te.id).map_or(true, |s| text_of(&s).map_or(true, |t| t.text.trim().is_empty()));
        if empty {
            for li in 0..self.doc.layers.len() {
                if let Some(k) = self.doc.layers[li].key_at_or_before(self.frame) {
                    self.doc.layers[li].keys.get_mut(&k).unwrap().shapes.retain(|s| s.id != te.id);
                }
            }
            self.sel.clear();
        } else {
            self.sel = vec![te.id];
        }
        self.commit_from(*te.base);
    }

    /// Data for the UI edit box (position in CSS pixels).
    pub fn text_edit_info(&self) -> String {
        let Some(s) = self.edited_shape() else { return "null".into() };
        let Some(tf) = text_of(&s) else { return "null".into() };
        let m = self.view_matrix().mul(&s.m);
        let k = 1.0 / self.dpr;
        let pick = crate::text::pick(&tf.font, tf.bold, tf.italic);
        let face = ttf_parser::Face::parse(pick.data, 0).ok();
        let line_h = face.map_or(tf.size * 1.15, |f| (f.ascender() - f.descender()) as f32 * tf.size / f.units_per_em() as f32);
        let c = s.fill.unwrap_or(Rgba::BLACK);
        json!({
            "id": s.id, "text": tf.text, "font": crate::text::real_family(&tf.font), "fontKey": tf.font,
            "bold": tf.bold, "italic": tf.italic, "fakeBold": pick.fake_bold, "fakeItalic": pick.fake_italic,
            "size": tf.size, "align": tf.align, "letter": tf.letter, "leading": tf.leading, "lineHeight": line_h,
            "width": tf.width, "kind": tf.kind, "lines": tf.lines, "maxChars": tf.max_chars,
            "color": c.hex(), "alpha": c.a,
            "runs": if tf.runs.is_empty() { Vec::new() } else { tf.full_runs().iter().map(|r| {
                let p = crate::text::pick(&r.font, r.bold, r.italic);
                let rc = r.color.unwrap_or(c);
                json!({ "len": r.len, "font": crate::text::real_family(&r.font), "fontKey": r.font, "size": r.size, "bold": r.bold, "italic": r.italic,
                        "fakeBold": p.fake_bold, "fakeItalic": p.fake_italic, "letter": r.letter, "color": rc.hex(), "alpha": rc.a })
            }).collect::<Vec<_>>() },
            "kerning": tf.kerning,
            "matrix": [m.a * k, m.b * k, m.c * k, m.d * k, m.tx * k, m.ty * k],
        })
        .to_string()
    }

    /// Changes the format (partial JSON: font, size, bold, italic, align, letter,
    /// leading, kind, lines, name, var, selectable, border, maxChars, width) of
    /// the affected fields and of the tool.
    pub fn set_text_style(&mut self, json: &str) {
        let Ok(Value::Object(patch)) = serde_json::from_str::<Value>(json) else {
            self.message = "Formato de texto no válido".into();
            return;
        };
        let patch2 = patch.clone();
        let apply = move |tf: &mut TextField| {
            for (k, v) in &patch {
                match k.as_str() {
                    "font" => tf.font = v.as_str().unwrap_or(&tf.font).to_string(),
                    "size" => tf.size = v.as_f64().map_or(tf.size, |x| (x as f32).clamp(1.0, 2000.0)),
                    "bold" => tf.bold = v.as_bool().unwrap_or(tf.bold),
                    "italic" => tf.italic = v.as_bool().unwrap_or(tf.italic),
                    "align" => tf.align = serde_json::from_value(v.clone()).unwrap_or(tf.align),
                    "letter" => tf.letter = v.as_f64().map_or(tf.letter, |x| (x as f32).clamp(-60.0, 60.0)),
                    "leading" => tf.leading = v.as_f64().map_or(tf.leading, |x| (x as f32).clamp(-360.0, 720.0)),
                    "kind" => tf.kind = serde_json::from_value(v.clone()).unwrap_or(tf.kind),
                    "lines" => tf.lines = serde_json::from_value::<LineType>(v.clone()).unwrap_or(tf.lines),
                    "name" => tf.name = v.as_str().unwrap_or("").trim().to_string(),
                    "var" => tf.var = v.as_str().unwrap_or("").trim().to_string(),
                    "selectable" => tf.selectable = v.as_bool().unwrap_or(tf.selectable),
                    "border" => tf.border = v.as_bool().unwrap_or(tf.border),
                    "maxChars" => tf.max_chars = v.as_u64().unwrap_or(0) as u32,
                    "width" => tf.width = v.as_f64().map(|x| (x as f32).max(8.0)),
                    "kerning" => tf.kerning = v.as_bool().unwrap_or(tf.kerning),
                    "embed" => tf.embed = v.as_bool().unwrap_or(tf.embed),
                    _ => {}
                }
            }
        };
        // With part of the text selected, character formatting goes to that part.
        let range = self.text_edit.as_ref().map(|t| t.sel).filter(|(a, b)| b > a);
        if let (Some((a, b)), Some(id)) = (range, self.text_edit.as_ref().map(|t| t.id)) {
            let char_keys = ["font", "size", "bold", "italic", "letter"];
            let (chars, para): (serde_json::Map<String, Value>, serde_json::Map<String, Value>) = patch2.clone().into_iter().partition(|(k, _)| char_keys.contains(&k.as_str()));
            if !chars.is_empty() {
                self.modify_shapes(&[id], |s| {
                    if let Geometry::Text(tf) = &mut s.geom {
                        format_range(tf, a, b, |r| {
                            for (k, v) in &chars {
                                match k.as_str() {
                                    "font" => r.font = v.as_str().unwrap_or(&r.font).to_string(),
                                    "size" => r.size = v.as_f64().map_or(r.size, |x| (x as f32).clamp(1.0, 2000.0)),
                                    "bold" => r.bold = v.as_bool().unwrap_or(r.bold),
                                    "italic" => r.italic = v.as_bool().unwrap_or(r.italic),
                                    "letter" => r.letter = v.as_f64().map_or(r.letter, |x| (x as f32).clamp(-60.0, 60.0)),
                                    _ => {}
                                }
                            }
                        });
                    }
                });
            }
            if para.is_empty() {
                return;
            }
            return self.set_text_style(&Value::Object(para).to_string());
        }
        let mut style = self.text_style.clone();
        apply(&mut style);
        // The name and variable belong to each field, not to the tool.
        style.name.clear();
        style.var.clear();
        style.text.clear();
        style.width = None;
        self.text_style = style;
        let ids = self.text_targets();
        if ids.is_empty() {
            return;
        }
        let editing = self.text_edit.is_some();
        let base = self.doc.clone();
        self.modify_shapes(&ids, |s| {
            if let Geometry::Text(tf) = &mut s.geom {
                apply(tf);
                // What changes for the whole field applies to all its runs.
                if !tf.runs.is_empty() {
                    let all = tf.text.chars().count() as u32;
                    let p = patch2.clone();
                    format_range(tf, 0, all, |r| {
                        for (k, v) in &p {
                            match k.as_str() {
                                "font" => r.font = v.as_str().unwrap_or(&r.font).to_string(),
                                "size" => r.size = v.as_f64().map_or(r.size, |x| (x as f32).clamp(1.0, 2000.0)),
                                "bold" => r.bold = v.as_bool().unwrap_or(r.bold),
                                "italic" => r.italic = v.as_bool().unwrap_or(r.italic),
                                "letter" => r.letter = v.as_f64().map_or(r.letter, |x| (x as f32).clamp(-60.0, 60.0)),
                                _ => {}
                            }
                        }
                    });
                }
            }
        });
        // While editing, everything goes into the same undo step as the text.
        if !editing {
            self.commit_from(base);
        }
    }

    /// Text color (the tool's and that of the affected fields).
    pub fn set_text_color(&mut self, hex: &str, alpha: u8) {
        let Some(c) = Rgba::parse(hex, alpha) else { return };
        self.text_rgba = c;
        if let Some((id, (a, b))) = self.text_edit.as_ref().map(|t| (t.id, t.sel)).filter(|(_, (a, b))| b > a) {
            self.modify_shapes(&[id], |s| {
                if let Geometry::Text(tf) = &mut s.geom {
                    format_range(tf, a, b, |r| r.color = Some(c));
                }
            });
            return;
        }
        let ids = self.text_targets();
        if ids.is_empty() {
            return;
        }
        let editing = self.text_edit.is_some();
        let base = self.doc.clone();
        self.modify_shapes(&ids, |s| {
            s.fill = Some(c);
            s.grad = None;
            if let Geometry::Text(tf) = &mut s.geom {
                for r in tf.runs.iter_mut() {
                    r.color = None;
                }
                tf.normalize_runs();
            }
        });
        if !editing {
            self.commit_from(base);
        }
    }

    /// Registers a font (file or system font) for use in text.
    pub fn register_font(&mut self, data: Vec<u8>, family: &str, bold: bool, italic: bool) -> Result<String, JsValue> {
        let b64 = crate::bitmap::b64_encode(&data);
        let fam = crate::text::register(data, family, bold, italic).map_err(|e| JsValue::from_str(&e))?;
        // Saved with the document (no undo step: the drawing doesn't change).
        self.doc.fonts.retain(|f| !(f.family == fam && f.bold == bold && f.italic == italic));
        self.doc.fonts.push(crate::text::FontAsset { family: fam.clone(), bold, italic, data: b64 });
        Ok(fam)
    }

    /// Fonts saved in the document (JSON: family and variants).
    pub fn doc_fonts(&self) -> String {
        serde_json::to_string(&self.doc.fonts.iter().map(|f| json!({ "family": f.family, "bold": f.bold, "italic": f.italic, "bytes": f.data.len() * 3 / 4 })).collect::<Vec<_>>()).unwrap_or_default()
    }

    /// Bytes of the font to use for a family and style.
    pub fn font_data(&self, family: &str, bold: bool, italic: bool) -> Vec<u8> {
        crate::text::font_data(family, bold, italic).to_vec()
    }
}

impl Editor {
    fn edited_shape_by(&self, id: u32) -> Option<Shape> {
        (0..self.doc.layers.len()).flat_map(|li| self.doc.frame_shapes(li, self.frame)).find(|s| s.id == id)
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

    fn type_text(e: &mut Editor, at: Pt, to: Pt, text: &str) -> u32 {
        e.set_tool("text");
        e.pointer_down(at.0, at.1, 0, false, false);
        e.pointer_up(to.0, to.1, false, false);
        e.text_set(text);
        let id = e.text_edit.as_ref().unwrap().id;
        e.text_commit();
        id
    }

    #[test]
    fn click_types_and_one_undo_step() {
        let mut e = editor();
        type_text(&mut e, (100.0, 100.0), (100.0, 100.0), "Hola");
        let s = &e.doc.frame_shapes(0, 0)[0];
        assert_eq!(s.kind_name(), "Texto estático");
        assert!(s.world_bounds().w() > 40.0);
        e.undo();
        assert!(e.doc.frame_shapes(0, 0).is_empty(), "crear y escribir es un solo paso");
        // A field left empty disappears.
        type_text(&mut e, (100.0, 100.0), (100.0, 100.0), "  ");
        assert!(e.doc.frame_shapes(0, 0).is_empty());
    }

    #[test]
    fn range_format_and_typing_keeps_runs() {
        let mut e = editor();
        e.set_tool("text");
        e.pointer_down(100.0, 100.0, 0, false, false);
        e.pointer_up(100.0, 100.0, false, false);
        e.text_set("Hola mundo");
        e.text_select(5, 10);
        e.set_text_style(r#"{"size":40,"bold":true}"#);
        e.set_text_color("#ff0000", 255);
        let tf = |e: &Editor| match &e.doc.frame_shapes(0, 0)[0].geom { Geometry::Text(t) => t.clone(), _ => panic!() };
        let t = tf(&e);
        assert_eq!(t.runs.len(), 2, "{:?}", t.runs);
        assert_eq!((t.runs[0].len, t.runs[1].len), (5, 5));
        assert!(t.runs[1].bold && t.runs[1].size == 40.0 && t.runs[1].color.unwrap().r == 255);
        assert!(!t.runs[0].bold && t.size != 40.0, "el resto no cambia");
        // Typing at the end inherits the format of the last letter; at the start, that of the first.
        e.text_set("Hola mundo!");
        e.text_set("¡Hola mundo!");
        let t = tf(&e);
        assert_eq!((t.runs[0].len, t.runs[1].len), (6, 6));
        // Deleting the whole large run removes it.
        e.text_set("¡Hola ");
        assert!(tf(&e).runs.len() <= 1);
        e.text_commit();
        e.undo();
        assert!(e.doc.frame_shapes(0, 0).is_empty(), "todo en un paso");
    }

    #[test]
    fn rich_text_exports_to_swf() {
        let mut e = editor();
        e.set_tool("text");
        e.pointer_down(100.0, 100.0, 0, false, false);
        e.pointer_up(100.0, 100.0, false, false);
        e.text_set("Rojo azul");
        e.text_select(0, 4);
        e.set_text_color("#ff0000", 255);
        e.text_commit();
        let codes = |e: &mut Editor| crate::swf::tests_support::tag_codes(&crate::swf::uncompressed(&e.export_swf()));
        let c = codes(&mut e);
        assert_eq!(c.iter().filter(|x| **x == 2 || **x == 32 || **x == 83).count(), 2, "una forma por color: {c:?}");
        // Dynamic with formatting: HTML; with an embedded font, DefineFont3.
        e.set_text_style(r#"{"kind":"dynamic","embed":true}"#);
        let swf = crate::swf::uncompressed(&e.export_swf());
        let c = crate::swf::tests_support::tag_codes(&swf);
        assert!(c.contains(&37) && c.contains(&75), "{c:?}");
        let s = String::from_utf8_lossy(&swf);
        let at = s.find("<P").unwrap_or(0);
        assert!(s.contains("<FONT FACE=\"Arimo\"") && s.contains("COLOR=\"#FF0000\" LETTERSPACING=\"0\" KERNING=\"1\">Rojo</FONT>"), "HTML con el color del tramo: {}", &s[at..(at + 400).min(s.len())]);
    }

    #[test]
    fn break_apart_keeps_each_letter_style() {
        let mut e = editor();
        e.set_tool("text");
        e.pointer_down(100.0, 100.0, 0, false, false);
        e.pointer_up(100.0, 100.0, false, false);
        e.text_set("ab");
        e.text_select(1, 2);
        e.set_text_style(r#"{"size":50}"#);
        e.set_text_color("#00ff00", 255);
        e.text_commit();
        e.sel = vec![e.doc.frame_shapes(0, 0)[0].id];
        e.break_apart();
        let v = e.doc.frame_shapes(0, 0);
        let b = v.iter().find(|s| matches!(&s.geom, Geometry::Text(t) if t.text == "b")).unwrap();
        let Geometry::Text(tb) = &b.geom else { panic!() };
        assert_eq!(tb.size, 50.0);
        assert_eq!(b.fill.unwrap().g, 255);
    }

    #[test]
    fn style_changes_selected_text_and_break_apart() {
        let mut e = editor();
        type_text(&mut e, (100.0, 100.0), (300.0, 100.0), "AB C");
        let w0 = e.doc.frame_shapes(0, 0)[0].world_bounds().w();
        assert!((w0 - 200.0).abs() < 0.5, "ancho fijo");
        e.set_text_style(r#"{"size":48,"bold":true,"align":"center","kind":"dynamic","name":"titulo"}"#);
        let s = e.doc.frame_shapes(0, 0)[0].clone();
        let Geometry::Text(tf) = &s.geom else { panic!() };
        assert!(tf.bold && tf.size == 48.0 && tf.kind == TextKind::Dynamic && tf.name == "titulo");
        e.set_text_style(r#"{"kind":"static"}"#);
        e.break_apart();
        let v = e.doc.frame_shapes(0, 0);
        assert_eq!(v.len(), 3, "una letra por campo");
        // The broken-apart "A" stays in place.
        let a = v.iter().find(|s| matches!(&s.geom, Geometry::Text(t) if t.text == "A")).unwrap();
        let ga = tf.laid().lines[0].glyphs[0].x;
        assert!((a.world_bounds().x0 - (s.m.tx + ga - 2.0)).abs() < 0.5);
        // Ctrl+B again: shapes.
        e.sel = vec![a.id];
        e.break_apart();
        assert!(e.doc.frame_shapes(0, 0).iter().any(|s| s.raw && s.fill.is_some()));
    }
}
