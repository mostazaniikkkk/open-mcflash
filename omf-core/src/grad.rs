//! Flash 8 gradients: linear and radial (with focal point), up to 15
//! colors, and overflow modes (extend, reflect, repeat).
//!
//! The gradient lives in a unit square: the linear one goes from x = −1 to x = 1 and
//! the radial one is the radius-1 circle centered at the origin. Its matrix `m`
//! maps that square to the shape's local coordinates, so the
//! gradient moves, rotates and scales with it (like the "gradient box"
//! edited by the Fill Transform tool).

use crate::geom::*;
use crate::model::Rgba;
use crate::symbols::CxForm;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GradKind {
    #[default]
    Linear,
    Radial,
}

/// Overflow (Flash 8: Extend, Reflect, Repeat).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Spread {
    #[default]
    Pad,
    Reflect,
    Repeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stop {
    /// Position from 0 to 1.
    pub t: f32,
    pub c: Rgba,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gradient {
    #[serde(default)]
    pub kind: GradKind,
    pub stops: Vec<Stop>,
    /// Gradient unit square → shape's local coordinates.
    #[serde(default = "ident")]
    pub m: Matrix,
    #[serde(default)]
    pub spread: Spread,
    /// Focal point of the radial gradient, from −1 to 1 along the gradient's x axis.
    #[serde(default)]
    pub focal: f32,
    /// "Linear RGB" interpolation (brighter in the middle, as in Flash 8).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub linear_rgb: bool,
}

fn ident() -> Matrix {
    Matrix::IDENTITY
}

/// Maximum number of colors in a gradient in SWF 8.
pub const MAX_STOPS: usize = 15;

impl Gradient {
    /// Flash's default gradient: white to black.
    pub fn default_of(kind: GradKind) -> Gradient {
        Gradient {
            kind,
            stops: vec![Stop { t: 0.0, c: Rgba::WHITE }, Stop { t: 1.0, c: Rgba::BLACK }],
            m: Matrix::IDENTITY,
            spread: Spread::Pad,
            focal: 0.0,
            linear_rgb: false,
        }
    }

    /// Sorted stops, between 2 and 15, with positions in [0, 1].
    pub fn normalized(mut self) -> Gradient {
        for s in &mut self.stops {
            s.t = s.t.clamp(0.0, 1.0);
        }
        self.stops.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
        self.stops.truncate(MAX_STOPS);
        if self.stops.is_empty() {
            self.stops.push(Stop { t: 0.0, c: Rgba::WHITE });
        }
        if self.stops.len() == 1 {
            let s = self.stops[0];
            self.stops.push(Stop { t: 1.0, c: s.c });
        }
        self.focal = self.focal.clamp(-0.98, 0.98);
        self
    }

    /// Color at position `t` (0…1).
    pub fn color_at(&self, t: f32) -> Rgba {
        let s = &self.stops;
        if s.is_empty() {
            return Rgba::BLACK;
        }
        if t <= s[0].t {
            return s[0].c;
        }
        for w in s.windows(2) {
            if t <= w[1].t {
                let k = if w[1].t - w[0].t > 1e-6 { (t - w[0].t) / (w[1].t - w[0].t) } else { 1.0 };
                return w[0].c.lerp(&w[1].c, k);
            }
        }
        s[s.len() - 1].c
    }

    /// Representative color (for the swatch and the rest of the program, which
    /// treats the fill as a color).
    pub fn base_color(&self) -> Rgba {
        self.stops.first().map_or(Rgba::BLACK, |s| s.c)
    }

    /// Same gradient with the matrix premultiplied by `t` (change of space).
    pub fn transformed(&self, t: &Matrix) -> Gradient {
        Gradient { m: t.mul(&self.m), ..self.clone() }
    }

    pub fn approx_eq(&self, o: &Gradient) -> bool {
        let m = |a: &Matrix, b: &Matrix| {
            let s = a.avg_scale().max(b.avg_scale()).max(1.0);
            [(a.a, b.a), (a.b, b.b), (a.c, b.c), (a.d, b.d)].iter().all(|(x, y)| (x - y).abs() * s < 0.05)
                && (a.tx - b.tx).abs() < 0.05
                && (a.ty - b.ty).abs() < 0.05
        };
        self.kind == o.kind
            && self.spread == o.spread
            && (self.focal - o.focal).abs() < 1e-3
            && self.stops.len() == o.stops.len()
            && self.stops.iter().zip(&o.stops).all(|(a, b)| a.c == b.c && (a.t - b.t).abs() < 1e-3)
            && m(&self.m, &o.m)
    }

    pub fn lerp(&self, o: &Gradient, t: f32) -> Gradient {
        if self.kind != o.kind || self.stops.len() != o.stops.len() {
            return if t < 0.5 { self.clone() } else { o.clone() };
        }
        Gradient {
            kind: self.kind,
            stops: self.stops.iter().zip(&o.stops).map(|(a, b)| Stop { t: lerp(a.t, b.t, t), c: a.c.lerp(&b.c, t) }).collect(),
            m: self.m.lerp(&o.m, t),
            spread: self.spread,
            focal: lerp(self.focal, o.focal, t),
            linear_rgb: self.linear_rgb,
        }
    }

    pub fn with_cx(&self, cx: &CxForm) -> Gradient {
        let mut g = self.clone();
        for s in &mut g.stops {
            s.c = cx.apply(s.c);
        }
        g
    }

    /// Matrix that fits the gradient to a rectangle (as Flash does when
    /// filling): the linear one spans the width; the radial one, a circle covering the box.
    pub fn fit(kind: GradKind, b: &BBox) -> Matrix {
        let c = b.center();
        let (hw, hh) = ((b.w() / 2.0).max(0.5), (b.h() / 2.0).max(0.5));
        match kind {
            GradKind::Linear => Matrix::translate(c.0, c.1).mul(&Matrix::scale(hw, hh)),
            GradKind::Radial => {
                let r = hw.max(hh);
                Matrix::translate(c.0, c.1).mul(&Matrix::scale(r, r))
            }
        }
    }

    /// Stops for drawing: with "Linear RGB", intermediate stops computed
    /// in linear space are added (tiny-skia interpolates in sRGB).
    fn render_stops(&self) -> Vec<Stop> {
        if !self.linear_rgb {
            return self.stops.clone();
        }
        let lin = |v: u8| {
            let c = v as f32 / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        let srgb = |c: f32| {
            let v = if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
            (v * 255.0).round().clamp(0.0, 255.0) as u8
        };
        let mut out = Vec::new();
        for w in self.stops.windows(2) {
            let (a, b) = (w[0], w[1]);
            for i in 0..8 {
                let k = i as f32 / 8.0;
                let c = |x: u8, y: u8| srgb(lerp(lin(x), lin(y), k));
                out.push(Stop { t: lerp(a.t, b.t, k), c: Rgba { r: c(a.c.r, b.c.r), g: c(a.c.g, b.c.g), b: c(a.c.b, b.c.b), a: lerp(a.c.a as f32, b.c.a as f32, k).round() as u8 } });
            }
        }
        out.extend(self.stops.last().copied());
        out
    }

    pub fn shader(&self, alpha: f32) -> Option<tiny_skia::Shader<'static>> {
        let stops: Vec<tiny_skia::GradientStop> = self
            .render_stops()
            .iter()
            .map(|s| {
                let a = (s.c.a as f32 / 255.0 * alpha).clamp(0.0, 1.0);
                tiny_skia::GradientStop::new(s.t, tiny_skia::Color::from_rgba(s.c.r as f32 / 255.0, s.c.g as f32 / 255.0, s.c.b as f32 / 255.0, a).unwrap())
            })
            .collect();
        let mode = match self.spread {
            Spread::Pad => tiny_skia::SpreadMode::Pad,
            Spread::Reflect => tiny_skia::SpreadMode::Reflect,
            Spread::Repeat => tiny_skia::SpreadMode::Repeat,
        };
        let t = self.m.to_skia();
        let p = tiny_skia::Point::from_xy;
        match self.kind {
            GradKind::Linear => tiny_skia::LinearGradient::new(p(-1.0, 0.0), p(1.0, 0.0), stops, mode, t),
            GradKind::Radial => {
                let f = self.focal.clamp(-0.98, 0.98);
                tiny_skia::RadialGradient::new(p(f, 0.0), p(0.0, 0.0), 1.0, stops, mode, t)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_at_and_normalize() {
        let g = Gradient {
            stops: vec![Stop { t: 1.0, c: Rgba::BLACK }, Stop { t: 0.0, c: Rgba::WHITE }],
            ..Gradient::default_of(GradKind::Linear)
        }
        .normalized();
        assert_eq!(g.stops[0].c, Rgba::WHITE);
        assert_eq!(g.color_at(0.5).r, 128);
    }

    #[test]
    fn linear_rgb_is_brighter_in_the_middle() {
        let g = Gradient { stops: vec![Stop { t: 0.0, c: Rgba { r: 255, g: 0, b: 0, a: 255 } }, Stop { t: 1.0, c: Rgba { r: 0, g: 0, b: 255, a: 255 } }], ..Gradient::default_of(GradKind::Linear) };
        let l = Gradient { linear_rgb: true, ..g.clone() };
        let mid = |g: &Gradient| g.render_stops().iter().find(|s| (s.t - 0.5).abs() < 1e-3).map(|s| s.c);
        assert!(mid(&g).is_none());
        let m = mid(&l).unwrap();
        assert!(m.r > 180 && m.b > 180, "en lineal el medio no se oscurece: {m:?}");
    }

    #[test]
    fn fit_linear_covers_the_box() {
        let b = BBox { x0: 10.0, y0: 20.0, x1: 110.0, y1: 60.0 };
        let m = Gradient::fit(GradKind::Linear, &b);
        assert_eq!(m.apply(-1.0, 0.0), (10.0, 40.0));
        assert_eq!(m.apply(1.0, 0.0), (110.0, 40.0));
    }
}
