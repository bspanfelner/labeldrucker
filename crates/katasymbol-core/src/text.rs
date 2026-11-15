//! Rasterise text onto a [`Canvas`].
//!
//! Fonts come from the platform (system fonts, a bundled file, …) as raw
//! TTF/OTF bytes. Layout is simple: lines stacked and centred vertically in a
//! rectangle, each aligned left, centred or right.

pub use ab_glyph::{Font, FontRef, FontVec, VariableFont};
use ab_glyph::{PxScale, ScaleFont, point};

use crate::draw::{Canvas, Rect};
use crate::page::HEAD_DOTS;
use crate::{Error, Result};

/// Line height as a multiple of the font's ascent − descent.
const LINE_GAP: f32 = 1.05;

/// Load a font from TTF/OTF/TTC bytes (`index` selects a face in a collection).
pub fn load_font(data: Vec<u8>, index: u32) -> Result<FontVec> {
    FontVec::try_from_vec_and_index(data, index).map_err(|e| Error::Encode(format!("font: {e}")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Align {
    Left,
    #[default]
    Center,
    Right,
}

fn line_width(font: &impl Font, scale: PxScale, line: &str) -> f32 {
    let f = font.as_scaled(scale);
    let mut w = 0.0;
    let mut prev = None;
    for c in line.chars() {
        let id = f.glyph_id(c);
        if let Some(p) = prev {
            w += f.kern(p, id);
        }
        w += f.h_advance(id);
        prev = Some(id);
    }
    w
}

fn line_count(text: &str) -> usize {
    text.lines().count().max(1)
}

/// Height of a block of `n` lines at size `px`, relative to `px`.
fn block_factor(n: usize) -> f32 {
    n as f32 * LINE_GAP - (LINE_GAP - 1.0)
}

/// Width and height of `text` at size `px`.
pub fn measure(font: &impl Font, text: &str, px: f32) -> (f32, f32) {
    let scale = PxScale::from(px);
    let w = text.lines().map(|l| line_width(font, scale, l)).fold(0.0, f32::max);
    (w, px * block_factor(line_count(text)))
}

/// The largest size at which `text` fits into `w` × `h`.
pub fn fit_size(font: &impl Font, text: &str, w: f32, h: f32) -> f32 {
    const PROBE: f32 = 100.0;
    let (tw, _) = measure(font, text, PROBE);
    let by_h = h / block_factor(line_count(text));
    let by_w = if tw > 0.0 { w * PROBE / tw } else { by_h };
    by_h.min(by_w).max(1.0)
}

/// The size used for `text` in `r`: `size_px`, or fitted when `None`.
pub fn size_for(font: &impl Font, text: &str, r: Rect, size_px: Option<f32>) -> f32 {
    size_px.unwrap_or_else(|| fit_size(font, text, r.w(), r.h()))
}

/// Draw `text` (lines separated by `\n`) into `r` on `canvas`.
pub fn draw(canvas: &mut Canvas, font: &impl Font, text: &str, r: Rect, size_px: Option<f32>, align: Align) {
    let px = size_for(font, text, r, size_px);
    let scale = PxScale::from(px);
    let f = font.as_scaled(scale);
    let height = f.ascent() - f.descent();
    let line_h = height * LINE_GAP;
    let n = line_count(text);
    let block_h = line_h * n as f32 - (line_h - height);
    let mut baseline = r.y0 + (r.h() - block_h) / 2.0 + f.ascent();

    for line in text.lines() {
        let lw = line_width(font, scale, line);
        let mut x = match align {
            Align::Left => r.x0,
            Align::Center => r.x0 + (r.w() - lw) / 2.0,
            Align::Right => r.x1 - lw,
        };
        let mut prev = None;
        for c in line.chars() {
            let id = f.glyph_id(c);
            if let Some(p) = prev {
                x += f.kern(p, id);
            }
            let glyph = id.with_scale_and_position(scale, point(x, baseline));
            if let Some(g) = font.outline_glyph(glyph) {
                let b = g.px_bounds();
                g.draw(|gx, gy, cov| canvas.add(b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32, cov));
            }
            x += f.h_advance(id);
            prev = Some(id);
        }
        baseline += line_h;
    }
}

/// Draw one glyph (e.g. an icon) as large as fits into `r`, centred.
pub fn draw_glyph(canvas: &mut Canvas, font: &impl Font, c: char, r: Rect) {
    let id = font.glyph_id(c);
    // Measure the outline at a probe size, then scale to fit the box.
    let probe = id.with_scale_and_position(100.0, point(0.0, 0.0));
    let Some(g) = font.outline_glyph(probe) else { return };
    let b = g.px_bounds();
    let s = (r.w() / b.width()).min(r.h() / b.height()) * 100.0;
    let scale = PxScale::from(s);
    let k = s / 100.0;
    let (w, h) = (b.width() * k, b.height() * k);
    let pos = point(r.x0 + (r.w() - w) / 2.0 - b.min.x * k, r.y0 + (r.h() - h) / 2.0 - b.min.y * k);
    if let Some(g) = font.outline_glyph(id.with_scale_and_position(scale, pos)) {
        let b = g.px_bounds();
        g.draw(|gx, gy, cov| canvas.add(b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32, cov));
    }
}

/// Render `text` black on white into a `width` × `height` grayscale bitmap
/// (row-major, 0 = black), centred with a 4-dot margin.
pub fn render(font: &impl Font, text: &str, width: usize, height: usize, size_px: Option<f32>) -> Vec<u8> {
    let mut c = Canvas::new(width, height);
    let r = Rect::new(0.0, 0.0, width as f32, height as f32).inset(4.0, 4.0);
    draw(&mut c, font, text, r, size_px, Align::Center);
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| ((1.0 - c.get(x, y)) * 255.0).round() as u8)
        .collect()
}

/// Default height for [`render`]: the print head.
pub const DEFAULT_HEIGHT: usize = HEAD_DOTS;

#[cfg(test)]
mod tests {
    use super::*;

    const FONT: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf";

    #[test]
    fn renders_centred_ink() {
        let Ok(data) = std::fs::read(FONT) else { return }; // no system font here
        let font = load_font(data, 0).unwrap();
        let (w, h) = (320, 96);
        let g = render(&font, "Hallo\nE16", w, h, None);
        assert_eq!(g.len(), w * h);
        let dark: Vec<(usize, usize)> =
            g.iter().enumerate().filter(|(_, v)| **v < 128).map(|(i, _)| (i % w, i / w)).collect();
        assert!(dark.len() > 500);
        let (x0, x1) = (dark.iter().map(|p| p.0).min().unwrap(), dark.iter().map(|p| p.0).max().unwrap());
        let (y0, y1) = (dark.iter().map(|p| p.1).min().unwrap(), dark.iter().map(|p| p.1).max().unwrap());
        // Fitted to the height, inside the margin, roughly centred.
        assert!(y0 >= 2 && y1 <= h - 2, "{y0}..{y1}");
        assert!(y1 - y0 > 70, "{y0}..{y1}");
        assert!((x0 as i32 - (w - 1 - x1) as i32).abs() < 6, "{x0}..{x1}");
    }
}
