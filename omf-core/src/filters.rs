//! Flash 8 filters (drop shadow, blur, glow, bevel, adjust color) and
//! blend modes, applied to an instance's image.

use crate::symbols::{adjust_color_matrix, Blend, Filter};
use tiny_skia::{BlendMode, FilterQuality, Mask, Pixmap, PixmapPaint, Transform};

/// Premultiplied floating-point RGBA image (0…1).
struct Img {
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
}

impl Img {
    fn from(pm: &Pixmap) -> Img {
        let px = pm.data().chunks(4).map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, c[3] as f32 / 255.0]).collect();
        Img { w: pm.width() as usize, h: pm.height() as usize, px }
    }
    fn write(&self, pm: &mut Pixmap) {
        for (d, s) in pm.data_mut().chunks_mut(4).zip(&self.px) {
            let a = s[3].clamp(0.0, 1.0);
            for k in 0..3 {
                d[k] = (s[k].clamp(0.0, a) * 255.0).round() as u8;
            }
            d[3] = (a * 255.0).round() as u8;
        }
    }
    fn alpha(&self) -> Vec<f32> {
        self.px.iter().map(|p| p[3]).collect()
    }
}

/// Separable box blur (`passes` times ≈ Gaussian, like Flash's quality).
fn blur_plane(v: &mut [f32], w: usize, h: usize, rx: usize, ry: usize, passes: u8) {
    let mut tmp = vec![0.0f32; v.len()];
    for _ in 0..passes.max(1) {
        if rx > 0 {
            for y in 0..h {
                let row = &v[y * w..(y + 1) * w];
                let mut acc = 0.0;
                let n = (2 * rx + 1) as f32;
                for x in 0..=rx.min(w - 1) {
                    acc += row[x];
                }
                for x in 0..w {
                    tmp[y * w + x] = acc / n;
                    if x + rx + 1 < w {
                        acc += row[x + rx + 1];
                    }
                    if x >= rx {
                        acc -= row[x - rx];
                    }
                }
            }
            v.copy_from_slice(&tmp);
        }
        if ry > 0 {
            for x in 0..w {
                let mut acc = 0.0;
                let n = (2 * ry + 1) as f32;
                for y in 0..=ry.min(h - 1) {
                    acc += v[y * w + x];
                }
                for y in 0..h {
                    tmp[y * w + x] = acc / n;
                    if y + ry + 1 < h {
                        acc += v[(y + ry + 1) * w + x];
                    }
                    if y >= ry {
                        acc -= v[(y - ry) * w + x];
                    }
                }
            }
            v.copy_from_slice(&tmp);
        }
    }
}

fn radius(blur: f32, scale: f32) -> usize {
    ((blur * scale) / 2.0).round().max(0.0) as usize
}

fn shift(v: &[f32], w: usize, h: usize, dx: i32, dy: i32, fill: f32) -> Vec<f32> {
    let mut out = vec![fill; v.len()];
    for y in 0..h as i32 {
        let sy = y - dy;
        if sy < 0 || sy >= h as i32 {
            continue;
        }
        for x in 0..w as i32 {
            let sx = x - dx;
            if sx >= 0 && sx < w as i32 {
                out[(y as usize) * w + x as usize] = v[(sy as usize) * w + sx as usize];
            }
        }
    }
    out
}

fn over(dst: &mut [f32; 4], src: [f32; 4]) {
    let k = 1.0 - src[3];
    for i in 0..4 {
        dst[i] = src[i] + dst[i] * k;
    }
}

fn colored(c: crate::model::Rgba, a: f32) -> [f32; 4] {
    let a = (a * c.a as f32 / 255.0).clamp(0.0, 1.0);
    [c.r as f32 / 255.0 * a, c.g as f32 / 255.0 * a, c.b as f32 / 255.0 * a, a]
}

/// Applies the filters (in order) to an instance's image. `scale` = pixels per unit.
pub fn apply(pm: &mut Pixmap, filters: &[&Filter], scale: f32) {
    if filters.is_empty() {
        return;
    }
    let mut img = Img::from(pm);
    let (w, h) = (img.w, img.h);
    for f in filters {
        match f {
            Filter::Blur { blur_x, blur_y, quality } => {
                let (rx, ry) = (radius(*blur_x, scale), radius(*blur_y, scale));
                for ch in 0..4 {
                    let mut plane: Vec<f32> = img.px.iter().map(|p| p[ch]).collect();
                    blur_plane(&mut plane, w, h, rx, ry, *quality);
                    for (p, v) in img.px.iter_mut().zip(plane) {
                        p[ch] = v;
                    }
                }
            }
            Filter::Dropshadow { color, blur_x, blur_y, strength, angle, distance, quality, inner, knockout, hide } => {
                let a = img.alpha();
                let rad = angle.to_radians();
                let (dx, dy) = ((rad.cos() * distance * scale).round() as i32, (rad.sin() * distance * scale).round() as i32);
                let k = strength / 100.0;
                if *inner {
                    let mut inv: Vec<f32> = a.iter().map(|v| 1.0 - v).collect();
                    blur_plane(&mut inv, w, h, radius(*blur_x, scale), radius(*blur_y, scale), *quality);
                    let sh = shift(&inv, w, h, dx, dy, 1.0);
                    for i in 0..w * h {
                        let s = colored(*color, (sh[i] * k).min(1.0) * a[i]);
                        if *knockout {
                            img.px[i] = s;
                        } else {
                            over(&mut img.px[i], s);
                        }
                    }
                } else {
                    let mut m = a.clone();
                    blur_plane(&mut m, w, h, radius(*blur_x, scale), radius(*blur_y, scale), *quality);
                    let sh = shift(&m, w, h, dx, dy, 0.0);
                    for i in 0..w * h {
                        let mut s = colored(*color, (sh[i] * k).min(1.0));
                        if *knockout {
                            let keep = 1.0 - a[i];
                            s.iter_mut().for_each(|v| *v *= keep);
                            img.px[i] = s;
                        } else if *hide {
                            img.px[i] = s;
                        } else {
                            let obj = img.px[i];
                            over(&mut s, obj);
                            img.px[i] = s;
                        }
                    }
                }
            }
            Filter::Glow { color, blur_x, blur_y, strength, quality, inner, knockout } => {
                let a = img.alpha();
                let k = strength / 100.0;
                let mut m: Vec<f32> = if *inner { a.iter().map(|v| 1.0 - v).collect() } else { a.clone() };
                blur_plane(&mut m, w, h, radius(*blur_x, scale), radius(*blur_y, scale), *quality);
                for i in 0..w * h {
                    if *inner {
                        let s = colored(*color, (m[i] * k).min(1.0) * a[i]);
                        if *knockout {
                            img.px[i] = s;
                        } else {
                            over(&mut img.px[i], s);
                        }
                    } else {
                        let mut s = colored(*color, (m[i] * k).min(1.0));
                        if *knockout {
                            let keep = 1.0 - a[i];
                            s.iter_mut().for_each(|v| *v *= keep);
                            img.px[i] = s;
                        } else {
                            let obj = img.px[i];
                            over(&mut s, obj);
                            img.px[i] = s;
                        }
                    }
                }
            }
            Filter::Bevel { shadow, highlight, blur_x, blur_y, strength, angle, distance, quality, knockout, bevel } => {
                let a = img.alpha();
                let mut m = a.clone();
                blur_plane(&mut m, w, h, radius(*blur_x, scale), radius(*blur_y, scale), *quality);
                let rad = angle.to_radians();
                let (dx, dy) = ((rad.cos() * distance * scale).round() as i32, (rad.sin() * distance * scale).round() as i32);
                // The light comes from the side opposite the shadow angle.
                let lit = shift(&m, w, h, -dx, -dy, 0.0);
                let dark = shift(&m, w, h, dx, dy, 0.0);
                let k = strength / 100.0;
                for i in 0..w * h {
                    let region = match bevel.as_str() {
                        "outer" => 1.0 - a[i],
                        "full" => 1.0,
                        _ => a[i],
                    };
                    let hl = ((dark[i] - lit[i]).max(0.0) * k * 2.0).min(1.0) * region;
                    let sh = ((lit[i] - dark[i]).max(0.0) * k * 2.0).min(1.0) * region;
                    if *knockout {
                        img.px[i] = [0.0; 4];
                    }
                    over(&mut img.px[i], colored(*highlight, hl));
                    over(&mut img.px[i], colored(*shadow, sh));
                }
            }
            Filter::Adjustcolor { brightness, contrast, saturation, hue } => {
                let mx = adjust_color_matrix(*brightness, *contrast, *saturation, *hue);
                for p in img.px.iter_mut() {
                    if p[3] <= 0.0 {
                        continue;
                    }
                    let (r, g, b, al) = (p[0] / p[3] * 255.0, p[1] / p[3] * 255.0, p[2] / p[3] * 255.0, p[3] * 255.0);
                    let row = |k: usize| (mx[k * 5] * r + mx[k * 5 + 1] * g + mx[k * 5 + 2] * b + mx[k * 5 + 3] * al + mx[k * 5 + 4]).clamp(0.0, 255.0) / 255.0;
                    let na = row(3);
                    *p = [row(0) * na, row(1) * na, row(2) * na, na];
                }
            }
        }
    }
    img.write(pm);
}

/// Composites an instance's image onto the destination with its blend mode.
pub fn composite(dst: &mut Pixmap, src: &Pixmap, blend: Blend, clip: Option<&Mask>) {
    let mode = match blend {
        Blend::Normal | Blend::Layer => Some(BlendMode::SourceOver),
        Blend::Darken => Some(BlendMode::Darken),
        Blend::Multiply => Some(BlendMode::Multiply),
        Blend::Lighten => Some(BlendMode::Lighten),
        Blend::Screen => Some(BlendMode::Screen),
        Blend::Overlay => Some(BlendMode::Overlay),
        Blend::Hardlight => Some(BlendMode::HardLight),
        Blend::Add => Some(BlendMode::Plus),
        Blend::Difference => Some(BlendMode::Difference),
        Blend::Alpha => Some(BlendMode::DestinationIn),
        Blend::Erase => Some(BlendMode::DestinationOut),
        Blend::Subtract | Blend::Invert => None,
    };
    if let Some(m) = mode {
        let paint = PixmapPaint { opacity: 1.0, blend_mode: m, quality: FilterQuality::Nearest };
        dst.draw_pixmap(0, 0, src.as_ref(), &paint, Transform::identity(), clip);
        return;
    }
    // Subtract and invert don't exist in tiny-skia: pixel by pixel.
    let clip_at = |i: usize| clip.map_or(1.0, |c| c.data()[i] as f32 / 255.0);
    let s = src.data().to_vec();
    for (i, d) in dst.data_mut().chunks_mut(4).enumerate() {
        let sa = s[i * 4 + 3] as f32 / 255.0 * clip_at(i);
        if sa <= 0.0 {
            continue;
        }
        for k in 0..3 {
            let dv = d[k] as f32;
            let nv = match blend {
                Blend::Subtract => (dv - s[i * 4 + k] as f32).max(0.0),
                _ => d[3] as f32 - dv,
            };
            d[k] = (dv + (nv - dv) * sa).round().clamp(0.0, 255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Rgba;

    fn square() -> Pixmap {
        let mut pm = Pixmap::new(40, 40).unwrap();
        for y in 15..25 {
            for x in 15..25 {
                let i = (y * 40 + x) * 4;
                pm.data_mut()[i..i + 4].copy_from_slice(&[0, 0, 255, 255]);
            }
        }
        pm
    }

    #[test]
    fn blur_spreads_and_shadow_offsets() {
        let mut pm = square();
        apply(&mut pm, &[&Filter::Blur { blur_x: 6.0, blur_y: 6.0, quality: 1 }], 1.0);
        assert!(pm.pixel(13, 20).unwrap().alpha() > 0, "el desenfoque se sale del borde");
        let mut pm = square();
        let f = Filter::new("dropshadow").unwrap();
        apply(&mut pm, &[&f], 1.0);
        let p = pm.pixel(27, 27).unwrap();
        assert!(p.alpha() > 0 && p.blue() < 50, "sombra negra abajo a la derecha");
        assert_eq!(pm.pixel(20, 20).unwrap().blue(), 255, "el objeto queda encima");
    }

    #[test]
    fn glow_and_adjust_color() {
        let mut pm = square();
        let mut g = Filter::new("glow").unwrap();
        if let Filter::Glow { color, .. } = &mut g {
            *color = Rgba { r: 255, g: 0, b: 0, a: 255 };
        }
        apply(&mut pm, &[&g], 1.0);
        assert!(pm.pixel(13, 20).unwrap().red() > 0);
        let mut pm = square();
        apply(&mut pm, &[&Filter::Adjustcolor { brightness: 0.0, contrast: 0.0, saturation: -100.0, hue: 0.0 }], 1.0);
        let p = pm.pixel(20, 20).unwrap();
        assert!((p.red() as i32 - p.blue() as i32).abs() < 3, "desaturado a gris");
    }

    #[test]
    fn blend_multiply_and_subtract() {
        let mut dst = Pixmap::new(4, 4).unwrap();
        dst.fill(tiny_skia::Color::from_rgba8(200, 200, 200, 255));
        let mut src = Pixmap::new(4, 4).unwrap();
        src.fill(tiny_skia::Color::from_rgba8(100, 100, 100, 255));
        let mut d1 = dst.clone();
        composite(&mut d1, &src, Blend::Multiply, None);
        assert!(d1.pixel(0, 0).unwrap().red() < 100);
        composite(&mut dst, &src, Blend::Subtract, None);
        assert_eq!(dst.pixel(0, 0).unwrap().red(), 100);
    }
}
