//! Look and feel: fonts (with Font Awesome icons), spacing, colours.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke};
use katasymbol_core::icons;
use std::sync::Arc;

pub const ACCENT: Color32 = Color32::from_rgb(0x2f, 0x6f, 0xeb);
pub const OK: Color32 = Color32::from_rgb(0x2e, 0xa0, 0x4f);

// Font Awesome 4.7 code points used in the UI.
pub const I_TAG: &str = "\u{f02b}";
pub const I_SEARCH: &str = "\u{f002}";
pub const I_BLUETOOTH: &str = "\u{f293}";
pub const I_UNLINK: &str = "\u{f127}";
pub const I_PRINT: &str = "\u{f02f}";
pub const I_FONT: &str = "\u{f031}";
pub const I_BOLD: &str = "\u{f032}";
pub const I_ITALIC: &str = "\u{f033}";
pub const I_ALIGN_L: &str = "\u{f036}";
pub const I_ALIGN_C: &str = "\u{f037}";
pub const I_ALIGN_R: &str = "\u{f038}";
pub const I_PICTURE: &str = "\u{f03e}";
pub const I_GRID: &str = "\u{f00a}";
pub const I_STAR: &str = "\u{f005}";
pub const I_SQUARE: &str = "\u{f096}";
pub const I_MAGIC: &str = "\u{f0d0}";
pub const I_MINUS: &str = "\u{f068}";
pub const I_PLUS: &str = "\u{f067}";
pub const I_ADJUST: &str = "\u{f042}";
pub const I_COPY: &str = "\u{f0c5}";
pub const I_FOLDER: &str = "\u{f07c}";
pub const I_TIMES: &str = "\u{f00d}";
pub const I_ARROWS_H: &str = "\u{f07e}";
pub const I_UNDO: &str = "\u{f0e2}";
pub const I_CIRCLE: &str = "\u{f111}";
pub const I_COG: &str = "\u{f013}";
pub const I_CUT: &str = "\u{f0c4}";
pub const I_LINK: &str = "\u{f0c1}";

pub fn setup(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fa = FontData::from_static(icons::FONT_DATA).tweak(egui::FontTweak {
        scale: 0.92,
        y_offset_factor: 0.04,
        ..Default::default()
    });
    fonts.font_data.insert("fa".into(), Arc::new(fa));
    for fam in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(fam).or_default().push("fa".into());
    }
    // Icons only: the default UI font also maps U+F000–F002 (ligatures).
    fonts.families.insert(icon_family(), vec!["fa".into()]);
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.interact_size.y = 26.0;
        s.spacing.slider_width = 150.0;
        let r = CornerRadius::same(6);
        let w = &mut s.visuals.widgets;
        for v in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
            v.corner_radius = r;
        }
        s.visuals.window_corner_radius = CornerRadius::same(10);
        s.visuals.menu_corner_radius = CornerRadius::same(8);
        s.visuals.selection.bg_fill = ACCENT;
        s.visuals.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
        s.visuals.hyperlink_color = ACCENT;
        use egui::{FontId, TextStyle};
        s.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(19.0));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(11.5));
    });
}

pub fn icon_family() -> FontFamily {
    FontFamily::Name("icons".into())
}

/// Text in the icon font (for glyphs that clash with the UI font).
pub fn ic(icon: &str) -> egui::RichText {
    egui::RichText::new(icon).family(icon_family())
}

/// A big, filled primary button.
pub fn primary(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text.into()).size(16.0).strong().color(Color32::WHITE))
        .fill(ACCENT)
        .min_size(egui::vec2(150.0, 38.0))
        .corner_radius(CornerRadius::same(8))
}

/// A section header with an icon.
pub fn section<R>(ui: &mut egui::Ui, icon: &str, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) {
    egui::CollapsingHeader::new(egui::RichText::new(format!("{icon}   {title}")).strong().size(15.0))
        .id_salt(title)
        .default_open(true)
        .show(ui, |ui| {
            ui.add_space(2.0);
            add(ui);
            ui.add_space(4.0);
        });
    ui.add_space(2.0);
}
