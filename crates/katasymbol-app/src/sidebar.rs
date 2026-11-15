//! The design controls on the left.

use crate::style::*;
use crate::{App, Mode, Slot};
use eframe::egui::{self, Color32, RichText, Sense, Stroke};
use katasymbol_core::icons;
use katasymbol_core::label::{Frame, FrameStyle};
use katasymbol_core::text::Align;

/// Toggle-style button.
fn toggle(ui: &mut egui::Ui, on: bool, text: &str, tip: &str) -> bool {
    ui.add(egui::Button::new(RichText::new(text).size(15.0)).selected(on).min_size(egui::vec2(34.0, 28.0)))
        .on_hover_text(tip)
        .clicked()
}

/// A labelled slider row with the value shown in mm.
fn mm_slider(ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(122.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(122.0);
            ui.label(label);
        });
        ui.add(
            egui::Slider::new(v, range)
                .step_by(1.0)
                .custom_formatter(|v, _| format!("{:.2} mm", v / 8.0).replace('.', ","))
                .custom_parser(|s| s.trim_end_matches("mm").trim().replace(',', ".").parse::<f64>().ok().map(|mm| mm * 8.0)),
        );
    });
}

impl App {
    pub fn sidebar(&mut self, ui: &mut egui::Ui) {
        // Content type as a segmented control.
        ui.columns(3, |cols| {
            for (col, (mode, text)) in cols.iter_mut().zip([
                (Mode::Text, format!("{I_FONT}  Text")),
                (Mode::Image, format!("{I_PICTURE}  Bild")),
                (Mode::Test, format!("{I_GRID}  Test")),
            ]) {
                let b = egui::Button::new(RichText::new(text).size(14.0)).selected(self.design.mode == mode);
                if col.add_sized([col.available_width(), 32.0], b).clicked() {
                    self.design.mode = mode;
                }
            }
        });
        ui.add_space(6.0);
        let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if std::env::var_os("KATASYMBOL_SCREENSHOT_SCROLL").is_some() {
            scroll = scroll.vertical_scroll_offset(560.0);
        }
        scroll.show(ui, |ui| {
            match self.design.mode {
                Mode::Text => self.text_section(ui),
                Mode::Image => self.image_section(ui),
                Mode::Test => {
                    ui.add_space(8.0);
                    ui.label("Testmuster: Rahmen, X und Striche alle 5 mm.\nZum Prüfen von Ausrichtung und Dichte.");
                    return;
                }
            }
            self.icon_section(ui);
            self.frame_section(ui);
        });
    }

    fn text_section(&mut self, ui: &mut egui::Ui) {
        section(ui, I_FONT, "Text", |ui| {
            let d = &mut self.design;
            ui.add(
                egui::TextEdit::multiline(&mut d.label.text)
                    .font(egui::FontId::proportional(17.0))
                    .hint_text("Text eingeben … (Enter für neue Zeile)")
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );

            // Font family with a filter.
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.font_filter).hint_text("Schrift suchen …").desired_width(130.0));
                let filter = self.font_filter.to_lowercase();
                let families = &self.fonts.families;
                egui::ComboBox::from_id_salt("family")
                    .selected_text(RichText::new(&d.family).strong())
                    .width(ui.available_width() - 8.0)
                    .height(420.0)
                    .show_ui(ui, |ui| {
                        for f in families.iter().filter(|f| filter.is_empty() || f.to_lowercase().contains(&filter)) {
                            ui.selectable_value(&mut d.family, f.clone(), f);
                        }
                    });
            });

            ui.horizontal(|ui| {
                if toggle(ui, d.bold, I_BOLD, "Fett") {
                    d.bold = !d.bold;
                }
                if toggle(ui, d.italic, I_ITALIC, "Kursiv") {
                    d.italic = !d.italic;
                }
                ui.separator();
                for (a, icon, tip) in [(Align::Left, I_ALIGN_L, "Links"), (Align::Center, I_ALIGN_C, "Mitte"), (Align::Right, I_ALIGN_R, "Rechts")] {
                    if toggle(ui, d.label.align == a, icon, tip) {
                        d.label.align = a;
                    }
                }
                ui.separator();
                if toggle(ui, d.label.invert, I_ADJUST, "Invertieren: weiß auf schwarz") {
                    d.label.invert = !d.label.invert;
                }
            });

            ui.horizontal(|ui| {
                let mut auto = d.label.size_px.is_none();
                if toggle(ui, auto, &format!("{I_MAGIC} Auto"), "Schriftgröße automatisch an den Platz anpassen") {
                    auto = !auto;
                    d.label.size_px = if auto { None } else { Some(48.0) };
                }
                let mut size = d.label.size_px.unwrap_or(48.0);
                let r = ui.add_enabled(
                    !auto,
                    egui::Slider::new(&mut size, 8.0..=96.0).step_by(1.0).custom_formatter(|v, _| format!("{:.1} mm", v / 8.0).replace('.', ",")),
                );
                if r.changed() {
                    d.label.size_px = Some(size);
                }
            });
        });
    }

    fn image_section(&mut self, ui: &mut egui::Ui) {
        section(ui, I_PICTURE, "Bild", |ui| {
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(format!("{I_FOLDER}  Bild öffnen …")).min_size(egui::vec2(0.0, 30.0))).clicked() {
                    self.pick_image();
                }
                if self.design.image.is_some() && ui.button(format!("{I_TIMES}  Entfernen")).clicked() {
                    self.design.image = None;
                }
            });
            match &self.design.image {
                Some(p) => ui.label(RichText::new(p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into())).weak()),
                None => ui.label(RichText::new("… oder eine PNG/JPEG-Datei ins Fenster ziehen.").weak()),
            };
            let d = &mut self.design;
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(egui::vec2(122.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_width(122.0);
                    ui.label("Schwelle");
                });
                ui.add(egui::Slider::new(&mut d.threshold, 1..=254)).on_hover_text("Hellere Werte drucken mehr Grautöne schwarz");
            });
            ui.horizontal(|ui| {
                for (a, icon, tip) in [(Align::Left, I_ALIGN_L, "Links"), (Align::Center, I_ALIGN_C, "Mitte"), (Align::Right, I_ALIGN_R, "Rechts")] {
                    if toggle(ui, d.label.align == a, icon, tip) {
                        d.label.align = a;
                    }
                }
                ui.separator();
                if toggle(ui, d.label.invert, I_ADJUST, "Invertieren: weiß auf schwarz") {
                    d.label.invert = !d.label.invert;
                }
            });
        });
    }

    fn icon_section(&mut self, ui: &mut egui::Ui) {
        section(ui, I_STAR, "Symbole", |ui| {
            let l = &mut self.design.label;
            // The two slots; the selected one receives clicks in the grid.
            ui.horizontal(|ui| {
                for (slot, name) in [(Slot::Left, "Links"), (Slot::Right, "Rechts")] {
                    let icon = if slot == Slot::Left { &mut l.icon_left } else { &mut l.icon_right };
                    let glyph = icon.map_or("–".to_string(), |c| c.to_string());
                    ui.vertical(|ui| {
                        let b = egui::Button::new(ic(&glyph).size(26.0)).selected(self.slot == slot).min_size(egui::vec2(64.0, 50.0));
                        if ui.add(b).on_hover_text(format!("Symbol {name} – dann unten wählen")).clicked() {
                            self.slot = slot;
                        }
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name).small());
                            if icon.is_some() && ui.small_button(I_TIMES).on_hover_text("Entfernen").clicked() {
                                *icon = None;
                            }
                        });
                    });
                }
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("Größe");
                        ui.add(egui::Slider::new(&mut l.icon_scale, 0.4..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
                    });
                    ui.checkbox(&mut l.separator, "Trennstrich");
                });
            });
            ui.add_space(4.0);
            let current = if self.slot == Slot::Left { l.icon_left } else { l.icon_right };
            // One group at a time, picked with tabs.
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for (i, g) in icons::GROUPS.iter().enumerate() {
                    if ui.add(egui::Button::new(RichText::new(g.name).size(12.5)).selected(self.icon_group == i).corner_radius(12)).clicked() {
                        self.icon_group = i;
                    }
                }
            });
            ui.add_space(2.0);
            if let Some(g) = icons::GROUPS.get(self.icon_group) {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                    for ic in g.icons {
                        let b = egui::Button::new(crate::style::ic(&ic.ch.to_string()).size(17.0))
                            .selected(current == Some(ic.ch))
                            .min_size(egui::vec2(34.0, 32.0));
                        if ui.add(b).on_hover_text(ic.name).clicked() {
                            let target = if self.slot == Slot::Left { &mut l.icon_left } else { &mut l.icon_right };
                            *target = if *target == Some(ic.ch) { None } else { Some(ic.ch) };
                        }
                    }
                });
            }
        });
    }

    fn frame_section(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let thumbs: Vec<_> = self.frame_thumbs(&ctx).to_vec();
        section(ui, I_SQUARE, "Rahmen", |ui| {
            let f = &mut self.design.label.frame;
            // Gallery of styles.
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                for (style, tex) in FrameStyle::ALL.iter().zip(&thumbs) {
                    let size = egui::vec2(100.0, 66.0);
                    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                    let selected = f.style == *style;
                    let v = ui.visuals();
                    let stroke = if selected {
                        Stroke::new(2.0_f32, ACCENT)
                    } else if resp.hovered() {
                        v.widgets.hovered.bg_stroke
                    } else {
                        v.widgets.noninteractive.bg_stroke
                    };
                    let p = ui.painter();
                    p.rect(rect, 6, v.faint_bg_color, stroke, egui::StrokeKind::Inside);
                    let img = egui::Rect::from_min_size(rect.min + egui::vec2(6.0, 6.0), egui::vec2(88.0, 88.0 * 96.0 / 208.0));
                    p.rect_filled(img, 2, Color32::WHITE);
                    p.image(tex.id(), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                    p.text(
                        egui::pos2(rect.center().x, rect.max.y - 10.0),
                        egui::Align2::CENTER_CENTER,
                        style.name(),
                        egui::FontId::proportional(11.5),
                        if selected { ACCENT } else { v.text_color() },
                    );
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        f.style = *style;
                    }
                }
            });
            if f.style != FrameStyle::None {
                ui.add_space(4.0);
                mm_slider(ui, "Linienstärke", &mut f.thickness, 1.0..=10.0);
                mm_slider(ui, "Abstand zum Rand", &mut f.margin, 0.0..=16.0);
                mm_slider(ui, "Innenabstand", &mut f.padding, 0.0..=20.0);
                if f.style.uses_radius() {
                    let name = match f.style {
                        FrameStyle::Chamfer => "Schräge",
                        FrameStyle::Ticket => "Kerbe",
                        _ => "Eckenradius",
                    };
                    mm_slider(ui, name, &mut f.radius, 0.0..=40.0);
                }
                if ui.button(format!("{I_UNDO}  Werte zurücksetzen")).clicked() {
                    *f = Frame { style: f.style, ..Frame::default() };
                }
            }
        });
    }
}
