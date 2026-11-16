//! A label design — text or image, pictograms, frame — rendered to a [`Page`].
//!
//! Layout across the head (top to bottom) is the 96-dot printable band; along
//! the tape the label is `length` dots long. Inside the frame the content row
//! is: `[icon] [separator] text-or-image [separator] [icon]`.

use crate::Page;
use crate::draw::{Canvas, Rect};
pub use crate::frames::{Category, Frame, FrameStyle};
use crate::page::{DOTS_PER_MM, HEAD_DOTS};
use crate::text::{self, Align, Font};

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
    /// Blank space at the start and end of the label (the cut edges), dots.
    pub end_margin: f32,
}

impl Default for Label {
    fn default() -> Self {
        Self {
            length: 40 * DOTS_PER_MM,
            text: String::new(),
            align: Align::Center,
            size_px: None,
            icon_left: None,
            icon_right: None,
            icon_scale: 0.9,
            separator: false,
            frame: Frame::default(),
            invert: false,
            end_margin: DEFAULT_END_MARGIN,
        }
    }
}

/// Default blank space at each cut edge: 2 mm.
pub const DEFAULT_END_MARGIN: f32 = 2.0 * DOTS_PER_MM as f32;

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
        // Wide pictograms get their width (at most half the content each),
        // a narrower box then shrinks them in height.
        let size = |ch: char| {
            let a = crate::icons::aspect(ch).max(0.1);
            let w = (side * a).min(content.w() / 2.0);
            (w, w / a)
        };
        if let Some(ch) = self.icon_left {
            let (w, h) = size(ch);
            let y0 = content.y0 + (content.h() - h) / 2.0;
            icons[0] = Some((ch, Rect::new(content.x0, y0, content.x0 + w, y0 + h)));
            body.x0 += w + gap;
            if self.separator {
                let x = content.x0 + w + gap / 2.0 - sep_w / 2.0;
                separators.push(Rect::new(x, content.y0, x + sep_w, content.y1));
            }
        }
        if let Some(ch) = self.icon_right {
            let (w, h) = size(ch);
            let y0 = content.y0 + (content.h() - h) / 2.0;
            icons[1] = Some((ch, Rect::new(content.x1 - w, y0, content.x1, y0 + h)));
            body.x1 -= w + gap;
            if self.separator {
                let x = content.x1 - w - gap / 2.0 - sep_w / 2.0;
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

    /// The part of a label `width` dots long inside the end margins.
    fn area(&self, width: usize) -> Rect {
        let m = self.end_margin.clamp(0.0, width as f32 / 4.0);
        Rect::new(m, 0.0, width as f32 - m, HEAD_DOTS as f32)
    }

    pub fn canvas(&self, font: &impl Font, icons: &impl Font, picture: Option<&Picture>) -> Canvas {
        let mut c = Canvas::for_page(self.length.max(8));
        let area = self.area(c.width);
        let content = self.frame.draw(&mut c, area, icons);
        let l = self.layout(content);
        for (ch, r) in l.icons.iter().flatten() {
            crate::icons::draw(&mut c, icons, *ch, *r);
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
    pub fn natural_length(&self, font: &impl Font, icons: &impl Font, picture: Option<&Picture>) -> usize {
        // The frame's horizontal overhead does not depend on the length.
        const PROBE: usize = 2000;
        let mut probe = Canvas::for_page(PROBE);
        let area = Rect::new(self.end_margin, 0.0, PROBE as f32 - self.end_margin, HEAD_DOTS as f32);
        let content = self.frame.draw(&mut probe, area, icons);
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
        for &style in FrameStyle::ALL {
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
                // The frame keeps its margins: outermost rows and the end margins stay blank.
                assert!((0..c.width).all(|x| c.get(x, 0) < 0.5), "{style:?} touches the top edge");
                let m = DEFAULT_END_MARGIN as usize;
                for x in (0..m).chain(c.width - m..c.width) {
                    assert!((0..c.height).all(|y| c.get(x, y) < 0.5), "{style:?} draws into the end margin at x={x}");
                }
            }
        }
    }

    #[test]
    fn natural_length_grows_with_text_and_icons() {
        let Some(f) = font() else { return };
        let short = Label { text: "Hi".into(), ..Label::default() };
        let long = Label { text: "Hallo Welt".into(), ..Label::default() };
        let with_icon = Label { icon_left: Some('\u{f015}'), ..long.clone() };
        let i = icons::font();
        let (a, b, c) = (short.natural_length(&f, &i, None), long.natural_length(&f, &i, None), with_icon.natural_length(&f, &i, None));
        assert!(a < b && b < c, "{a} {b} {c}");
        // The fitted text must then fit without shrinking much.
        let fitted = Label { length: b, ..long };
        let page = fitted.render(&f, &icons::font(), None);
        let first = (0..page.width()).find(|&x| (0..HEAD_DOTS).any(|y| page.get(x, y))).unwrap();
        assert!(first < DEFAULT_END_MARGIN as usize + 16, "text starts at {first}");
        assert!(first >= DEFAULT_END_MARGIN as usize, "text inside the end margin: {first}");
    }
}
