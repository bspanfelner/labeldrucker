//! Rasterise text into an 8-bit grayscale bitmap for [`Page::from_gray`].
//!
//! Fonts come from the platform (system fonts, a bundled file, …) as raw
//! TTF/OTF bytes; layout is deliberately simple: lines centred horizontally,
//! the block centred vertically.
//!
//! [`Page::from_gray`]: crate::Page::from_gray

pub use ab_glyph::{FontVec, VariableFont};
use ab_glyph::{Font, PxScale, ScaleFont, point};

use crate::{Error, Result};

/// Line height as a multiple of the font's ascent − descent.
const LINE_GAP: f32 = 1.05;

/// Load a font from TTF/OTF/TTC bytes (`index` selects a face in a collection).
pub fn load_font(data: Vec<u8>, index: u32) -> Result<FontVec> {
    FontVec::try_from_vec_and_index(data, index).map_err(|e| Error::Encode(format!("font: {e}")))
}

/// How text is placed on the bitmap.
#[derive(Debug, Clone, Copy)]
pub struct TextLayout {
    /// Font size in pixels (ascent − descent); `None` picks the largest size
    /// that fits inside `width`/`height` minus `margin`.
    pub size_px: Option<f32>,
    /// Blank border in pixels on every side.
    pub margin: f32,
}

impl Default for TextLayout {
    fn default() -> Self {
        Self { size_px: None, margin: 4.0 }
    }
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

/// The size that makes `text` fill `width` × `height` minus margins.
pub fn fit_size(font: &impl Font, text: &str, width: usize, height: usize, margin: f32) -> f32 {
    const PROBE: f32 = 100.0;
    let lines: Vec<&str> = text.lines().collect();
    let n = lines.len().max(1) as f32;
    let widest = lines.iter().map(|l| line_width(font, PxScale::from(PROBE), l)).fold(0.0, f32::max);
    let by_h = (height as f32 - 2.0 * margin) / (n * LINE_GAP - (LINE_GAP - 1.0));
    let by_w = if widest > 0.0 { (width as f32 - 2.0 * margin) * PROBE / widest } else { by_h };
    by_h.min(by_w).max(1.0)
}

/// Render `text` (lines separated by `\n`) black on white into a
/// `width` × `height` grayscale bitmap (row-major, 0 = black).
pub fn render(font: &impl Font, text: &str, width: usize, height: usize, layout: TextLayout) -> Vec<u8> {
    let mut ink = vec![0f32; width * height];
    let px = layout.size_px.unwrap_or_else(|| fit_size(font, text, width, height, layout.margin));
    let scale = PxScale::from(px);
    let f = font.as_scaled(scale);
    let line_h = (f.ascent() - f.descent()) * LINE_GAP;
    let lines: Vec<&str> = text.lines().collect();
    let block_h = line_h * lines.len() as f32 - (line_h - (f.ascent() - f.descent()));
    let mut baseline = (height as f32 - block_h) / 2.0 + f.ascent();

    for line in &lines {
        let mut x = (width as f32 - line_width(font, scale, line)) / 2.0;
        let mut prev = None;
        for c in line.chars() {
            let id = f.glyph_id(c);
            if let Some(p) = prev {
                x += f.kern(p, id);
            }
            let glyph = id.with_scale_and_position(scale, point(x, baseline));
            if let Some(g) = font.outline_glyph(glyph) {
                let b = g.px_bounds();
                g.draw(|gx, gy, cov| {
                    let (px, py) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                    if (0..width as i32).contains(&px) && (0..height as i32).contains(&py) {
                        let i = py as usize * width + px as usize;
                        ink[i] = (ink[i] + cov).min(1.0);
                    }
                });
            }
            x += f.h_advance(id);
            prev = Some(id);
        }
        baseline += line_h;
    }
    ink.iter().map(|&a| ((1.0 - a) * 255.0).round() as u8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONT: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf";

    #[test]
    fn renders_centred_ink() {
        let Ok(data) = std::fs::read(FONT) else { return }; // no system font here
        let font = load_font(data, 0).unwrap();
        let (w, h) = (320, 96);
        let g = render(&font, "Hallo\nE16", w, h, TextLayout::default());
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
