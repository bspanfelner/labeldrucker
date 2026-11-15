//! Frame styles: classic outlines plus decorative designs for occasions.
//!
//! Every style draws into an area (the label minus its end margins) and
//! returns the rectangle left for the content. Lengths are in dots (8/mm);
//! the head is 96 dots tall, so decorations stay in thin bands.

use crate::draw::{
    Canvas, P, Pattern, Rect, chamfer_rect, ellipse, rect_poly, regular, rounded_rect, star, ticket, wave_rect,
};
use crate::icons;
use crate::text::Font;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Classic,
    Decorative,
    Work,
    Birthday,
    Christmas,
    Storage,
}

impl Category {
    pub const ALL: [Category; 6] =
        [Self::Classic, Self::Decorative, Self::Work, Self::Birthday, Self::Christmas, Self::Storage];

    pub fn name(self) -> &'static str {
        match self {
            Self::Classic => "Klassisch",
            Self::Decorative => "Dekorativ",
            Self::Work => "Arbeit",
            Self::Birthday => "Geburtstag",
            Self::Christmas => "Weihnachten",
            Self::Storage => "Aufbewahrung",
        }
    }
}

macro_rules! styles {
    ($($v:ident $name:literal $cat:ident),* $(,)?) => {
        /// Frame styles.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub enum FrameStyle {
            #[default]
            $($v),*
        }

        impl FrameStyle {
            pub const ALL: &'static [FrameStyle] = &[$(Self::$v),*];

            /// German display name.
            pub fn name(self) -> &'static str {
                match self { $(Self::$v => $name),* }
            }

            pub fn category(self) -> Category {
                match self { $(Self::$v => Category::$cat),* }
            }
        }
    };
}

styles! {
    None "Ohne" Classic,
    Line "Linie" Classic,
    Double "Doppelt" Classic,
    ThickThin "Dick-dünn" Classic,
    Dashed "Gestrichelt" Classic,
    Dotted "Gepunktet" Classic,
    Corners "Ecken" Classic,
    Brackets "Klammern" Classic,
    Chamfer "Abgeschrägt" Classic,
    Capsule "Kapsel" Classic,
    Ticket "Ticket" Classic,
    Wave "Welle" Classic,
    Shadow "Schatten" Classic,

    Scallop "Bogenkante" Decorative,
    Zigzag "Zickzack" Decorative,
    Stamp "Briefmarke" Decorative,
    ArtDeco "Art déco" Decorative,
    Ornament "Ornament" Decorative,
    Rope "Kordel" Decorative,
    Ribbon "Banderole" Decorative,
    Stitched "Naht" Decorative,
    Film "Filmstreifen" Decorative,
    Hearts "Herzen" Decorative,
    Stars "Sterne" Decorative,

    Hazard "Warnband" Work,
    Nameplate "Typenschild" Work,
    Technical "Technisch" Work,
    HeaderBar "Kopfbalken" Work,
    FolderTab "Ordnerreiter" Work,
    Circuit "Platine" Work,

    Bunting "Wimpelkette" Birthday,
    Confetti "Konfetti" Birthday,
    Balloons "Luftballons" Birthday,
    Party "Party" Birthday,
    Candles "Kerzen" Birthday,

    CandyCane "Zuckerstange" Christmas,
    Snowflakes "Schneeflocken" Christmas,
    FirTrees "Tannenwald" Christmas,
    FairyLights "Lichterkette" Christmas,
    GiftRibbon "Geschenkband" Christmas,

    Crate "Holzkiste" Storage,
    TapeCorners "Klebestreifen" Storage,
    Holder "Schubladenhalter" Storage,
    Gingham "Vorratsglas" Storage,
    Carton "Karton" Storage,
}

impl FrameStyle {
    /// Whether the corner radius setting applies, and what it is called.
    pub fn radius_name(self) -> Option<&'static str> {
        match self {
            Self::Line | Self::Double | Self::ThickThin | Self::Dashed | Self::Dotted | Self::Shadow | Self::Stitched => {
                Some("Eckenradius")
            }
            Self::Chamfer => Some("Schräge"),
            Self::Ticket => Some("Kerbe"),
            _ => None,
        }
    }

    pub fn in_category(c: Category) -> impl Iterator<Item = FrameStyle> {
        Self::ALL.iter().copied().filter(move |s| s.category() == c)
    }
}

/// Frame settings; all lengths in dots (8 per mm).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct Frame {
    pub style: FrameStyle,
    /// Line thickness.
    pub thickness: f32,
    /// Distance from the label edge to the frame.
    pub margin: f32,
    /// Corner radius (also chamfer size / ticket notch size).
    pub radius: f32,
    /// Space between frame and content.
    pub padding: f32,
}

impl Default for Frame {
    fn default() -> Self {
        Self { style: FrameStyle::None, thickness: 3.0, margin: 2.0, radius: 10.0, padding: 4.0 }
    }
}

/// Deterministic pseudo-random numbers for confetti and the like.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 % 10_000) as f32 / 10_000.0
    }
}

/// Centres of `n` evenly spread cells of about `cell` along `x0..x1`.
fn spread(x0: f32, x1: f32, cell: f32) -> Vec<f32> {
    let n = ((x1 - x0) / cell).floor().max(1.0) as usize;
    let step = (x1 - x0) / n as f32;
    (0..n).map(|i| x0 + step * (i as f32 + 0.5)).collect()
}

fn glyph(c: &mut Canvas, font: &impl Font, ch: char, ctr: P, size: f32) {
    let h = size / 2.0;
    icons::draw(c, font, ch, Rect::new(ctr.0 - h, ctr.1 - h, ctr.0 + h, ctr.1 + h));
}

/// Fill the band between `outer` and `inner` with a pattern.
fn pattern_ring(c: &mut Canvas, outer: &[P], inner: &[P], f: impl Fn(usize, usize) -> bool) {
    c.fill_with(outer, f);
    c.fill(inner, 0.0);
}

fn stripes(period: usize) -> impl Fn(usize, usize) -> bool {
    move |x, y| (x + y) % period < period / 2
}

impl Frame {
    /// Draw the frame inside `area` and return the rectangle left for content.
    /// `icons` is the pictogram font (for star, heart, … motifs).
    pub fn draw(&self, c: &mut Canvas, area: Rect, icons: &impl Font) -> Rect {
        let t = self.thickness.max(1.0);
        let b = area.inset(self.margin, self.margin);
        let r = self.radius;
        let pad = self.padding;
        let gap = t.max(2.0);
        let content = |e: f32, extra_x: f32| b.inset(e + pad + extra_x, e + pad);
        let ring = |c: &mut Canvas, o: Rect, r: f32, t: f32| {
            c.ring(&rounded_rect(o, r), &rounded_rect(o.inset(t, t), (r - t).max(0.0)))
        };
        let corner_room = |r: f32| r * 0.3;
        // Height of a decorative band at top/bottom.
        let band = (b.h() * 0.16).clamp(8.0, 15.0);
        let thin = (t * 0.5).clamp(1.0, 2.0);

        match self.style {
            FrameStyle::None => area.inset(self.margin.min(4.0) + pad / 2.0, self.margin.min(4.0)),
            FrameStyle::Line => {
                ring(c, b, r, t);
                content(t, corner_room(r))
            }
            FrameStyle::Double => {
                ring(c, b, r, t);
                ring(c, b.inset(t + gap, t + gap), (r - t - gap).max(0.0), t);
                content(2.0 * t + gap, corner_room(r))
            }
            FrameStyle::ThickThin => {
                let thick = (t * 2.0).max(3.0);
                let thin = (t / 2.0).round().max(1.0);
                ring(c, b, r, thick);
                ring(c, b.inset(thick + gap, thick + gap), (r - thick - gap).max(0.0), thin);
                content(thick + gap + thin, corner_room(r))
            }
            FrameStyle::Dashed | FrameStyle::Dotted => {
                let mid = b.inset(t / 2.0, t / 2.0);
                let path = rounded_rect(mid, (r - t / 2.0).max(0.0));
                if self.style == FrameStyle::Dashed {
                    c.stroke(&path, true, t, Pattern::Dash { on: 3.0 * t + 3.0, off: 2.0 * t + 2.0 });
                } else {
                    c.stroke(&path, true, t + 0.8, Pattern::Dots { spacing: t * 2.2 + 1.5 });
                }
                content(t, corner_room(r))
            }
            FrameStyle::Corners => {
                let len = (b.h().min(b.w()) / 3.0).max(t * 3.0);
                for (x, y, sx, sy) in [(b.x0, b.y0, 1.0, 1.0), (b.x1, b.y0, -1.0, 1.0), (b.x0, b.y1, 1.0, -1.0), (b.x1, b.y1, -1.0, -1.0)] {
                    let span = |a: f32, l: f32, s: f32| if s > 0.0 { (a, a + l) } else { (a - l, a) };
                    let ((hx0, hx1), (hy0, hy1)) = (span(x, len, sx), span(y, t, sy));
                    c.fill_rect(Rect::new(hx0, hy0, hx1, hy1), 1.0);
                    let ((vx0, vx1), (vy0, vy1)) = (span(x, t, sx), span(y, len, sy));
                    c.fill_rect(Rect::new(vx0, vy0, vx1, vy1), 1.0);
                }
                content(t, 0.0)
            }
            FrameStyle::Brackets => {
                let stub = (b.h() / 5.0).max(t * 2.0);
                for (x, s) in [(b.x0, 1.0f32), (b.x1, -1.0)] {
                    let (x0, x1) = if s > 0.0 { (x, x + t) } else { (x - t, x) };
                    c.fill_rect(Rect::new(x0, b.y0, x1, b.y1), 1.0);
                    let (sx0, sx1) = if s > 0.0 { (x, x + stub) } else { (x - stub, x) };
                    c.fill_rect(Rect::new(sx0, b.y0, sx1, b.y0 + t), 1.0);
                    c.fill_rect(Rect::new(sx0, b.y1 - t, sx1, b.y1), 1.0);
                }
                content(t, 0.0)
            }
            FrameStyle::Chamfer => {
                let cut = r.max(t * 2.0);
                c.ring(&chamfer_rect(b, cut), &chamfer_rect(b.inset(t, t), (cut - t * 0.6).max(0.0)));
                content(t, cut * 0.5)
            }
            FrameStyle::Capsule => {
                let rr = b.h() / 2.0;
                ring(c, b, rr, t);
                content(t, rr * 0.45)
            }
            FrameStyle::Ticket => {
                let notch = (r * 1.4).clamp(6.0, b.h() / 2.0 - t - 4.0);
                c.ring(&ticket(b, notch), &ticket(b.inset(t, t), notch + t));
                content(t, notch)
            }
            FrameStyle::Wave => {
                let amp = (t + 1.0).max(3.0);
                let period = 16.0 + 2.0 * t;
                c.ring(&wave_rect(b, amp, period), &wave_inset(b, amp, period, t));
                b.inset(t + pad, amp + t + pad)
            }
            FrameStyle::Shadow => {
                let s = (t + 2.0).max(3.0);
                let main = Rect::new(b.x0, b.y0, b.x1 - s, b.y1 - s);
                c.fill(&rounded_rect(Rect::new(b.x0 + s, b.y0 + s, b.x1, b.y1), r), 1.0);
                c.fill(&rounded_rect(main, r), 0.0);
                ring(c, main, r, t);
                main.inset(t + pad + corner_room(r), t + pad)
            }

            // ---------------------------------------------------- decorative
            FrameStyle::Scallop => {
                let s = (b.h() * 0.09).clamp(4.0, 8.0);
                let inner = b.inset(s, s);
                c.stroke(&scallop_path(inner, s), true, t.min(3.0), Pattern::Solid);
                inner.inset(t + pad + s * 0.5, t + pad)
            }
            FrameStyle::Zigzag => {
                let a = (b.h() * 0.07).clamp(3.0, 6.0);
                let inner = b.inset(a, a);
                c.stroke(&zigzag_path(inner, a), true, t.min(3.0), Pattern::Solid);
                inner.inset(t + pad, t + pad)
            }
            FrameStyle::Stamp => {
                let w = (2.0 * t).max(6.0);
                let hole = w * 0.55;
                ring(c, b, 0.0, w);
                for (a, z) in [((b.x0, b.y0), (b.x1, b.y0)), ((b.x1, b.y0), (b.x1, b.y1)), ((b.x1, b.y1), (b.x0, b.y1)), ((b.x0, b.y1), (b.x0, b.y0))] {
                    let len = ((z.0 - a.0) as f32).hypot(z.1 - a.1);
                    let n = (len / (hole * 2.6)).round().max(1.0) as usize;
                    for i in 0..=n {
                        let k = i as f32 / n as f32;
                        c.disc((a.0 + (z.0 - a.0) * k, a.1 + (z.1 - a.1) * k), hole, 0.0);
                    }
                }
                ring(c, b.inset(w + 2.0, w + 2.0), 0.0, 1.0);
                b.inset(w + 3.0 + pad, w + 3.0 + pad)
            }
            FrameStyle::ArtDeco => {
                let k = (b.h() * 0.09).clamp(4.0, 8.0);
                c.ring(&stepped(b, k), &stepped(b.inset(t, t), k));
                let inner = b.inset(t + 3.0, t + 3.0);
                c.ring(&stepped(inner, k), &stepped(inner.inset(thin, thin), k));
                // Fan of bars at both ends.
                let ym = b.y0 + b.h() / 2.0;
                for (x, s) in [(inner.x0 + thin + 3.0, 1.0f32), (inner.x1 - thin - 3.0, -1.0)] {
                    for i in 0..3 {
                        let h = b.h() * (0.16 + 0.08 * (2 - i) as f32);
                        let xi = x + s * i as f32 * (thin + 2.0);
                        c.fill_rect(Rect::new(xi.min(xi + s * thin), ym - h, xi.max(xi + s * thin), ym + h), 1.0);
                    }
                }
                inner.inset(thin + pad + 12.0, thin + pad)
            }
            FrameStyle::Ornament => {
                ring(c, b, 0.0, t);
                let inner = b.inset(t + 3.0, t + 3.0);
                ring(c, inner, 0.0, thin);
                let d = (b.h() * 0.12).clamp(6.0, 11.0);
                for p in [(b.x0, b.y0), (b.x1, b.y0), (b.x0, b.y1), (b.x1, b.y1)] {
                    let x = p.0 + if p.0 < b.x1 { d * 0.8 } else { -d * 0.8 };
                    let y = p.1 + if p.1 < b.y1 { d * 0.8 } else { -d * 0.8 };
                    c.fill(&regular((x, y), d * 0.75, 4, 0.0), 0.0);
                    c.fill(&regular((x, y), d * 0.55, 4, 0.0), 1.0);
                }
                let xm = b.x0 + b.w() / 2.0;
                for y in [b.y0 + d * 0.8, b.y1 - d * 0.8] {
                    c.fill(&regular((xm, y), d * 0.8, 4, 0.0), 0.0);
                    c.fill(&regular((xm, y), d * 0.6, 4, 0.0), 1.0);
                    for s in [-1.0f32, 1.0] {
                        c.disc((xm + s * d * 1.3, y), d * 0.22, 1.0);
                    }
                }
                inner.inset(thin + pad + d * 0.6, thin + pad + d)
            }
            FrameStyle::Rope => {
                let w = (b.h() * 0.12).clamp(6.0, 10.0);
                let rr = w;
                let outer = rounded_rect(b, rr);
                let inner = rounded_rect(b.inset(w, w), (rr - w).max(0.0));
                let period = (w * 0.9).round().max(4.0) as usize;
                pattern_ring(c, &outer, &inner, move |x, y| (x + y) % period < period * 3 / 5);
                c.stroke(&outer, true, 1.0, Pattern::Solid);
                c.stroke(&inner, true, 1.0, Pattern::Solid);
                b.inset(w + pad + 1.0, w + pad + 1.0)
            }
            FrameStyle::Ribbon => {
                let n = b.h() * 0.28;
                c.ring(&fishtail(b, n), &fishtail(b.inset(t, t), n));
                for x in [b.x0 + n * 1.7, b.x1 - n * 1.7] {
                    c.fill_rect(Rect::new(x - thin / 2.0, b.y0, x + thin / 2.0, b.y1), 1.0);
                }
                b.inset(n * 1.7 + thin + pad, t + pad)
            }
            FrameStyle::Stitched => {
                ring(c, b, r, t);
                let inner = b.inset(t + 3.0, t + 3.0);
                let w = (t * 0.6).max(1.2);
                let path = rounded_rect(inner.inset(w / 2.0, w / 2.0), (r - t - 3.0).max(0.0));
                c.stroke(&path, true, w, Pattern::Dash { on: 4.0, off: 3.0 });
                inner.inset(w + pad + corner_room(r), w + pad)
            }
            FrameStyle::Film => {
                let h = band.max(10.0);
                for y0 in [b.y0, b.y1 - h] {
                    c.fill_rect(Rect::new(b.x0, y0, b.x1, y0 + h), 1.0);
                    let s = h * 0.5;
                    for x in spread(b.x0, b.x1, h * 1.15) {
                        c.fill(&rounded_rect(Rect::new(x - s / 2.0, y0 + h / 2.0 - s / 2.0, x + s / 2.0, y0 + h / 2.0 + s / 2.0), 1.5), 0.0);
                    }
                }
                Rect::new(b.x0 + pad, b.y0 + h + pad, b.x1 - pad, b.y1 - h - pad)
            }
            FrameStyle::Hearts | FrameStyle::Stars => {
                let ch = if self.style == FrameStyle::Hearts { '\u{f004}' } else { '\u{f005}' };
                motif_bands(c, icons, b, band, &[ch], true);
                Rect::new(b.x0 + pad, b.y0 + band + pad, b.x1 - pad, b.y1 - band - pad)
            }

            // ---------------------------------------------------------- work
            FrameStyle::Hazard => {
                let h = band;
                let period = (h * 1.1).round().max(6.0) as usize;
                for y0 in [b.y0, b.y1 - h] {
                    c.fill_with(&rect_poly(Rect::new(b.x0, y0, b.x1, y0 + h)), stripes(period));
                }
                c.stroke(&rect_poly(b), true, 1.0, Pattern::Solid);
                Rect::new(b.x0 + 1.0 + pad, b.y0 + h + pad, b.x1 - 1.0 - pad, b.y1 - h - pad)
            }
            FrameStyle::Nameplate => {
                let rr = (b.h() * 0.14).max(r);
                ring(c, b, rr, t);
                let sr = (b.h() * 0.1).clamp(3.5, 8.0);
                let off = t + sr + 2.5;
                for (x, y) in [(b.x0 + off, b.y0 + off), (b.x1 - off, b.y0 + off), (b.x0 + off, b.y1 - off), (b.x1 - off, b.y1 - off)] {
                    c.disc((x, y), sr, 1.0);
                    let d = sr * 0.65;
                    c.line((x - d, y - d * 0.4), (x + d, y + d * 0.4), (sr * 0.35).max(1.0), 0.0);
                }
                b.inset(t + 2.0 * sr + 5.0 + pad, t + pad)
            }
            FrameStyle::Technical => {
                ring(c, b, 0.0, thin);
                let x0 = b.x0 + thin;
                let mut x = x0;
                let mut i = 0;
                while x < b.x1 - thin {
                    let len = if i % 5 == 0 { 7.0 } else { 3.5 };
                    for (y, s) in [(b.y0 + thin, 1.0f32), (b.y1 - thin, -1.0)] {
                        c.fill_rect(Rect::new(x, y.min(y + s * len), x + 1.0, y.max(y + s * len)), 1.0);
                    }
                    x += 8.0;
                    i += 1;
                }
                // Cross-hair marks at both ends.
                let ym = b.y0 + b.h() / 2.0;
                for xm in [b.x0 + 9.0, b.x1 - 9.0] {
                    c.fill_rect(Rect::new(xm - 5.0, ym - 0.5, xm + 5.0, ym + 0.5), 1.0);
                    c.fill_rect(Rect::new(xm - 0.5, ym - 5.0, xm + 0.5, ym + 5.0), 1.0);
                }
                b.inset(thin + pad + 15.0, thin + 7.0 + pad)
            }
            FrameStyle::HeaderBar => {
                ring(c, b, r.min(6.0), t);
                let bar = (b.h() * 0.3).clamp(14.0, 30.0);
                c.fill(&rounded_rect(Rect::new(b.x0, b.y0, b.x0 + bar, b.y1), r.min(6.0)), 1.0);
                c.fill_rect(Rect::new(b.x0 + bar / 2.0, b.y0, b.x0 + bar, b.y1), 1.0);
                c.disc((b.x0 + bar / 2.0, b.y0 + b.h() / 2.0), (bar * 0.18).max(2.0), 0.0);
                Rect::new(b.x0 + bar + pad + 2.0, b.y0 + t + pad, b.x1 - t - pad, b.y1 - t - pad)
            }
            FrameStyle::FolderTab => {
                let tab = (b.h() * 0.14).clamp(7.0, 12.0);
                let tw = (b.w() * 0.32).min(120.0);
                let shape = |q: Rect, tab: f32, tw: f32| -> Vec<P> {
                    vec![(q.x0, q.y0), (q.x0 + tw, q.y0), (q.x0 + tw + tab, q.y0 + tab), (q.x1, q.y0 + tab), (q.x1, q.y1), (q.x0, q.y1)]
                };
                c.ring(&shape(b, tab, tw), &shape(b.inset(t, t), tab, tw - t * 0.4));
                Rect::new(b.x0 + t + pad, b.y0 + tab + t + pad, b.x1 - t - pad, b.y1 - t - pad)
            }
            FrameStyle::Circuit => {
                ring(c, b, 0.0, thin);
                let mut rng = Rng(0x5eed_1234);
                for (y, s) in [(b.y0 + thin, 1.0f32), (b.y1 - thin, -1.0)] {
                    for x in spread(b.x0 + 12.0, b.x1 - 12.0, 22.0) {
                        let len = 7.0 + rng.next() * 5.0;
                        let x2 = x + if rng.next() > 0.5 { 7.0 } else { -7.0 };
                        let (y1, y2) = (y + s * len * 0.5, y + s * len);
                        c.line((x, y), (x, y1), 1.8, 1.0);
                        c.line((x, y1), (x2, y2), 1.8, 1.0);
                        c.disc((x2, y2), 3.0, 1.0);
                        c.disc((x2, y2), 1.3, 0.0);
                    }
                }
                let ym = b.y0 + b.h() / 2.0;
                for x in [b.x0 + thin, b.x1 - thin] {
                    c.fill_rect(Rect::new(x - 3.0, ym - 8.0, x + 3.0, ym + 8.0), 1.0);
                }
                b.inset(thin + pad + 4.0, thin + 15.0 + pad)
            }

            // ------------------------------------------------------ birthday
            FrameStyle::Bunting => {
                let pw = (b.h() * 0.2).clamp(10.0, 18.0);
                let ph = pw * 0.9;
                let sag = 3.0;
                let string_y = |x: f32| {
                    let k = (x - b.x0) / b.w();
                    b.y0 + 1.0 + sag * 4.0 * k * (1.0 - k)
                };
                let pts: Vec<P> = (0..=40).map(|i| {
                    let x = b.x0 + b.w() * i as f32 / 40.0;
                    (x, string_y(x))
                }).collect();
                c.stroke(&pts, false, 1.2, Pattern::Solid);
                for (i, x) in spread(b.x0, b.x1, pw * 1.25).into_iter().enumerate() {
                    let (l, rr) = (x - pw / 2.0, x + pw / 2.0);
                    let tri = vec![(l, string_y(l)), (rr, string_y(rr)), (x, string_y(x) + ph)];
                    if i % 2 == 0 {
                        c.fill(&tri, 1.0);
                    } else {
                        c.stroke(&tri, true, 1.4, Pattern::Solid);
                        c.disc((x, string_y(x) + ph * 0.38), 1.3, 1.0);
                    }
                }
                c.fill_rect(Rect::new(b.x0 + 4.0, b.y1 - thin, b.x1 - 4.0, b.y1), 1.0);
                Rect::new(b.x0 + pad, b.y0 + ph + sag + pad, b.x1 - pad, b.y1 - thin - pad)
            }
            FrameStyle::Confetti => {
                let side = band * 1.6;
                let mut rng = Rng(0xc0f_fe77);
                let zones = [
                    Rect::new(b.x0, b.y0, b.x1, b.y0 + band),
                    Rect::new(b.x0, b.y1 - band, b.x1, b.y1),
                    Rect::new(b.x0, b.y0, b.x0 + side, b.y1),
                    Rect::new(b.x1 - side, b.y0, b.x1, b.y1),
                ];
                for z in zones {
                    let n = (z.w() * z.h() / 110.0) as usize;
                    let z = z.inset(3.5, 3.5);
                    for _ in 0..n {
                        let p = (z.x0 + rng.next() * z.w(), z.y0 + rng.next() * z.h());
                        match (rng.next() * 4.0) as u32 {
                            0 => c.disc(p, 1.6 + rng.next() * 1.4, 1.0),
                            1 => {
                                let a = rng.next() * std::f32::consts::PI;
                                let l = 2.0 + rng.next() * 2.0;
                                c.line((p.0 - l * a.cos(), p.1 - l * a.sin()), (p.0 + l * a.cos(), p.1 + l * a.sin()), 1.5, 1.0);
                            }
                            2 => c.fill(&star(p, 3.2, 1.4, 5), 1.0),
                            _ => c.stroke(&ellipse(p, 2.2, 2.2), true, 1.0, Pattern::Solid),
                        }
                    }
                }
                Rect::new(b.x0 + side + pad * 0.5, b.y0 + band + pad * 0.5, b.x1 - side - pad * 0.5, b.y1 - band - pad * 0.5)
            }
            FrameStyle::Balloons => {
                let rx = (b.h() * 0.11).clamp(5.0, 10.0);
                let ry = rx * 1.25;
                let zone = rx * 4.2;
                for (x0, s) in [(b.x0, 1.0f32), (b.x1, -1.0)] {
                    let balloons = [(x0 + s * rx * 1.3, b.y0 + ry + 1.0, true), (x0 + s * rx * 3.0, b.y0 + ry * 1.9, false)];
                    for (bx, by, filled) in balloons {
                        let e = ellipse((bx, by), rx, ry);
                        if filled {
                            c.fill(&e, 1.0);
                            c.disc((bx - s * rx * 0.35, by - ry * 0.4), rx * 0.18, 0.0);
                        } else {
                            c.stroke(&e, true, 1.5, Pattern::Solid);
                        }
                        c.fill(&[(bx - 1.5, by + ry + 2.0), (bx + 1.5, by + ry + 2.0), (bx, by + ry - 0.5)], 1.0);
                        let string: Vec<P> = (0..=16).map(|i| {
                            let k = i as f32 / 16.0;
                            let y = by + ry + 2.0 + k * (b.y1 - (by + ry + 2.0));
                            (bx + (k * 9.0).sin() * 2.0, y)
                        }).collect();
                        c.stroke(&string, false, 1.0, Pattern::Solid);
                    }
                }
                Rect::new(b.x0 + zone + pad, b.y0 + pad, b.x1 - zone - pad, b.y1 - pad)
            }
            FrameStyle::Party => {
                motif_bands(c, icons, b, band, &['\u{f005}', '\u{f111}'], true);
                let g = (b.h() * 0.42).min(34.0);
                let ym = b.y0 + b.h() / 2.0;
                glyph(c, icons, '\u{f06b}', (b.x0 + g / 2.0 + 2.0, ym), g);
                glyph(c, icons, '\u{f1fd}', (b.x1 - g / 2.0 - 2.0, ym), g);
                Rect::new(b.x0 + g + 4.0 + pad, b.y0 + band + pad, b.x1 - g - 4.0 - pad, b.y1 - band - pad)
            }
            FrameStyle::Candles => {
                let ch = (b.h() * 0.3).clamp(14.0, 28.0);
                let base = b.y1 - 2.0;
                c.fill_rect(Rect::new(b.x0, base, b.x1, b.y1), 1.0);
                let mut rng = Rng(0xcafe_babe);
                for (i, x) in spread(b.x0 + 4.0, b.x1 - 4.0, 26.0).into_iter().enumerate() {
                    let h = ch * (0.75 + 0.25 * rng.next());
                    let w = 6.0;
                    let r = Rect::new(x - w / 2.0, base - h, x + w / 2.0, base);
                    if i % 2 == 0 {
                        c.fill_rect(r, 1.0);
                    } else {
                        c.stroke(&rect_poly(r.inset(0.5, 0.5)), true, 1.0, Pattern::Solid);
                        for k in 0..((h / 5.0) as usize) {
                            let y = base - 3.0 - k as f32 * 5.0;
                            c.line((x - w / 2.0, y), (x + w / 2.0, y - 2.0), 1.0, 1.0);
                        }
                    }
                    let fy = base - h - 5.0;
                    c.fill(&[(x, fy - 5.0), (x + 3.0, fy + 1.0), (x, fy + 3.5), (x - 3.0, fy + 1.0)], 1.0);
                    c.fill_rect(Rect::new(x - 0.5, fy + 3.0, x + 0.5, base - h), 1.0);
                }
                c.fill(&star((b.x0 + 8.0, b.y0 + 6.0), 4.5, 2.0, 5), 1.0);
                c.fill(&star((b.x1 - 8.0, b.y0 + 6.0), 4.5, 2.0, 5), 1.0);
                Rect::new(b.x0 + pad + 14.0, b.y0 + pad, b.x1 - pad - 14.0, base - ch - 10.0 - pad * 0.5)
            }

            // ----------------------------------------------------- christmas
            FrameStyle::CandyCane => {
                let w = (b.h() * 0.13).clamp(6.0, 12.0);
                let outer = rounded_rect(b, w * 1.5);
                let inner = rounded_rect(b.inset(w, w), w * 0.5);
                let period = (w * 1.2).round().max(6.0) as usize;
                pattern_ring(c, &outer, &inner, stripes(period));
                c.stroke(&outer, true, 1.0, Pattern::Solid);
                c.stroke(&inner, true, 1.0, Pattern::Solid);
                b.inset(w + pad + 1.0 + w * 0.2, w + pad + 1.0)
            }
            FrameStyle::Snowflakes => {
                motif_bands(c, icons, b, band, &['\u{f2dc}'], true);
                for y in [b.y0 + band + 1.5, b.y1 - band - 1.5] {
                    c.fill_rect(Rect::new(b.x0, y - 0.5, b.x1, y + 0.5), 1.0);
                }
                Rect::new(b.x0 + pad, b.y0 + band + 2.0 + pad, b.x1 - pad, b.y1 - band - 2.0 - pad)
            }
            FrameStyle::FirTrees => {
                let th = (b.h() * 0.24).clamp(12.0, 22.0);
                let base = b.y1;
                let mut rng = Rng(0x7ee5_7ee5);
                for x in spread(b.x0, b.x1, th * 0.8) {
                    let h = th * (0.7 + 0.3 * rng.next());
                    let w = h * 0.62;
                    let trunk = h * 0.15;
                    c.fill_rect(Rect::new(x - 1.2, base - trunk, x + 1.2, base), 1.0);
                    let top = base - h;
                    for k in 0..3 {
                        let y0 = top + k as f32 * h * 0.25;
                        let y1 = y0 + h * 0.42;
                        let hw = w * (0.3 + 0.2 * k as f32);
                        c.fill(&[(x, y0), (x + hw, y1.min(base - trunk)), (x - hw, y1.min(base - trunk))], 1.0);
                    }
                }
                let mut rng = Rng(0x57a2_57a2);
                for x in spread(b.x0 + 4.0, b.x1 - 4.0, 16.0) {
                    let y = b.y0 + 3.0 + rng.next() * (band - 5.0);
                    if rng.next() > 0.4 {
                        c.fill(&star((x, y), 3.0, 1.2, 5), 1.0);
                    } else {
                        c.disc((x, y), 1.0, 1.0);
                    }
                }
                Rect::new(b.x0 + pad, b.y0 + band + pad * 0.5, b.x1 - pad, base - th - pad * 0.5)
            }
            FrameStyle::FairyLights => {
                let amp = 2.5;
                let bulb = (b.h() * 0.07).clamp(3.0, 5.0);
                for (y, s) in [(b.y0 + amp + 1.0, 1.0f32), (b.y1 - amp - 1.0, -1.0)] {
                    let wire: Vec<P> = (0..=80).map(|i| {
                        let x = b.x0 + b.w() * i as f32 / 80.0;
                        (x, y + amp * (x / 9.0).sin())
                    }).collect();
                    c.stroke(&wire, false, 1.0, Pattern::Solid);
                    for (i, x) in spread(b.x0, b.x1, 14.0).into_iter().enumerate() {
                        let wy = y + amp * (x / 9.0).sin();
                        let cap = Rect::new(x - 1.5, wy.min(wy + s * 2.5), x + 1.5, wy.max(wy + s * 2.5));
                        c.fill_rect(cap, 1.0);
                        let e = ellipse((x, wy + s * (2.5 + bulb * 1.2)), bulb * 0.8, bulb * 1.25);
                        if i % 2 == 0 {
                            c.fill(&e, 1.0);
                        } else {
                            c.stroke(&e, true, 1.0, Pattern::Solid);
                        }
                    }
                }
                let ext = amp * 2.0 + 2.5 + bulb * 2.6 + 1.0;
                Rect::new(b.x0 + pad, b.y0 + ext + pad * 0.5, b.x1 - pad, b.y1 - ext - pad * 0.5)
            }
            FrameStyle::GiftRibbon => {
                ring(c, b, r.min(6.0), thin);
                let rw = (b.h() * 0.12).clamp(6.0, 11.0);
                let x = b.x0 + b.h() * 0.45;
                c.fill_rect(Rect::new(x - rw / 2.0, b.y0, x + rw / 2.0, b.y1), 1.0);
                c.fill_rect(Rect::new(x - rw / 2.0 + 1.5, b.y0, x + rw / 2.0 - 1.5, b.y1), 0.0);
                c.fill_rect(Rect::new(x - 0.5, b.y0, x + 0.5, b.y1), 1.0);
                // Bow: two loops and two tails around a knot.
                let ym = b.y0 + b.h() / 2.0;
                let lw = rw * 1.5;
                for s in [-1.0f32, 1.0] {
                    let loop_ = ellipse((x + s * lw * 0.9, ym - lw * 0.35), lw * 0.85, lw * 0.5);
                    c.fill(&loop_, 0.0);
                    c.stroke(&loop_, true, 2.0, Pattern::Solid);
                    c.line((x, ym), (x + s * lw * 0.8, ym + lw * 1.1), 2.5, 1.0);
                }
                c.disc((x, ym), rw * 0.45, 1.0);
                Rect::new(x + lw * 1.9 + pad, b.y0 + thin + pad, b.x1 - thin - pad, b.y1 - thin - pad)
            }

            // ------------------------------------------------------- storage
            FrameStyle::Crate => {
                let p = (b.h() * 0.12).clamp(6.0, 11.0);
                ring(c, b, 0.0, thin + 0.5);
                ring(c, b.inset(p, p), 0.0, thin);
                let end = b.h() * 0.55;
                for (x0, x1) in [(b.x0 + p, b.x0 + end), (b.x1 - end, b.x1 - p)] {
                    let inner = Rect::new(x0, b.y0 + p, x1, b.y1 - p);
                    c.fill_rect(Rect::new(inner.x0.max(x1 - thin), inner.y0, x1.max(inner.x0 + thin), inner.y1), 1.0);
                    c.fill_rect(Rect::new(x0, inner.y0, x0 + thin, inner.y1), 1.0);
                    c.line((x0 + 1.0, inner.y1 - 1.0), (x1 - 1.0, inner.y0 + 1.0), 2.5, 1.0);
                }
                // Nails in the plank corners.
                for (x, y) in [(b.x0 + p / 2.0, b.y0 + p / 2.0), (b.x1 - p / 2.0, b.y0 + p / 2.0), (b.x0 + p / 2.0, b.y1 - p / 2.0), (b.x1 - p / 2.0, b.y1 - p / 2.0)] {
                    c.disc((x, y), 1.4, 1.0);
                }
                Rect::new(b.x0 + end + pad, b.y0 + p + thin + pad, b.x1 - end - pad, b.y1 - p - thin - pad)
            }
            FrameStyle::TapeCorners => {
                let l = (b.h() * 0.36).clamp(18.0, 34.0);
                let w = (b.h() * 0.12).clamp(6.0, 11.0);
                let ang = 25.0f32.to_radians();
                // Half extents of a rotated strip, so the strips stay on the label.
                let ex = l / 2.0 * ang.cos() + w / 2.0 * ang.sin();
                let ey = l / 2.0 * ang.sin() + w / 2.0 * ang.cos();
                let o = b.inset(ex, ey);
                ring(c, o, 0.0, thin);
                for (cx, cy, a) in [(o.x0, o.y0, -ang), (o.x1, o.y0, ang), (o.x0, o.y1, ang), (o.x1, o.y1, -ang)] {
                    let (s, co) = a.sin_cos();
                    let rot = |u: f32, v: f32| (cx + u * co - v * s, cy + u * s + v * co);
                    // Torn ends: a small zigzag on both short edges.
                    let mut pts = vec![rot(-l / 2.0, -w / 2.0), rot(l / 2.0, -w / 2.0)];
                    for k in 1..4 {
                        let v = -w / 2.0 + w * k as f32 / 4.0;
                        pts.push(rot(l / 2.0 + if k % 2 == 1 { -1.5 } else { 0.0 }, v));
                    }
                    pts.push(rot(l / 2.0, w / 2.0));
                    pts.push(rot(-l / 2.0, w / 2.0));
                    for k in (1..4).rev() {
                        let v = -w / 2.0 + w * k as f32 / 4.0;
                        pts.push(rot(-l / 2.0 + if k % 2 == 1 { 1.5 } else { 0.0 }, v));
                    }
                    c.fill(&pts, 0.0);
                    c.stroke(&pts, true, 1.0, Pattern::Solid);
                    c.fill_with(&pts, |x, y| (x * 7 + y * 3) % 9 == 0);
                }
                o.inset(thin + pad + l * 0.15, thin + pad)
            }
            FrameStyle::Holder => {
                let thick = (t * 1.5).max(3.0);
                let rr = b.h() * 0.22;
                ring(c, b, rr, thick);
                ring(c, b.inset(thick + 2.0, thick + 2.0), (rr - thick - 2.0).max(0.0), 1.0);
                let ear = (b.h() * 0.13).clamp(5.0, 10.0);
                let ym = b.y0 + b.h() / 2.0;
                let zone = ear * 2.0 + thick + 6.0;
                for x in [b.x0 + thick + 3.0 + ear, b.x1 - thick - 3.0 - ear] {
                    c.disc((x, ym), ear, 1.0);
                    c.disc((x, ym), ear * 0.55, 0.0);
                    c.disc((x, ym), ear * 0.25, 1.0);
                }
                b.inset(zone + pad, thick + 3.0 + pad)
            }
            FrameStyle::Gingham => {
                let h = band;
                let cell = (h / 3.0).round().max(3.0) as usize;
                let outer = rounded_rect(b, 6.0);
                for (y0, y1) in [(b.y0, b.y0 + h), (b.y1 - h, b.y1)] {
                    c.fill_with(&rect_poly(Rect::new(b.x0, y0, b.x1, y1)), move |x, y| (x / cell + y / cell) % 2 == 0);
                }
                // Clip the checks to the rounded outline, then outline it.
                let mut clip = Canvas::new(c.width, c.height);
                clip.fill(&outer, 1.0);
                for y in 0..c.height {
                    for x in 0..c.width {
                        if clip.get(x, y) < 0.5 && (y as f32) < b.y1 + 1.0 && (y as f32) >= b.y0 - 1.0 && (x as f32) >= b.x0 - 1.0 && (x as f32) < b.x1 + 1.0 {
                            c.fill_rect(Rect::new(x as f32, y as f32, x as f32 + 1.0, y as f32 + 1.0), 0.0);
                        }
                    }
                }
                c.stroke(&outer, true, 1.2, Pattern::Solid);
                for y in [b.y0 + h, b.y1 - h] {
                    c.fill_rect(Rect::new(b.x0, y - 0.6, b.x1, y + 0.6), 1.0);
                }
                Rect::new(b.x0 + pad + 2.0, b.y0 + h + pad, b.x1 - pad - 2.0, b.y1 - h - pad)
            }
            FrameStyle::Carton => {
                ring(c, b, 0.0, t);
                let zone = (b.h() * 0.42).clamp(20.0, 38.0);
                let x = b.x1 - t - zone;
                c.fill_rect(Rect::new(x - thin, b.y0, x, b.y1), 1.0);
                let ym = b.y0 + b.h() / 2.0;
                let g = (b.h() - 2.0 * t) * 0.6;
                glyph(c, icons, '\u{f176}', (x + zone * 0.32, ym), g);
                glyph(c, icons, '\u{f176}', (x + zone * 0.68, ym), g);
                // Tape strip across the left end.
                let tx = b.x0 + t + zone * 0.35;
                c.fill_rect(Rect::new(tx, b.y0, tx + thin, b.y1), 1.0);
                c.fill_rect(Rect::new(tx + zone * 0.35, b.y0, tx + zone * 0.35 + thin, b.y1), 1.0);
                Rect::new(tx + zone * 0.35 + thin + pad, b.y0 + t + pad, x - thin - pad, b.y1 - t - pad)
            }
        }
    }
}

/// A row of glyph motifs along the top and bottom bands of `b`.
fn motif_bands(c: &mut Canvas, icons: &impl Font, b: Rect, band: f32, chars: &[char], alternate_size: bool) {
    for (y, phase) in [(b.y0 + band / 2.0, 0usize), (b.y1 - band / 2.0, 1)] {
        for (i, x) in spread(b.x0, b.x1, band * 1.5).into_iter().enumerate() {
            let k = i + phase;
            let ch = chars[k % chars.len()];
            let size = if alternate_size && k % 2 == 1 { band * 0.6 } else { band * 0.95 };
            let size = if ch == '\u{f111}' { band * 0.35 } else { size };
            glyph(c, icons, ch, (x, y), size);
        }
    }
}

/// The inner outline of a wave ring: same wave, moved inward by `t`.
fn wave_inset(b: Rect, amp: f32, period: f32, t: f32) -> Vec<P> {
    let waves = (b.w() / period).round().max(1.0);
    let n = (waves * 16.0) as usize;
    let tau = std::f32::consts::TAU;
    let x = |i: usize| b.x0 + b.w() * i as f32 / n as f32;
    let w = |i: usize| amp * (1.0 - (i as f32 / n as f32 * waves * tau).cos()) / 2.0;
    let (x0, x1) = (b.x0 + t, b.x1 - t);
    let mut v: Vec<P> = (0..=n).map(|i| (x(i).clamp(x0, x1), b.y0 + t + w(i))).collect();
    v.extend((0..=n).rev().map(|i| (x(i).clamp(x0, x1), b.y1 - t - w(i))));
    v
}

/// Arcs bulging outward by `s` along every edge of `r`.
fn scallop_path(r: Rect, s: f32) -> Vec<P> {
    let mut pts = Vec::new();
    let corners = [(r.x0, r.y0), (r.x1, r.y0), (r.x1, r.y1), (r.x0, r.y1)];
    for i in 0..4 {
        let (a, z) = (corners[i], corners[(i + 1) % 4]);
        let len = (z.0 - a.0).hypot(z.1 - a.1);
        let n = (len / (s * 2.6)).round().max(1.0) as usize;
        let (dx, dy) = ((z.0 - a.0) / len, (z.1 - a.1) / len);
        let (nx, ny) = (dy, -dx); // outward normal for a clockwise outline
        for k in 0..n {
            for j in 0..=8 {
                let f = (k as f32 + j as f32 / 8.0) / n as f32;
                let bulge = (j as f32 / 8.0 * std::f32::consts::PI).sin() * s;
                pts.push((a.0 + dx * len * f + nx * bulge, a.1 + dy * len * f + ny * bulge));
            }
        }
    }
    pts
}

/// Teeth of height `a` outward along every edge of `r`.
fn zigzag_path(r: Rect, a: f32) -> Vec<P> {
    let mut pts = Vec::new();
    let corners = [(r.x0, r.y0), (r.x1, r.y0), (r.x1, r.y1), (r.x0, r.y1)];
    for i in 0..4 {
        let (p, q) = (corners[i], corners[(i + 1) % 4]);
        let len = (q.0 - p.0).hypot(q.1 - p.1);
        let n = (len / (a * 2.0)).round().max(1.0) as usize;
        let (dx, dy) = ((q.0 - p.0) / len, (q.1 - p.1) / len);
        let (nx, ny) = (dy, -dx);
        for k in 0..n {
            let f0 = k as f32 / n as f32;
            let fm = (k as f32 + 0.5) / n as f32;
            pts.push((p.0 + dx * len * f0, p.1 + dy * len * f0));
            pts.push((p.0 + dx * len * fm + nx * a, p.1 + dy * len * fm + ny * a));
        }
    }
    pts
}

/// A rectangle with two-step notched corners (Art déco).
fn stepped(r: Rect, k: f32) -> Vec<P> {
    let (x0, y0, x1, y1) = (r.x0, r.y0, r.x1, r.y1);
    vec![
        (x0 + 2.0 * k, y0), (x1 - 2.0 * k, y0), (x1 - 2.0 * k, y0 + k), (x1 - k, y0 + k), (x1 - k, y0 + 2.0 * k), (x1, y0 + 2.0 * k),
        (x1, y1 - 2.0 * k), (x1 - k, y1 - 2.0 * k), (x1 - k, y1 - k), (x1 - 2.0 * k, y1 - k), (x1 - 2.0 * k, y1),
        (x0 + 2.0 * k, y1), (x0 + 2.0 * k, y1 - k), (x0 + k, y1 - k), (x0 + k, y1 - 2.0 * k), (x0, y1 - 2.0 * k),
        (x0, y0 + 2.0 * k), (x0 + k, y0 + 2.0 * k), (x0 + k, y0 + k), (x0 + 2.0 * k, y0 + k),
    ]
}

/// A banner with V-notched (fishtail) ends of depth `n`.
fn fishtail(r: Rect, n: f32) -> Vec<P> {
    let ym = (r.y0 + r.y1) / 2.0;
    vec![(r.x0, r.y0), (r.x1, r.y0), (r.x1 - n, ym), (r.x1, r.y1), (r.x0, r.y1), (r.x0 + n, ym)]
}
