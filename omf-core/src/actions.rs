//! Actions panel (F9): which script is being edited (the keyframe's or that of
//! the selected button or movie clip instance), syntax checking, the
//! errors from the last export and the "simple frame actions"
//! that the editor obeys during playback.

use super::*;
use crate::as2::{self, AsError, Kind, Simple};
use crate::symbols::SymbolKind;

enum Target {
    Frame { layer: usize, key: u32 },
    Instance { id: u32, kind: SymbolKind },
    None(&'static str),
}

impl Editor {
    fn action_target(&self) -> Target {
        // With a shape selected (or nothing), the panel shows the frame, like Flash.
        let sel = self.selected_shapes();
        if sel.len() == 1 {
            if let Some(i) = sel[0].1.inst() {
                return match i.kind {
                    SymbolKind::Button | SymbolKind::Movieclip => Target::Instance { id: sel[0].1.id, kind: i.kind },
                    _ => Target::None("A los gráficos y mapas de bits no se les pueden asignar acciones"),
                };
            }
        }
        let l = &self.doc.layers[self.layer.min(self.doc.layers.len() - 1)];
        if l.kind == LayerKind::Folder {
            return Target::None("Las carpetas de capas no tienen fotogramas");
        }
        match l.key_at_or_before(self.frame).filter(|_| self.frame < l.length) {
            Some(key) => Target::Frame { layer: self.layer, key },
            None => Target::None("No hay ningún fotograma clave aquí"),
        }
    }

    fn as_ctx(&self) -> as2::Ctx {
        self.doc.closed().as_ctx()
    }
}

#[wasm_bindgen]
impl Editor {
    /// What the Actions panel edits: {kind: "frame" | "button" | "movieclip" | "none", label, src}.
    pub fn actions_target(&self) -> String {
        match self.action_target() {
            Target::Frame { layer, key } => {
                let l = &self.doc.layers[layer];
                json!({ "kind": "frame", "label": format!("{} : {}", l.name, key + 1), "src": l.keys[&key].actions, "layer": layer, "key": key })
            }
            Target::Instance { id, kind } => {
                let s = self.selected_shapes().into_iter().find(|(_, s)| s.id == id).map(|(_, s)| s).unwrap();
                let i = s.inst().unwrap();
                let sym = self.doc.library.iter().find(|x| x.id == i.symbol).map(|x| x.name.clone()).unwrap_or_default();
                let label = if i.name.is_empty() { sym } else { format!("{} ({sym})", i.name) };
                json!({ "kind": if kind == SymbolKind::Button { "button" } else { "movieclip" }, "label": label, "src": i.actions })
            }
            Target::None(why) => json!({ "kind": "none", "label": why, "src": "" }),
        }
        .to_string()
    }

    /// Saves the script to the keyframe or the instance.
    pub fn set_actions(&mut self, src: &str) {
        let base = self.doc.clone();
        match self.action_target() {
            Target::Frame { layer, key } => {
                if let Some(k) = self.doc.layers[layer].keys.get_mut(&key) {
                    k.actions = src.to_string();
                }
            }
            Target::Instance { id, .. } => {
                let s = src.to_string();
                self.modify_shapes(&[id], |sh| {
                    if let Geometry::Instance(i) = &mut sh.geom {
                        i.actions = s.clone();
                    }
                });
            }
            Target::None(why) => {
                self.message = why.into();
                return;
            }
        }
        self.commit_from(base);
    }

    /// Check syntax: [{line, msg}] for the current target.
    pub fn check_actions(&self, src: &str) -> String {
        let kind = match self.action_target() {
            Target::Instance { kind: SymbolKind::Button, .. } => Kind::Button,
            Target::Instance { .. } => Kind::Clip,
            _ => Kind::Frame,
        };
        let errs: Vec<Value> = as2::check(src, kind, &self.as_ctx()).into_iter().map(|e| json!({ "line": e.line, "msg": e.msg })).collect();
        Value::Array(errs).to_string()
    }

    // ───────── ActionScript files (.as) ─────────

    /// [{name, class (full name if it is a class), lines}]
    pub fn scripts_list(&self) -> String {
        let v: Vec<Value> = self
            .doc
            .scripts
            .iter()
            .map(|s| {
                let class = if as2::parse::is_class_file(&s.code) { as2::parse::parse_class(&s.code).ok().map(|c| c.name) } else { None };
                json!({ "name": s.name, "class": class, "lines": s.code.lines().count() })
            })
            .collect();
        Value::Array(v).to_string()
    }

    pub fn script_code(&self, name: &str) -> String {
        self.doc.scripts.iter().find(|s| s.name == name).map(|s| s.code.clone()).unwrap_or_default()
    }

    /// Creates or saves an .as file.
    pub fn set_script(&mut self, name: &str, code: &str) {
        let mut name = name.trim().replace('\\', "/");
        if name.is_empty() {
            self.message = "El archivo necesita un nombre".into();
            return;
        }
        if !name.to_ascii_lowercase().ends_with(".as") {
            name.push_str(".as");
        }
        let base = self.doc.clone();
        match self.doc.scripts.iter_mut().find(|s| s.name == name) {
            Some(s) => s.code = code.to_string(),
            None => self.doc.scripts.push(crate::model::Script { name, code: code.to_string() }),
        }
        self.commit_from(base);
    }

    pub fn delete_script(&mut self, name: &str) {
        let base = self.doc.clone();
        self.doc.scripts.retain(|s| s.name != name);
        self.commit_from(base);
    }

    pub fn rename_script(&mut self, old: &str, new: &str) {
        let mut new = new.trim().replace('\\', "/");
        if !new.to_ascii_lowercase().ends_with(".as") {
            new.push_str(".as");
        }
        if new.len() <= 3 || self.doc.scripts.iter().any(|s| s.name == new) {
            self.message = "Ya hay un archivo con ese nombre".into();
            return;
        }
        let base = self.doc.clone();
        if let Some(s) = self.doc.scripts.iter_mut().find(|s| s.name == old) {
            s.name = new;
        }
        self.commit_from(base);
    }

    /// Check syntax of an .as file: [{line, msg}].
    pub fn check_script(&self, name: &str, code: &str) -> String {
        let mut ctx = self.as_ctx();
        match ctx.scripts.iter_mut().find(|s| s.0 == name) {
            Some(s) => s.1 = code.to_string(),
            None => ctx.scripts.push((name.to_string(), code.to_string())),
        }
        let errs: Vec<AsError> = if as2::parse::is_class_file(code) {
            let (_, e) = as2::class::compile_classes(&ctx, &[]);
            e.into_iter().filter(|(f, _)| f == name).map(|(_, e)| e).collect()
        } else {
            as2::check(code, Kind::Frame, &ctx)
        };
        Value::Array(errs.into_iter().map(|e| json!({ "line": e.line, "msg": e.msg })).collect()).to_string()
    }

    /// ActionScript keywords (for syntax highlighting).
    pub fn as_keywords(&self) -> String {
        serde_json::to_string(&as2::parse::KEYWORDS[..]).unwrap_or_default()
    }

    /// Symbol linkage: JSON {"id", "class"} or "null" to remove it.
    pub fn set_linkage(&mut self, symbol: u32, json: &str) {
        let link = if json.trim() == "null" {
            None
        } else {
            match serde_json::from_str::<crate::symbols::Linkage>(json) {
                Ok(mut l) => {
                    l.id = l.id.trim().to_string();
                    l.class = l.class.trim().to_string();
                    if l.id.is_empty() {
                        self.message = "El identificador de vinculación no puede estar vacío".into();
                        return;
                    }
                    if self.doc.library.iter().any(|s| s.id != symbol && s.linkage.as_ref().map_or(false, |o| o.id == l.id)) {
                        self.message = format!("Ya hay otro símbolo con el identificador «{}»", l.id);
                        return;
                    }
                    Some(l)
                }
                Err(e) => {
                    self.message = format!("Vinculación no válida: {e}");
                    return;
                }
            }
        };
        let base = self.doc.clone();
        let Some(s) = self.doc.library.iter_mut().find(|s| s.id == symbol) else { return };
        s.linkage = link;
        self.commit_from(base);
    }

    /// ActionScript errors from the last export or movie test.
    pub fn compile_errors(&self) -> String {
        serde_json::to_string(&crate::swf::last_errors()).unwrap_or_default()
    }

    /// Simple frame actions of the keyframes starting at `frame`
    /// (current timeline): [{op: "stop"|"play"|"goto", frame, play}].
    pub fn simple_actions_at(&self, frame: u32) -> String {
        let mut out = Vec::new();
        for l in &self.doc.layers {
            if matches!(l.kind, LayerKind::Folder | LayerKind::Guide | LayerKind::MotionGuide) || frame >= l.length {
                continue;
            }
            let Some(k) = l.keys.get(&frame) else { continue };
            for a in as2::simple_actions(&k.actions) {
                out.push(match a {
                    Simple::Stop => json!({ "op": "stop" }),
                    Simple::Play => json!({ "op": "play" }),
                    Simple::Next => json!({ "op": "goto", "frame": frame + 1, "play": false }),
                    Simple::Prev => json!({ "op": "goto", "frame": frame.saturating_sub(1), "play": false }),
                    Simple::Goto(target, play) => {
                        let f = match target {
                            Ok(n) => Some(n - 1),
                            Err(label) => self.doc.layers.iter().flat_map(|l| l.keys.iter()).find(|(_, k)| k.label == label).map(|(f, _)| *f),
                        };
                        match f {
                            Some(f) => json!({ "op": "goto", "frame": f, "play": play }),
                            None => continue,
                        }
                    }
                });
            }
        }
        Value::Array(out).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_and_instance_actions_go_into_the_swf() {
        let mut e = Editor::new();
        e.set_viewport(800, 600, 1.0);
        e.zoom = 1.0;
        e.pan = (0.0, 0.0);
        let t: Value = serde_json::from_str(&e.actions_target()).unwrap();
        assert_eq!(t["kind"], "frame");
        e.set_actions("stop();\ntrace('hola');");
        assert!(e.doc.layers[0].keys[&0].actions.contains("stop"));
        // Button with on (release).
        e.object_mode = true;
        e.set_tool("rect");
        e.pointer_down(100.0, 100.0, 0, false, false);
        e.pointer_up(150.0, 150.0, false, false);
        e.select_all();
        e.convert_to_symbol("Boton", "button", 0, false);
        let t: Value = serde_json::from_str(&e.actions_target()).unwrap();
        assert_eq!(t["kind"], "button");
        assert_eq!(serde_json::from_str::<Vec<Value>>(&e.check_actions("stop();")).unwrap().len(), 1, "en un botón hace falta on()");
        e.set_actions("on (release) { gotoAndPlay(2); }");
        let swf = crate::swf::uncompressed(&e.export_swf());
        let codes = crate::swf::tests_support::tag_codes(&swf);
        assert!(codes.contains(&12), "DoAction");
        assert!(codes.contains(&34), "DefineButton2 con acciones");
        assert_eq!(e.compile_errors(), "[]");
        // An error is reported with its location.
        e.deselect();
        e.set_actions("stop(;");
        let _ = e.export_swf();
        let errs: Vec<String> = serde_json::from_str(&e.compile_errors()).unwrap();
        assert!(errs[0].contains("Escena=Escena 1, capa=Capa 1, fotograma=1") && errs[0].contains("Línea 1"), "{errs:?}");
    }

    #[test]
    fn simple_frame_actions() {
        let mut e = Editor::new();
        e.select_cell(0, 9);
        e.insert_frame();
        e.select_cell(0, 4);
        e.insert_keyframe(true);
        e.set_label("fin", "name");
        e.select_cell(0, 2);
        e.insert_keyframe(true);
        e.set_actions("gotoAndStop(\"fin\")");
        let v: Vec<Value> = serde_json::from_str(&e.simple_actions_at(2)).unwrap();
        assert_eq!(v[0]["frame"], 4);
        assert_eq!(v[0]["play"], false);
    }
}
