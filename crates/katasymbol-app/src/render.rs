//! Turning the label design into a printable [`Page`].

use katasymbol_core::page::{DOTS_PER_MM, HEAD_DOTS};
use katasymbol_core::text::{self, FontVec, TextLayout, VariableFont};
use katasymbol_core::Page;
use std::path::Path;

/// System fonts, by family name.
pub struct Fonts {
    db: fontdb::Database,
    pub families: Vec<String>,
}

impl Fonts {
    pub fn system() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let mut families: Vec<String> =
            db.faces().filter_map(|f| f.families.first().map(|(n, _)| n.clone())).collect();
        families.sort_by_key(|f| f.to_lowercase());
        families.dedup();
        Self { db, families }
    }

    /// A sensible default family that exists on this system.
    pub fn default_family(&self) -> String {
        ["DejaVu Sans", "Noto Sans", "Liberation Sans", "Ubuntu", "Helvetica", "Arial"]
            .iter()
            .find(|f| self.families.iter().any(|g| g == *f))
            .map(|f| f.to_string())
            .or_else(|| self.families.first().cloned())
            .unwrap_or_default()
    }

    pub fn load(&self, family: &str, bold: bool) -> Option<FontVec> {
        let weight = if bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL };
        let id = self.db.query(&fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            weight,
            ..Default::default()
        })?;
        let face_weight = self.db.face(id)?.weight;
        let mut font =
            self.db.with_face_data(id, |data, index| text::load_font(data.to_vec(), index).ok())??;
        // Variable fonts come as one face; select the weight on its axis.
        if face_weight != weight {
            font.set_variation(b"wght", f32::from(weight.0));
        }
        Some(font)
    }
}

/// Grayscale image fitted into `width` × 96, centred, white background.
pub fn image_gray(path: &Path, width: usize) -> Result<Vec<u8>, String> {
    let img = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?.to_luma8();
    let s = (HEAD_DOTS as f32 / img.height() as f32).min(width as f32 / img.width() as f32);
    let (w, h) = (((img.width() as f32 * s).round() as u32).max(1), ((img.height() as f32 * s).round() as u32).max(1));
    let img = image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle);
    let mut out = vec![255u8; width * HEAD_DOTS];
    let (ox, oy) = ((width - w as usize) / 2, (HEAD_DOTS - h as usize) / 2);
    for (x, y, p) in img.enumerate_pixels() {
        out[(y as usize + oy) * width + x as usize + ox] = p.0[0];
    }
    Ok(out)
}

pub fn width_dots(length_mm: f32) -> usize {
    ((length_mm * DOTS_PER_MM as f32).round() as usize).max(8)
}

pub fn text_gray(font: &FontVec, text: &str, width: usize, size_px: Option<f32>) -> Vec<u8> {
    text::render(font, text, width, HEAD_DOTS, TextLayout { size_px, margin: 4.0 })
}

/// A 2-dot frame along the page edges.
pub fn add_frame(page: &mut Page) {
    let (w, h) = (page.width(), HEAD_DOTS);
    page.fill_rect(0, 0, w, 2);
    page.fill_rect(0, h - 2, w, h);
    page.fill_rect(0, 0, 2, h);
    page.fill_rect(w - 2, 0, w, h);
}
