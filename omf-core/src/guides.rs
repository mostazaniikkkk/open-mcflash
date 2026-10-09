//! Guides (View > Guides): lines dragged out from the rulers, with
//! "Snap to Guides", lock, edit and clear. Each timeline has
//! its own: scenes share the stage's and each symbol has its own.

use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    /// Vertical (comes from the left ruler, fixes an x) or horizontal (fixes a y).
    pub vertical: bool,
    pub pos: f32,
    /// Symbol it belongs to (None: the stage).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sym: Option<u32>,
}

/// View > Guides > Edit Guides.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GuideOpts {
    pub show: bool,
    pub lock: bool,
    pub snap: bool,
    pub color: Rgba,
    /// Snap accuracy: 0 must be close, 1 normal, 2 can be distant.
    pub accuracy: u8,
}

impl Default for GuideOpts {
    fn default() -> Self {
        GuideOpts { show: true, lock: false, snap: true, color: Rgba { r: 0x2c, g: 0xc8, b: 0xf0, a: 255 }, accuracy: 1 }
    }
}

impl Editor {
    fn guide_owner(&self) -> Option<u32> {
        self.doc.sym_edit.last().map(|e| e.symbol)
    }

    /// Indices of the guides of the timeline being edited.
    pub(crate) fn guides_here(&self) -> Vec<usize> {
        let o = self.guide_owner();
        (0..self.doc.guides.len()).filter(|&i| self.doc.guides[i].sym == o).collect()
    }

    /// Guide under the pointer (if visible and not locked).
    pub(crate) fn guide_at(&self, x: f32, y: f32) -> Option<usize> {
        let o = &self.doc.guide_opts;
        if !o.show || o.lock {
            return None;
        }
        let vm = self.view_matrix();
        let r = 3.0 * self.dpr;
        self.guides_here().into_iter().rev().find(|&i| {
            let g = &self.doc.guides[i];
            let s = vm.apply(g.pos, g.pos);
            if g.vertical {
                (x - s.0).abs() <= r
            } else {
                (y - s.1).abs() <= r
            }
        })
    }

    pub(crate) fn guide_drag(&mut self, idx: usize, p: Pt) {
        if let Some(g) = self.doc.guides.get_mut(idx) {
            let v = if g.vertical { p.0 } else { p.1 };
            g.pos = (v * 10.0).round() / 10.0;
        }
    }

    /// Release: over the ruler (off the canvas at the top or left) it is deleted.
    pub(crate) fn guide_drop(&mut self, idx: usize, base: Document, x: f32, y: f32) {
        if let Some(g) = self.doc.guides.get(idx) {
            if (g.vertical && x < 0.0) || (!g.vertical && y < 0.0) {
                self.doc.guides.remove(idx);
            }
        }
        self.commit_from(base);
    }

    /// Snap to guides: each axis separately.
    pub(crate) fn guide_snap(&self, p: Pt) -> Option<(Pt, f32)> {
        let o = &self.doc.guide_opts;
        if !o.snap || !o.show {
            return None;
        }
        let tol = [4.0, 8.0, 14.0][o.accuracy.min(2) as usize] * self.px();
        let (mut q, mut best) = (p, f32::INFINITY);
        let mut hit = false;
        for i in self.guides_here() {
            let g = &self.doc.guides[i];
            let d = if g.vertical { (p.0 - g.pos).abs() } else { (p.1 - g.pos).abs() };
            if d <= tol {
                hit = true;
                best = best.min(d);
                if g.vertical {
                    q.0 = g.pos;
                } else {
                    q.1 = g.pos;
                }
            }
        }
        hit.then_some((q, best))
    }

    pub(crate) fn draw_guides(&self, pm: &mut Pixmap) {
        if !self.doc.guide_opts.show {
            return;
        }
        let vm = self.view_matrix();
        let c = self.doc.guide_opts.color;
        let w = self.dpr.round().max(1.0);
        let (pw, ph) = (pm.width() as f32, pm.height() as f32);
        for i in self.guides_here() {
            let g = &self.doc.guides[i];
            let s = vm.apply(g.pos, g.pos);
            if g.vertical {
                render::fill_rect(pm, s.0.floor(), 0.0, w, ph, c);
            } else {
                render::fill_rect(pm, 0.0, s.1.floor(), pw, w, c);
            }
        }
    }
}

#[wasm_bindgen]
impl Editor {
    /// Starts dragging a new guide from a ruler (`vertical`: the left one).
    /// Movements continue through `pointer_move` / `pointer_up`.
    pub fn guide_begin(&mut self, vertical: bool) {
        if self.doc.guide_opts.lock {
            self.message = "Las guías están bloqueadas (Ver > Guías)".into();
            return;
        }
        let base = self.doc.clone();
        self.doc.guide_opts.show = true;
        self.doc.guides.push(Guide { vertical, pos: -1.0e6, sym: self.guide_owner() });
        self.drag = Drag::Guide { base: Box::new(base), idx: self.doc.guides.len() - 1 };
    }

    /// View > Guides: show, lock, snap, color and accuracy (JSON).
    pub fn set_guide_opts(&mut self, json: &str) {
        match serde_json::from_str::<GuideOpts>(json) {
            Ok(o) => {
                let base = self.doc.clone();
                self.doc.guide_opts = o;
                self.commit_from(base);
            }
            Err(e) => self.message = format!("Opciones de guías no válidas: {e}"),
        }
    }

    /// View > Guides > Clear Guides (those of the current timeline).
    pub fn clear_guides(&mut self) {
        let base = self.doc.clone();
        let o = self.guide_owner();
        self.doc.guides.retain(|g| g.sym != o);
        self.commit_from(base);
    }

    /// Guides of the current timeline, as JSON.
    pub fn guides(&self) -> String {
        let v: Vec<&Guide> = self.guides_here().into_iter().map(|i| &self.doc.guides[i]).collect();
        serde_json::to_string(&v).unwrap_or_default()
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
    fn drag_from_ruler_snap_and_remove() {
        let mut e = editor();
        // Vertical guide from the left ruler to x = 200.
        e.guide_begin(true);
        e.pointer_move(200.0, 300.0, false, false);
        e.pointer_up(200.0, 300.0, false, false);
        assert_eq!(e.doc.guides.len(), 1);
        assert_eq!(e.doc.guides[0].pos, 200.0);
        // A rectangle starting nearby snaps to the guide.
        e.object_mode = true;
        e.set_tool("rect");
        e.pointer_down(195.0, 100.0, 0, false, false);
        e.pointer_up(300.0, 150.0, false, false);
        let b = e.doc.frame_shapes(0, 0)[0].world_bounds();
        assert!((b.x0 - 200.0).abs() < 0.01, "{b:?}");
        // It moves with the arrow tool, and dropped over the ruler it disappears.
        e.set_tool("select");
        e.pointer_down(200.0, 400.0, 0, false, false);
        e.pointer_move(250.0, 400.0, false, false);
        assert_eq!(e.doc.guides[0].pos, 250.0);
        e.pointer_up(-5.0, 400.0, false, false);
        assert!(e.doc.guides.is_empty());
        e.undo();
        assert_eq!(e.doc.guides[0].pos, 200.0);
        // Locked: they don't move.
        e.set_guide_opts(r#"{"show":true,"lock":true,"snap":true,"color":{"r":0,"g":0,"b":0,"a":255},"accuracy":1}"#);
        assert!(e.guide_at(200.0, 50.0).is_none());
    }

    #[test]
    fn each_symbol_has_its_own_guides() {
        let mut e = editor();
        e.guide_begin(false);
        e.pointer_up(10.0, 120.0, false, false);
        let sid = e.new_symbol("S", "movieclip");
        assert_eq!(e.guides(), "[]");
        e.guide_begin(false);
        e.pointer_up(10.0, 50.0, false, false);
        assert_eq!(e.guides_here().len(), 1);
        e.edit_document();
        let v: Vec<Guide> = serde_json::from_str(&e.guides()).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].pos, 120.0);
        let _ = sid;
        e.clear_guides();
        assert_eq!(e.doc.guides.len(), 1, "solo se borran las de la línea de tiempo actual");
    }
}
