//! A label design — text or image, pictograms, frame — rendered to a [`Page`].
//!
//! Layout across the head (top to bottom) is the 96-dot printable band; along
//! the tape the label is `length` dots long. Inside the frame the content row
//! is: `[icon] [separator] text-or-image [separator] [icon]`.

use crate::Page;
use crate::draw::{Canvas, P, Pattern, Rect, chamfer_rect, rounded_rect, ticket, wave_rect};
use crate::page::HEAD_DOTS;
use crate::text::{self, Align, Font};

/// Frame styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FrameStyle {
    #[default]
    None,
    Line,
    Double,
    ThickThin,
    Dashed,
    Dotted,
    Corners,
    Brackets,
    Chamfer,
    Capsule,
    Ticket,
    Wave,
    Shadow,
}

impl FrameStyle {
    pub const ALL: [FrameStyle; 13] = [
        Self::None,
        Self::Line,
        Self::Double,
        Self::ThickThin,
        Self::Dashed,
        Self::Dotted,
        Self::Corners,
        Self::Brackets,
        Self::Chamfer,
        Self::Capsule,
        Self::Ticket,
        Self::Wave,
        Self::Shadow,
    ];

    /// German display name.
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "Ohne",
            Self::Line => "Linie",
            Self::Double => "Doppelt",
            Self::ThickThin => "Dick-dünn",
            Self::Dashed => "Gestrichelt",
            Self::Dotted => "Gepunktet",
            Self::Corners => "Ecken",
            Self::Brackets => "Klammern",
            Self::Chamfer => "Abgeschrägt",
            Self::Capsule => "Kapsel",
            Self::Ticket => "Ticket",
            Self::Wave => "Welle",
            Self::Shadow => "Schatten",
        }
    }

    /// Whether the corner radius setting applies.
    pub fn uses_radius(self) -> bool {
        matches!(self, Self::Line | Self::Double | Self::ThickThin | Self::Dashed | Self::Dotted | Self::Shadow | Self::Chamfer | Self::Ticket)
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

impl Frame {
    /// Draw the frame on `c` and return the rectangle left for content.
    pub fn draw(&self, c: &mut Canvas) -> Rect {
        let page = Rect::new(0.0, 0.0, c.width as f32, c.height as f32);
        let t = self.thickness.max(1.0);
        let b = page.inset(self.margin, self.margin);
        let r = self.radius;
        let pad = self.padding;
        let gap = t.max(2.0);
        // Content inset from `b` by `e` on every side, plus `extra_x` left/right.
        let content = |e: f32, extra_x: f32| b.inset(e + pad + extra_x, e + pad);
        let ring = |c: &mut Canvas, o: Rect, r: f32, t: f32| c.ring(&rounded_rect(o, r), &rounded_rect(o.inset(t, t), (r - t).max(0.0)));
        // Rounded corners need some room so text does not touch the curve.
        let corner_room = |r: f32| r * 0.3;

        match self.style {
            FrameStyle::None => page.inset(self.margin.min(4.0) + pad / 2.0, self.margin.min(4.0)),
            FrameStyle::Line => {
                ring(c, b, r, t);
                content(t, corner_room(r))
            }
            FrameStyle::Double => {
                ring(c, b, r, t);
                let inner = b.inset(t + gap, t + gap);
                ring(c, inner, (r - t - gap).max(0.0), t);
                content(2.0 * t + gap, corner_room(r))
            }
            FrameStyle::ThickThin => {
                let thick = (t * 2.0).max(3.0);
                let thin = (t / 2.0).round().max(1.0);
                ring(c, b, r, thick);
                let inner = b.inset(thick + gap, thick + gap);
                ring(c, inner, (r - thick - gap).max(0.0), thin);
                content(thick + gap + thin, corner_room(r))
            }
            FrameStyle::Dashed | FrameStyle::Dotted => {
                let mid = b.inset(t / 2.0, t / 2.0);
                let path = rounded_rect(mid, (r - t / 2.0).max(0.0));
                let pattern = if self.style == FrameStyle::Dashed {
                    Pattern::Dash { on: 3.0 * t + 3.0, off: 2.0 * t + 2.0 }
                } else {
                    Pattern::Dots { spacing: t * 2.2 + 1.5 }
                };
                if self.style == FrameStyle::Dashed {
                    // Square caps look crisper for dashes: draw with round caps
                    // but slightly thinner so they stay within the frame band.
                    c.stroke(&path, true, t, pattern);
                } else {
                    c.stroke(&path, true, t + 0.8, pattern);
                }
                content(t, corner_room(r))
            }
            FrameStyle::Corners => {
                let len = (b.h().min(b.w()) / 3.0).max(t * 3.0);
                for (x, y, sx, sy) in [(b.x0, b.y0, 1.0, 1.0), (b.x1, b.y0, -1.0, 1.0), (b.x0, b.y1, 1.0, -1.0), (b.x1, b.y1, -1.0, -1.0)] {
                    let rx = |a: f32, l: f32| if sx > 0.0 { (a, a + l) } else { (a - l, a) };
                    let ry = |a: f32, l: f32| if sy > 0.0 { (a, a + l) } else { (a - l, a) };
                    let (hx0, hx1) = rx(x, len);
                    let (hy0, hy1) = ry(y, t);
                    c.fill_rect(Rect::new(hx0, hy0, hx1, hy1), 1.0);
                    let (vx0, vx1) = rx(x, t);
                    let (vy0, vy1) = ry(y, len);
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
                let outer = wave_rect(b, amp, period);
                let inner = wave_rect_inset(b, amp, period, t);
                c.ring(&outer, &inner);
                b.inset(t + pad, amp + t + pad)
            }
            FrameStyle::Shadow => {
                let s = (t + 2.0).max(3.0);
                let main = Rect::new(b.x0, b.y0, b.x1 - s, b.y1 - s);
                c.fill(&rounded_rect(Rect::new(b.x0 + s, b.y0 + s, b.x1, b.y1), r), 1.0);
                c.fill(&rounded_rect(main, r), 0.0);
                ring(c, main, r, t);
                Rect::new(main.x0 + t + pad + corner_room(r), main.y0 + t + pad, main.x1 - t - pad - corner_room(r), main.y1 - t - pad)
            }
        }
    }
}

/// The inner outline of a wave ring: same wave, moved inward by `t`.
fn wave_rect_inset(b: Rect, amp: f32, period: f32, t: f32) -> Vec<P> {
    // Keep the wave phase of the outer outline: same x range per period.
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

/// A grayscale picture as the label's content instead of text.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub width: usize,
    pub height: usize,
    /// Row-major, 0 = black.
    pub gray: Vec<u8>,
    /// Darker than this prints.
    pub threshold: u8,
}

/// Everything that makes up a label.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct Label {
    /// Length along the tape, in dots.
    pub length: usize,
    pub text: String,
    pub align: Align,
    /// Fixed font size in dots; `None` fits the text to the space.
    pub size_px: Option<f32>,
    pub icon_left: Option<char>,
    pub icon_right: Option<char>,
    /// Icon size as a fraction of the content height.
    pub icon_scale: f32,
    /// A thin line between icons and text.
    pub separator: bool,
    pub frame: Frame,
    pub invert: bool,
}

impl Default for Label {
    fn default() -> Self {
        Self {
            length: 40 * crate::page::DOTS_PER_MM,
            text: String::new(),
            align: Align::Center,
            size_px: None,
            icon_left: None,
            icon_right: None,
            icon_scale: 0.9,
            separator: false,
            frame: Frame::default(),
            invert: false,
        }
    }
}

/// Where things go, for a given length.
struct Layout {
    icons: [Option<(char, Rect)>; 2],
    separators: Vec<Rect>,
    body: Rect,
}

impl Label {
    fn icon_gap(&self) -> f32 {
        self.frame.padding.max(4.0) * if self.separator { 2.0 } else { 1.0 }
    }

    fn layout(&self, content: Rect) -> Layout {
        let side = (content.h() * self.icon_scale).min(content.w() / 2.0).max(0.0);
        let gap = self.icon_gap();
        let sep_w = self.frame.thickness.clamp(1.0, 3.0);
        let mut body = content;
        let mut icons = [None, None];
        let mut separators = Vec::new();
        let y0 = content.y0 + (content.h() - side) / 2.0;
        if let Some(ch) = self.icon_left {
            icons[0] = Some((ch, Rect::new(content.x0, y0, content.x0 + side, y0 + side)));
            body.x0 += side + gap;
            if self.separator {
                let x = content.x0 + side + gap / 2.0 - sep_w / 2.0;
                separators.push(Rect::new(x, content.y0, x + sep_w, content.y1));
            }
        }
        if let Some(ch) = self.icon_right {
            icons[1] = Some((ch, Rect::new(content.x1 - side, y0, content.x1, y0 + side)));
            body.x1 -= side + gap;
            if self.separator {
                let x = content.x1 - side - gap / 2.0 - sep_w / 2.0;
                separators.push(Rect::new(x, content.y0, x + sep_w, content.y1));
            }
        }
        Layout { icons, separators, body }
    }

    /// Render to a printable page. `font` sets the text, `icons` the
    /// pictograms ([`crate::icons::font`]); `picture` replaces the text.
    pub fn render(&self, font: &impl Font, icons: &impl Font, picture: Option<&Picture>) -> Page {
        self.canvas(font, icons, picture).to_page()
    }

    pub fn canvas(&self, font: &impl Font, icons: &impl Font, picture: Option<&Picture>) -> Canvas {
        let mut c = Canvas::for_page(self.length.max(8));
        let content = self.frame.draw(&mut c);
        let l = self.layout(content);
        for (ch, r) in l.icons.iter().flatten() {
            text::draw_glyph(&mut c, icons, *ch, *r);
        }
        for s in &l.separators {
            c.fill_rect(*s, 1.0);
        }
        match picture {
            Some(p) => {
                let align = match self.align {
                    Align::Left => -1,
                    Align::Center => 0,
                    Align::Right => 1,
                };
                c.image(&p.gray, p.width, p.height, l.body, align, p.threshold);
            }
            None if !self.text.trim().is_empty() => {
                text::draw(&mut c, font, &self.text, l.body, self.size_px, self.align);
            }
            None => {}
        }
        if self.invert {
            c.invert();
        }
        c
    }

    /// The length (in dots) at which the content fits snugly: text at its
    /// set size (or as large as the height allows), plus icons and frame.
    pub fn natural_length(&self, font: &impl Font, picture: Option<&Picture>) -> usize {
        // The frame's horizontal overhead does not depend on the length.
        const PROBE: usize = 2000;
        let mut probe = Canvas::for_page(PROBE);
        let content = self.frame.draw(&mut probe);
        let l = self.layout(content);
        let overhead = PROBE as f32 - l.body.w();
        let body = match picture {
            Some(p) if p.height > 0 => p.width as f32 * l.body.h() / p.height as f32,
            Some(_) => 0.0,
            None if self.text.trim().is_empty() => 0.0,
            None => {
                let size = self.size_px.unwrap_or_else(|| text::fit_size(font, &self.text, f32::MAX / 4.0, l.body.h()));
                text::measure(font, &self.text, size).0
            }
        };
        (overhead + body + 2.0).ceil().max(HEAD_DOTS as f32 / 2.0) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons;

    fn font() -> Option<crate::text::FontVec> {
        let data = std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf").ok()?;
        crate::text::load_font(data, 0).ok()
    }

    #[test]
    fn every_frame_style_renders_inside_the_page() {
        let icon_font = icons::font();
        for style in FrameStyle::ALL {
            let label = Label {
                length: 320,
                frame: Frame { style, ..Frame::default() },
                icon_left: Some('\u{f015}'),
                ..Label::default()
            };
            let c = label.canvas(&icon_font, &icon_font, None);
            let inked = (0..c.height).flat_map(|y| (0..c.width).map(move |x| (x, y))).filter(|&(x, y)| c.get(x, y) >= 0.5).count();
            assert!(inked > 100, "{style:?} drew only {inked} dots");
            if style != FrameStyle::None {
                // The frame keeps its margin: the outermost rows/columns stay blank.
                assert!((0..c.width).all(|x| c.get(x, 0) < 0.5), "{style:?} touches the top edge");
                assert!((0..c.height).all(|y| c.get(0, y) < 0.5), "{style:?} touches the left edge");
            }
        }
    }

    #[test]
    fn natural_length_grows_with_text_and_icons() {
        let Some(f) = font() else { return };
        let short = Label { text: "Hi".into(), ..Label::default() };
        let long = Label { text: "Hallo Welt".into(), ..Label::default() };
        let with_icon = Label { icon_left: Some('\u{f015}'), ..long.clone() };
        let (a, b, c) = (short.natural_length(&f, None), long.natural_length(&f, None), with_icon.natural_length(&f, None));
        assert!(a < b && b < c, "{a} {b} {c}");
        // The fitted text must then fit without shrinking much.
        let fitted = Label { length: b, ..long };
        let page = fitted.render(&f, &icons::font(), None);
        let first = (0..page.width()).find(|&x| (0..HEAD_DOTS).any(|y| page.get(x, y))).unwrap();
        assert!(first < 16, "text starts at {first}");
    }
}
