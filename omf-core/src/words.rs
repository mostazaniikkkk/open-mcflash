//! Document text for "Check Spelling": text fields, layer
//! names, frame labels and Library names, in all
//! scenes and symbols; and replacing one of them.

use super::*;
use serde::{Deserialize, Serialize};

/// Where a checkable text is located.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Spot {
    Text { place: Place, layer: usize, key: u32, id: u32 },
    Layer { place: Place, layer: usize },
    Label { place: Place, layer: usize, key: u32 },
    Symbol { id: u32 },
}

/// Timeline: a scene or a symbol.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Place {
    Scene(usize),
    Symbol(u32),
}

fn walk_texts(v: &[Shape], out: &mut Vec<(u32, String)>) {
    for s in v {
        match &s.geom {
            Geometry::Text(t) => out.push((s.id, t.text.clone())),
            Geometry::Group { children, .. } => walk_texts(children, out),
            _ => {}
        }
    }
}

fn set_text(v: &mut [Shape], id: u32, text: &str) -> bool {
    for s in v.iter_mut() {
        if s.id == id {
            if let Geometry::Text(t) = &mut s.geom {
                t.text = text.to_string();
                return true;
            }
        }
        if let Geometry::Group { children, .. } = &mut s.geom {
            if set_text(children, id, text) {
                return true;
            }
        }
    }
    false
}

impl Editor {
    fn place_layers(&mut self, p: Place) -> Option<&mut Vec<Layer>> {
        match p {
            Place::Scene(i) if i == self.doc.scene => Some(&mut self.doc.layers),
            Place::Scene(i) => self.doc.scenes.get_mut(i).map(|s| &mut s.layers),
            Place::Symbol(id) => self.doc.library.iter_mut().find(|s| s.id == id).map(|s| &mut s.layers),
        }
    }
}

#[wasm_bindgen]
impl Editor {
    /// All checkable texts: [{spot, text, where}]. First closes
    /// group and symbol editing (the whole document is checked).
    pub fn spell_items(&mut self, texts: bool, layers: bool, labels: bool, library: bool) -> String {
        self.text_commit();
        self.exit_all_groups();
        self.exit_all_symbols();
        let mut places: Vec<(Place, String, Vec<Layer>)> = Vec::new();
        for i in 0..self.doc.scenes.len().max(1) {
            let name = self.doc.scenes.get(i).map_or("Escena 1".to_string(), |s| s.name.clone());
            let ls = if i == self.doc.scene { self.doc.layers.clone() } else { self.doc.scenes[i].layers.clone() };
            places.push((Place::Scene(i), name, ls));
        }
        for s in &self.doc.library {
            places.push((Place::Symbol(s.id), s.name.clone(), s.layers.clone()));
        }
        let mut out = Vec::new();
        for (place, pname, ls) in &places {
            for (li, l) in ls.iter().enumerate() {
                if layers {
                    out.push(json!({ "spot": Spot::Layer { place: *place, layer: li }, "text": l.name, "where": format!("{pname} › capa") }));
                }
                for (k, kf) in &l.keys {
                    if labels && !kf.label.is_empty() {
                        out.push(json!({ "spot": Spot::Label { place: *place, layer: li, key: *k }, "text": kf.label, "where": format!("{pname} › {} › etiqueta del fotograma {}", l.name, k + 1) }));
                    }
                    if texts {
                        let mut v = Vec::new();
                        walk_texts(&kf.shapes, &mut v);
                        for (id, t) in v {
                            out.push(json!({ "spot": Spot::Text { place: *place, layer: li, key: *k, id }, "text": t, "where": format!("{pname} › {} › fotograma {}", l.name, k + 1) }));
                        }
                    }
                }
            }
        }
        if library {
            for s in &self.doc.library {
                out.push(json!({ "spot": Spot::Symbol { id: s.id }, "text": s.name, "where": "Biblioteca" }));
            }
        }
        Value::Array(out).to_string()
    }

    /// Changes the text of a location returned by `spell_items`.
    pub fn spell_replace(&mut self, spot: &str, text: &str) -> bool {
        let Ok(spot) = serde_json::from_str::<Spot>(spot) else { return false };
        let base = self.doc.clone();
        let ok = match spot {
            Spot::Text { place, layer, key, id } => self
                .place_layers(place)
                .and_then(|ls| ls.get_mut(layer))
                .and_then(|l| l.keys.get_mut(&key))
                .map_or(false, |k| set_text(&mut k.shapes, id, text)),
            Spot::Layer { place, layer } => self.place_layers(place).and_then(|ls| ls.get_mut(layer)).map(|l| l.name = text.to_string()).is_some(),
            Spot::Label { place, layer, key } => self
                .place_layers(place)
                .and_then(|ls| ls.get_mut(layer))
                .and_then(|l| l.keys.get_mut(&key))
                .map(|k| k.label = text.to_string())
                .is_some(),
            Spot::Symbol { id } => self.doc.library.iter_mut().find(|s| s.id == id).map(|s| s.name = text.to_string()).is_some(),
        };
        self.commit_from(base);
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_and_replace_text_everywhere() {
        let mut e = Editor::new();
        e.set_viewport(800, 600, 1.0);
        e.zoom = 1.0;
        e.pan = (0.0, 0.0);
        e.set_tool("text");
        e.pointer_down(100.0, 100.0, 0, false, false);
        e.pointer_up(100.0, 100.0, false, false);
        e.text_set("Ola mundo");
        e.text_commit();
        let items: Vec<Value> = serde_json::from_str(&e.spell_items(true, true, true, true)).unwrap();
        let t = items.iter().find(|i| i["text"] == "Ola mundo").unwrap();
        assert!(e.spell_replace(&t["spot"].to_string(), "Hola mundo"));
        let Geometry::Text(tf) = &e.doc.frame_shapes(0, 0)[0].geom else { panic!() };
        assert_eq!(tf.text, "Hola mundo");
        let l = items.iter().find(|i| i["spot"]["kind"] == "layer").unwrap();
        e.spell_replace(&l["spot"].to_string(), "Fondo");
        assert_eq!(e.doc.layers[0].name, "Fondo");
        e.undo();
        assert_eq!(e.doc.layers[0].name, "Capa 1");
    }
}
