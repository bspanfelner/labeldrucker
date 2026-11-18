//! A label page in the printhead's native layout.
//!
//! Coordinates: `x` runs along the label (feed direction), `y` runs across
//! the printed band, `y = 0` being the label's top edge. The E16 prints a
//! 96-dot band at 8 dots per mm; other printers (Brother PT) use other
//! heights, up to [`MAX_DOTS`].

/// E16 printhead height in dots (12 mm at 8 dots/mm).
pub const HEAD_DOTS: usize = 96;
/// E16 bytes per printed column.
pub const BYTES_PER_COL: usize = HEAD_DOTS / 8;
/// E16 dots per millimetre in both directions.
pub const DOTS_PER_MM: usize = 8;
/// The tallest band a page can hold (Brother's 128-pin head).
pub const MAX_DOTS: usize = 128;

/// One printed column: bit `y & 7` of byte `y >> 3` set means a black dot.
pub type Column = [u8; MAX_DOTS / 8];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    columns: Vec<Column>,
    height: usize,
}

impl Page {
    /// A blank E16 page `length_mm` long.
    pub fn blank(length_mm: usize) -> Self {
        Self::blank_dots(length_mm * DOTS_PER_MM)
    }

    /// A blank E16 page `width` dots long.
    pub fn blank_dots(width: usize) -> Self {
        Self::blank_sized(width, HEAD_DOTS)
    }

    /// A blank page `width` dots long and `height` (at most [`MAX_DOTS`]) high.
    pub fn blank_sized(width: usize, height: usize) -> Self {
        Self { columns: vec![[0; MAX_DOTS / 8]; width], height: height.min(MAX_DOTS) }
    }

    pub fn width(&self) -> usize {
        self.columns.len()
    }

    /// Dots across the band.
    pub fn height(&self) -> usize {
        self.height
    }

    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    pub fn set(&mut self, x: usize, y: usize) {
        if x < self.columns.len() && y < self.height {
            self.columns[x][y >> 3] |= 1 << (y & 7);
        }
    }

    pub fn get(&self, x: usize, y: usize) -> bool {
        x < self.columns.len() && y < self.height && self.columns[x][y >> 3] & (1 << (y & 7)) != 0
    }

    /// Fill a rectangle (inclusive-exclusive bounds).
    pub fn fill_rect(&mut self, x0: usize, y0: usize, x1: usize, y1: usize) {
        for x in x0..x1 {
            for y in y0..y1 {
                self.set(x, y);
            }
        }
    }

    /// Build a page from an 8-bit grayscale image (row-major, `width * height`
    /// bytes), thresholding at `threshold` (darker than it prints). The image is
    /// placed at the left edge and centred across the head; anything taller
    /// than [`HEAD_DOTS`] is cropped.
    pub fn from_gray(width: usize, height: usize, pixels: &[u8], threshold: u8) -> Self {
        let mut page = Self::blank_dots(width);
        let top = HEAD_DOTS.saturating_sub(height) / 2;
        let skip = height.saturating_sub(HEAD_DOTS) / 2;
        for y in 0..height.min(HEAD_DOTS) {
            let row = &pixels[(y + skip) * width..(y + skip + 1) * width];
            for (x, &v) in row.iter().enumerate() {
                if v < threshold {
                    page.set(x, y + top);
                }
            }
        }
        page
    }

    /// The page turned by 180° (for printers fed the other way round).
    pub fn rotated_180(&self) -> Self {
        let (w, h) = (self.width(), self.height);
        let mut p = Self::blank_sized(w, h);
        for x in 0..w {
            for y in 0..h {
                if self.get(x, y) {
                    p.set(w - 1 - x, h - 1 - y);
                }
            }
        }
        p
    }

    /// The page mirrored along the tape (left ↔ right).
    pub fn mirrored(&self) -> Self {
        let mut p = self.clone();
        p.columns.reverse();
        p
    }

    /// A calibration pattern: 2-dot frame inset by 1 mm, an X from corner to
    /// corner, and 2 mm ticks every 5 mm along the top edge.
    pub fn test_pattern(length_mm: usize) -> Self {
        Self::test_pattern_sized(length_mm * DOTS_PER_MM, HEAD_DOTS, DOTS_PER_MM as f32)
    }

    /// [`Self::test_pattern`] for any band: `width` × `height` dots at
    /// `dpmm` dots per mm.
    pub fn test_pattern_sized(width: usize, height: usize, dpmm: f32) -> Self {
        let mut p = Self::blank_sized(width.max(24), height.max(16));
        let (w, h) = (p.width(), p.height());
        let mm = |n: f32| (n * dpmm).round() as usize;
        let inset = mm(1.0).min(h / 8);
        let (x0, x1, y0, y1) = (inset, w - 1 - inset, inset, h - 1 - inset);
        p.fill_rect(x0, y0, x1 + 1, y0 + 2);
        p.fill_rect(x0, y1 - 1, x1 + 1, y1 + 1);
        p.fill_rect(x0, y0, x0 + 2, y1 + 1);
        p.fill_rect(x1 - 1, y0, x1 + 1, y1 + 1);
        for x in x0..=x1 {
            let y = y0 + (x - x0) * (y1 - y0) / (x1 - x0);
            p.set(x, y);
            p.set(x, y1 + y0 - y);
        }
        for x in (x0..=x1).step_by(mm(5.0).max(1)) {
            p.fill_rect(x, y0, x + 2, y0 + mm(2.0).min(h / 4));
        }
        p
    }

    /// Index of the column before the first non-blank one, capped at
    /// `limit - 1` (`ImgConverter.GetNoZeroIndex(0, limit)`).
    pub(crate) fn leading_blank(&self, limit: usize) -> usize {
        let first = self.columns.iter().take(limit).position(|c| c.iter().any(|&b| b != 0));
        first.unwrap_or(limit).saturating_sub(1).min(limit - 1)
    }
}

#[cfg(test)]
mod transform_tests {
    use super::*;

    #[test]
    fn rotate_and_mirror() {
        let mut p = Page::blank_dots(10);
        p.set(1, 2);
        let r = p.rotated_180();
        assert!(r.get(8, HEAD_DOTS - 3) && !r.get(1, 2));
        assert_eq!(r.rotated_180(), p);
        let m = p.mirrored();
        assert!(m.get(8, 2));
    }
}
