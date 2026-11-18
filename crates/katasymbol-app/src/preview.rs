//! The label preview with its length controls: a toolbar (−/+, presets,
//! auto, "as label") and a drag handle on the label's end.

use crate::style::*;
use crate::{App, MAX_MM, MIN_MM, Mode};
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, pos2, vec2};
use crate::backend::Kind;
use katasymbol_core::brother;

const PRESETS: [f32; 7] = [20.0, 30.0, 40.0, 50.0, 60.0, 80.0, 100.0];

impl App {
    pub fn preview(&mut self, ui: &mut egui::Ui) {
        self.length_toolbar(ui);
        ui.add_space(10.0);
        self.label_view(ui);
    }

    fn length_toolbar(&mut self, ui: &mut egui::Ui) {
        let test = self.design.mode == Mode::Test;
        let auto = self.design.auto_length && !test;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("{I_ARROWS_H}  Länge")).strong().size(15.0));
            ui.add_enabled_ui(!auto, |ui| {
                let mut mm = self.design.length_mm();
                if ui.add(egui::Button::new(I_MINUS).min_size(vec2(30.0, 30.0))).on_hover_text("1 mm kürzer").clicked() {
                    mm -= 1.0;
                }
                ui.add(
                    egui::DragValue::new(&mut mm)
                        .range(MIN_MM..=MAX_MM)
                        .speed(0.5)
                        .fixed_decimals(0)
                        .suffix(" mm"),
                )
                .on_hover_text("Ziehen, tippen oder Pfeile – oder in der Vorschau am Griff ziehen");
                if ui.add(egui::Button::new(I_PLUS).min_size(vec2(30.0, 30.0))).on_hover_text("1 mm länger").clicked() {
                    mm += 1.0;
                }
                if mm != self.design.length_mm() {
                    self.design.set_length_mm(mm);
                }
            });
            ui.separator();
            if !test {
                let b = egui::Button::new(format!("{I_MAGIC}  Automatisch")).selected(auto);
                if ui.add(b).on_hover_text("Länge passt sich dem Inhalt an").clicked() {
                    self.design.auto_length = !auto;
                }
            }
            if let Some(len) = self.material_length() {
                let on = !auto && (self.design.length_mm() - len).abs() < 0.5;
                if ui.add(egui::Button::new(format!("{I_TAG}  Etikett {len:.0} mm")).selected(on)).on_hover_text("Länge des eingelegten Etiketts").clicked() {
                    self.design.auto_length = false;
                    self.design.set_length_mm(len);
                }
            }
            for p in PRESETS {
                let on = !auto && (self.design.length_mm() - p).abs() < 0.5;
                if ui.add(egui::Button::new(format!("{p:.0}")).selected(on).min_size(vec2(38.0, 0.0))).clicked() {
                    self.design.auto_length = false;
                    self.design.set_length_mm(p);
                }
            }
        });
        if self.kind == Kind::Brother {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{I_TAG}  Band")).strong());
                let known = self.info.as_ref().and_then(|i| i.tape).is_some_and(|t| t.media_width > 0) && self.connected;
                for w in brother::TAPE_WIDTHS {
                    let text = if w == 4 { "3,5".to_string() } else { w.to_string() };
                    let b = egui::Button::new(format!("{text} mm")).selected(self.brother_tape == w);
                    let r = ui.add_enabled(!known || self.brother_tape == w, b);
                    if r.on_hover_text(if known { "Breite der eingelegten Bandkassette" } else { "Bandbreite (wird beim Verbinden erkannt)" }).clicked() {
                        self.brother_tape = w;
                    }
                }
            });
        }
        if !test {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{I_CUT}  Schnittrand")).strong());
                ui.add(
                    egui::Slider::new(&mut self.design.label.end_margin, 0.0..=48.0)
                        .step_by(2.0)
                        .custom_formatter(|v, _| format!("{:.2} mm", v / 8.0).replace('.', ",")),
                )
                .on_hover_text("Freier Rand am Anfang und Ende des Etiketts, damit der Druck nicht zu nah an der Schnittkante liegt");
                if ui.small_button(I_UNDO).on_hover_text("Standard: 2 mm").clicked() {
                    self.design.label.end_margin = katasymbol_core::label::DEFAULT_END_MARGIN;
                }
            });
        }
    }

    fn label_view(&mut self, ui: &mut egui::Ui) {
        let Some(tex) = self.preview.clone() else { return };
        let media = self.media();
        let page_w = self.page.as_ref().map_or(self.design.label.length, |p| p.width());
        let page_h = self.page.as_ref().map_or(media.band, |p| p.height());
        let len_mm = page_w as f32 / media.dpmm;
        let label_w_mm = media.width_mm;
        let band_mm = page_h as f32 / media.dpmm;
        let avail = ui.available_size();

        // Pixels per mm: fit the width, keep the height reasonable; frozen while dragging.
        let fit = ((avail.x - 70.0) / len_mm).min((avail.y - 110.0).max(60.0) / label_w_mm).clamp(2.0, 18.0);
        let scale = self.drag.map_or(fit, |(_, s, _)| s);
        let label_size = vec2(len_mm * scale, label_w_mm * scale);

        let (area, _) = ui.allocate_exact_size(vec2(avail.x, avail.y.max(label_size.y + 110.0)), Sense::hover());
        let top = area.min.y + 34.0;
        let left = (area.center().x - label_size.x / 2.0).max(area.min.x + 10.0);
        let label = Rect::from_min_size(pos2(left, top), label_size);
        let p = ui.painter_at(area);
        let v = ui.visuals().clone();

        // Backing tape, label with die-cut corners and a soft shadow.
        let tape = Rect::from_min_max(pos2(area.min.x, label.min.y - 8.0), pos2(area.max.x, label.max.y + 8.0));
        p.rect_filled(tape, 4, if v.dark_mode { Color32::from_gray(52) } else { Color32::from_gray(224) });
        let corner = CornerRadius::same((1.5 * scale).min(12.0) as u8);
        p.rect_filled(label.translate(vec2(0.0, 2.0)), corner, Color32::from_black_alpha(40));
        p.rect_filled(label, corner, Color32::WHITE);

        // The printable band, with the bitmap.
        let band = Rect::from_center_size(label.center(), vec2(label.width(), band_mm * scale));
        p.image(tex.id(), band, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        if label_w_mm > band_mm + 0.5 {
            let guide = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(47, 111, 235, 60));
            for y in [band.min.y, band.max.y] {
                p.line_segment([pos2(label.min.x + 4.0, y), pos2(label.max.x - 4.0, y)], guide);
            }
        }

        // The blank end margins at the cut edges.
        if self.design.mode != Mode::Test && self.design.label.end_margin > 0.0 {
            let m = self.design.label.end_margin / 8.0 * scale;
            let hint = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(47, 111, 235, 110));
            for x in [label.min.x + m, label.max.x - m] {
                p.add(egui::Shape::dashed_line(&[pos2(x, label.min.y + 3.0), pos2(x, label.max.y - 3.0)], hint, 3.0, 3.0));
            }
        }

        // End of the loaded label, when the design is longer or shorter.
        if let Some(mat) = self.material_length()
            && (mat - len_mm).abs() >= 0.5
            && mat < len_mm + 40.0
        {
            let x = label.min.x + mat * scale;
            let warn = Color32::from_rgb(0xd9, 0x82, 0x1e);
            p.add(egui::Shape::dashed_line(&[pos2(x, tape.min.y), pos2(x, tape.max.y)], Stroke::new(1.5_f32, warn), 5.0, 4.0));
            p.text(pos2(x, tape.min.y - 2.0), egui::Align2::CENTER_BOTTOM, format!("Etikett {mat:.0} mm"), egui::FontId::proportional(11.0), warn);
        }

        // Length badge above.
        p.text(
            pos2(label.center().x, area.min.y + 10.0),
            egui::Align2::CENTER_CENTER,
            format!("{len_mm:.0} × {label_w_mm:.0} mm{}", if self.design.auto_length && self.design.mode != Mode::Test { "   (automatisch)" } else { "" }),
            egui::FontId::proportional(14.0),
            v.text_color(),
        );

        self.ruler(&p, &v, label, scale, len_mm);
        self.drag_handle(ui, label, scale, len_mm);

        // Details under the ruler.
        p.text(
            pos2(label.min.x, label.max.y + 48.0),
            egui::Align2::LEFT_TOP,
            match self.kind {
                Kind::E16 => format!("Druckbild {page_w} × {page_h} Punkte (8 Punkte/mm). Bedruckbar sind die mittleren 12 mm."),
                Kind::Brother => format!(
                    "Druckbild {page_w} × {page_h} Punkte (180 dpi). Bedruckbar sind die mittleren {} mm des Bands.",
                    format!("{band_mm:.1}").replace('.', ",")
                ),
            },
            egui::FontId::proportional(11.5),
            v.weak_text_color(),
        );
    }

    fn ruler(&self, p: &egui::Painter, v: &egui::Visuals, label: Rect, scale: f32, len_mm: f32) {
        let y = label.max.y + 12.0;
        let color = v.weak_text_color();
        let every = if scale < 4.0 { 20 } else { 10 };
        for mm in 0..=(len_mm + 0.1).floor() as i32 {
            let x = label.min.x + mm as f32 * scale;
            let h = if mm % 10 == 0 { 9.0 } else if mm % 5 == 0 { 6.0 } else if scale >= 4.0 { 3.0 } else { continue };
            p.line_segment([pos2(x, y), pos2(x, y + h)], Stroke::new(1.0_f32, color));
            if mm % every == 0 {
                p.text(pos2(x, y + 11.0), egui::Align2::CENTER_TOP, mm.to_string(), egui::FontId::proportional(10.5), color);
            }
        }
        p.line_segment([pos2(label.min.x, y), pos2(label.max.x, y)], Stroke::new(1.0_f32, color));
    }

    fn drag_handle(&mut self, ui: &mut egui::Ui, label: Rect, scale: f32, len_mm: f32) {
        if self.design.auto_length && self.design.mode != Mode::Test {
            return;
        }
        let c = pos2(label.max.x, label.center().y);
        let hit = Rect::from_center_size(c, vec2(22.0, label.height().max(40.0)));
        let resp = ui.interact(hit, ui.id().with("length-handle"), Sense::drag());
        let active = resp.hovered() || resp.dragged();
        if active {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if resp.drag_started() {
            self.drag = Some((len_mm, scale, 0.0));
        }
        if let Some((start, s, acc)) = &mut self.drag {
            *acc += resp.drag_delta().x;
            let mm = (*start + *acc / *s).round();
            if mm != self.design.length_mm() {
                self.design.set_length_mm(mm);
            }
        }
        if resp.drag_stopped() {
            self.drag = None;
        }
        // A pill-shaped grip.
        let p = ui.painter();
        let grip = Rect::from_center_size(c, vec2(12.0, 38.0));
        p.rect(grip, 6, if active { ACCENT } else { Color32::from_gray(150) }, Stroke::new(1.5_f32, Color32::WHITE), egui::StrokeKind::Outside);
        for dy in [-6.0, 0.0, 6.0] {
            p.line_segment([Pos2::new(c.x - 3.0, c.y + dy), Pos2::new(c.x + 3.0, c.y + dy)], Stroke::new(1.2_f32, Color32::WHITE));
        }
        if resp.dragged() {
            p.text(c + vec2(16.0, -26.0), egui::Align2::LEFT_CENTER, format!("{:.0} mm", self.design.length_mm()), egui::FontId::proportional(13.0), ACCENT);
        }
    }
}
