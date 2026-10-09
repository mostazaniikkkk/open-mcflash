//! Basic geometry: affine matrices (in the style of SWF's), outline
//! segments and flattening / distance utilities for hit-testing.

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

pub type Pt = (f32, f32);

/// 2D affine matrix. Transforms `x' = a*x + c*y + tx`, `y' = b*x + d*y + ty`,
/// same as SWF's MATRIX and tiny-skia's `Transform::from_row`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

/// Decomposition of a matrix into translation, rotation, scale and skew,
/// used to interpolate motion tweens.
#[derive(Clone, Copy, Debug)]
pub struct Decomp {
    pub tx: f32,
    pub ty: f32,
    pub rot: f32,
    pub sx: f32,
    pub sy: f32,
    pub k: f32,
}

impl Default for Matrix {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 };

    pub fn translate(x: f32, y: f32) -> Self {
        Matrix { tx: x, ty: y, ..Self::IDENTITY }
    }

    pub fn scale(sx: f32, sy: f32) -> Self {
        Matrix { a: sx, d: sy, ..Self::IDENTITY }
    }

    pub fn rotate(r: f32) -> Self {
        let (s, c) = r.sin_cos();
        Matrix { a: c, b: s, c: -s, d: c, tx: 0.0, ty: 0.0 }
    }

    /// `self * o`: applies `o` first and then `self`.
    pub fn mul(&self, o: &Matrix) -> Matrix {
        Matrix {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            tx: self.a * o.tx + self.c * o.ty + self.tx,
            ty: self.b * o.tx + self.d * o.ty + self.ty,
        }
    }

    /// Matrix that applies `m` around point `p`.
    pub fn about(p: Pt, m: &Matrix) -> Matrix {
        Matrix::translate(p.0, p.1).mul(m).mul(&Matrix::translate(-p.0, -p.1))
    }

    pub fn apply(&self, x: f32, y: f32) -> Pt {
        (self.a * x + self.c * y + self.tx, self.b * x + self.d * y + self.ty)
    }

    pub fn invert(&self) -> Option<Matrix> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-12 {
            return None;
        }
        let ia = self.d / det;
        let ib = -self.b / det;
        let ic = -self.c / det;
        let id = self.a / det;
        Some(Matrix {
            a: ia,
            b: ib,
            c: ic,
            d: id,
            tx: -(ia * self.tx + ic * self.ty),
            ty: -(ib * self.tx + id * self.ty),
        })
    }

    /// Average scale factor (root of the determinant).
    pub fn avg_scale(&self) -> f32 {
        (self.a * self.d - self.b * self.c).abs().sqrt().max(1e-6)
    }

    pub fn to_skia(&self) -> tiny_skia::Transform {
        tiny_skia::Transform::from_row(self.a, self.b, self.c, self.d, self.tx, self.ty)
    }

    /// M = T · R(rot) · [sx k; 0 sy]
    pub fn decompose(&self) -> Decomp {
        let sx = (self.a * self.a + self.b * self.b).sqrt();
        let rot = self.b.atan2(self.a);
        let (s, c) = rot.sin_cos();
        let k = c * self.c + s * self.d;
        let sy = -s * self.c + c * self.d;
        Decomp { tx: self.tx, ty: self.ty, rot, sx, sy, k }
    }

    pub fn compose(d: &Decomp) -> Matrix {
        let (s, c) = d.rot.sin_cos();
        Matrix {
            a: c * d.sx,
            b: s * d.sx,
            c: c * d.k - s * d.sy,
            d: s * d.k + c * d.sy,
            tx: d.tx,
            ty: d.ty,
        }
    }

    /// Interpolation with Flash's options: `rot` 0 = auto (shortest
    /// path), 1 = none, 2 = CW, 3 = CCW (+ `spins` turns);
    /// without `scale` the size doesn't change during the tween.
    pub fn lerp_opts(&self, o: &Matrix, t: f32, rot: u8, spins: u32, scale: bool) -> Matrix {
        let p = self.decompose();
        let q = o.decompose();
        let tau = 2.0 * PI;
        let raw = q.rot - p.rot;
        let dr = match rot {
            1 => 0.0,
            2 => raw.rem_euclid(tau) + tau * spins as f32,
            3 => -((-raw).rem_euclid(tau)) - tau * spins as f32,
            _ => {
                let mut d = raw;
                while d > PI {
                    d -= tau;
                }
                while d < -PI {
                    d += tau;
                }
                d
            }
        };
        let (sx, sy, k) = if scale { (lerp(p.sx, q.sx, t), lerp(p.sy, q.sy, t), lerp(p.k, q.k, t)) } else { (p.sx, p.sy, p.k) };
        Matrix::compose(&Decomp { tx: lerp(p.tx, q.tx, t), ty: lerp(p.ty, q.ty, t), rot: p.rot + dr * t, sx, sy, k })
    }

    /// "Flash-like" interpolation: by decomposed components and with
    /// rotation along the shortest path.
    pub fn lerp(&self, o: &Matrix, t: f32) -> Matrix {
        let p = self.decompose();
        let q = o.decompose();
        let mut dr = q.rot - p.rot;
        while dr > PI {
            dr -= 2.0 * PI;
        }
        while dr < -PI {
            dr += 2.0 * PI;
        }
        Matrix::compose(&Decomp {
            tx: lerp(p.tx, q.tx, t),
            ty: lerp(p.ty, q.ty, t),
            rot: p.rot + dr * t,
            sx: lerp(p.sx, q.sx, t),
            sy: lerp(p.sy, q.sy, t),
            k: lerp(p.k, q.k, t),
        })
    }
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Outline segment. Only lines and quadratics: exactly what
/// SWF can encode, so the editor and the exporter see the same thing.
#[derive(Clone, Copy, Debug)]
pub enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

pub fn cubic_point(p0: Pt, c1: Pt, c2: Pt, p3: Pt, t: f32) -> Pt {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (a * p0.0 + b * c1.0 + c * c2.0 + d * p3.0, a * p0.1 + b * c1.1 + c * c2.1 + d * p3.1)
}

/// Splits a cubic at t (de Casteljau). Returns (c1a, c2a, p, c1b, c2b).
pub fn split_cubic(p0: Pt, c1: Pt, c2: Pt, p3: Pt, t: f32) -> (Pt, Pt, Pt, Pt, Pt) {
    let l = |a: Pt, b: Pt| (lerp(a.0, b.0, t), lerp(a.1, b.1, t));
    let (q0, q1, q2) = (l(p0, c1), l(c1, c2), l(c2, p3));
    let (r0, r1) = (l(q0, q1), l(q1, q2));
    let s = l(r0, r1);
    (q0, r0, s, r1, q2)
}

/// Bounding box (x0, y0, x1, y1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl BBox {
    pub const EMPTY: BBox = BBox { x0: f32::INFINITY, y0: f32::INFINITY, x1: f32::NEG_INFINITY, y1: f32::NEG_INFINITY };

    pub fn from_pts(a: Pt, b: Pt) -> BBox {
        BBox { x0: a.0.min(b.0), y0: a.1.min(b.1), x1: a.0.max(b.0), y1: a.1.max(b.1) }
    }
    pub fn add(&mut self, p: Pt) {
        self.x0 = self.x0.min(p.0);
        self.y0 = self.y0.min(p.1);
        self.x1 = self.x1.max(p.0);
        self.y1 = self.y1.max(p.1);
    }
    pub fn union(&self, o: &BBox) -> BBox {
        BBox { x0: self.x0.min(o.x0), y0: self.y0.min(o.y0), x1: self.x1.max(o.x1), y1: self.y1.max(o.y1) }
    }
    pub fn is_empty(&self) -> bool {
        !(self.x0 <= self.x1 && self.y0 <= self.y1)
    }
    pub fn w(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn h(&self) -> f32 {
        self.y1 - self.y0
    }
    pub fn center(&self) -> Pt {
        ((self.x0 + self.x1) * 0.5, (self.y0 + self.y1) * 0.5)
    }
    pub fn intersects(&self, o: &BBox) -> bool {
        self.x0 <= o.x1 && o.x0 <= self.x1 && self.y0 <= o.y1 && o.y0 <= self.y1
    }
    pub fn transformed(&self, m: &Matrix) -> BBox {
        let mut b = BBox::EMPTY;
        for p in [(self.x0, self.y0), (self.x1, self.y0), (self.x1, self.y1), (self.x0, self.y1)] {
            b.add(m.apply(p.0, p.1));
        }
        b
    }
}

/// Flattens an outline into polylines `(points, closed)`.
pub fn flatten(segs: &[Seg]) -> Vec<(Vec<Pt>, bool)> {
    let mut out: Vec<(Vec<Pt>, bool)> = Vec::new();
    let mut cur: Vec<Pt> = Vec::new();
    let mut last = (0.0, 0.0);
    for s in segs {
        match *s {
            Seg::Move(x, y) => {
                if cur.len() > 1 {
                    out.push((std::mem::take(&mut cur), false));
                }
                cur.clear();
                cur.push((x, y));
                last = (x, y);
            }
            Seg::Line(x, y) => {
                cur.push((x, y));
                last = (x, y);
            }
            Seg::Quad(cx, cy, x, y) => {
                // Subdivision based on length: precise on large curves, cheap on small ones.
                let len = (cx - last.0).hypot(cy - last.1) + (x - cx).hypot(y - cy);
                let n = ((len / 2.0).ceil() as usize).clamp(4, 64);
                for i in 1..=n {
                    let t = i as f32 / n as f32;
                    let u = 1.0 - t;
                    cur.push((
                        u * u * last.0 + 2.0 * u * t * cx + t * t * x,
                        u * u * last.1 + 2.0 * u * t * cy + t * t * y,
                    ));
                }
                last = (x, y);
            }
            Seg::Cubic(c1x, c1y, c2x, c2y, x, y) => {
                let len = (c1x - last.0).hypot(c1y - last.1) + (c2x - c1x).hypot(c2y - c1y) + (x - c2x).hypot(y - c2y);
                let n = ((len / 2.0).ceil() as usize).clamp(6, 96);
                for i in 1..=n {
                    cur.push(cubic_point(last, (c1x, c1y), (c2x, c2y), (x, y), i as f32 / n as f32));
                }
                last = (x, y);
            }
            Seg::Close => {
                if !cur.is_empty() {
                    out.push((std::mem::take(&mut cur), true));
                }
            }
        }
    }
    if cur.len() > 1 {
        out.push((cur, false));
    }
    out
}

pub fn dist_to_segment(p: Pt, a: Pt, b: Pt) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let (qx, qy) = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - qx).powi(2) + (p.1 - qy).powi(2)).sqrt()
}

pub fn dist_to_polyline(p: Pt, pts: &[Pt], closed: bool) -> f32 {
    let mut d = f32::INFINITY;
    if pts.len() == 1 {
        return ((p.0 - pts[0].0).powi(2) + (p.1 - pts[0].1).powi(2)).sqrt();
    }
    for w in pts.windows(2) {
        d = d.min(dist_to_segment(p, w[0], w[1]));
    }
    if closed && pts.len() > 2 {
        d = d.min(dist_to_segment(p, pts[pts.len() - 1], pts[0]));
    }
    d
}

/// Even-odd rule.
pub fn point_in_polygon(p: Pt, pts: &[Pt]) -> bool {
    let mut inside = false;
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if (yi > p.1) != (yj > p.1) && p.0 < (xj - xi) * (p.1 - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Ramer–Douglas–Peucker simplification for freehand strokes.
pub fn simplify(pts: &[Pt], eps: f32) -> Vec<Pt> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0usize, pts.len() - 1)];
    while let Some((s, e)) = stack.pop() {
        let mut best = 0.0;
        let mut idx = 0;
        for i in s + 1..e {
            let d = dist_to_segment(pts[i], pts[s], pts[e]);
            if d > best {
                best = d;
                idx = i;
            }
        }
        if best > eps {
            keep[idx] = true;
            stack.push((s, idx));
            stack.push((idx, e));
        }
    }
    pts.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompose_roundtrip() {
        let m = Matrix::translate(10.0, 20.0)
            .mul(&Matrix::rotate(0.7))
            .mul(&Matrix { c: 0.3, ..Matrix::IDENTITY })
            .mul(&Matrix::scale(2.0, -1.5));
        let r = Matrix::compose(&m.decompose());
        for (x, y) in [(m.a, r.a), (m.b, r.b), (m.c, r.c), (m.d, r.d), (m.tx, r.tx), (m.ty, r.ty)] {
            assert!((x - y).abs() < 1e-4, "{m:?} vs {r:?}");
        }
    }

    #[test]
    fn invert_works() {
        let m = Matrix::translate(5.0, -3.0).mul(&Matrix::rotate(1.1)).mul(&Matrix::scale(3.0, 0.5));
        let p = m.invert().unwrap().apply(m.apply(7.0, 9.0).0, m.apply(7.0, 9.0).1);
        assert!((p.0 - 7.0).abs() < 1e-3 && (p.1 - 9.0).abs() < 1e-3);
    }
}
