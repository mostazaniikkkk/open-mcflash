//! Advanced timeline commands in the editor: frame ranges,
//! tween types and their options, labels, shape hints,
//! special layers (guides, masks, folders), onion skin and scenes.

use super::*;
use crate::timeline::{self, Cell};

impl Editor {
    /// Selected range (or the current cell): (layer0, layer1, frame0, frame1).
    pub(crate) fn sel_range(&self) -> (usize, usize, u32, u32) {
        let n = self.doc.layers.len().saturating_sub(1);
        match self.frame_sel {
            Some((l0, l1, f0, f1)) => (l0.min(n), l1.min(n), f0, f1),
            None => (self.layer, self.layer, self.frame, self.frame),
        }
    }

    /// Applies `f` to each layer in the range that is not locked (nor a folder).
    fn range_edit(&mut self, f: impl Fn(&mut Layer, u32, u32)) {
        let (l0, l1, f0, f1) = self.sel_range();
        self.pen_finish();
        self.settle();
        self.exit_all_groups();
        let base = self.doc.clone();
        let mut locked = false;
        for li in l0..=l1 {
            if timeline::eff_locked(&self.doc.layers, li) {
                locked = true;
                continue;
            }
            if self.doc.layers[li].kind == LayerKind::Folder {
                continue;
            }
            f(&mut self.doc.layers[li], f0, f1);
        }
        if locked {
            self.message = "Las capas bloqueadas no se han modificado".into();
        }
        self.sel.clear();
        self.anchors.clear();
        self.commit_from(base);
    }

    /// Settings of the keyframes covering the range, on the current layer.
    fn key_edit(&mut self, f: impl Fn(&mut Keyframe)) {
        let (l0, l1, f0, f1) = self.sel_range();
        let base = self.doc.clone();
        for li in l0..=l1 {
            let l = &mut self.doc.layers[li];
            let first = l.key_at_or_before(f0);
            let keys: Vec<u32> = l.keys.range(..=f1).map(|(k, _)| *k).filter(|k| *k >= first.unwrap_or(0) && *k < l.length).collect();
            for k in keys {
                f(l.keys.get_mut(&k).unwrap());
            }
        }
        self.commit_from(base);
    }

    fn move_layer_block(&mut self, from: usize, to: usize) {
        // A folder, mask or guide moves along with the layers that hang from it.
        let uid = self.doc.layers[from].uid;
        let mut end = from + 1;
        while end < self.doc.layers.len() && self.descends(end, uid) {
            end += 1;
        }
        if to >= from && to < end {
            return;
        }
        let block: Vec<Layer> = self.doc.layers.drain(from..end).collect();
        let n = block.len();
        let at = if to > from { (to + 1).saturating_sub(n) } else { to }.min(self.doc.layers.len());
        for (i, l) in block.into_iter().enumerate() {
            self.doc.layers.insert(at + i, l);
        }
        // The parent is inferred from the position, as when dragging in Flash.
        let parent = if at == 0 {
            None
        } else {
            let above = &self.doc.layers[at - 1];
            match above.kind {
                LayerKind::Mask | LayerKind::MotionGuide => Some(above.uid),
                LayerKind::Folder if above.open => Some(above.uid),
                _ => above.parent,
            }
        };
        let moved_uid = self.doc.layers[at].uid;
        if parent != Some(moved_uid) {
            let kind = self.doc.layers[at].kind;
            // A mask or guide does not go inside another mask or guide.
            let ok = parent.and_then(|p| timeline::layer_index(&self.doc.layers, p)).map_or(true, |pi| {
                self.doc.layers[pi].kind == LayerKind::Folder || matches!(kind, LayerKind::Normal | LayerKind::Guide)
            });
            if ok {
                self.doc.layers[at].parent = parent;
            }
        }
        self.layer = at;
    }

    fn descends(&self, li: usize, ancestor: u32) -> bool {
        let mut p = self.doc.layers[li].parent;
        let mut guard = 0;
        while let Some(u) = p {
            if u == ancestor {
                return true;
            }
            p = timeline::layer_index(&self.doc.layers, u).and_then(|i| self.doc.layers[i].parent);
            guard += 1;
            if guard > 64 {
                break;
            }
        }
        false
    }

    /// Start and end keyframes of the shape tween containing the current frame.
    pub(crate) fn shape_tween_keys(&self) -> Option<(u32, u32)> {
        let l = &self.doc.layers[self.layer];
        let f = self.frame;
        if let Some(k) = l.key_at_or_before(f) {
            if let Some(n) = l.tween_target(k).filter(|_| l.keys[&k].shape_tween) {
                return Some((k, n));
            }
            // On the end keyframe of a shape tween.
            if k == f {
                if let Some(p) = l.keys.range(..f).next_back().map(|(p, _)| *p) {
                    if l.keys[&p].shape_tween && l.tween_target(p) == Some(f) {
                        return Some((p, f));
                    }
                }
            }
        }
        None
    }

    /// Hints visible on the current frame: (key, position, is the end keyframe).
    pub(crate) fn visible_hints(&self) -> Option<(u32, Vec<Pt>, bool)> {
        if !self.show_hints {
            return None;
        }
        let (k, n) = self.shape_tween_keys()?;
        let l = &self.doc.layers[self.layer];
        if self.frame == k {
            Some((k, l.keys[&k].hints.clone(), false))
        } else if self.frame == n {
            Some((n, l.keys[&n].hints.clone(), true))
        } else {
            None
        }
    }

    pub(crate) fn hint_at(&self, x: f32, y: f32) -> Option<(u32, usize)> {
        let (k, hints, _) = self.visible_hints()?;
        let vm = self.view_matrix();
        hints.iter().position(|h| dist(vm.apply(h.0, h.1), (x, y)) <= 9.0 * self.dpr).map(|i| (k, i))
    }

    /// Snaps a hint to the nearest edge of the shapes on its keyframe.
    pub(crate) fn snap_hint(&self, key: u32, p: Pt) -> Pt {
        let l = &self.doc.layers[self.layer];
        let mut best = (p, 12.0 * self.px());
        for s in l.keys[&key].shapes.iter().flat_map(|s| s.leaves(&Matrix::IDENTITY)) {
            for (pts, closed) in flatten(&s.outline()) {
                let w: Vec<Pt> = pts.iter().map(|q| s.m.apply(q.0, q.1)).collect();
                let n = w.len();
                let segs = if closed { n } else { n.saturating_sub(1) };
                for i in 0..segs {
                    let (a, b) = (w[i], w[(i + 1) % n]);
                    let d = dist_to_segment(p, a, b);
                    if d < best.1 {
                        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                        let l2 = (dx * dx + dy * dy).max(1e-9);
                        let u = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0);
                        best = ((a.0 + u * dx, a.1 + u * dy), d);
                    }
                }
            }
        }
        best.0
    }

    pub(crate) fn tl_state(&self) -> Value {
        let d = &self.doc;
        let cur = &d.layers[self.layer.min(d.layers.len() - 1)];
        let fi = cur.key_at_or_before(self.frame).filter(|_| self.frame < cur.length).map(|k| {
            let kf = &cur.keys[&k];
            json!({
                "tweenKind": if kf.shape_tween { "shape" } else if kf.tween { "motion" } else { "none" },
                "rotate": match kf.rotate { RotateMode::Auto => "auto", RotateMode::None => "none", RotateMode::Cw => "cw", RotateMode::Ccw => "ccw" },
                "spins": kf.spins, "scale": kf.scale, "orient": kf.orient, "snap": kf.snap, "sync": kf.sync,
                "easeCurve": kf.ease_curve, "label": kf.label,
                "labelKind": match kf.label_kind { LabelKind::Name => "name", LabelKind::Comment => "comment", LabelKind::Anchor => "anchor" },
                "hints": kf.hints.len(), "guided": timeline::guide_of(&d.layers, self.layer).is_some(),
            })
        });
        let vm = self.view_matrix();
        let hints = self.visible_hints().map(|(_, h, end)| {
            json!({ "end": end, "points": h.iter().map(|p| { let s = vm.apply(p.0, p.1); [s.0 / self.dpr, s.1 / self.dpr] }).collect::<Vec<_>>() })
        });
        json!({
            "frameSel": self.frame_sel.map(|(l0, l1, f0, f1)| json!({ "l0": l0, "l1": l1, "f0": f0, "f1": f1 })),
            "frameClip": self.frame_clip.len(),
            "onionBefore": self.onion_before, "onionAfter": self.onion_after,
            "onionOutlines": self.onion_outlines, "editMultiple": self.edit_multiple,
            "showHints": self.show_hints, "hintMarks": hints,
            "scenes": d.scenes.iter().map(|s| s.name.clone()).collect::<Vec<_>>(), "scene": d.scene,
            "movieTotal": d.movie_total(), "tween": fi,
        })
    }

    pub(crate) fn layer_state(&self, li: usize) -> Value {
        let layers = &self.doc.layers;
        let l = &layers[li];
        json!({
            "uid": l.uid,
            "kind": match l.kind { LayerKind::Normal => "normal", LayerKind::Guide => "guide", LayerKind::MotionGuide => "motionguide", LayerKind::Mask => "mask", LayerKind::Folder => "folder" },
            "parent": l.parent, "depth": timeline::depth(layers, li), "open": l.open, "height": l.height,
            "effVisible": timeline::eff_visible(layers, li), "effLocked": timeline::eff_locked(layers, li),
            "collapsed": timeline::collapsed(layers, li),
            "masked": timeline::mask_of(layers, li).is_some(), "guided": timeline::guide_of(layers, li).is_some(),
        })
    }
}

#[wasm_bindgen]
impl Editor {
    // ───────── Frame selection ─────────

    /// Selects a range of cells (dragging across the timeline).
    pub fn select_frames(&mut self, la: usize, fa: u32, lb: usize, fb: u32) {
        let n = self.doc.layers.len() - 1;
        let (l0, l1) = (la.min(lb).min(n), la.max(lb).min(n));
        let (f0, f1) = (fa.min(fb), fa.max(fb));
        self.layer = lb.min(n);
        self.set_frame(fb);
        self.frame_sel = Some((l0, l1, f0, f1));
    }

    /// Ctrl+Alt+A: all frames of all layers.
    pub fn select_all_frames(&mut self) {
        let total = self.doc.total_frames();
        self.frame_sel = Some((0, self.doc.layers.len() - 1, 0, total - 1));
    }

    // ───────── Range operations ─────────

    pub fn insert_frames(&mut self) {
        self.range_edit(|l, f0, f1| l.insert_frames(f0, f1 - f0 + 1));
    }

    pub fn remove_frames(&mut self) {
        self.range_edit(|l, f0, f1| l.remove_frames(f0, f1));
    }

    /// F6 / F7 on the range: every frame becomes a keyframe.
    pub fn convert_keyframes(&mut self, blank: bool) {
        self.range_edit(|l, f0, f1| l.convert_to_keyframes(f0, f1, blank));
    }

    /// Shift+F6 on the range.
    pub fn clear_keyframes(&mut self) {
        self.range_edit(|l, f0, f1| {
            let keys: Vec<u32> = l.keys.range(f0..=f1).map(|(k, _)| *k).collect();
            for k in keys.into_iter().rev() {
                l.clear_keyframe(k);
            }
        });
    }

    /// Alt+Backspace: clear frames (they become empty).
    pub fn clear_frames(&mut self) {
        self.range_edit(|l, f0, f1| l.clear_frames(f0, f1));
    }

    pub fn reverse_frames(&mut self) {
        self.range_edit(|l, f0, f1| l.reverse_frames(f0, f1));
    }

    /// Drags the selected frames by `df` positions.
    pub fn move_frames(&mut self, df: i32) {
        if df == 0 {
            return;
        }
        self.range_edit(|l, f0, f1| l.move_frames(f0, f1, df));
        if let Some((l0, l1, f0, f1)) = self.frame_sel {
            let sh = |f: u32| (f as i64 + df as i64).max(0) as u32;
            self.frame_sel = Some((l0, l1, sh(f0), sh(f1)));
            self.frame = sh(self.frame);
        }
    }

    /// Ctrl+Alt+C
    pub fn copy_frames(&mut self) {
        let (l0, l1, f0, f1) = self.sel_range();
        self.frame_clip = (l0..=l1).map(|li| (li, self.doc.layers[li].cells(f0, f1))).collect();
    }

    /// Ctrl+Alt+X
    pub fn cut_frames(&mut self) {
        self.copy_frames();
        self.clear_frames();
    }

    /// Ctrl+Alt+V: pastes starting at the selected cell.
    pub fn paste_frames(&mut self) {
        if self.frame_clip.is_empty() {
            return;
        }
        let (l0, _, f0, _) = self.sel_range();
        self.pen_finish();
        self.settle();
        self.exit_all_groups();
        let base = self.doc.clone();
        let clip = self.frame_clip.clone();
        let width = clip[0].1.len() as u32;
        for (j, (src, cells)) in clip.into_iter().enumerate() {
            let li = l0 + j;
            if li >= self.doc.layers.len() || timeline::eff_locked(&self.doc.layers, li) || self.doc.layers[li].kind == LayerKind::Folder {
                continue;
            }
            // On another layer, new ids so they don't clash with those of the source layer.
            let cells: Vec<Cell> = if src == li {
                cells
            } else {
                let mut map: Vec<(u32, u32)> = Vec::new();
                cells
                    .into_iter()
                    .map(|c| match c {
                        Cell::Key(mut k) => {
                            for s in k.shapes.iter_mut() {
                                let nid = match map.iter().find(|(o, _)| *o == s.id) {
                                    Some((_, n)) => *n,
                                    None => {
                                        let n = self.doc.alloc_id();
                                        map.push((s.id, n));
                                        n
                                    }
                                };
                                s.id = nid;
                            }
                            Cell::Key(k)
                        }
                        other => other,
                    })
                    .collect()
            };
            self.doc.layers[li].write_cells(f0, &cells);
        }
        self.frame_sel = Some((l0, (l0 + self.frame_clip.len() - 1).min(self.doc.layers.len() - 1), f0, f0 + width - 1));
        self.sel.clear();
        self.commit_from(base);
    }

    // ───────── Tweens ─────────

    /// "none" | "motion" | "shape"
    pub fn set_tween_kind(&mut self, kind: &str) {
        let (motion, shape) = match kind {
            "motion" => (true, false),
            "shape" => (false, true),
            _ => (false, false),
        };
        self.key_edit(|k| {
            k.tween = motion;
            k.shape_tween = shape;
        });
    }

    /// Motion tween options.
    pub fn set_tween_options(&mut self, rotate: &str, spins: u32, scale: bool, orient: bool, snap: bool) {
        let r = match rotate {
            "none" => RotateMode::None,
            "cw" => RotateMode::Cw,
            "ccw" => RotateMode::Ccw,
            _ => RotateMode::Auto,
        };
        self.key_edit(|k| {
            k.rotate = r;
            k.spins = spins.min(99);
            k.scale = scale;
            k.orient = orient;
            k.snap = snap;
        });
    }

    /// "Sync" for graphics in a motion tween.
    pub fn set_tween_sync(&mut self, on: bool) {
        self.key_edit(|k| k.sync = on);
    }

    /// Custom easing (Edit…). `x1 < 0` removes it.
    pub fn set_ease_curve(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        let c = (x1 >= 0.0).then_some([x1.clamp(0.0, 1.0), y1, x2.clamp(0.0, 1.0), y2]);
        self.key_edit(|k| k.ease_curve = c);
    }

    // ───────── Labels ─────────

    /// Label of the current keyframe: "name" | "comment" | "anchor".
    pub fn set_label(&mut self, text: &str, kind: &str) {
        let lk = match kind {
            "comment" => LabelKind::Comment,
            "anchor" => LabelKind::Anchor,
            _ => LabelKind::Name,
        };
        let (li, f) = (self.layer, self.frame);
        let text = text.trim().to_string();
        self.timeline_edit(move |d| {
            let l = &mut d.layers[li];
            let k = if l.keys.contains_key(&f) { f } else { l.ensure_key(f) };
            let kf = l.keys.get_mut(&k).unwrap();
            kf.label = text;
            kf.label_kind = lk;
        });
    }

    // ───────── Shape hints ─────────

    /// Ctrl+Shift+H: adds a hint (a, b, c…) at the center of the shape.
    pub fn add_shape_hint(&mut self) {
        let Some((k, n)) = self.shape_tween_keys() else {
            self.message = "Los consejos de forma se añaden en el primer fotograma de una interpolación de forma".into();
            return;
        };
        if self.doc.layers[self.layer].keys[&k].hints.len() >= 26 {
            return;
        }
        let base = self.doc.clone();
        let l = &mut self.doc.layers[self.layer];
        let center = |kf: &Keyframe| kf.shapes.iter().fold(BBox::EMPTY, |b, s| b.union(&s.world_bounds())).center();
        let (ca, cb) = (center(&l.keys[&k]), center(&l.keys[&n]));
        l.keys.get_mut(&k).unwrap().hints.push(ca);
        l.keys.get_mut(&n).unwrap().hints.push(cb);
        self.show_hints = true;
        self.commit_from(base);
    }

    pub fn remove_all_hints(&mut self) {
        let Some((k, n)) = self.shape_tween_keys() else { return };
        let base = self.doc.clone();
        let l = &mut self.doc.layers[self.layer];
        l.keys.get_mut(&k).unwrap().hints.clear();
        l.keys.get_mut(&n).unwrap().hints.clear();
        self.commit_from(base);
    }

    /// Ctrl+Alt+H
    pub fn set_show_hints(&mut self, on: bool) {
        self.show_hints = on;
    }

    // ───────── Special layers ─────────

    /// "normal" | "guide" | "mask" — like Flash's layer context menu.
    pub fn set_layer_kind(&mut self, i: usize, kind: &str) {
        if i >= self.doc.layers.len() {
            return;
        }
        let base = self.doc.clone();
        let uid = self.doc.layers[i].uid;
        let old = self.doc.layers[i].kind;
        match kind {
            "guide" => self.doc.layers[i].kind = if old == LayerKind::Guide { LayerKind::Normal } else { LayerKind::Guide },
            "mask" => {
                self.doc.layers[i].kind = LayerKind::Mask;
                self.doc.layers[i].parent = None;
                // The layer below becomes masked; both are locked to show the effect.
                if i + 1 < self.doc.layers.len() && matches!(self.doc.layers[i + 1].kind, LayerKind::Normal | LayerKind::Guide) {
                    self.doc.layers[i + 1].parent = Some(uid);
                    self.doc.layers[i + 1].locked = true;
                }
                self.doc.layers[i].locked = true;
            }
            _ => self.doc.layers[i].kind = LayerKind::Normal,
        }
        // If it stops being a mask or guide, its layers are released.
        if !matches!(self.doc.layers[i].kind, LayerKind::Mask | LayerKind::MotionGuide | LayerKind::Folder) {
            for l in self.doc.layers.iter_mut().filter(|l| l.parent == Some(uid)) {
                l.parent = None;
            }
        }
        self.commit_from(base);
    }

    /// "Add Motion Guide" button: guide above the current layer.
    pub fn add_motion_guide(&mut self) {
        let li = self.layer;
        if !matches!(self.doc.layers[li].kind, LayerKind::Normal | LayerKind::Guide) {
            self.message = "Elige una capa normal para guiarla".into();
            return;
        }
        let base = self.doc.clone();
        let name = format!("Guía: {}", self.doc.layers[li].name);
        let total = self.doc.total_frames();
        let mut g = Layer::new(&name);
        g.uid = self.doc.new_layer_uid();
        g.kind = LayerKind::MotionGuide;
        g.length = total;
        g.parent = self.doc.layers[li].parent.filter(|p| timeline::layer_index(&self.doc.layers, *p).map_or(false, |pi| self.doc.layers[pi].kind == LayerKind::Folder));
        let guid = g.uid;
        self.doc.layers.insert(li, g);
        self.doc.layers[li + 1].parent = Some(guid);
        self.layer = li;
        self.commit_from(base);
    }

    /// "Insert Layer Folder" button.
    pub fn add_folder(&mut self) {
        let base = self.doc.clone();
        let li = self.layer;
        self.doc.layer_counter += 1;
        let mut f = Layer::new(&format!("Carpeta {}", self.doc.layer_counter));
        f.uid = self.doc.new_layer_uid();
        f.kind = LayerKind::Folder;
        f.keys.clear();
        f.length = 0;
        self.doc.layers.insert(li, f);
        self.layer = li;
        self.commit_from(base);
    }

    pub fn toggle_folder(&mut self, i: usize) {
        if let Some(l) = self.doc.layers.get_mut(i) {
            if l.kind == LayerKind::Folder {
                l.open = !l.open;
            }
        }
    }

    /// Layer Properties dialog.
    #[allow(clippy::too_many_arguments)]
    pub fn set_layer_props(&mut self, i: usize, name: &str, visible: bool, locked: bool, kind: &str, color: &str, outline: bool, height: u8) {
        if i >= self.doc.layers.len() {
            return;
        }
        let base = self.doc.clone();
        let uid = self.doc.layers[i].uid;
        let new_kind = match kind {
            "guide" => LayerKind::Guide,
            "motionguide" => LayerKind::MotionGuide,
            "mask" => LayerKind::Mask,
            "folder" => LayerKind::Folder,
            "masked" | "guided" | _ => LayerKind::Normal,
        };
        {
            let l = &mut self.doc.layers[i];
            if !name.trim().is_empty() {
                l.name = name.trim().to_string();
            }
            l.visible = visible;
            l.locked = locked;
            l.outline = outline;
            l.height = height.clamp(1, 3);
            if let Some(c) = Rgba::parse(color, 255) {
                l.color = c;
            }
            if new_kind == LayerKind::Folder && l.kind != LayerKind::Folder {
                l.keys.clear();
                l.length = 0;
            }
            l.kind = new_kind;
        }
        // "Masked" / "Guided": hangs from the mask or guide above.
        if kind == "masked" || kind == "guided" {
            let want = if kind == "masked" { LayerKind::Mask } else { LayerKind::MotionGuide };
            if let Some(p) = (0..i).rev().find(|&k| self.doc.layers[k].kind == want) {
                self.doc.layers[i].parent = Some(self.doc.layers[p].uid);
            }
        }
        if !matches!(new_kind, LayerKind::Mask | LayerKind::MotionGuide | LayerKind::Folder) {
            for l in self.doc.layers.iter_mut().filter(|l| l.parent == Some(uid)) {
                l.parent = None;
            }
        }
        self.commit_from(base);
    }

    /// Dragging layers: moves the layer (with what hangs from it) and infers its parent.
    pub fn drag_layer(&mut self, from: usize, to: usize) {
        let n = self.doc.layers.len();
        if from >= n || to >= n || from == to {
            return;
        }
        let base = self.doc.clone();
        self.move_layer_block(from, to);
        self.commit_from(base);
    }

    // ───────── Onion skin ─────────

    pub fn set_onion(&mut self, before: u32, after: u32) {
        self.onion_before = before.min(999);
        self.onion_after = after.min(999);
    }

    pub fn set_onion_outlines(&mut self, on: bool) {
        self.onion_outlines = on;
    }

    pub fn set_edit_multiple(&mut self, on: bool) {
        self.edit_multiple = on;
    }

    // ───────── Scenes ─────────

    fn scene_change(&mut self, f: impl FnOnce(&mut Document)) {
        self.exit_all_symbols();
        self.pen_finish();
        self.settle();
        self.exit_all_groups();
        let base = self.doc.clone();
        f(&mut self.doc);
        self.frame = 0;
        self.layer = 0;
        self.frame_sel = None;
        self.sel.clear();
        self.anchors.clear();
        self.commit_from(base);
    }

    pub fn scene_add(&mut self) {
        self.scene_change(|d| {
            let i = d.add_scene(d.scene + 1);
            d.switch_scene(i);
        });
    }

    pub fn scene_duplicate(&mut self) {
        self.scene_change(|d| {
            let i = d.duplicate_scene(d.scene);
            d.switch_scene(i);
        });
    }

    pub fn scene_delete(&mut self, i: usize) {
        if self.doc.scenes.len() <= 1 {
            self.message = "Una película necesita al menos una escena".into();
            return;
        }
        self.scene_change(|d| d.delete_scene(i));
    }

    pub fn scene_rename(&mut self, i: usize, name: &str) {
        if i < self.doc.scenes.len() && !name.trim().is_empty() {
            let base = self.doc.clone();
            self.doc.scenes[i].name = name.trim().to_string();
            self.commit_from(base);
        }
    }

    pub fn scene_move(&mut self, i: usize, to: usize) {
        let base = self.doc.clone();
        self.doc.move_scene(i, to);
        self.commit_from(base);
    }

    pub fn scene_go(&mut self, i: usize) {
        if i != self.doc.scene {
            self.scene_change(|d| d.switch_scene(i));
        }
    }

    pub fn movie_total(&self) -> u32 {
        self.doc.closed().movie_total()
    }

    // ───────── Timeline preview ─────────

    /// RGBA thumbnail of a layer at a frame (Preview).
    pub fn render_thumb(&self, layer: usize, frame: u32, w: u32, h: u32) -> Vec<u8> {
        if layer >= self.doc.layers.len() {
            return Vec::new();
        }
        render::layer_thumb(&self.doc, layer, frame, w, h).map(|p| p.data().to_vec()).unwrap_or_default()
    }

    /// Document fingerprint, to know when to regenerate thumbnails.
    pub fn doc_hash(&self) -> u32 {
        let s = serde_json::to_string(&self.doc.layers).unwrap_or_default();
        s.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619))
    }

    /// Document point → CSS pixels of the canvas (for labels drawn by the UI).
    pub fn to_screen(&self, x: f32, y: f32) -> Vec<f32> {
        let p = self.view_matrix().apply(x, y);
        vec![p.0 / self.dpr, p.1 / self.dpr]
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
        e.object_mode = true;
        e
    }
    fn rect(e: &mut Editor, a: Pt, b: Pt) {
        e.set_tool("rect");
        e.pointer_down(a.0, a.1, 0, false, false);
        e.pointer_up(b.0, b.1, false, false);
    }
    #[test]
    fn edit_multiple_selects_shapes_from_other_keyframes() {
        let mut e = editor();
        rect(&mut e, (100.0, 100.0), (150.0, 150.0));
        e.select_cell(0, 5);
        e.insert_keyframe(true);
        rect(&mut e, (300.0, 100.0), (350.0, 150.0));
        e.select_cell(0, 0);
        e.set_onion(0, 5);
        e.set_edit_multiple(true);
        e.set_tool("select");
        // The shape on frame 6 is grabbed from frame 1 and moved there.
        e.pointer_down(325.0, 125.0, 0, false, false);
        e.pointer_up(345.0, 125.0, false, false);
        assert_eq!(e.frame, 0);
        let b = e.doc.layers[0].keys[&5].shapes[0].world_bounds();
        assert!((b.x0 - 320.0).abs() < 0.5, "{b:?}");
        assert_eq!(e.doc.layers[0].keys[&0].shapes.len(), 1, "no aparece en el fotograma 1");
        // And it is included in the marquee selection.
        e.deselect();
        e.pointer_down(90.0, 90.0, 0, false, false);
        e.pointer_up(400.0, 200.0, false, false);
        assert_eq!(e.sel.len(), 2);
    }

    #[test]
    fn sync_keeps_graphic_animation_going() {
        let mut e = editor();
        // Graphic with 10 frames.
        rect(&mut e, (0.0, 0.0), (10.0, 10.0));
        e.select_all();
        let sid = e.convert_to_symbol("", "graphic", 0, false);
        e.edit_symbol(sid);
        e.select_cell(0, 9);
        e.insert_frame();
        e.leave_symbol();
        // Tween from 0 to 4 and from 4 to 8.
        e.select_cell(0, 8);
        e.insert_keyframe(false);
        e.select_cell(0, 4);
        e.insert_keyframe(false);
        e.select_cell(0, 0);
        e.set_tween_kind("motion");
        e.select_cell(0, 4);
        e.set_tween_kind("motion");
        assert_eq!(e.doc.layers[0].anim_offset(6), 2, "sin sincronizar: empieza en cada clave");
        e.select_cell(0, 0);
        e.set_tween_sync(true);
        e.select_cell(0, 4);
        e.set_tween_sync(true);
        assert_eq!(e.doc.layers[0].anim_offset(6), 6, "sincronizado: sigue desde el primer clave");
    }

    /// (header frame count, flags of each PlaceObject2)
    fn swf_info(swf: &[u8]) -> (u16, Vec<u8>) {
        let nbits = (swf[8] >> 3) as usize;
        let rect = (5 + 4 * nbits + 7) / 8;
        let count = u16::from_le_bytes([swf[8 + rect + 2], swf[8 + rect + 3]]);
        let mut i = 8 + rect + 4;
        let mut flags = Vec::new();
        while i + 2 <= swf.len() {
            let h = u16::from_le_bytes([swf[i], swf[i + 1]]);
            let (code, mut len) = (h >> 6, (h & 0x3f) as usize);
            i += 2;
            if len == 0x3f {
                len = u32::from_le_bytes([swf[i], swf[i + 1], swf[i + 2], swf[i + 3]]) as usize;
                i += 4;
            }
            if code == 26 {
                flags.push(swf[i]);
            }
            if code == 0 {
                break;
            }
            i += len;
        }
        (count, flags)
    }
    fn xs(e: &Editor, li: usize, f: u32) -> Vec<f32> {
        e.doc.frame_shapes(li, f).iter().map(|s| s.world_bounds().x0).collect()
    }

    #[test]
    fn copy_paste_and_reverse_frames() {
        let mut e = editor();
        rect(&mut e, (0.0, 0.0), (10.0, 10.0));
        e.select_cell(0, 10);
        e.insert_keyframe(false);
        let id = e.doc.layers[0].keys[&10].shapes[0].id;
        e.modify_shapes(&[id], |s| s.m.tx += 100.0);
        e.select_cell(0, 0);
        e.set_tween_kind("motion");
        e.select_frames(0, 0, 0, 10);
        e.copy_frames();
        e.select_cell(0, 20);
        e.paste_frames();
        assert!((xs(&e, 0, 25)[0] - 50.0).abs() < 0.5, "el tramo pegado conserva la interpolación");
        e.select_frames(0, 20, 0, 30);
        e.reverse_frames();
        assert!((xs(&e, 0, 20)[0] - 100.0).abs() < 0.5 && (xs(&e, 0, 30)[0]).abs() < 0.5);
        e.undo();
        assert!(xs(&e, 0, 20)[0].abs() < 0.5);
    }

    #[test]
    fn shape_tween_and_hints() {
        let mut e = editor();
        rect(&mut e, (0.0, 0.0), (100.0, 100.0));
        e.select_cell(0, 10);
        e.insert_keyframe(true);
        rect(&mut e, (0.0, 0.0), (300.0, 100.0));
        e.select_cell(0, 0);
        e.set_tween_kind("shape");
        let mid = e.doc.frame_shapes(0, 5);
        assert!((mid[0].world_bounds().w() - 200.0).abs() < 2.0);
        e.add_shape_hint();
        assert_eq!(e.doc.layers[0].keys[&0].hints.len(), 1);
        assert_eq!(e.doc.layers[0].keys[&10].hints.len(), 1);
        assert!(e.show_hints);
    }

    #[test]
    fn mask_guide_and_folder_layers() {
        let mut e = editor();
        rect(&mut e, (0.0, 0.0), (100.0, 100.0));
        e.add_layer();
        rect(&mut e, (40.0, 40.0), (60.0, 60.0));
        e.set_layer_kind(0, "mask");
        assert_eq!(e.doc.layers[0].kind, LayerKind::Mask);
        assert_eq!(e.doc.layers[1].parent, Some(e.doc.layers[0].uid));
        assert!(e.doc.layers[0].locked && e.doc.layers[1].locked);
        let swf = crate::swf::uncompressed(&e.export_swf());
        // PlaceObject2 with ClipDepth (bit 0x40) for the mask.
        assert!(swf_info(&swf).1.iter().any(|f| f & 0x40 != 0));
        e.select_cell(1, 0);
        e.add_motion_guide();
        assert_eq!(e.doc.layers[1].kind, LayerKind::MotionGuide);
        e.add_folder();
        assert_eq!(e.doc.layers[e.layer].kind, LayerKind::Folder);
        let n = e.doc.layers.len();
        e.drag_layer(n - 1, e.layer);
        let state: serde_json::Value = serde_json::from_str(&e.state()).unwrap();
        assert!(state["layers"].as_array().unwrap().iter().any(|l| l["kind"] == "folder"));
    }

    #[test]
    fn scenes_play_in_sequence() {
        let mut e = editor();
        rect(&mut e, (0.0, 0.0), (100.0, 100.0));
        e.select_cell(0, 4);
        e.insert_frame();
        e.scene_add();
        assert_eq!(e.doc.scenes.len(), 2);
        assert_eq!(e.doc.scene, 1);
        e.select_cell(0, 2);
        e.insert_frame();
        assert_eq!(e.movie_total(), 8);
        let swf = crate::swf::uncompressed(&e.export_swf());
        assert_eq!(swf_info(&swf).0, 8, "cabecera: 5 + 3 fotogramas");
        e.scene_rename(0, "Intro");
        e.scene_go(0);
        assert_eq!(e.doc.layers[0].length, 5);
        e.undo();
        assert_eq!(e.doc.scene, 1);
    }
}
