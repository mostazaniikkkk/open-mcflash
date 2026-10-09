//! Symbols in the editor: Convert to Symbol (F8), New Symbol (Ctrl+F8),
//! placing from the Library, editing in symbol-editing mode or in place, Library
//! operations, instance properties, filters, 9-slice scaling and
//! simple buttons.

use super::*;
use crate::symbols::{self, Blend, ColorFx, Filter, FilterEntry, Inst, LibFolder, LoopMode, Symbol, SymbolKind};

/// Image fill of a bitmap symbol.
fn bitmap_of(s: &Symbol) -> Option<crate::bitmap::BitmapFill> {
    if s.kind != SymbolKind::Bitmap {
        return None;
    }
    s.layers.iter().flat_map(|l| l.keys.values()).flat_map(|k| k.shapes.iter()).find_map(|sh| sh.bmp.clone())
}

impl Editor {
    pub(crate) fn sync_lib(&self) {
        symbols::set_library(&self.doc);
    }

    /// Accumulated matrix of edit in place (identity when editing in symbol-editing mode).
    pub(crate) fn edit_matrix(&self) -> Matrix {
        let mut m = Matrix::IDENTITY;
        for e in &self.doc.sym_edit {
            m = match e.in_place {
                Some(mi) => m.mul(&mi),
                None => Matrix::IDENTITY,
            };
        }
        m
    }

    /// Scene frame (the scene's one even while a symbol is being edited).
    pub(crate) fn scene_frame(&self) -> u32 {
        self.doc.sym_edit.first().map_or(self.frame, |e| e.frame)
    }

    /// Registration point cross and 9-slice guides of the symbol being edited.
    pub(crate) fn draw_symbol_guides(&self, pm: &mut tiny_skia::Pixmap, vm: &Matrix) {
        let d = self.dpr.round().max(1.0);
        let (cx, cy) = vm.apply(0.0, 0.0);
        let (cx, cy) = (cx.floor(), cy.floor());
        let r = 7.0 * d;
        render::fill_rect(pm, cx - r - d, cy - d, 2.0 * r + 3.0 * d, 3.0 * d, Rgba::WHITE);
        render::fill_rect(pm, cx - d, cy - r - d, 3.0 * d, 2.0 * r + 3.0 * d, Rgba::WHITE);
        render::fill_rect(pm, cx - r, cy, 2.0 * r + d, d, Rgba::BLACK);
        render::fill_rect(pm, cx, cy - r, d, 2.0 * r + d, Rgba::BLACK);
        if let Some(g) = self.scale9_lines() {
            let a = vm.apply(g[0], g[1]);
            let b = vm.apply(g[2], g[3]);
            let (w, h) = (pm.width() as f32, pm.height() as f32);
            let c = Rgba { r: 0, g: 0xc0, b: 0xff, a: 255 };
            let mut t = 0.0;
            while t < w.max(h) {
                let len = 4.0 * d;
                render::fill_rect(pm, a.0.floor(), t, d, len, c);
                render::fill_rect(pm, b.0.floor(), t, d, len, c);
                render::fill_rect(pm, t, a.1.floor(), len, d, c);
                render::fill_rect(pm, t, b.1.floor(), len, d, c);
                t += 7.0 * d;
            }
        }
    }

    pub(crate) fn symbol_index(&self, id: u32) -> Option<usize> {
        self.doc.library.iter().position(|s| s.id == id)
    }

    fn unique_symbol_name(&mut self, base: &str) -> String {
        let base = base.trim();
        if !base.is_empty() && !self.doc.library.iter().any(|s| s.name == base) {
            return base.to_string();
        }
        loop {
            self.doc.symbol_counter += 1;
            let n = format!("Símbolo {}", self.doc.symbol_counter);
            if !self.doc.library.iter().any(|s| s.name == n) {
                return n;
            }
        }
    }

    fn new_symbol_layers(&mut self, shapes: Vec<Shape>) -> Vec<Layer> {
        let mut l = Layer::new("Capa 1");
        l.uid = self.doc.new_layer_uid();
        l.keys.get_mut(&0).unwrap().shapes = shapes;
        vec![l]
    }

    /// Enters editing of a symbol (in symbol-editing mode or in place of an instance).
    pub(crate) fn enter_symbol(&mut self, id: u32, in_place: Option<(Matrix, u32)>) {
        let Some(idx) = self.symbol_index(id) else { return };
        if self.doc.library[idx].kind == SymbolKind::Bitmap {
            self.message = "Los mapas de bits no se editan como símbolos (Ctrl+B los separa en una forma)".into();
            return;
        }
        if self.doc.library[idx].kind == SymbolKind::Sound {
            self.message = "Los sonidos se ponen en un fotograma clave (panel Propiedades)".into();
            return;
        }
        if self.doc.sym_edit.iter().any(|e| e.symbol == id) {
            self.message = "Ese símbolo ya se está editando".into();
            return;
        }
        self.pen_finish();
        self.settle();
        self.exit_all_groups();
        let base = self.doc.clone();
        let layers = std::mem::take(&mut self.doc.library[idx].layers);
        let parent = std::mem::replace(&mut self.doc.layers, layers);
        if self.doc.layers.is_empty() {
            self.doc.layers = self.new_symbol_layers(Vec::new());
        }
        self.doc.sym_edit.push(SymEdit {
            symbol: id,
            parent_layers: parent,
            in_place: in_place.map(|x| x.0),
            instance: in_place.map(|x| x.1),
            frame: self.frame,
            layer: self.layer,
        });
        self.sym_views.push((self.pan, self.zoom));
        self.frame = 0;
        self.layer = 0;
        self.frame_sel = None;
        self.sel.clear();
        self.anchors.clear();
        if in_place.is_none() {
            // Symbol-editing mode: the registration point at the center of the window, as in Flash.
            self.pan = (self.vw as f32 / 2.0, self.vh as f32 / 2.0);
        }
        self.commit_from(base);
        self.sync_lib();
    }

    pub(crate) fn exit_symbol(&mut self) {
        if self.doc.sym_edit.is_empty() {
            return;
        }
        self.pen_finish();
        self.settle();
        self.exit_all_groups();
        let base = self.doc.clone();
        let e = self.doc.sym_edit.pop().unwrap();
        let layers = std::mem::replace(&mut self.doc.layers, e.parent_layers);
        if let Some(idx) = self.symbol_index(e.symbol) {
            self.doc.library[idx].layers = layers;
        }
        self.frame = e.frame;
        self.layer = e.layer.min(self.doc.layers.len() - 1);
        if let Some((pan, zoom)) = self.sym_views.pop() {
            self.pan = pan;
            self.zoom = zoom;
        }
        self.frame_sel = None;
        self.sel = e.instance.into_iter().collect();
        self.anchors.clear();
        self.commit_from(base);
        self.sync_lib();
    }

    pub(crate) fn exit_all_symbols(&mut self) {
        while !self.doc.sym_edit.is_empty() {
            self.exit_symbol();
        }
    }

    /// Double-click on an instance: edit in place.
    pub(crate) fn edit_instance_in_place(&mut self, li: usize, id: u32) {
        let Some(s) = self.scope_shapes(li).into_iter().find(|s| s.id == id) else { return };
        let Some(inst) = s.inst() else { return };
        self.enter_symbol(inst.symbol, Some((s.m, id)));
    }

    fn for_each_layer_set(&mut self, mut f: impl FnMut(&mut Vec<Layer>)) {
        f(&mut self.doc.layers);
        for s in self.doc.scenes.iter_mut() {
            f(&mut s.layers);
        }
        for s in self.doc.library.iter_mut() {
            f(&mut s.layers);
        }
        for e in self.doc.sym_edit.iter_mut() {
            f(&mut e.parent_layers);
        }
    }

    fn selected_instances(&self) -> Vec<u32> {
        self.selected_shapes().into_iter().filter(|(_, s)| s.is_instance()).map(|(_, s)| s.id).collect()
    }

    fn edit_instances(&mut self, f: impl Fn(&mut Inst)) {
        let ids = self.selected_instances();
        if ids.is_empty() {
            return;
        }
        let base = self.doc.clone();
        self.modify_shapes(&ids, |s| {
            if let Geometry::Instance(i) = &mut s.geom {
                f(i);
            }
        });
        self.commit_from(base);
    }

    /// State of the Library and of the selected instance, for the UI.
    pub(crate) fn sym_state(&self) -> Value {
        let uses = |id: u32| -> usize {
            let count = |ls: &[Layer]| -> usize {
                fn walk(v: &[Shape], id: u32) -> usize {
                    v.iter()
                        .map(|s| match &s.geom {
                            Geometry::Instance(i) => (i.symbol == id) as usize,
                            Geometry::Group { children, .. } => walk(children, id),
                            _ => 0,
                        })
                        .sum()
                }
                ls.iter().flat_map(|l| l.keys.values()).map(|k| walk(&k.shapes, id)).sum()
            };
            let mut n = count(&self.doc.layers);
            for s in &self.doc.scenes {
                n += count(&s.layers);
            }
            for s in &self.doc.library {
                n += count(&s.layers);
            }
            for e in &self.doc.sym_edit {
                n += count(&e.parent_layers);
            }
            n
        };
        let lib: Vec<Value> = self
            .doc
            .library
            .iter()
            .map(|s| {
                let bmp = bitmap_of(s);
                json!({
                    "id": s.id, "name": s.name, "kind": s.kind.name(), "folder": s.folder, "uses": uses(s.id), "scale9": s.scale9.is_some(), "linkage": s.linkage,
                    "bitmap": bmp.as_ref().map(|b| b.id), "smooth": bmp.as_ref().map(|b| b.smooth),
                    "quality": bmp.as_ref().and_then(|b| self.doc.bitmaps.iter().find(|a| a.id == b.id)).and_then(|a| a.quality),
                    "jpegBytes": bmp.as_ref().and_then(|b| self.doc.bitmaps.iter().find(|a| a.id == b.id)).and_then(|a| a.jpeg.as_ref()).map(|j| j.len() * 3 / 4),
                    "pngBytes": bmp.as_ref().and_then(|b| self.doc.bitmaps.iter().find(|a| a.id == b.id)).map(|a| a.png.len() * 3 / 4),
                    "size": bmp.as_ref().and_then(|b| self.doc.bitmaps.iter().find(|a| a.id == b.id)).map(|a| [a.w, a.h]),
                })
            })
            .collect();
        let path: Vec<Value> = self
            .doc
            .sym_edit
            .iter()
            .map(|e| {
                let s = self.doc.library.iter().find(|s| s.id == e.symbol);
                json!({ "name": s.map(|s| s.name.clone()).unwrap_or_default(), "kind": s.map(|s| s.kind.name()).unwrap_or("graphic"), "inPlace": e.in_place.is_some() })
            })
            .collect();
        let sel = self.selected_shapes();
        let inst = if sel.len() == 1 {
            sel[0].1.inst().map(|i| {
                let sym = self.doc.library.iter().find(|s| s.id == i.symbol);
                json!({
                    "symbol": i.symbol, "symbolName": sym.map(|s| s.name.clone()).unwrap_or_default(),
                    "symbolKind": sym.map(|s| s.kind.name()).unwrap_or("graphic"),
                    "kind": i.kind.name(), "name": i.name, "color": i.color,
                    "loop": match i.loop_mode { LoopMode::Loop => "loop", LoopMode::Once => "once", LoopMode::Single => "single" },
                    "first": i.first, "blend": i.blend, "cache": i.cache, "filters": i.filters,
                })
            })
        } else {
            None
        };
        let editing = self.doc.sym_edit.last().and_then(|e| self.doc.library.iter().find(|s| s.id == e.symbol));
        json!({
            "library": lib, "libFolders": self.doc.lib_folders, "symPath": path, "inst": inst,
            "editingKind": editing.map(|s| s.kind.name()), "editingScale9": editing.and_then(|s| s.scale9),
            "simpleButtons": self.simple_buttons,
        })
    }

    /// Asset of a Library image.
    fn bitmap_symbol_asset(&self, id: u32) -> Option<u32> {
        self.doc.library.iter().find(|s| s.id == id).and_then(bitmap_of).map(|b| b.id)
    }

    pub(crate) fn scale9_lines(&self) -> Option<[f32; 4]> {
        let e = self.doc.sym_edit.last()?;
        let s = self.doc.library.iter().find(|s| s.id == e.symbol)?;
        if s.kind != SymbolKind::Movieclip {
            return None;
        }
        s.scale9
    }

    /// 9-slice guide under the pointer: 0 left, 1 top, 2 right, 3 bottom.
    pub(crate) fn scale9_at(&self, x: f32, y: f32) -> Option<usize> {
        let g = self.scale9_lines()?;
        let vm = self.view_matrix();
        let a = vm.apply(g[0], g[1]);
        let b = vm.apply(g[2], g[3]);
        let r = 4.0 * self.dpr;
        if (x - a.0).abs() <= r {
            Some(0)
        } else if (y - a.1).abs() <= r {
            Some(1)
        } else if (x - b.0).abs() <= r {
            Some(2)
        } else if (y - b.1).abs() <= r {
            Some(3)
        } else {
            None
        }
    }

    pub(crate) fn move_scale9(&mut self, which: usize, p: Pt) {
        let Some(e) = self.doc.sym_edit.last() else { return };
        let id = e.symbol;
        if let Some(idx) = self.symbol_index(id) {
            if let Some(g) = &mut self.doc.library[idx].scale9 {
                match which {
                    0 => g[0] = p.0.min(g[2] - 1.0),
                    1 => g[1] = p.1.min(g[3] - 1.0),
                    2 => g[2] = p.0.max(g[0] + 1.0),
                    _ => g[3] = p.1.max(g[1] + 1.0),
                }
            }
        }
        self.sync_lib();
    }
}

#[wasm_bindgen]
impl Editor {
    // ───────── Creating symbols ─────────

    /// F8: converts the selection to a symbol. `reg` = 0…8 (3×3 registration point grid).
    pub fn convert_to_symbol(&mut self, name: &str, kind: &str, reg: u32, scale9: bool) -> u32 {
        self.settle();
        let li = self.layer;
        let shapes: Vec<Shape> = self.selected_shapes().into_iter().filter(|(l, _)| *l == li).map(|(_, s)| s).collect();
        if shapes.is_empty() {
            self.message = "Selecciona algo en el escenario para convertirlo en símbolo".into();
            return 0;
        }
        let base = self.doc.clone();
        let b = shapes.iter().fold(BBox::EMPTY, |acc, s| acc.union(&s.world_bounds()));
        let (rx, ry) = ((reg % 3) as f32 / 2.0, (reg / 3).min(2) as f32 / 2.0);
        let r = (b.x0 + b.w() * rx, b.y0 + b.h() * ry);
        let back = Matrix::translate(-r.0, -r.1);
        let ids: Vec<u32> = shapes.iter().map(|s| s.id).collect();
        let children: Vec<Shape> = shapes
            .into_iter()
            .map(|mut s| {
                s.m = back.mul(&s.m);
                s.tag = None;
                s
            })
            .collect();
        let kind = SymbolKind::parse(kind);
        let sid = self.doc.alloc_id();
        let name = self.unique_symbol_name(name);
        let layers = self.new_symbol_layers(children);
        let grid = scale9.then(|| [b.x0 - r.0 + b.w() * 0.25, b.y0 - r.1 + b.h() * 0.25, b.x0 - r.0 + b.w() * 0.75, b.y0 - r.1 + b.h() * 0.75]);
        self.doc.library.push(Symbol { id: sid, name, kind, layers, folder: None, scale9: grid.filter(|_| kind == SymbolKind::Movieclip), linkage: None });
        self.sync_lib();
        // The instance takes the place of the topmost element.
        let k = self.doc.layers[li].ensure_key(self.frame);
        let scope = self.scope();
        let iid = self.doc.alloc_id();
        let v = &mut self.doc.layers[li].keys.get_mut(&k).unwrap().shapes;
        let pos = v.iter().rposition(|s| ids.contains(&s.id)).map(|p| p + 1 - ids.len().min(p + 1)).unwrap_or(v.len());
        v.retain(|s| !ids.contains(&s.id));
        let mut inst = Shape::new(Geometry::Instance(Inst::new(sid, kind)), None, None, 1.0, Matrix::translate(r.0, r.1));
        inst.id = iid;
        inst.tag = scope;
        v.insert(pos.min(v.len()), inst);
        let fl = self.floating.clone();
        normalize_order(v, scope, &fl);
        self.floating.retain(|f| !ids.contains(f));
        self.sel = vec![iid];
        self.commit_from(base);
        sid
    }

    /// Ctrl+F8: new empty symbol, opened for editing.
    pub fn new_symbol(&mut self, name: &str, kind: &str) -> u32 {
        let base = self.doc.clone();
        let sid = self.doc.alloc_id();
        let name = self.unique_symbol_name(name);
        let layers = self.new_symbol_layers(Vec::new());
        self.doc.library.push(Symbol { id: sid, name, kind: SymbolKind::parse(kind), layers, folder: None, scale9: None, linkage: None });
        self.commit_from(base);
        self.sync_lib();
        self.enter_symbol(sid, None);
        sid
    }

    /// Dragging from the Library: instance with its registration point under the pointer.
    pub fn place_symbol(&mut self, id: u32, x: f32, y: f32) {
        let Some(idx) = self.symbol_index(id) else { return };
        if self.doc.sym_edit.iter().any(|e| e.symbol == id) {
            self.message = "Un símbolo no puede contenerse a sí mismo".into();
            return;
        }
        if !self.can_draw() {
            return;
        }
        let p = self.to_doc(x, y);
        let kind = self.doc.library[idx].kind;
        let base = self.doc.clone();
        let s = Shape::new(Geometry::Instance(Inst::new(id, kind)), None, None, 1.0, Matrix::translate(p.0, p.1));
        self.sel = self.insert_shapes(vec![s], None);
        self.commit_from(base);
    }

    // ───────── Editing symbols ─────────

    /// Edit the symbol in symbol-editing mode (double-click in the Library).
    pub fn edit_symbol(&mut self, id: u32) {
        self.enter_symbol(id, None);
    }

    pub fn leave_symbol(&mut self) {
        self.exit_symbol();
    }

    /// Edit > Edit Document (Ctrl+E): returns to the scene.
    pub fn edit_document(&mut self) {
        self.exit_all_groups();
        self.exit_all_symbols();
    }

    // ───────── Library ─────────

    pub fn lib_rename(&mut self, id: u32, name: &str) {
        let Some(idx) = self.symbol_index(id) else { return };
        let name = name.trim();
        if name.is_empty() || self.doc.library.iter().any(|s| s.name == name && s.id != id) {
            self.message = "Ya hay un elemento con ese nombre".into();
            return;
        }
        let base = self.doc.clone();
        self.doc.library[idx].name = name.to_string();
        self.commit_from(base);
    }

    pub fn lib_set_kind(&mut self, id: u32, kind: &str) {
        let Some(idx) = self.symbol_index(id) else { return };
        if matches!(self.doc.library[idx].kind, SymbolKind::Bitmap | SymbolKind::Sound) || kind == "bitmap" || kind == "sound" {
            self.message = "Un mapa de bits no puede cambiar de tipo".into();
            return;
        }
        let base = self.doc.clone();
        self.doc.library[idx].kind = SymbolKind::parse(kind);
        self.commit_from(base);
        self.sync_lib();
    }

    pub fn lib_duplicate(&mut self, id: u32) -> u32 {
        let Some(idx) = self.symbol_index(id) else { return 0 };
        let base = self.doc.clone();
        let mut s = self.doc.library[idx].clone();
        s.id = self.doc.alloc_id();
        s.name = self.unique_symbol_name(&format!("{} copia", s.name));
        for l in s.layers.iter_mut() {
            l.uid = self.doc.new_layer_uid();
        }
        let nid = s.id;
        // Duplicating a sound also duplicates its audio (the sound and its item share an id).
        if let Some(a) = self.doc.sounds.iter().find(|a| a.id == id).cloned() {
            self.doc.sounds.push(crate::sound::SoundAsset { id: nid, ..a });
        }
        self.doc.library.push(s);
        self.commit_from(base);
        self.sync_lib();
        nid
    }

    /// Deletes the symbol and all its instances (like Flash).
    pub fn lib_delete(&mut self, id: u32) {
        if self.doc.sym_edit.iter().any(|e| e.symbol == id) {
            self.message = "No se puede borrar un símbolo mientras se edita".into();
            return;
        }
        let base = self.doc.clone();
        fn purge(v: &mut Vec<Shape>, id: u32) {
            v.retain(|s| s.inst().map_or(true, |i| i.symbol != id));
            for s in v.iter_mut() {
                if let Geometry::Group { children, .. } = &mut s.geom {
                    purge(children, id);
                }
            }
        }
        self.for_each_layer_set(|ls| {
            for l in ls.iter_mut() {
                for k in l.keys.values_mut() {
                    purge(&mut k.shapes, id);
                }
            }
        });
        // A deleted sound is removed from the frames.
        if self.doc.sounds.iter().any(|s| s.id == id) {
            self.doc.sounds.retain(|s| s.id != id);
            self.for_each_layer_set(|ls| {
                for l in ls.iter_mut() {
                    for k in l.keys.values_mut() {
                        if k.sound.as_ref().map_or(false, |s| s.id == id) {
                            k.sound = None;
                        }
                    }
                }
            });
        }
        // A deleted image also takes its bitmap fills with it.
        let asset = self.bitmap_symbol_asset(id);
        self.doc.library.retain(|s| s.id != id);
        if let Some(a) = asset {
            let still = self.doc.library.iter().any(|s| bitmap_of(s).map_or(false, |b| b.id == a));
            if !still {
                self.doc.bitmaps.retain(|b| b.id != a);
                fn unbmp(v: &mut [Shape], a: u32) {
                    for s in v.iter_mut() {
                        if s.bmp.as_ref().map_or(false, |b| b.id == a) {
                            s.bmp = None;
                        }
                        if let Geometry::Group { children, .. } = &mut s.geom {
                            unbmp(children, a);
                        }
                    }
                }
                self.for_each_layer_set(|ls| {
                    for l in ls.iter_mut() {
                        for k in l.keys.values_mut() {
                            unbmp(&mut k.shapes, a);
                        }
                    }
                });
                if self.fill_bmp == Some(a) {
                    self.fill_bmp = None;
                }
            }
        }
        self.sel.clear();
        self.commit_from(base);
        self.sync_lib();
    }

    pub fn lib_new_folder(&mut self, name: &str) -> u32 {
        let base = self.doc.clone();
        let id = self.doc.alloc_id();
        let mut n = self.doc.lib_folders.len() + 1;
        let mut nm = name.trim().to_string();
        while nm.is_empty() || self.doc.lib_folders.iter().any(|f| f.name == nm) {
            nm = format!("Carpeta sin título {n}");
            n += 1;
        }
        self.doc.lib_folders.push(LibFolder { id, name: nm, open: true });
        self.commit_from(base);
        id
    }

    pub fn lib_rename_folder(&mut self, id: u32, name: &str) {
        let base = self.doc.clone();
        if let Some(f) = self.doc.lib_folders.iter_mut().find(|f| f.id == id) {
            if !name.trim().is_empty() {
                f.name = name.trim().to_string();
            }
        }
        self.commit_from(base);
    }

    pub fn lib_toggle_folder(&mut self, id: u32) {
        if let Some(f) = self.doc.lib_folders.iter_mut().find(|f| f.id == id) {
            f.open = !f.open;
        }
    }

    /// Deletes a folder (its symbols go back to the root).
    pub fn lib_delete_folder(&mut self, id: u32) {
        let base = self.doc.clone();
        self.doc.lib_folders.retain(|f| f.id != id);
        for s in self.doc.library.iter_mut().filter(|s| s.folder == Some(id)) {
            s.folder = None;
        }
        self.commit_from(base);
    }

    /// Moves a symbol to a folder (`folder` < 0: to the root).
    pub fn lib_move(&mut self, id: u32, folder: i32) {
        let Some(idx) = self.symbol_index(id) else { return };
        let base = self.doc.clone();
        self.doc.library[idx].folder = (folder >= 0).then_some(folder as u32).filter(|f| self.doc.lib_folders.iter().any(|x| x.id == *f));
        self.commit_from(base);
    }

    /// RGBA thumbnail of a symbol (Library preview), frame `frame`.
    pub fn render_symbol_thumb(&self, id: u32, frame: u32, w: u32, h: u32) -> Vec<u8> {
        let Some(sym) = symbols::symbol(id) else { return Vec::new() };
        let Some(mut pm) = tiny_skia::Pixmap::new(w.max(1), h.max(1)) else { return Vec::new() };
        pm.fill(tiny_skia::Color::from_rgba8(255, 255, 255, 255));
        let f = frame.min(sym.total() - 1);
        let shapes = symbols::symbol_shapes(&sym, f);
        let b = shapes.iter().fold(BBox::EMPTY, |acc, s| acc.union(&s.world_bounds()));
        if !b.is_empty() {
            let s = ((w as f32 - 8.0) / b.w().max(1.0)).min((h as f32 - 8.0) / b.h().max(1.0)).min(4.0);
            let base = Matrix::translate(w as f32 / 2.0, h as f32 / 2.0).mul(&Matrix::scale(s, s)).mul(&Matrix::translate(-b.center().0, -b.center().1));
            let o = render::DrawOpts::new(false, true, &|_, _| 1.0);
            render::draw_layers(&mut pm, &sym.layers, f, &base, &o);
        }
        pm.data().to_vec()
    }

    pub fn symbol_frames(&self, id: u32) -> u32 {
        symbols::symbol(id).map_or(1, |s| s.total())
    }

    // ───────── Instance properties ─────────

    pub fn set_inst_kind(&mut self, kind: &str) {
        let k = SymbolKind::parse(kind);
        if k == SymbolKind::Bitmap || self.selected_shapes().iter().any(|(_, s)| s.inst().map_or(false, |i| i.kind == SymbolKind::Bitmap)) {
            return;
        }
        self.edit_instances(|i| i.kind = k);
    }

    pub fn set_inst_name(&mut self, name: &str) {
        let n = name.trim().to_string();
        self.edit_instances(|i| i.name = n.clone());
    }

    /// Color effect as JSON: {"mode":"tint","color":{…},"amount":50}, etc.
    pub fn set_color_fx(&mut self, json: &str) {
        match serde_json::from_str::<ColorFx>(json) {
            Ok(fx) => self.edit_instances(|i| i.color = fx.clone()),
            Err(e) => self.message = format!("Efecto de color no válido: {e}"),
        }
    }

    /// "loop" | "once" | "single"
    pub fn set_loop(&mut self, mode: &str, first: u32) {
        let m = match mode {
            "once" => LoopMode::Once,
            "single" => LoopMode::Single,
            _ => LoopMode::Loop,
        };
        self.edit_instances(|i| {
            i.loop_mode = m;
            i.first = first;
        });
    }

    pub fn set_blend(&mut self, mode: &str) {
        let b = Blend::parse(mode);
        self.edit_instances(|i| i.blend = b);
    }

    pub fn set_cache(&mut self, on: bool) {
        self.edit_instances(|i| i.cache = on);
    }

    /// Swap Symbol (Properties > Swap…).
    pub fn swap_symbol(&mut self, id: u32) {
        let Some(idx) = self.symbol_index(id) else { return };
        if self.doc.sym_edit.iter().any(|e| e.symbol == id) {
            return;
        }
        // Bitmaps are swapped for bitmaps, and symbols for symbols.
        let to_bmp = self.doc.library[idx].kind == SymbolKind::Bitmap;
        let kind = self.doc.library[idx].kind;
        self.edit_instances(|i| {
            if (i.kind == SymbolKind::Bitmap) == to_bmp {
                i.symbol = id;
                if to_bmp {
                    i.kind = kind;
                }
            }
        });
    }

    // ───────── Bitmaps ─────────

    /// File > Import: an image (PNG; the UI converts the rest) into
    /// the Library and, if `to_stage`, an instance at the corner of the Stage.
    pub fn import_bitmap(&mut self, name: &str, png: Vec<u8>, to_stage: bool) -> u32 {
        let Ok(pm) = tiny_skia::Pixmap::decode_png(&png) else {
            self.message = format!("No se pudo leer la imagen «{name}»");
            return 0;
        };
        let (w, h) = (pm.width(), pm.height());
        let base = self.doc.clone();
        let asset = self.doc.alloc_id();
        self.doc.bitmaps.push(crate::bitmap::BitmapAsset { id: asset, w, h, png: crate::bitmap::b64_encode(&png), jpeg: None, quality: None });
        let layers = crate::bitmap::bitmap_layers(&mut self.doc, asset, w, h);
        let sid = self.doc.alloc_id();
        let name = self.unique_symbol_name(name);
        self.doc.library.push(Symbol { id: sid, name, kind: SymbolKind::Bitmap, layers, folder: None, scale9: None, linkage: None });
        self.sync_lib();
        if to_stage && self.can_draw() {
            let s = Shape::new(Geometry::Instance(Inst::new(sid, SymbolKind::Bitmap)), None, None, 1.0, Matrix::IDENTITY);
            self.sel = self.insert_shapes(vec![s], None);
        }
        self.commit_from(base);
        sid
    }

    /// Bitmap properties: allow smoothing.
    pub fn set_bitmap_smooth(&mut self, id: u32, on: bool) {
        let Some(idx) = self.symbol_index(id) else { return };
        let base = self.doc.clone();
        for l in self.doc.library[idx].layers.iter_mut() {
            for k in l.keys.values_mut() {
                for s in k.shapes.iter_mut() {
                    if let Some(b) = &mut s.bmp {
                        b.smooth = on;
                    }
                }
            }
        }
        self.commit_from(base);
        self.sync_lib();
    }

    /// Compression of a Library image: empty `jpeg` = lossless;
    /// otherwise, the JPEG already encoded by the UI and its quality (0…100).
    pub fn set_bitmap_jpeg(&mut self, symbol: u32, jpeg: Vec<u8>, quality: u8) {
        let Some(asset) = self.bitmap_symbol_asset(symbol) else { return };
        let base = self.doc.clone();
        if let Some(a) = self.doc.bitmaps.iter_mut().find(|a| a.id == asset) {
            if jpeg.is_empty() {
                a.jpeg = None;
                a.quality = None;
            } else {
                a.jpeg = Some(crate::bitmap::b64_encode(&jpeg));
                a.quality = Some(quality.min(100));
            }
        }
        self.commit_from(base);
    }

    /// PNG of a Library image (to encode it as JPEG).
    pub fn bitmap_png(&self, symbol: u32) -> Vec<u8> {
        self.bitmap_symbol_asset(symbol)
            .and_then(|a| self.doc.bitmaps.iter().find(|x| x.id == a))
            .map(|a| crate::bitmap::b64_decode(&a.png))
            .unwrap_or_default()
    }

    /// PNG of a Library image (for its preview in the Color Mixer).
    pub fn bitmap_asset_of(&self, symbol: u32) -> u32 {
        self.bitmap_symbol_asset(symbol).unwrap_or(0)
    }

    // ───────── Filters ─────────

    pub fn add_filter(&mut self, kind: &str) {
        let Some(f) = Filter::new(kind) else { return };
        self.edit_instances(|i| i.filters.push(FilterEntry { on: true, f: f.clone() }));
    }

    pub fn remove_filter(&mut self, idx: usize) {
        self.edit_instances(|i| {
            if idx < i.filters.len() {
                i.filters.remove(idx);
            }
        });
    }

    pub fn toggle_filter(&mut self, idx: usize) {
        self.edit_instances(|i| {
            if let Some(f) = i.filters.get_mut(idx) {
                f.on = !f.on;
            }
        });
    }

    /// Filter parameters as JSON (the same format the state returns).
    pub fn set_filter(&mut self, idx: usize, json: &str) {
        match serde_json::from_str::<FilterEntry>(json) {
            Ok(fe) => self.edit_instances(|i| {
                if let Some(f) = i.filters.get_mut(idx) {
                    *f = fe.clone();
                }
            }),
            Err(e) => self.message = format!("Filtro no válido: {e}"),
        }
    }

    // ───────── 9-slice scaling and buttons ─────────

    /// Enables/disables the symbol's 9-slice guides (movie clip).
    pub fn set_scale9(&mut self, id: u32, on: bool) {
        let Some(idx) = self.symbol_index(id).or_else(|| self.doc.sym_edit.last().and_then(|e| self.symbol_index(e.symbol))) else { return };
        let base = self.doc.clone();
        let sid = self.doc.library[idx].id;
        let editing = self.doc.sym_edit.last().map_or(false, |e| e.symbol == sid);
        let shapes = if editing {
            (0..self.doc.layers.len()).rev().flat_map(|li| self.doc.frame_shapes(li, 0)).collect::<Vec<_>>()
        } else {
            symbols::symbol(sid).map(|s| symbols::symbol_shapes(&s, 0)).unwrap_or_default()
        };
        let b = shapes.iter().fold(BBox::EMPTY, |acc, s| acc.union(&s.world_bounds()));
        let s = &mut self.doc.library[idx];
        s.scale9 = if on && !b.is_empty() {
            Some([b.x0 + b.w() * 0.25, b.y0 + b.h() * 0.25, b.x0 + b.w() * 0.75, b.y0 + b.h() * 0.75])
        } else {
            None
        };
        if on && s.kind != SymbolKind::Movieclip {
            self.message = "La escala en 9 divisiones solo actúa en clips de película".into();
        }
        self.commit_from(base);
        self.sync_lib();
    }

    /// Control > Enable Simple Buttons.
    pub fn set_simple_buttons(&mut self, on: bool) {
        self.simple_buttons = on;
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
    fn convert_place_edit_and_delete() {
        let mut e = editor();
        rect(&mut e, (100.0, 100.0), (200.0, 150.0));
        e.select_all();
        let sid = e.convert_to_symbol("Caja", "movieclip", 4, false);
        assert!(sid > 0);
        let shapes = e.doc.frame_shapes(0, 0);
        assert_eq!(shapes.len(), 1);
        assert!(shapes[0].is_instance());
        let b = shapes[0].world_bounds();
        assert!((b.x0 - 100.0).abs() < 0.5 && (b.x1 - 200.0).abs() < 0.5, "la instancia se ve donde estaba: {b:?}");
        // Another instance from the Library.
        e.place_symbol(sid, 400.0, 300.0);
        assert_eq!(e.doc.frame_shapes(0, 0).len(), 2);
        // Editing in symbol-editing mode: changing the content affects both instances.
        e.edit_symbol(sid);
        assert_eq!(e.doc.sym_edit.len(), 1);
        rect(&mut e, (-100.0, -100.0), (-80.0, -80.0));
        e.leave_symbol();
        assert!(e.doc.sym_edit.is_empty());
        let wide = e.doc.frame_shapes(0, 0).iter().map(|s| s.world_bounds().w()).fold(0.0f32, f32::max);
        assert!(wide > 150.0, "el contenido nuevo aparece en las instancias");
        e.undo();
        e.undo();
        assert_eq!(e.doc.sym_edit.len(), 1, "deshacer vuelve a la edición");
        e.redo();
        e.redo();
        e.lib_delete(sid);
        assert!(e.doc.frame_shapes(0, 0).is_empty() && e.doc.library.is_empty());
    }

    #[test]
    fn edit_in_place_uses_instance_matrix() {
        let mut e = editor();
        rect(&mut e, (100.0, 100.0), (200.0, 200.0));
        e.select_all();
        e.convert_to_symbol("", "graphic", 0, false);
        e.set_tool("select");
        e.double_click(150.0, 150.0);
        assert_eq!(e.doc.sym_edit.len(), 1);
        assert!(e.doc.sym_edit[0].in_place.is_some());
        // In place, screen point (150,150) is (50,50) in the symbol.
        let p = e.to_doc(150.0, 150.0);
        assert!((p.0 - 50.0).abs() < 0.01 && (p.1 - 50.0).abs() < 0.01);
        e.double_click(700.0, 500.0);
        assert!(e.doc.sym_edit.is_empty(), "doble clic fuera sale");
    }

    #[test]
    fn color_effect_and_filters_on_instances() {
        let mut e = editor();
        rect(&mut e, (100.0, 100.0), (200.0, 200.0));
        e.select_all();
        e.convert_to_symbol("", "movieclip", 0, false);
        e.set_color_fx(r#"{"mode":"tint","color":{"r":255,"g":0,"b":0,"a":255},"amount":100}"#);
        let leaf = &e.doc.frame_shapes(0, 0)[0].leaves(&Matrix::IDENTITY)[0];
        assert_eq!(leaf.fill.unwrap().r, 255);
        e.add_filter("dropshadow");
        e.add_filter("blur");
        let st: Value = serde_json::from_str(&e.state()).unwrap();
        assert_eq!(st["inst"]["filters"].as_array().unwrap().len(), 2);
        e.remove_filter(0);
        e.set_blend("multiply");
        let st: Value = serde_json::from_str(&e.state()).unwrap();
        assert_eq!(st["inst"]["blend"], "multiply");
        let png = e.export_png(1.0);
        assert!(png.len() > 100);
    }

    #[test]
    fn graphic_plays_its_timeline_in_the_parent() {
        let mut e = editor();
        rect(&mut e, (0.0, 0.0), (10.0, 10.0));
        e.select_all();
        let sid = e.convert_to_symbol("", "graphic", 0, false);
        e.edit_symbol(sid);
        e.select_cell(0, 4);
        e.insert_keyframe(false);
        let id = e.doc.layers[0].keys[&4].shapes[0].id;
        e.modify_shapes(&[id], |s| s.m.tx += 100.0);
        e.leave_symbol();
        e.select_cell(0, 9);
        e.insert_frame();
        let x = |e: &Editor, f: u32| {
            let s = &e.doc.frame_shapes(0, f)[0];
            crate::symbols::instance_leaves(s, s.inst().unwrap(), &Matrix::IDENTITY, f, true)[0].world_bounds().x0
        };
        assert!(x(&e, 0) < 1.0 && x(&e, 4) > 99.0);
        assert!(x(&e, 5) < 1.0, "el gráfico vuelve a empezar (bucle)");
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut pm = tiny_skia::Pixmap::new(w, h).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        pm.fill_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, (w / 2) as f32, h as f32).unwrap(), &render::paint(Rgba { r: 0, g: 0, b: 255, a: 255 }, 1.0), tiny_skia::Transform::identity(), None);
        pm.encode_png().unwrap()
    }

    #[test]
    fn import_bitmap_place_break_and_export() {
        let mut e = editor();
        let sid = e.import_bitmap("foto.png", png(40, 20), true);
        assert!(sid > 0 && e.doc.bitmaps.len() == 1);
        let s = &e.doc.frame_shapes(0, 0)[0];
        let b = s.world_bounds();
        assert!(b.x0.abs() < 0.01 && (b.x1 - 40.0).abs() < 0.01 && (b.y1 - 20.0).abs() < 0.01, "{b:?}");
        let pm = render::stage_pixmap(&e.doc, 0, 1.0).unwrap();
        let px = |x, y| pm.pixel(x, y).unwrap().demultiply();
        assert!(px(5, 10).blue() > 240 && px(35, 10).red() > 240, "la imagen se dibuja");
        // SWF: the image as DefineBitsLossless2.
        let swf = crate::swf::uncompressed(&crate::swf::export(&e.doc));
        assert!(swf.windows(2).any(|w| u16::from_le_bytes([w[0], w[1]]) >> 6 == 36));
        // Ctrl+B: a shape with a bitmap fill.
        e.break_apart();
        let v = e.doc.frame_shapes(0, 0);
        assert!(v.iter().all(|s| !s.is_instance()) && v.iter().any(|s| s.bmp.is_some()));
        // Fill another shape with the image and move it with the Fill Transform tool.
        e.set_fill_bitmap(e.doc.bitmaps[0].id);
        e.object_mode = true;
        e.set_tool("rect");
        e.pointer_down(100.0, 100.0, 0, false, false);
        e.pointer_up(200.0, 200.0, false, false);
        let r = e.doc.frame_shapes(0, 0).into_iter().find(|s| s.world_bounds().x0 > 90.0).unwrap();
        assert_eq!(r.bmp_world().unwrap().m.tx, 100.0);
        e.set_tool("filltransform");
        e.pointer_down(150.0, 150.0, 0, false, false);
        e.pointer_up(150.0, 150.0, false, false);
        let hs = e.grad_handles().unwrap();
        assert_eq!(hs.len(), 5, "centro, ancho, alto, tamaño y giro");
        let c = hs[0].1;
        e.pointer_down(c.0, c.1, 0, false, false);
        e.pointer_up(c.0 + 10.0, c.1, false, false);
        let r = e.doc.frame_shapes(0, 0).into_iter().find(|s| s.id == r.id).unwrap();
        assert!((r.bmp_world().unwrap().m.tx - 110.0).abs() < 0.01);
        // Deleting the image from the Library also removes its fill.
        e.lib_delete(sid);
        assert!(e.doc.bitmaps.is_empty() && e.doc.frame_shapes(0, 0).iter().all(|s| s.bmp.is_none()));
    }

    #[test]
    fn bitmap_color_effects_and_jpeg() {
        let mut e = editor();
        let sid = e.import_bitmap("foto.png", png(40, 20), true);
        // Red tint at 100 %: the image looks red in the editor and in the SWF.
        e.set_color_fx(r#"{"mode":"tint","color":{"r":255,"g":0,"b":0,"a":255},"amount":100}"#);
        let pm = render::stage_pixmap(&e.doc, 0, 1.0).unwrap();
        let c = pm.pixel(5, 10).unwrap().demultiply();
        assert!(c.red() > 240 && c.blue() < 15, "la parte azul se tiñe de rojo");
        // With JPEG compression, the SWF carries DefineBitsJPEG2.
        e.set_color_fx(r#"{"mode":"none"}"#);
        e.set_bitmap_jpeg(sid, vec![0xFF, 0xD8, 0xFF, 0xD9], 80);
        let swf = crate::swf::uncompressed(&e.export_swf());
        let codes = crate::swf::tests_support::tag_codes(&swf);
        assert!(codes.contains(&21) && !codes.contains(&36));
        e.set_bitmap_jpeg(sid, Vec::new(), 0);
        let codes = crate::swf::tests_support::tag_codes(&crate::swf::uncompressed(&e.export_swf()));
        assert!(codes.contains(&36));
    }

    #[test]
    fn history_slider_moves_through_steps() {
        let mut e = editor();
        let d0 = e.undo_depth();
        for i in 0..4 {
            rect(&mut e, (50.0 + i as f32 * 60.0, 100.0), (90.0 + i as f32 * 60.0, 140.0));
        }
        assert_eq!(e.undo_depth(), d0 + 4);
        e.history_go(d0 + 1);
        assert_eq!(e.doc.frame_shapes(0, 0).len(), 1);
        assert_eq!(e.redo_count(), 3);
        e.history_go(d0 + 3);
        assert_eq!(e.doc.frame_shapes(0, 0).len(), 3);
        e.history_go(d0 + 99);
        assert_eq!(e.doc.frame_shapes(0, 0).len(), 4);
        let ep = e.history_epoch();
        e.load_json(&e.to_json()).unwrap();
        assert_ne!(e.history_epoch(), ep);
    }

    #[test]
    fn break_apart_instance() {
        let mut e = editor();
        rect(&mut e, (100.0, 100.0), (200.0, 200.0));
        e.select_all();
        e.convert_to_symbol("", "graphic", 0, false);
        e.set_color_fx(r#"{"mode":"alpha","v":50}"#);
        e.break_apart();
        let s = e.doc.frame_shapes(0, 0);
        assert!(!s.iter().any(|x| x.is_instance()));
        assert!(s.iter().any(|x| x.fill.map_or(false, |c| c.a == 128)), "el efecto de color queda en los colores");
    }
}
