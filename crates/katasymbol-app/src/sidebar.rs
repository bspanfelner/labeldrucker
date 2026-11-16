//! The design controls on the left.

use crate::style::*;
use crate::{App, Mode, Slot};
use eframe::egui::{self, Color32, RichText, Sense, Stroke};
use katasymbol_core::icons;
use crate::THUMB_LEN;
use katasymbol_core::label::{Category, Frame, FrameStyle};
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
    /// The side panel (wide layout): content tabs over scrolling sections.
    pub fn sidebar(&mut self, ui: &mut egui::Ui) {
        self.mode_tabs(ui);
        ui.add_space(6.0);
        let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if std::env::var_os("KATASYMBOL_SCREENSHOT_SCROLL").is_some() {
            scroll = scroll.vertical_scroll_offset(560.0);
        }
        scroll.show(ui, |ui| self.sections(ui));
    }

    /// Content type as a segmented control.
    pub fn mode_tabs(&mut self, ui: &mut egui::Ui) {
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
    }

    /// All design sections for the current content type.
    pub fn sections(&mut self, ui: &mut egui::Ui) {
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
    }

    fn text_section(&mut self, ui: &mut egui::Ui) {
        section(ui, I_FONT, "Text", |ui| {
            let d = &mut self.design;
            let r = ui.add(
                egui::TextEdit::multiline(&mut d.label.text)
                    .font(egui::FontId::proportional(17.0))
                    .hint_text("Text eingeben … (Enter für neue Zeile)")
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            crate::keyboard::field(ui, &r);

            // Font family with a filter.
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut self.font_filter).hint_text("Schrift suchen …").desired_width(130.0));
                crate::keyboard::field(ui, &r);
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
                    self.pick_image(ui.ctx());
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
        let ctx = ui.ctx().clone();
        let tint = ui.visuals().text_color();
        section(ui, I_STAR, "Symbole", |ui| {
            // The two slots; the selected one receives clicks in the grid.
            ui.horizontal(|ui| {
                for (slot, name) in [(Slot::Left, "Links"), (Slot::Right, "Rechts")] {
                    let icon = if slot == Slot::Left { self.design.label.icon_left } else { self.design.label.icon_right };
                    ui.vertical(|ui| {
                        let b = match icon {
                            Some(ch) => {
                                let tex = self.icon_texture(&ctx, ch);
                                egui::Button::image(egui::Image::new(&tex).fit_to_exact_size(egui::vec2(34.0, 34.0)).tint(tint))
                            }
                            None => egui::Button::new(RichText::new("–").size(24.0)),
                        };
                        let tip = icon.and_then(icons::name).unwrap_or("leer");
                        if ui.add(b.selected(self.slot == slot).min_size(egui::vec2(64.0, 50.0))).on_hover_text(format!("{name}: {tip} – unten auswählen")).clicked() {
                            self.slot = slot;
                        }
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name).small());
                            if icon.is_some() && ui.small_button(I_TIMES).on_hover_text("Entfernen").clicked() {
                                if slot == Slot::Left { self.design.label.icon_left = None } else { self.design.label.icon_right = None }
                            }
                        });
                    });
                }
                let l = &mut self.design.label;
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("Größe");
                        ui.add(egui::Slider::new(&mut l.icon_scale, 0.4..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));
                    });
                    ui.checkbox(&mut l.separator, "Trennstrich");
                });
            });
            ui.add_space(4.0);
            let r = ui.add(egui::TextEdit::singleline(&mut self.icon_filter).hint_text("Symbol suchen … (z. B. Schraube, Torx, Herz)").desired_width(f32::INFINITY));
            crate::keyboard::field(ui, &r);
            let filter = self.icon_filter.trim().to_lowercase();
            // Group tabs (hidden while searching).
            if filter.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                    for (i, g) in self.icon_groups.iter().enumerate() {
                        if ui.add(egui::Button::new(RichText::new(g.name).size(12.5)).selected(self.icon_group == i).corner_radius(12)).clicked() {
                            self.icon_group = i;
                        }
                    }
                });
                ui.add_space(2.0);
            }
            let shown: Vec<icons::Icon> = if filter.is_empty() {
                self.icon_groups.get(self.icon_group).map(|g| g.icons.clone()).unwrap_or_default()
            } else {
                let mut v: Vec<icons::Icon> = Vec::new();
                for g in &self.icon_groups {
                    let group_hit = g.name.to_lowercase().contains(&filter);
                    for i in &g.icons {
                        if (group_hit || i.name.to_lowercase().contains(&filter)) && !v.iter().any(|x| x.ch == i.ch) {
                            v.push(*i);
                        }
                    }
                }
                v
            };
            if shown.is_empty() {
                ui.label(RichText::new("Kein Symbol gefunden.").weak());
            }
            let current = if self.slot == Slot::Left { self.design.label.icon_left } else { self.design.label.icon_right };
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for ic in shown {
                    let tex = self.icon_texture(&ctx, ic.ch);
                    let b = egui::Button::image(egui::Image::new(&tex).fit_to_exact_size(egui::vec2(28.0, 28.0)).tint(tint))
                        .selected(current == Some(ic.ch))
                        .min_size(egui::vec2(40.0, 38.0));
                    if ui.add(b).on_hover_text(ic.name).clicked() {
                        let target = if self.slot == Slot::Left { &mut self.design.label.icon_left } else { &mut self.design.label.icon_right };
                        *target = if *target == Some(ic.ch) { None } else { Some(ic.ch) };
                    }
                }
            });
        });
    }

    fn frame_section(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let thumbs: Vec<_> = self.frame_thumbs(&ctx).to_vec();
        section(ui, I_SQUARE, "Rahmen", |ui| {
            // Category tabs.
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for cat in Category::ALL {
                    if ui.add(egui::Button::new(RichText::new(cat.name()).size(12.5)).selected(self.frame_cat == cat).corner_radius(12)).clicked() {
                        self.frame_cat = cat;
                    }
                }
            });
            ui.add_space(2.0);
            let f = &mut self.design.label.frame;
            // Gallery of the category's styles.
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                for (style, tex) in FrameStyle::ALL.iter().zip(&thumbs).filter(|(s, _)| s.category() == self.frame_cat) {
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
                    let img = egui::Rect::from_min_size(rect.min + egui::vec2(6.0, 6.0), egui::vec2(88.0, 88.0 * 96.0 / THUMB_LEN as f32));
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
                ui.label(RichText::new(format!("Gewählt: {} ({})", f.style.name(), f.style.category().name())).small().weak());
                mm_slider(ui, "Linienstärke", &mut f.thickness, 1.0..=10.0);
                mm_slider(ui, "Abstand zum Rand", &mut f.margin, 0.0..=16.0);
                mm_slider(ui, "Innenabstand", &mut f.padding, 0.0..=20.0);
                if let Some(name) = f.style.radius_name() {
                    mm_slider(ui, name, &mut f.radius, 0.0..=40.0);
                }
                if ui.button(format!("{I_UNDO}  Werte zurücksetzen")).clicked() {
                    *f = Frame { style: f.style, ..Frame::default() };
                }
            }
        });
    }
}
