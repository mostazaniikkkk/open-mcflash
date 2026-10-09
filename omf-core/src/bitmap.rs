//! Bitmaps: imported images (File > Import), their Library item
//! and bitmap fills.
//!
//! The image is stored in the document as PNG (base64). The UI
//! decodes it with the browser (PNG, JPEG, GIF, WebP…) and delivers it already as PNG;
//! the core decodes it once and draws it with tiny-skia. A Library
//! bitmap is a symbol whose content is a rectangle with a bitmap
//! fill, just as Flash exports it.

use crate::geom::*;
use crate::model::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use tiny_skia::Pixmap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BitmapAsset {
    pub id: u32,
    pub w: u32,
    pub h: u32,
    /// PNG in base64.
    pub png: String,
    /// "Photo (JPEG)" compression for SWF: the already-encoded JPEG (base64) and its quality.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jpeg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<u8>,
}

fn yes() -> bool {
    true
}

/// Bitmap fill: `m` maps the image pixels to the shape's
/// local coordinates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BitmapFill {
    pub id: u32,
    pub m: Matrix,
    /// Clipped (edges extend) or tiled.
    #[serde(default)]
    pub clip: bool,
    #[serde(default = "yes")]
    pub smooth: bool,
    /// Color effect inherited from the containing instance (not saved).
    #[serde(skip)]
    pub cx: Option<crate::symbols::CxForm>,
}

impl BitmapFill {
    pub fn transformed(&self, t: &Matrix) -> BitmapFill {
        BitmapFill { m: t.mul(&self.m), ..self.clone() }
    }
    pub fn approx_eq(&self, o: &BitmapFill) -> bool {
        let (a, b) = (&self.m, &o.m);
        self.id == o.id
            && self.clip == o.clip
            && [(a.a, b.a), (a.b, b.b), (a.c, b.c), (a.d, b.d)].iter().all(|(x, y)| (x - y).abs() < 1e-3)
            && (a.tx - b.tx).abs() < 0.05
            && (a.ty - b.ty).abs() < 0.05
    }
    /// Image size (for the Fill Transform handles).
    pub fn size(&self) -> (f32, f32) {
        pixmap(self.id).map_or((1.0, 1.0), |p| (p.width() as f32, p.height() as f32))
    }
}

// ───────────────────────── Base64 ─────────────────────────

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

pub fn b64_decode(s: &str) -> Vec<u8> {
    let mut lut = [255u8; 256];
    for (i, c) in B64.iter().enumerate() {
        lut[*c as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0);
    for b in s.bytes() {
        let v = lut[b as usize];
        if v == 255 {
            continue;
        }
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

// ───────────────────────── Decoded image registry ─────────────────────────

thread_local! {
    static PIXMAPS: RefCell<HashMap<u32, (String, Rc<Pixmap>)>> = RefCell::new(HashMap::new());
}

fn key(a: &BitmapAsset) -> String {
    format!("{}:{}:{}", a.png.len(), &a.png[..a.png.len().min(48)], &a.png[a.png.len().saturating_sub(48)..])
}

/// Decodes (only once) the document's images.
pub fn sync(doc: &Document) {
    PIXMAPS.with(|m| {
        let mut m = m.borrow_mut();
        m.retain(|id, _| doc.bitmaps.iter().any(|a| a.id == *id));
        for a in &doc.bitmaps {
            let k = key(a);
            if m.get(&a.id).map_or(true, |(old, _)| *old != k) {
                if let Ok(p) = Pixmap::decode_png(&b64_decode(&a.png)) {
                    m.insert(a.id, (k, Rc::new(p)));
                }
            }
        }
    });
}

pub fn pixmap(id: u32) -> Option<Rc<Pixmap>> {
    PIXMAPS.with(|m| m.borrow().get(&id).map(|(_, p)| p.clone()))
}

thread_local! {
    static TINTED: RefCell<HashMap<String, Rc<Pixmap>>> = RefCell::new(HashMap::new());
}

/// The image with a color effect applied (tint, brightness, alpha…), cached.
pub fn pixmap_cx(id: u32, cx: Option<&crate::symbols::CxForm>) -> Option<Rc<Pixmap>> {
    let base = pixmap(id)?;
    let Some(cx) = cx.filter(|c| !c.is_identity()) else { return Some(base) };
    let key = format!("{id}:{:?}:{:?}", cx.mul, cx.add);
    if let Some(p) = TINTED.with(|t| t.borrow().get(&key).cloned()) {
        return Some(p);
    }
    let mut p = (*base).clone();
    for px in p.pixels_mut() {
        let c = px.demultiply();
        let o = cx.apply(Rgba { r: c.red(), g: c.green(), b: c.blue(), a: c.alpha() });
        *px = tiny_skia::ColorU8::from_rgba(o.r, o.g, o.b, o.a).premultiply();
    }
    let p = Rc::new(p);
    TINTED.with(|t| {
        let mut t = t.borrow_mut();
        if t.len() > 64 {
            t.clear();
        }
        t.insert(key, p.clone());
    });
    Some(p)
}

/// DefineBitsLossless2 data: premultiplied ARGB compressed with zlib.
pub fn lossless2(id: u32, cx: Option<&crate::symbols::CxForm>) -> Option<(u16, u16, Vec<u8>)> {
    let p = pixmap_cx(id, cx)?;
    let mut argb = Vec::with_capacity(p.data().len());
    for px in p.data().chunks(4) {
        argb.extend_from_slice(&[px[3], px[0], px[1], px[2]]);
    }
    Some((p.width().min(65535) as u16, p.height().min(65535) as u16, miniz_oxide::deflate::compress_to_vec_zlib(&argb, 6)))
}

/// Library symbol content for a bitmap: a rectangle
/// with the image, with the registration point at the top-left corner.
pub fn bitmap_layers(doc: &mut Document, asset: u32, w: u32, h: u32) -> Vec<Layer> {
    let (w, h) = (w as f32, h as f32);
    let mut s = Shape::new(Geometry::Rect { w, h, r: 0.0 }, Some(Rgba { r: 128, g: 128, b: 128, a: 255 }), None, 1.0, Matrix::translate(w / 2.0, h / 2.0));
    s.id = doc.alloc_id();
    s.bmp = Some(BitmapFill { id: asset, m: Matrix::translate(-w / 2.0, -h / 2.0), clip: true, smooth: true, cx: None });
    let mut l = Layer::new("Capa 1");
    l.uid = doc.new_layer_uid();
    l.keys.get_mut(&0).unwrap().shapes.push(s);
    vec![l]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip() {
        for v in [&b""[..], b"a", b"ab", b"abc", b"abcd", &[0, 255, 128, 7, 9]] {
            assert_eq!(b64_decode(&b64_encode(v)), v);
        }
        assert_eq!(b64_encode(b"Flash"), "Rmxhc2g=");
    }
}
