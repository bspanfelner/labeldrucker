//! A small ink canvas in print resolution, with the shape primitives the
//! label layout needs. Coordinates are in dots; pixel `(x, y)` covers
//! `[x, x+1) × [y, y+1)` and is sampled at its centre.

use crate::Page;
use crate::page::HEAD_DOTS;

pub type P = (f32, f32);

/// An axis-aligned rectangle in dots.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self { x0, y0, x1, y1 }
    }
    pub fn w(&self) -> f32 {
        (self.x1 - self.x0).max(0.0)
    }
    pub fn h(&self) -> f32 {
        (self.y1 - self.y0).max(0.0)
    }
    pub fn inset(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.x0 + dx, self.y0 + dy, self.x1 - dx, self.y1 - dy)
    }
}

/// How a centre-line path is stroked.
#[derive(Debug, Clone, Copy)]
pub enum Pattern {
    Solid,
    Dash { on: f32, off: f32 },
    Dots { spacing: f32 },
}

/// Ink coverage 0..=1 per dot.
#[derive(Debug, Clone)]
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    ink: Vec<f32>,
}

impl Canvas {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, ink: vec![0.0; width * height] }
    }

    /// A canvas covering the whole print head.
    pub fn for_page(width: usize) -> Self {
        Self::new(width, HEAD_DOTS)
    }

    pub fn get(&self, x: usize, y: usize) -> f32 {
        self.ink[y * self.width + x]
    }

    /// Add coverage at an integer dot, clamped to 1; outside is ignored.
    pub fn add(&mut self, x: i32, y: i32, c: f32) {
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            let i = y as usize * self.width + x as usize;
            self.ink[i] = (self.ink[i] + c).min(1.0);
        }
    }

    fn put(&mut self, x: usize, y: usize, v: f32) {
        self.ink[y * self.width + x] = v;
    }

    /// Visit the dots of `r` (clamped to the canvas).
    fn span(&self, r: Rect) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
        let cx = |v: f32| (v.max(0.0) as usize).min(self.width);
        let cy = |v: f32| (v.max(0.0) as usize).min(self.height);
        (cx(r.x0.floor())..cx(r.x1.ceil()), cy(r.y0.floor())..cy(r.y1.ceil()))
    }

    /// Set every dot whose centre lies in `r` to `v` (1 = ink, 0 = clear).
    pub fn fill_rect(&mut self, r: Rect, v: f32) {
        let (xs, ys) = self.span(r);
        for y in ys {
            for x in xs.clone() {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                if px >= r.x0 && px < r.x1 && py >= r.y0 && py < r.y1 {
                    self.put(x, y, v);
                }
            }
        }
    }

    /// Set every dot whose centre lies inside the closed polygon to `v`
    /// (even-odd rule).
    pub fn fill(&mut self, poly: &[P], v: f32) {
        if poly.len() < 3 {
            return;
        }
        let ymin = poly.iter().map(|p| p.1).fold(f32::MAX, f32::min);
        let ymax = poly.iter().map(|p| p.1).fold(f32::MIN, f32::max);
        let (_, ys) = self.span(Rect::new(0.0, ymin, 0.0, ymax));
        let mut xs = Vec::new();
        for y in ys {
            let yc = y as f32 + 0.5;
            xs.clear();
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                if (a.1 <= yc) != (b.1 <= yc) {
                    xs.push(a.0 + (yc - a.1) / (b.1 - a.1) * (b.0 - a.0));
                }
            }
            xs.sort_by(f32::total_cmp);
            for pair in xs.chunks_exact(2) {
                let start = (pair[0] - 0.5).ceil().max(0.0) as usize;
                let end = ((pair[1] - 0.5).floor() + 1.0).max(0.0) as usize;
                for x in start..end.min(self.width) {
                    self.put(x, y, v);
                }
            }
        }
    }

    /// A ring between two nested closed outlines: fill `outer`, clear `inner`.
    pub fn ring(&mut self, outer: &[P], inner: &[P]) {
        self.fill(outer, 1.0);
        self.fill(inner, 0.0);
    }

    /// Ink every dot within `w/2` of the segment `a`–`b` (round caps).
    pub fn segment(&mut self, a: P, b: P, w: f32) {
        let r = w / 2.0;
        let bb = Rect::new(a.0.min(b.0) - r, a.1.min(b.1) - r, a.0.max(b.0) + r, a.1.max(b.1) + r);
        let (xs, ys) = self.span(bb);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len2 = dx * dx + dy * dy;
        for y in ys {
            for x in xs.clone() {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let t = if len2 > 0.0 { (((px - a.0) * dx + (py - a.1) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
                let (qx, qy) = (a.0 + t * dx - px, a.1 + t * dy - py);
                if qx * qx + qy * qy <= r * r {
                    self.put(x, y, 1.0);
                }
            }
        }
    }

    /// Stroke a centre-line path of width `w`.
    pub fn stroke(&mut self, path: &[P], closed: bool, w: f32, pattern: Pattern) {
        let n = path.len();
        if n < 2 {
            return;
        }
        let segs: Vec<(P, P)> =
            (0..if closed { n } else { n - 1 }).map(|i| (path[i], path[(i + 1) % n])).collect();
        match pattern {
            Pattern::Solid => segs.iter().for_each(|&(a, b)| self.segment(a, b, w)),
            Pattern::Dash { on, off } => {
                let period = on + off;
                let mut pos = 0.0f32; // distance along the path
                for &(a, b) in &segs {
                    let len = dist(a, b);
                    let mut s = 0.0;
                    while s < len {
                        let phase = (pos + s) % period;
                        let (inked, left) = if phase < on { (true, on - phase) } else { (false, period - phase) };
                        let e = (s + left).min(len);
                        if inked {
                            self.segment(lerp(a, b, s / len), lerp(a, b, e / len), w);
                        }
                        s = e + 1e-4;
                    }
                    pos += len;
                }
            }
            Pattern::Dots { spacing } => {
                let total: f32 = segs.iter().map(|&(a, b)| dist(a, b)).sum();
                // An integer number of dots so a closed path closes evenly.
                let count = (total / spacing).round().max(1.0);
                let step = total / count;
                let dots = count as usize + usize::from(!closed);
                let (mut seg, mut seg_start) = (0, 0.0);
                for k in 0..dots {
                    let at = k as f32 * step;
                    while seg + 1 < segs.len() && at > seg_start + dist(segs[seg].0, segs[seg].1) {
                        seg_start += dist(segs[seg].0, segs[seg].1);
                        seg += 1;
                    }
                    let (a, b) = segs[seg];
                    let len = dist(a, b);
                    let p = lerp(a, b, if len > 0.0 { ((at - seg_start) / len).min(1.0) } else { 0.0 });
                    self.segment(p, p, w);
                }
            }
        }
    }

    /// Paint an 8-bit grayscale image scaled to fit inside `r`, centred
    /// (`align` −1 left, 0 centre, 1 right); darker than `threshold` inks.
    pub fn image(&mut self, gray: &[u8], gw: usize, gh: usize, r: Rect, align: i8, threshold: u8) {
        if gw == 0 || gh == 0 || r.w() < 1.0 || r.h() < 1.0 {
            return;
        }
        let s = (r.w() / gw as f32).min(r.h() / gh as f32);
        let (w, h) = (gw as f32 * s, gh as f32 * s);
        let x0 = match align {
            -1 => r.x0,
            1 => r.x1 - w,
            _ => r.x0 + (r.w() - w) / 2.0,
        };
        let y0 = r.y0 + (r.h() - h) / 2.0;
        let (xs, ys) = self.span(Rect::new(x0, y0, x0 + w, y0 + h));
        for y in ys {
            for x in xs.clone() {
                // Bilinear sample at the dot centre.
                let u = ((x as f32 + 0.5 - x0) / s - 0.5).clamp(0.0, gw as f32 - 1.0);
                let v = ((y as f32 + 0.5 - y0) / s - 0.5).clamp(0.0, gh as f32 - 1.0);
                let (ui, vi) = (u as usize, v as usize);
                let (u1, v1) = ((ui + 1).min(gw - 1), (vi + 1).min(gh - 1));
                let (fu, fv) = (u - ui as f32, v - vi as f32);
                let g = |a: usize, b: usize| f32::from(gray[b * gw + a]);
                let top = g(ui, vi) * (1.0 - fu) + g(u1, vi) * fu;
                let bot = g(ui, v1) * (1.0 - fu) + g(u1, v1) * fu;
                if top * (1.0 - fv) + bot * fv < f32::from(threshold) {
                    self.put(x, y, 1.0);
                }
            }
        }
    }

    pub fn invert(&mut self) {
        for v in &mut self.ink {
            *v = if *v >= 0.5 { 0.0 } else { 1.0 };
        }
    }

    /// Threshold at 50 % into a printable page (rows beyond the head are cut).
    pub fn to_page(&self) -> Page {
        let mut page = Page::blank_dots(self.width);
        for y in 0..self.height.min(HEAD_DOTS) {
            for x in 0..self.width {
                if self.get(x, y) >= 0.5 {
                    page.set(x, y);
                }
            }
        }
        page
    }
}

fn dist(a: P, b: P) -> f32 {
    ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
}

fn lerp(a: P, b: P, t: f32) -> P {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

// ------------------------------------------------------------------ shapes

fn arc(out: &mut Vec<P>, c: P, r: f32, from_deg: f32, to_deg: f32) {
    let steps = ((r * (to_deg - from_deg).abs() / 90.0).ceil() as usize).clamp(2, 64);
    for i in 0..=steps {
        let a = (from_deg + (to_deg - from_deg) * i as f32 / steps as f32).to_radians();
        out.push((c.0 + r * a.cos(), c.1 + r * a.sin()));
    }
}

/// Rectangle with corner radius `r` (clamped to fit), clockwise.
pub fn rounded_rect(b: Rect, r: f32) -> Vec<P> {
    let r = r.min(b.w() / 2.0).min(b.h() / 2.0).max(0.0);
    if r < 0.5 {
        return vec![(b.x0, b.y0), (b.x1, b.y0), (b.x1, b.y1), (b.x0, b.y1)];
    }
    let mut v = Vec::new();
    arc(&mut v, (b.x1 - r, b.y0 + r), r, -90.0, 0.0);
    arc(&mut v, (b.x1 - r, b.y1 - r), r, 0.0, 90.0);
    arc(&mut v, (b.x0 + r, b.y1 - r), r, 90.0, 180.0);
    arc(&mut v, (b.x0 + r, b.y0 + r), r, 180.0, 270.0);
    v
}

/// Rectangle with its corners cut off by `c`.
pub fn chamfer_rect(b: Rect, c: f32) -> Vec<P> {
    let c = c.min(b.w() / 2.0).min(b.h() / 2.0).max(0.0);
    vec![
        (b.x0 + c, b.y0),
        (b.x1 - c, b.y0),
        (b.x1, b.y0 + c),
        (b.x1, b.y1 - c),
        (b.x1 - c, b.y1),
        (b.x0 + c, b.y1),
        (b.x0, b.y1 - c),
        (b.x0, b.y0 + c),
    ]
}

/// Rectangle with semicircular notches of radius `r` in the left and right
/// edges, like a ticket.
pub fn ticket(b: Rect, r: f32) -> Vec<P> {
    let r = r.min(b.h() / 2.0 - 1.0).max(0.0);
    let ym = (b.y0 + b.y1) / 2.0;
    let mut v = vec![(b.x0, b.y0), (b.x1, b.y0)];
    arc(&mut v, (b.x1, ym), r, 270.0, 90.0); // bulges inward (counter-clockwise)
    v.push((b.x1, b.y1));
    v.push((b.x0, b.y1));
    arc(&mut v, (b.x0, ym), r, 90.0, -90.0);
    v
}

/// Rectangle whose top and bottom edges are sine waves.
pub fn wave_rect(b: Rect, amp: f32, period: f32) -> Vec<P> {
    let waves = (b.w() / period).round().max(1.0);
    let n = (waves * 16.0) as usize;
    let y = |i: usize, base: f32, sign: f32| {
        let t = i as f32 / n as f32;
        base + sign * amp * (1.0 - (t * waves * std::f32::consts::TAU).cos()) / 2.0
    };
    let x = |i: usize| b.x0 + b.w() * i as f32 / n as f32;
    let mut v: Vec<P> = (0..=n).map(|i| (x(i), y(i, b.y0, 1.0))).collect();
    v.extend((0..=n).rev().map(|i| (x(i), y(i, b.y1, -1.0))));
    v
}
