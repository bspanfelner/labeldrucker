//! Hand-drawn vector pictograms (screws, nuts, fixings) that no icon font
//! has. Each lives on a private-use code point from U+E000, so a label stores
//! it like any other icon `char`.
//!
//! Screws are drawn in side view, head on the left; drives in top view.
//! Shapes are designed in a unit square and scaled to the icon box.

use crate::draw::{Canvas, P, Rect, ellipse, regular, rounded_rect, star};

pub const FIRST: u32 = 0xE000;

/// Maps unit-square coordinates into the icon box.
struct U {
    r: Rect,
}

impl U {
    fn p(&self, u: f32, v: f32) -> P {
        (self.r.x0 + u * self.r.w(), self.r.y0 + v * self.r.h())
    }
    fn s(&self, d: f32) -> f32 {
        d * self.r.w()
    }
    fn rect(&self, u0: f32, v0: f32, u1: f32, v1: f32) -> Rect {
        let (a, b) = (self.p(u0, v0), self.p(u1, v1));
        Rect::new(a.0, a.1, b.0, b.1)
    }
    fn poly(&self, pts: &[(f32, f32)]) -> Vec<P> {
        pts.iter().map(|&(u, v)| self.p(u, v)).collect()
    }
    fn fill(&self, c: &mut Canvas, pts: &[(f32, f32)], v: f32) {
        c.fill(&self.poly(pts), v);
    }
    fn fill_rect(&self, c: &mut Canvas, u0: f32, v0: f32, u1: f32, v1: f32, v: f32) {
        c.fill_rect(self.rect(u0, v0, u1, v1), v);
    }
    fn ellipse(&self, c: &mut Canvas, u: f32, v: f32, ru: f32, rv: f32, val: f32) {
        c.fill(&ellipse(self.p(u, v), self.s(ru), self.s(rv)), val);
    }
    fn line(&self, c: &mut Canvas, a: (f32, f32), b: (f32, f32), w: f32, v: f32) {
        c.line(self.p(a.0, a.1), self.p(b.0, b.1), self.s(w), v);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Head {
    /// ISO 10642: 90° countersunk, hex socket.
    Countersunk,
    /// Countersunk with a cross recess.
    CountersunkCross,
    /// Countersunk with a lens-shaped top.
    RaisedCountersunk,
    /// ISO 7380: low lens dome, hex socket.
    Button,
    /// DIN 7985: domed pan head, cross recess.
    PanCross,
    Round,
    Truss,
    /// ISO 4762: tall cylinder, hex socket.
    SocketCap,
    /// Slotted cheese head.
    Cheese,
    /// ISO 4017: hex head.
    Hex,
    Carriage,
    Wing,
    Knurled,
}

#[derive(Clone, Copy, PartialEq)]
enum Tip {
    Point,
    Blunt,
    Drill,
}

use std::f32::consts::{FRAC_PI_2, PI};

/// Points on an elliptical arc from angle `a0` to `a1` (radians; 0 = +u,
/// π/2 = +v, i.e. down).
fn arc(cu: f32, cv: f32, ru: f32, rv: f32, a0: f32, a1: f32) -> Vec<(f32, f32)> {
    (0..=24).map(|i| a0 + (a1 - a0) * i as f32 / 24.0).map(|a| (cu + ru * a.cos(), cv + rv * a.sin())).collect()
}

/// Shank radius of the screws (the head sizes are in proportion).
const SHANK: f32 = 0.14;

/// A hex socket seen from the side: a white pocket in the head face at `u0`
/// with a drill-point bottom, `depth` deep.
fn socket(c: &mut Canvas, u: &U, u0: f32, depth: f32, half: f32) {
    let u1 = u0 + depth;
    u.fill(c, &[(u0 - 0.02, 0.5 - half), (u1, 0.5 - half), (u1 + half * 0.45, 0.5), (u1, 0.5 + half), (u0 - 0.02, 0.5 + half)], 0.0);
}

/// A cross recess seen from the side: a V notch.
fn cross(c: &mut Canvas, u: &U, u0: f32, depth: f32, half: f32) {
    u.fill(c, &[(u0 - 0.02, 0.5 - half), (u0 + depth, 0.5), (u0 - 0.02, 0.5 + half)], 0.0);
}

/// A slot seen from the side: a narrow rectangular cut.
fn slot(c: &mut Canvas, u: &U, u0: f32, depth: f32) {
    u.fill_rect(c, u0 - 0.02, 0.465, u0 + depth, 0.535, 0.0);
}

/// Draw a head (side view, the free face on the left); returns where the
/// shank starts (u).
fn head(c: &mut Canvas, u: &U, h: Head) -> f32 {
    let (top, bot) = (0.5 - SHANK, 0.5 + SHANK);
    match h {
        Head::Countersunk | Head::CountersunkCross | Head::RaisedCountersunk => {
            // A short rim, then the 90° cone down to the shank.
            let (u0, rim, r) = (0.05, 0.035, 0.42);
            let cone = u0 + rim + (r - SHANK);
            u.fill(c, &[(u0, 0.5 - r), (u0 + rim, 0.5 - r), (cone, top), (cone, bot), (u0 + rim, 0.5 + r), (u0, 0.5 + r)], 1.0);
            let face = if h == Head::RaisedCountersunk {
                // A shallow lens on the free face.
                let bulge = 0.07;
                let rr = (r * r + bulge * bulge) / (2.0 * bulge);
                let a = (r / rr).asin();
                u.fill(c, &arc(u0 - bulge + rr, 0.5, rr, rr, PI - a, PI + a), 1.0);
                u0 - bulge
            } else {
                u0
            };
            if h == Head::CountersunkCross {
                cross(c, u, face, 0.17, 0.17);
            } else {
                socket(c, u, face, 0.13, 0.1);
            }
            cone
        }
        Head::Button => {
            // ISO 7380: a thin rim and a flat lens (circle segment).
            let (rim0, rim1, r, bulge) = (0.22, 0.26, 0.42, 0.17);
            u.fill_rect(c, rim0, 0.5 - r, rim1, 0.5 + r, 1.0);
            let rr = (r * r + bulge * bulge) / (2.0 * bulge);
            let a = (r / rr).asin();
            u.fill(c, &arc(rim0 - bulge + rr, 0.5, rr, rr, PI - a, PI + a), 1.0);
            socket(c, u, rim0 - bulge, 0.12, 0.1);
            rim1
        }
        Head::PanCross => {
            // DIN 7985: steep sides, domed top.
            let (u0, u1, r) = (0.05, 0.26, 0.36);
            let mut pts = vec![(u1, 0.5 - r), (u1, 0.5 + r)];
            pts.extend(arc(u0 + 0.1, 0.5, 0.1, r, FRAC_PI_2, 3.0 * FRAC_PI_2));
            u.fill(c, &pts, 1.0);
            cross(c, u, u0, 0.15, 0.15);
            u1
        }
        Head::Round => {
            let r = 0.29;
            let mut pts = vec![(0.25, 0.5 - r), (0.25, 0.5 + r)];
            pts.extend(arc(0.25, 0.5, 0.2, r, FRAC_PI_2, 3.0 * FRAC_PI_2));
            u.fill(c, &pts, 1.0);
            slot(c, u, 0.05, 0.08);
            0.25
        }
        Head::Truss => {
            let r = 0.38;
            let mut pts = vec![(0.2, 0.5 - r), (0.2, 0.5 + r)];
            pts.extend(arc(0.2, 0.5, 0.13, r, FRAC_PI_2, 3.0 * FRAC_PI_2));
            u.fill(c, &pts, 1.0);
            cross(c, u, 0.07, 0.09, 0.1);
            0.2
        }
        Head::SocketCap => {
            // ISO 4762: tall cylinder, chamfered top edge, deep socket.
            let (u0, u1, r, ch) = (0.04, 0.32, 0.32, 0.04);
            u.fill(c, &[(u0, 0.5 - r + ch), (u0 + ch, 0.5 - r), (u1, 0.5 - r), (u1, 0.5 + r), (u0 + ch, 0.5 + r), (u0, 0.5 + r - ch)], 1.0);
            socket(c, u, u0, 0.17, 0.12);
            u1
        }
        Head::Cheese => {
            let (u0, u1, r) = (0.05, 0.25, 0.25);
            c.fill(&rounded_rect(u.rect(u0, 0.5 - r, u1, 0.5 + r), u.s(0.03)), 1.0);
            u.fill_rect(c, u1 - 0.04, 0.5 - r, u1, 0.5 + r, 1.0);
            slot(c, u, u0, 0.08);
            u1
        }
        Head::Hex => {
            // ISO 4017 across flats: three faces (the middle one twice as
            // wide), each with the chamfer arc on the free face, and a
            // washer face towards the shank.
            let (u0, u1, r) = (0.05, 0.26, 0.42);
            let edges = [0.5 - r, 0.5 - r / 2.0, 0.5 + r / 2.0, 0.5 + r];
            let mut pts = vec![(u1, edges[0])];
            for w in edges.windows(2) {
                let (a, b) = (w[0], w[1]);
                let bulge = (b - a) * 0.3;
                pts.extend(arc(u0 + bulge, (a + b) / 2.0, bulge, (b - a) / 2.0, 3.0 * FRAC_PI_2, FRAC_PI_2));
            }
            pts.push((u1, edges[3]));
            u.fill(c, &pts, 1.0);
            for &e in &edges[1..3] {
                u.line(c, (u0, e), (u1 - 0.02, e), 0.022, 0.0);
            }
            u.fill_rect(c, u1, 0.5 - r * 0.7, u1 + 0.03, 0.5 + r * 0.7, 1.0);
            u1 + 0.03
        }
        Head::Carriage => {
            let r = 0.32;
            let mut pts = vec![(0.2, 0.5 - r), (0.2, 0.5 + r)];
            pts.extend(arc(0.2, 0.5, 0.15, r, FRAC_PI_2, 3.0 * FRAC_PI_2));
            u.fill(c, &pts, 1.0);
            u.fill_rect(c, 0.19, 0.5 - SHANK * 1.15, 0.31, 0.5 + SHANK * 1.15, 1.0); // square neck
            0.31
        }
        Head::Wing => {
            u.ellipse(c, 0.13, 0.25, 0.08, 0.2, 1.0);
            u.ellipse(c, 0.13, 0.75, 0.08, 0.2, 1.0);
            u.fill_rect(c, 0.08, 0.34, 0.27, 0.66, 1.0);
            0.27
        }
        Head::Knurled => {
            let r = 0.3;
            u.fill_rect(c, 0.05, 0.5 - r, 0.23, 0.5 + r, 1.0);
            for k in 0..5 {
                let v = 0.5 - r + 0.07 + k as f32 * 0.115;
                u.line(c, (0.065, v), (0.215, v + 0.05), 0.022, 0.0);
            }
            u.fill_rect(c, 0.23, 0.5 - SHANK * 1.4, 0.28, 0.5 + SHANK * 1.4, 1.0);
            0.28
        }
    }
}

/// A threaded section from `u0` to `u1` around the centre line.
fn thread(c: &mut Canvas, u: &U, u0: f32, u1: f32, ri: f32, ro: f32, pitch: f32, tip: Tip) {
    let taper = match tip {
        Tip::Point => 0.16,
        Tip::Blunt => 0.0,
        Tip::Drill => 0.0,
    };
    let end = if tip == Tip::Drill { u1 - 0.14 } else { u1 };
    let scale = |x: f32| if taper > 0.0 && x > end - taper { ((end - x) / taper).clamp(0.0, 1.0) } else { 1.0 };
    // Root at the start, then crest/root every half pitch; the bottom edge
    // is offset by a quarter pitch so the flanks look helical.
    let edge = |phase: f32, sign: f32| {
        let mut pts = vec![(u0, 0.5 + sign * ri * scale(u0))];
        let mut x = u0 + phase;
        let mut crest = true;
        while x < end - 1e-4 {
            let r = if crest { ro } else { ri };
            pts.push((x, 0.5 + sign * r * scale(x)));
            x += pitch / 2.0;
            crest = !crest;
        }
        let r_end = if tip == Tip::Blunt { ri * 0.9 } else { ri * scale(end) };
        pts.push((end, 0.5 + sign * r_end));
        pts
    };
    let top = edge(pitch / 4.0, -1.0);
    let mut bottom = edge(pitch / 2.0, 1.0);
    bottom.reverse();
    let mut pts = top;
    pts.extend(bottom);
    u.fill(c, &pts, 1.0);
    if tip == Tip::Drill {
        // Plain drill shank with a flute, then a sharp point.
        u.fill(c, &[(end, 0.5 - ri), (u1 - 0.04, 0.5 - ri), (u1, 0.5), (u1 - 0.04, 0.5 + ri), (end, 0.5 + ri)], 1.0);
        u.line(c, (end + 0.02, 0.5 + ri * 0.6), (u1 - 0.05, 0.5 - ri * 0.4), 0.022, 0.0);
    }
}

fn screw(c: &mut Canvas, u: &U, h: Head, thread_kind: Thread) {
    let start = head(c, u, h);
    let (ri, ro) = (SHANK - 0.035, SHANK);
    match thread_kind {
        Thread::Machine => thread(c, u, start - 0.005, 0.95, ri, ro, 0.085, Tip::Blunt),
        Thread::Wood => {
            u.fill_rect(c, start - 0.01, 0.5 - ri, 0.52, 0.5 + ri, 1.0);
            thread(c, u, 0.52, 0.97, ri - 0.02, ro + 0.015, 0.1, Tip::Point);
        }
        Thread::Chipboard => thread(c, u, start - 0.005, 0.97, ri - 0.02, ro + 0.015, 0.1, Tip::Point),
        Thread::Sheet => thread(c, u, start - 0.005, 0.95, ri - 0.01, ro, 0.085, Tip::Point),
        Thread::Drill => thread(c, u, start - 0.005, 0.97, ri, ro, 0.085, Tip::Drill),
    }
}

#[derive(Clone, Copy)]
enum Thread {
    Machine,
    Wood,
    Chipboard,
    Sheet,
    Drill,
}

fn head_top(c: &mut Canvas, u: &U) -> P {
    let ctr = u.p(0.5, 0.5);
    c.disc(ctr, u.s(0.42), 1.0);
    ctr
}

#[derive(Clone, Copy)]
pub struct Picto {
    pub ch: char,
    pub name: &'static str,
    draw: fn(&mut Canvas, &U),
}

macro_rules! picto {
    ($n:expr, $name:literal, $f:expr) => {
        Picto { ch: match char::from_u32(FIRST + $n) { Some(c) => c, None => ' ' }, name: $name, draw: $f }
    };
}

/// Screw heads (all with machine thread). Code points stay stable: saved
/// designs refer to them.
pub const HEADS: &[Picto] = &[
    picto!(0, "Senkkopf Innensechskant", |c, u| screw(c, u, Head::Countersunk, Thread::Machine)),
    picto!(11, "Senkkopf Kreuz", |c, u| screw(c, u, Head::CountersunkCross, Thread::Machine)),
    picto!(5, "Zylinderkopf Innensechskant", |c, u| screw(c, u, Head::SocketCap, Thread::Machine)),
    picto!(2, "Linsenkopf Innensechskant", |c, u| screw(c, u, Head::Button, Thread::Machine)),
    picto!(12, "Linsenkopf Kreuz", |c, u| screw(c, u, Head::PanCross, Thread::Machine)),
    picto!(6, "Sechskantkopf", |c, u| screw(c, u, Head::Hex, Thread::Machine)),
    picto!(1, "Linsensenkkopf", |c, u| screw(c, u, Head::RaisedCountersunk, Thread::Machine)),
    picto!(3, "Halbrundkopf", |c, u| screw(c, u, Head::Round, Thread::Machine)),
    picto!(4, "Flachrundkopf", |c, u| screw(c, u, Head::Truss, Thread::Machine)),
    picto!(13, "Zylinderkopf Schlitz", |c, u| screw(c, u, Head::Cheese, Thread::Machine)),
    picto!(7, "Schlossschraube", |c, u| screw(c, u, Head::Carriage, Thread::Machine)),
    picto!(8, "Flügelschraube", |c, u| screw(c, u, Head::Wing, Thread::Machine)),
    picto!(9, "Rändelschraube", |c, u| screw(c, u, Head::Knurled, Thread::Machine)),
    picto!(10, "Madenschraube", |c, u| {
        thread(c, u, 0.08, 0.92, SHANK - 0.032, SHANK, 0.085, Tip::Blunt);
        socket(c, u, 0.08, 0.1, 0.07);
    }),
];

/// Thread kinds.
pub const THREADS: &[Picto] = &[
    picto!(16, "Holzschraube", |c, u| screw(c, u, Head::CountersunkCross, Thread::Wood)),
    picto!(17, "Spanplattenschraube", |c, u| screw(c, u, Head::CountersunkCross, Thread::Chipboard)),
    picto!(18, "Maschinenschraube", |c, u| screw(c, u, Head::SocketCap, Thread::Machine)),
    picto!(19, "Blechschraube", |c, u| screw(c, u, Head::PanCross, Thread::Sheet)),
    picto!(20, "Bohrschraube", |c, u| screw(c, u, Head::Hex, Thread::Drill)),
    picto!(21, "Gewindestange", |c, u| thread(c, u, 0.04, 0.96, 0.085, 0.115, 0.055, Tip::Blunt)),
    picto!(22, "Stockschraube", |c, u| {
        thread(c, u, 0.04, 0.44, 0.085, 0.115, 0.055, Tip::Blunt);
        u.fill_rect(c, 0.43, 0.42, 0.52, 0.58, 1.0);
        thread(c, u, 0.52, 0.96, 0.06, 0.13, 0.09, Tip::Point);
    }),
    picto!(23, "Holzschraube Linsenkopf", |c, u| screw(c, u, Head::PanCross, Thread::Wood)),
];

/// Drives (top view).
pub const DRIVES: &[Picto] = &[
    picto!(32, "Schlitz", |c, u| {
        head_top(c, u);
        u.fill_rect(c, 0.14, 0.45, 0.86, 0.55, 0.0);
    }),
    picto!(33, "Kreuz (PH)", |c, u| {
        head_top(c, u);
        u.fill_rect(c, 0.22, 0.445, 0.78, 0.555, 0.0);
        u.fill_rect(c, 0.445, 0.22, 0.555, 0.78, 0.0);
    }),
    picto!(34, "Pozidriv (PZ)", |c, u| {
        head_top(c, u);
        u.fill_rect(c, 0.22, 0.445, 0.78, 0.555, 0.0);
        u.fill_rect(c, 0.445, 0.22, 0.555, 0.78, 0.0);
        for (a, b) in [((0.36, 0.36), (0.29, 0.29)), ((0.64, 0.36), (0.71, 0.29)), ((0.36, 0.64), (0.29, 0.71)), ((0.64, 0.64), (0.71, 0.71))] {
            u.line(c, a, b, 0.03, 0.0);
        }
    }),
    picto!(35, "Torx", |c, u| {
        let ctr = head_top(c, u);
        c.fill(&star(ctr, u.s(0.25), u.s(0.15), 6), 0.0);
        for k in 0..6 {
            let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::TAU / 6.0;
            c.disc((ctr.0 + u.s(0.21) * a.cos(), ctr.1 + u.s(0.21) * a.sin()), u.s(0.055), 0.0);
        }
    }),
    picto!(36, "Innensechskant", |c, u| {
        let ctr = head_top(c, u);
        c.fill(&regular(ctr, u.s(0.21), 6, 0.0), 0.0);
    }),
    picto!(37, "Vierkant", |c, u| {
        head_top(c, u);
        u.fill_rect(c, 0.36, 0.36, 0.64, 0.64, 0.0);
    }),
    picto!(38, "Außensechskant", |c, u| {
        let ctr = u.p(0.5, 0.5);
        c.fill(&regular(ctr, u.s(0.45), 6, 0.0), 1.0);
        c.fill(&ellipse(ctr, u.s(0.33), u.s(0.33)), 0.0);
        c.fill(&ellipse(ctr, u.s(0.3), u.s(0.3)), 1.0);
    }),
];

/// Nuts, washers, fixings, tools.
pub const FIXINGS: &[Picto] = &[
    picto!(48, "Sechskantmutter", |c, u| {
        let ctr = u.p(0.5, 0.5);
        c.fill(&regular(ctr, u.s(0.45), 6, 0.0), 1.0);
        c.disc(ctr, u.s(0.2), 0.0);
        c.fill(&ellipse(ctr, u.s(0.165), u.s(0.165)), 1.0);
        c.disc(ctr, u.s(0.13), 0.0);
    }),
    picto!(49, "Flügelmutter", |c, u| {
        u.ellipse(c, 0.24, 0.4, 0.17, 0.23, 1.0);
        u.ellipse(c, 0.76, 0.4, 0.17, 0.23, 1.0);
        u.ellipse(c, 0.5, 0.64, 0.2, 0.2, 1.0);
        u.ellipse(c, 0.5, 0.64, 0.085, 0.085, 0.0);
    }),
    picto!(50, "Hutmutter", |c, u| {
        u.fill_rect(c, 0.2, 0.5, 0.8, 0.86, 1.0);
        for x in [0.4, 0.6] {
            u.line(c, (x, 0.52), (x, 0.84), 0.025, 0.0);
        }
        let mut pts = Vec::new();
        for i in 0..=24 {
            let a = i as f32 / 24.0 * std::f32::consts::PI;
            pts.push((0.5 + 0.27 * a.cos(), 0.5 - 0.34 * a.sin()));
        }
        u.fill(c, &pts, 1.0);
    }),
    picto!(51, "Unterlegscheibe", |c, u| {
        u.ellipse(c, 0.5, 0.5, 0.43, 0.43, 1.0);
        u.ellipse(c, 0.5, 0.5, 0.18, 0.18, 0.0);
    }),
    picto!(52, "Federring", |c, u| {
        u.ellipse(c, 0.5, 0.5, 0.42, 0.42, 1.0);
        u.ellipse(c, 0.5, 0.5, 0.25, 0.25, 0.0);
        u.fill(c, &[(0.66, 0.58), (0.72, 0.52), (0.96, 0.76), (0.9, 0.82)], 0.0);
    }),
    picto!(53, "Dübel", |c, u| {
        u.fill(c, &[(0.16, 0.36), (0.8, 0.39), (0.9, 0.46), (0.9, 0.54), (0.8, 0.61), (0.16, 0.64)], 1.0);
        u.fill_rect(c, 0.1, 0.3, 0.17, 0.7, 1.0);
        for k in 0..5 {
            let x = 0.26 + k as f32 * 0.1;
            u.line(c, (x, 0.37), (x + 0.03, 0.43), 0.025, 0.0);
            u.line(c, (x, 0.63), (x + 0.03, 0.57), 0.025, 0.0);
        }
        u.line(c, (0.62, 0.5), (0.92, 0.5), 0.03, 0.0);
    }),
    picto!(54, "Nagel", |c, u| {
        u.fill_rect(c, 0.06, 0.3, 0.11, 0.7, 1.0);
        u.fill_rect(c, 0.11, 0.455, 0.84, 0.545, 1.0);
        u.fill(c, &[(0.84, 0.455), (0.96, 0.5), (0.84, 0.545)], 1.0);
    }),
    picto!(55, "Blindniete", |c, u| {
        u.fill_rect(c, 0.02, 0.46, 0.12, 0.54, 1.0);
        c.fill(&rounded_rect(u.rect(0.1, 0.26, 0.17, 0.74), u.s(0.03)), 1.0);
        u.fill_rect(c, 0.17, 0.38, 0.62, 0.62, 1.0);
        u.ellipse(c, 0.66, 0.5, 0.05, 0.08, 1.0);
    }),
    picto!(56, "Montagewinkel", |c, u| {
        u.fill_rect(c, 0.18, 0.12, 0.32, 0.86, 1.0);
        u.fill_rect(c, 0.18, 0.72, 0.88, 0.86, 1.0);
        for (x, y) in [(0.25, 0.3), (0.25, 0.52), (0.5, 0.79), (0.72, 0.79)] {
            u.ellipse(c, x, y, 0.035, 0.035, 0.0);
        }
    }),
    picto!(57, "Schraubendreher", |c, u| {
        c.fill(&rounded_rect(u.rect(0.04, 0.34, 0.4, 0.66), u.s(0.08)), 1.0);
        for v in [0.43, 0.5, 0.57] {
            u.line(c, (0.1, v), (0.34, v), 0.02, 0.0);
        }
        u.fill_rect(c, 0.4, 0.465, 0.86, 0.535, 1.0);
        u.fill(c, &[(0.86, 0.44), (0.95, 0.47), (0.95, 0.53), (0.86, 0.56)], 1.0);
    }),
    picto!(58, "Inbusschlüssel", |c, u| {
        u.line(c, (0.24, 0.14), (0.24, 0.74), 0.1, 1.0);
        u.line(c, (0.24, 0.74), (0.88, 0.74), 0.1, 1.0);
    }),
    picto!(59, "Gewindeeinsatz", |c, u| {
        u.fill_rect(c, 0.1, 0.3, 0.18, 0.7, 1.0);
        thread(c, u, 0.18, 0.88, 0.12, 0.17, 0.08, Tip::Blunt);
        u.fill_rect(c, 0.1, 0.43, 0.9, 0.57, 0.0);
    }),
];

pub const GROUPS: &[(&str, &[Picto])] =
    &[("Schraubenköpfe", HEADS), ("Gewinde", THREADS), ("Antriebe", DRIVES), ("Muttern & Zubehör", FIXINGS)];

pub fn find(ch: char) -> Option<&'static Picto> {
    GROUPS.iter().flat_map(|(_, g)| g.iter()).find(|p| p.ch == ch)
}

/// Draw pictogram `ch` into the square centred in `r`. Returns false if
/// `ch` is not one of ours.
pub fn draw(c: &mut Canvas, ch: char, r: Rect) -> bool {
    let Some(p) = find(ch) else { return false };
    let side = r.w().min(r.h());
    let (x0, y0) = (r.x0 + (r.w() - side) / 2.0, r.y0 + (r.h() - side) / 2.0);
    (p.draw)(c, &U { r: Rect::new(x0, y0, x0 + side, y0 + side) });
    true
}
