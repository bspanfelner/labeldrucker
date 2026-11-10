//! A label page in the printhead's native layout.
//!
//! Coordinates: `x` runs along the label (feed direction, 8 dots per mm),
//! `y` runs across the 96-dot printhead, `y = 0` being the label's top edge.

/// Printhead height in dots (12 mm at 8 dots/mm).
pub const HEAD_DOTS: usize = 96;
/// Bytes per printed column.
pub const BYTES_PER_COL: usize = HEAD_DOTS / 8;
/// Dots per millimetre in both directions.
pub const DOTS_PER_MM: usize = 8;

/// One printed column: bit `y & 7` of byte `y >> 3` set means a black dot.
pub type Column = [u8; BYTES_PER_COL];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    columns: Vec<Column>,
}

impl Page {
    /// A blank page `length_mm` long.
    pub fn blank(length_mm: usize) -> Self {
        Self::blank_dots(length_mm * DOTS_PER_MM)
    }

    /// A blank page `width` dots long.
    pub fn blank_dots(width: usize) -> Self {
        Self { columns: vec![[0; BYTES_PER_COL]; width] }
    }

    pub fn width(&self) -> usize {
        self.columns.len()
    }

    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    pub fn set(&mut self, x: usize, y: usize) {
        if x < self.columns.len() && y < HEAD_DOTS {
            self.columns[x][y >> 3] |= 1 << (y & 7);
        }
    }

    pub fn get(&self, x: usize, y: usize) -> bool {
        x < self.columns.len() && y < HEAD_DOTS && self.columns[x][y >> 3] & (1 << (y & 7)) != 0
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

    /// A calibration pattern: 2-dot frame inset by 1 mm, an X from corner to
    /// corner, and 2 mm ticks every 5 mm along the top edge.
    pub fn test_pattern(length_mm: usize) -> Self {
        let mut p = Self::blank(length_mm);
        let w = p.width();
        let (x0, x1, y0, y1) = (8, w - 9, 8, HEAD_DOTS - 9);
        p.fill_rect(x0, y0, x1 + 1, y0 + 2);
        p.fill_rect(x0, y1 - 1, x1 + 1, y1 + 1);
        p.fill_rect(x0, y0, x0 + 2, y1 + 1);
        p.fill_rect(x1 - 1, y0, x1 + 1, y1 + 1);
        for x in x0..=x1 {
            let y = y0 + (x - x0) * (y1 - y0) / (x1 - x0);
            p.set(x, y);
            p.set(x, y1 + y0 - y);
        }
        for x in (x0..=x1).step_by(40) {
            p.fill_rect(x, y0, x + 2, y0 + 16);
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
