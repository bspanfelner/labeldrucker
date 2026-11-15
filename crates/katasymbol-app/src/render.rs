//! System fonts and pictures for the label design.

use katasymbol_core::label::Picture;
use katasymbol_core::text::{self, FontVec, VariableFont};
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

    pub fn has(&self, family: &str) -> bool {
        self.families.iter().any(|f| f == family)
    }

    pub fn load(&self, family: &str, bold: bool, italic: bool) -> Option<FontVec> {
        let weight = if bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL };
        let style = if italic { fontdb::Style::Italic } else { fontdb::Style::Normal };
        let id = self.db.query(&fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            weight,
            style,
            ..Default::default()
        })?;
        let face_weight = self.db.face(id)?.weight;
        let mut font = self.db.with_face_data(id, |data, index| text::load_font(data.to_vec(), index).ok())??;
        // Variable fonts come as one face; select the weight on its axis.
        if face_weight != weight {
            font.set_variation(b"wght", f32::from(weight.0));
        }
        Some(font)
    }
}

/// Load an image as grayscale, downscaled to a sensible size for a 96-dot head.
/// Transparent areas count as white.
pub fn load_picture(path: &Path) -> Result<Picture, String> {
    let img = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let max_h = 384;
    let img = if img.height() > max_h { img.resize(u32::MAX, max_h, image::imageops::FilterType::Triangle) } else { img };
    let rgba = img.to_rgba8();
    let gray = rgba
        .pixels()
        .map(|p| {
            let [r, g, b, a] = p.0.map(u32::from);
            let l = (r * 299 + g * 587 + b * 114) / 1000;
            (255 - (255 - l) * a / 255) as u8
        })
        .collect();
    Ok(Picture { width: rgba.width() as usize, height: rgba.height() as usize, gray, threshold: 128 })
}
