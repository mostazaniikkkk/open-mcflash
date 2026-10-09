//! Advanced timeline in the model: operations on frame ranges
//! (copy, paste, reverse, move, convert), special layers
//! (guides, motion guides, masks, folders) and scenes.

use crate::geom::*;
use crate::model::*;

/// Cell of a copied frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    /// Starts a span with this content.
    Key(Keyframe),
    /// Continues the previous span.
    Cont,
    /// Past the end of the layer: no frame.
    Empty,
}

impl Layer {
    /// Cells of `f0..=f1`. The first is always a keyframe (with what is visible there).
    pub fn cells(&self, f0: u32, f1: u32) -> Vec<Cell> {
        (f0..=f1)
            .map(|f| {
                if f >= self.length {
                    Cell::Empty
                } else if let Some(k) = self.keys.get(&f) {
                    Cell::Key(k.clone())
                } else if f == f0 {
                    match self.key_at_or_before(f) {
                        Some(k) => Cell::Key(Keyframe::with_settings_of(self.frame_shapes(f), &self.keys[&k])),
                        None => Cell::Empty,
                    }
                } else {
                    Cell::Cont
                }
            })
            .collect()
    }

    /// Writes cells starting at `f`, replacing that span and keeping what follows.
    pub fn write_cells(&mut self, f: u32, cells: &[Cell]) {
        let end = f + cells.len() as u32;
        let tail = (end < self.length && !self.keys.contains_key(&end))
            .then(|| self.key_at_or_before(end).map(|k| Keyframe::with_settings_of(self.frame_shapes(end), &self.keys[&k])))
            .flatten();
        self.keys.retain(|k, _| *k < f || *k >= end);
        if let Some(t) = tail {
            self.keys.insert(end, t);
        }
        self.length = self.length.max(end);
        for (i, c) in cells.iter().enumerate() {
            let fi = f + i as u32;
            match c {
                Cell::Key(k) => {
                    self.keys.insert(fi, k.clone());
                }
                Cell::Cont => {}
                Cell::Empty => {
                    if i == 0 || !matches!(cells[i - 1], Cell::Empty) {
                        self.keys.insert(fi, Keyframe::default());
                    }
                }
            }
        }
    }

    pub fn insert_frames(&mut self, f: u32, n: u32) {
        for _ in 0..n {
            self.insert_frame(f);
        }
    }

    pub fn remove_frames(&mut self, f0: u32, f1: u32) {
        for _ in f0..=f1 {
            self.remove_frame(f0);
        }
    }

    /// F6/F7 on a range: every frame becomes a keyframe (with what was visible) or a blank keyframe.
    pub fn convert_to_keyframes(&mut self, f0: u32, f1: u32, blank: bool) {
        let shown: Vec<(u32, Vec<Shape>, Option<u32>)> =
            (f0..=f1).map(|f| (f, if blank { Vec::new() } else { self.frame_shapes(f.min(self.length.saturating_sub(1))) }, self.key_at_or_before(f))).collect();
        for (f, shapes, k) in shown {
            if self.keys.contains_key(&f) && f < self.length && !blank {
                continue;
            }
            let mut kf = match k {
                Some(k) => Keyframe::with_settings_of(shapes, &self.keys[&k]),
                None => Keyframe { shapes, ..Default::default() },
            };
            if blank {
                kf.tween = false;
                kf.shape_tween = false;
            }
            self.keys.insert(f, kf);
        }
        self.length = self.length.max(f1 + 1);
    }

    /// Clear frames (Alt+Backspace): the span becomes empty.
    pub fn clear_frames(&mut self, f0: u32, f1: u32) {
        self.write_cells(f0, &vec![Cell::Empty; (f1 - f0 + 1) as usize]);
    }

    /// Modify > Timeline > Reverse Frames.
    pub fn reverse_frames(&mut self, f0: u32, f1: u32) {
        if f1 <= f0 || f0 >= self.length {
            return;
        }
        let f1 = f1.min(self.length - 1);
        // So that what comes after doesn't change when the span is reversed.
        if f1 + 1 < self.length && !self.keys.contains_key(&(f1 + 1)) {
            self.insert_keyframe(f1 + 1, false);
        }
        for f in [f0, f1] {
            if !self.keys.contains_key(&f) {
                self.insert_keyframe(f, false);
            }
        }
        let old: Vec<(u32, Keyframe)> = self.keys.range(f0..=f1).map(|(k, v)| (*k, v.clone())).collect();
        self.keys.retain(|k, _| *k < f0 || *k > f1);
        let m = old.len() - 1;
        for (i, (k, kf)) in old.iter().enumerate() {
            // The tween that started from the previous keyframe now starts from this one.
            let settings = if i == 0 { &old[m].1 } else { &old[i - 1].1 };
            let mut nk = Keyframe::with_settings_of(kf.shapes.clone(), settings);
            nk.label = kf.label.clone();
            nk.label_kind = kf.label_kind;
            nk.hints = kf.hints.clone();
            self.keys.insert(f0 + f1 - k, nk);
        }
    }

    /// Drags keyframes `f0..=f1` by `df` positions.
    pub fn move_frames(&mut self, f0: u32, f1: u32, df: i32) {
        if df == 0 {
            return;
        }
        let moved: Vec<(u32, Keyframe)> = self.keys.range(f0..=f1).map(|(k, v)| (*k, v.clone())).collect();
        if moved.is_empty() {
            return;
        }
        let span_end = self.span_end(*moved.iter().map(|(k, _)| k).max().unwrap());
        self.keys.retain(|k, _| *k < f0 || *k > f1);
        for (k, kf) in moved {
            let nk = (k as i64 + df as i64).max(0) as u32;
            self.keys.insert(nk, kf);
        }
        let last_moved = (span_end as i64 - 1 + df as i64).max(0) as u32;
        self.length = self.length.max(last_moved + 1);
    }
}

// ───────────────────────── Special layers ─────────────────────────

pub fn layer_index(layers: &[Layer], uid: u32) -> Option<usize> {
    layers.iter().position(|l| l.uid == uid)
}

pub fn parent_index(layers: &[Layer], li: usize) -> Option<usize> {
    layers[li].parent.and_then(|p| layer_index(layers, p))
}

/// Visible if it and all its folders/masks/guides are.
pub fn eff_visible(layers: &[Layer], li: usize) -> bool {
    let mut i = Some(li);
    let mut guard = 0;
    while let Some(k) = i {
        if !layers[k].visible {
            return false;
        }
        i = parent_index(layers, k);
        guard += 1;
        if guard > 64 {
            break;
        }
    }
    true
}

pub fn eff_locked(layers: &[Layer], li: usize) -> bool {
    let mut i = Some(li);
    let mut guard = 0;
    while let Some(k) = i {
        if layers[k].locked {
            return true;
        }
        i = parent_index(layers, k);
        guard += 1;
        if guard > 64 {
            break;
        }
    }
    false
}

pub fn depth(layers: &[Layer], li: usize) -> usize {
    let mut d = 0;
    let mut i = parent_index(layers, li);
    while let Some(k) = i {
        d += 1;
        i = parent_index(layers, k);
        if d > 64 {
            break;
        }
    }
    d
}

/// Mask layer that `li` depends on (if any).
pub fn mask_of(layers: &[Layer], li: usize) -> Option<usize> {
    parent_index(layers, li).filter(|&p| layers[p].kind == LayerKind::Mask)
}

/// Motion guide that `li` depends on (if any).
pub fn guide_of(layers: &[Layer], li: usize) -> Option<usize> {
    parent_index(layers, li).filter(|&p| layers[p].kind == LayerKind::MotionGuide)
}

/// Is it hidden inside a collapsed folder?
pub fn collapsed(layers: &[Layer], li: usize) -> bool {
    let mut i = parent_index(layers, li);
    while let Some(k) = i {
        if layers[k].kind == LayerKind::Folder && !layers[k].open {
            return true;
        }
        i = parent_index(layers, k);
    }
    false
}

// ───────────────────────── Motion guides ─────────────────────────

/// Path of a guide: its strokes chained by their endpoints.
pub fn guide_path(layers: &[Layer], gi: usize, f: u32) -> Vec<Pt> {
    let mut pieces: Vec<Vec<Pt>> = layers[gi]
        .frame_shapes(f)
        .iter()
        .flat_map(|s| s.leaves(&Matrix::IDENTITY))
        .filter(|s| s.stroke.is_some())
        .flat_map(|s| {
            flatten(&s.outline()).into_iter().map(move |(pts, closed)| {
                let mut v: Vec<Pt> = pts.iter().map(|p| s.m.apply(p.0, p.1)).collect();
                if closed && !v.is_empty() {
                    v.push(v[0]);
                }
                v
            })
        })
        .filter(|v| v.len() >= 2)
        .collect();
    if pieces.is_empty() {
        return Vec::new();
    }
    pieces.sort_by(|a, b| poly_len(b).partial_cmp(&poly_len(a)).unwrap());
    let mut path = pieces.remove(0);
    loop {
        let (start, end) = (path[0], *path.last().unwrap());
        let found = pieces.iter().position(|p| {
            let (a, b) = (p[0], *p.last().unwrap());
            dist(a, end) < 1.0 || dist(b, end) < 1.0 || dist(a, start) < 1.0 || dist(b, start) < 1.0
        });
        let Some(i) = found else { break };
        let mut p = pieces.remove(i);
        let (a, b) = (p[0], *p.last().unwrap());
        if dist(a, end) < 1.0 {
            path.extend(p.into_iter().skip(1));
        } else if dist(b, end) < 1.0 {
            p.reverse();
            path.extend(p.into_iter().skip(1));
        } else if dist(b, start) < 1.0 {
            p.pop();
            p.extend(path);
            path = p;
        } else {
            p.reverse();
            p.pop();
            p.extend(path);
            path = p;
        }
    }
    path
}

fn poly_len(p: &[Pt]) -> f32 {
    p.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// Arc length of the path point closest to `q`.
pub fn project(path: &[Pt], q: Pt) -> f32 {
    let (mut best, mut acc, mut at) = (f32::INFINITY, 0.0, 0.0);
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let l = dist(a, b);
        let u = if l > 0.0 { (((q.0 - a.0) * (b.0 - a.0) + (q.1 - a.1) * (b.1 - a.1)) / (l * l)).clamp(0.0, 1.0) } else { 0.0 };
        let p = (lerp(a.0, b.0, u), lerp(a.1, b.1, u));
        let d = dist(p, q);
        if d < best {
            best = d;
            at = acc + u * l;
        }
        acc += l;
    }
    at
}

/// Point and tangent angle at a given arc length.
pub fn point_at(path: &[Pt], s: f32) -> (Pt, f32) {
    let mut acc = 0.0;
    let last = path.len().saturating_sub(2);
    for (i, w) in path.windows(2).enumerate() {
        let l = dist(w[0], w[1]);
        if acc + l >= s || i == last {
            let u = if l > 0.0 { ((s - acc) / l).clamp(0.0, 1.0) } else { 0.0 };
            let p = (lerp(w[0].0, w[1].0, u), lerp(w[0].1, w[1].1, u));
            return (p, (w[1].1 - w[0].1).atan2(w[1].0 - w[0].0));
        }
        acc += l;
    }
    (path[0], 0.0)
}

/// Shapes of a layer at a frame, applying the motion guide.
pub fn layer_shapes(layers: &[Layer], li: usize, f: u32) -> Vec<Shape> {
    let l = &layers[li];
    let mut v = l.frame_shapes(f);
    let Some(gi) = guide_of(layers, li) else { return v };
    let Some((k, n, t)) = l.tween_at(f) else { return v };
    let kf = &l.keys[&k];
    if !kf.tween {
        return v;
    }
    let path = guide_path(layers, gi, f);
    if path.len() < 2 {
        return v;
    }
    let nk = &l.keys[&n];
    for s in v.iter_mut() {
        let (Some(a), Some(b)) = (kf.shapes.iter().find(|x| x.id == s.id), nk.shapes.iter().find(|x| x.id == s.id)) else { continue };
        let (s0, s1) = (project(&path, a.world_bounds().center()), project(&path, b.world_bounds().center()));
        let (pos, ang) = point_at(&path, lerp(s0, s1, t));
        let c = s.world_bounds().center();
        let mut m = Matrix::translate(pos.0 - c.0, pos.1 - c.1);
        if kf.orient {
            let (_, a0) = point_at(&path, s0);
            m = Matrix::about(pos, &Matrix::rotate(ang - a0)).mul(&m);
        }
        s.m = m.mul(&s.m);
    }
    v
}

// ───────────────────────── Document: layers and scenes ─────────────────────────

impl Document {
    pub fn frame_shapes(&self, li: usize, f: u32) -> Vec<Shape> {
        layer_shapes(&self.layers, li, f)
    }

    pub fn scene_layers(&self, i: usize) -> &[Layer] {
        if i == self.scene {
            &self.layers
        } else {
            &self.scenes[i].layers
        }
    }

    pub fn scene_total(&self, i: usize) -> u32 {
        self.scene_layers(i).iter().map(|l| l.length).max().unwrap_or(1).max(1)
    }

    /// Frames of the whole movie (all scenes in sequence).
    pub fn movie_total(&self) -> u32 {
        (0..self.scenes.len()).map(|i| self.scene_total(i)).sum::<u32>().max(1)
    }

    /// Global frame → (scene, frame within the scene).
    pub fn movie_frame(&self, g: u32) -> (usize, u32) {
        let mut acc = 0;
        for i in 0..self.scenes.len() {
            let t = self.scene_total(i);
            if g < acc + t {
                return (i, g - acc);
            }
            acc += t;
        }
        (self.scenes.len() - 1, self.scene_total(self.scenes.len() - 1) - 1)
    }

    pub fn switch_scene(&mut self, i: usize) {
        if i == self.scene || i >= self.scenes.len() {
            return;
        }
        let cur = self.scene;
        self.scenes[cur].layers = std::mem::take(&mut self.layers);
        self.layers = std::mem::take(&mut self.scenes[i].layers);
        self.scene = i;
    }

    pub fn add_scene(&mut self, at: usize) -> usize {
        let mut n = self.scenes.len() + 1;
        while self.scenes.iter().any(|s| s.name == format!("Escena {n}")) {
            n += 1;
        }
        let mut l = Layer::new("Capa 1");
        l.uid = self.new_layer_uid();
        let at = at.min(self.scenes.len());
        self.scenes.insert(at, SceneStore { name: format!("Escena {n}"), layers: vec![l] });
        if at <= self.scene {
            self.scene += 1;
        }
        at
    }

    pub fn duplicate_scene(&mut self, i: usize) -> usize {
        let mut layers = self.scene_layers(i).to_vec();
        // New layer ids (and reassigned parents).
        let map: Vec<(u32, u32)> = layers.iter().map(|l| (l.uid, 0)).collect();
        let mut map = map;
        for (old, new) in map.iter_mut() {
            let _ = old;
            *new = self.new_layer_uid();
        }
        for l in layers.iter_mut() {
            l.uid = map.iter().find(|(o, _)| *o == l.uid).unwrap().1;
            l.parent = l.parent.and_then(|p| map.iter().find(|(o, _)| *o == p).map(|x| x.1));
        }
        let name = format!("{} copia", self.scenes[i].name);
        self.scenes.insert(i + 1, SceneStore { name, layers });
        if i + 1 <= self.scene {
            self.scene += 1;
        }
        i + 1
    }

    pub fn delete_scene(&mut self, i: usize) {
        if self.scenes.len() <= 1 || i >= self.scenes.len() {
            return;
        }
        if i == self.scene {
            self.switch_scene(if i > 0 { i - 1 } else { 1 });
        }
        self.scenes.remove(i);
        if i < self.scene {
            self.scene -= 1;
        }
    }

    pub fn move_scene(&mut self, i: usize, to: usize) {
        if i >= self.scenes.len() || to >= self.scenes.len() || i == to {
            return;
        }
        let s = self.scenes.remove(i);
        self.scenes.insert(to, s);
        self.scene = if self.scene == i {
            to
        } else if i < self.scene && to >= self.scene {
            self.scene - 1
        } else if i > self.scene && to <= self.scene {
            self.scene + 1
        } else {
            self.scene
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer_with(keys: &[u32], len: u32) -> Layer {
        let mut l = Layer::new("a");
        l.keys.clear();
        for &k in keys {
            let mut kf = Keyframe::default();
            let mut s = Shape::new(Geometry::Rect { w: 10.0, h: 10.0, r: 0.0 }, Some(Rgba::BLACK), None, 1.0, Matrix::translate(k as f32 * 10.0, 0.0));
            s.id = 1;
            kf.shapes.push(s);
            l.keys.insert(k, kf);
        }
        l.length = len;
        l
    }

    fn x_at(l: &Layer, f: u32) -> f32 {
        l.frame_shapes(f)[0].m.tx
    }

    #[test]
    fn copy_paste_cells() {
        let mut l = layer_with(&[0, 5], 10);
        let cells = l.cells(5, 9);
        assert!(matches!(cells[0], Cell::Key(_)) && cells[1] == Cell::Cont);
        l.write_cells(12, &cells);
        assert_eq!(l.length, 17);
        assert_eq!(x_at(&l, 14), 50.0);
        // The middle span (10..12) still shows the previous content.
        assert_eq!(x_at(&l, 11), 50.0);
    }

    #[test]
    fn paste_over_keeps_tail() {
        let mut l = layer_with(&[0], 10);
        let other = layer_with(&[3], 5);
        l.write_cells(2, &other.cells(3, 4));
        assert_eq!(x_at(&l, 2), 30.0);
        assert_eq!(x_at(&l, 4), 0.0, "lo que seguía se conserva");
    }

    #[test]
    fn reverse_moves_keys_and_tween() {
        let mut l = layer_with(&[0, 4], 9);
        l.keys.get_mut(&0).unwrap().tween = true;
        l.reverse_frames(0, 4);
        assert_eq!(x_at(&l, 0), 40.0);
        assert_eq!(x_at(&l, 4), 0.0);
        assert!(l.keys[&0].tween, "la interpolación sigue yendo hacia delante");
        assert!((x_at(&l, 2) - 20.0).abs() < 1e-3);
    }

    #[test]
    fn convert_range_to_keyframes() {
        let mut l = layer_with(&[0, 6], 8);
        l.keys.get_mut(&0).unwrap().tween = true;
        let before: Vec<f32> = (0..6).map(|f| x_at(&l, f)).collect();
        l.convert_to_keyframes(0, 5, false);
        assert_eq!(l.keys.len(), 7);
        for f in 0..6 {
            assert!((x_at(&l, f) - before[f as usize]).abs() < 1e-3);
        }
    }

    #[test]
    fn move_keyframe() {
        let mut l = layer_with(&[0, 5], 10);
        l.move_frames(5, 5, 3);
        assert!(l.keys.contains_key(&8) && !l.keys.contains_key(&5));
        assert_eq!(x_at(&l, 6), 0.0);
        assert_eq!(l.length, 13);
    }

    #[test]
    fn clear_frames_leaves_blank() {
        let mut l = layer_with(&[0], 10);
        l.clear_frames(3, 5);
        assert!(l.frame_shapes(4).is_empty());
        assert_eq!(x_at(&l, 6), 0.0);
    }

    #[test]
    fn motion_guide_path_following() {
        let mut guide = Layer::new("guía");
        guide.uid = 1;
        guide.kind = LayerKind::MotionGuide;
        let mut arc = Shape::new(
            Geometry::Path { pts: vec![(0.0, 0.0), (100.0, -100.0), (200.0, 0.0)], closed: false, smooth: false },
            None,
            Some(Rgba::BLACK),
            1.0,
            Matrix::IDENTITY,
        );
        arc.id = 9;
        guide.keys.get_mut(&0).unwrap().shapes.push(arc);
        guide.length = 11;
        let mut l = layer_with(&[0, 10], 11);
        l.uid = 2;
        l.parent = Some(1);
        l.keys.get_mut(&0).unwrap().shapes[0].m = Matrix::translate(0.0, 0.0);
        l.keys.get_mut(&10).unwrap().shapes[0].m = Matrix::translate(200.0, 0.0);
        l.keys.get_mut(&0).unwrap().tween = true;
        let layers = vec![guide, l];
        let mid = layer_shapes(&layers, 1, 5);
        let c = mid[0].world_bounds().center();
        assert!((c.0 - 100.0).abs() < 2.0 && (c.1 + 100.0).abs() < 2.0, "sigue la curva: {c:?}");
    }

    #[test]
    fn scenes_switch_and_movie_frames() {
        let mut d = Document::new(100.0, 100.0);
        d.layers[0].length = 5;
        let i = d.add_scene(1);
        assert_eq!(d.scenes.len(), 2);
        d.switch_scene(i);
        d.layers[0].length = 3;
        assert_eq!(d.movie_total(), 8);
        assert_eq!(d.movie_frame(6), (1, 1));
        d.switch_scene(0);
        assert_eq!(d.layers[0].length, 5);
        d.delete_scene(1);
        assert_eq!(d.movie_total(), 5);
    }

    #[test]
    fn rotation_options() {
        let a = Matrix::IDENTITY;
        let b = Matrix::rotate(0.5);
        let cw = a.lerp_opts(&b, 0.5, 2, 1, true).decompose().rot;
        let want = (0.5 + std::f32::consts::TAU) / 2.0;
        assert!((cw.cos() - want.cos()).abs() < 1e-3 && (cw.sin() - want.sin()).abs() < 1e-3);
        let ccw = a.lerp_opts(&b, 0.5, 3, 0, true).decompose().rot;
        assert!(ccw < 0.0);
        let none = a.lerp_opts(&b, 0.5, 1, 0, true).decompose().rot;
        assert!(none.abs() < 1e-6);
        let noscale = a.lerp_opts(&Matrix::scale(3.0, 3.0), 0.5, 0, 0, false).decompose();
        assert!((noscale.sx - 1.0).abs() < 1e-6);
    }

    #[test]
    fn custom_ease_curve() {
        let mut k = Keyframe::default();
        k.ease_curve = Some([0.0, 1.0, 0.0, 1.0]);
        assert!(k.eased(0.25) > 0.6);
        k.ease_curve = Some([0.25, 0.25, 0.75, 0.75]);
        assert!((k.eased(0.3) - 0.3).abs() < 0.01);
    }
}
