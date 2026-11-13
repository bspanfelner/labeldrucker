//! Katasymbol desktop app: design a label (text, image or test pattern),
//! preview it at print resolution and print it on an E16 over BLE.

mod render;
mod worker;

use eframe::egui::{self, Color32, ColorImage, RichText, TextureHandle, TextureOptions};
use katasymbol_core::page::HEAD_DOTS;
use katasymbol_core::printer::Info;
use katasymbol_core::text::FontVec;
use katasymbol_core::Page;
use render::Fonts;
use std::path::PathBuf;
use worker::{Cmd, Ev, Worker};

const PREVIEW_SCALE: f32 = 3.0;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Katasymbol")
            .with_app_id("katasymbol")
            .with_inner_size([820.0, 640.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("Katasymbol", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Text,
    Image,
    Test,
}

/// Everything that determines the printed page; a change triggers a re-render.
#[derive(Clone, PartialEq)]
struct Design {
    mode: Mode,
    text: String,
    family: String,
    bold: bool,
    auto_size: bool,
    size_px: f32,
    frame: bool,
    image: Option<PathBuf>,
    threshold: u8,
    length_mm: f32,
    invert: bool,
}

struct App {
    worker: Worker,
    fonts: Fonts,
    font: Option<(String, bool, FontVec)>,
    design: Design,
    rendered: Option<Design>,
    page: Option<Page>,
    preview: Option<TextureHandle>,
    addr: String,
    density: u8,
    rotate: bool,
    mirror: bool,
    connected: bool,
    busy: bool,
    info: Option<Info>,
    message: Option<(String, bool)>,
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("katasymbol").join("printer"))
}

fn load_addr() -> String {
    std::env::var("KATASYMBOL_ADDR")
        .ok()
        .or_else(|| std::fs::read_to_string(config_path()?).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}

fn save_addr(addr: &str) {
    if let Some(p) = config_path() {
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let _ = std::fs::write(p, addr);
    }
}

fn valid_addr(a: &str) -> bool {
    let parts: Vec<&str> = a.split(':').collect();
    parts.len() == 6 && parts.iter().all(|p| p.len() == 2 && u8::from_str_radix(p, 16).is_ok())
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        let ctx = cc.egui_ctx.clone();
        let worker = Worker::spawn(move || ctx.request_repaint());
        let fonts = Fonts::system();
        let design = Design {
            mode: Mode::Text,
            text: "Hallo E16".into(),
            family: fonts.default_family(),
            bold: true,
            auto_size: true,
            size_px: 48.0,
            frame: false,
            image: None,
            threshold: 128,
            length_mm: 40.0,
            invert: false,
        };
        Self {
            worker,
            fonts,
            font: None,
            design,
            rendered: None,
            page: None,
            preview: None,
            addr: load_addr(),
            density: 4,
            rotate: false,
            mirror: false,
            connected: false,
            busy: false,
            info: None,
            message: None,
        }
    }

    fn font(&mut self) -> Option<&FontVec> {
        let d = &self.design;
        if !matches!(&self.font, Some((f, b, _)) if *f == d.family && *b == d.bold) {
            self.font = self.fonts.load(&d.family, d.bold).map(|f| (d.family.clone(), d.bold, f));
        }
        self.font.as_ref().map(|(_, _, f)| f)
    }

    /// Re-render the page and its preview texture if the design changed.
    fn update_page(&mut self, ctx: &egui::Context) {
        if self.rendered.as_ref() == Some(&self.design) {
            return;
        }
        let d = self.design.clone();
        let width = render::width_dots(d.length_mm);
        let page = match d.mode {
            Mode::Test => Ok(Page::test_pattern((d.length_mm.round() as usize).max(5))),
            Mode::Text => match self.font() {
                Some(font) => {
                    let gray = render::text_gray(font, &d.text, width, (!d.auto_size).then_some(d.size_px));
                    Ok(Page::from_gray(width, HEAD_DOTS, &gray, 128))
                }
                None => Err(format!("Schrift „{}“ nicht ladbar", d.family)),
            },
            Mode::Image => match &d.image {
                Some(p) => render::image_gray(p, width).map(|g| Page::from_gray(width, HEAD_DOTS, &g, d.threshold)),
                None => Ok(Page::blank_dots(width)),
            },
        };
        match page {
            Ok(mut page) => {
                if d.invert && d.mode != Mode::Test {
                    page = invert(&page);
                }
                if d.frame && d.mode != Mode::Test {
                    render::add_frame(&mut page);
                }
                let img = preview_image(&page);
                match &mut self.preview {
                    Some(t) => t.set(img, TextureOptions::NEAREST),
                    None => self.preview = Some(ctx.load_texture("preview", img, TextureOptions::NEAREST)),
                }
                self.page = Some(page);
            }
            Err(e) => {
                self.page = None;
                self.message = Some((e, true));
            }
        }
        self.rendered = Some(d);
    }

    fn poll_worker(&mut self) {
        while let Ok(ev) = self.worker.rx.try_recv() {
            match ev {
                Ev::Connected(c) => {
                    self.connected = c;
                    if !c {
                        self.info = None;
                    }
                }
                Ev::Info(info) => {
                    if let Some(m) = &info.material
                        && self.info.is_none()
                        && m.length_mm > 0
                    {
                        self.design.length_mm = f32::from(m.length_mm);
                    }
                    self.info = Some(info);
                    self.busy = false;
                    if matches!(&self.message, Some((m, false)) if m.ends_with('…')) {
                        self.message = None;
                    }
                }
                Ev::Progress(p) => self.message = Some((p, false)),
                Ev::Printed => self.message = Some(("Gedruckt.".into(), false)),
                Ev::Error(e) => {
                    self.message = Some((e, true));
                    self.busy = false;
                }
            }
        }
    }

    fn request(&mut self, cmd: Cmd) {
        save_addr(&self.addr);
        self.busy = true;
        self.message = None;
        self.worker.send(cmd);
    }

    fn printer_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Drucker");
            ui.add(egui::TextEdit::singleline(&mut self.addr).hint_text("A4:93:40:3E:7B:0C").desired_width(150.0));
            let ok = valid_addr(self.addr.trim());
            if ui.add_enabled(ok && !self.busy, egui::Button::new("Verbinden")).clicked() {
                self.addr = self.addr.trim().to_uppercase();
                self.request(Cmd::Probe { addr: self.addr.clone() });
            }
            if ui.add_enabled(self.connected && !self.busy, egui::Button::new("Trennen")).clicked() {
                self.worker.send(Cmd::Disconnect);
            }
            let (dot, txt) = if self.connected { (Color32::from_rgb(40, 160, 70), "verbunden") } else { (Color32::GRAY, "getrennt") };
            ui.label(RichText::new("●").color(dot));
            ui.label(txt);
        });
        if let Some(i) = &self.info {
            let s = &i.status;
            let mut line = format!(
                "{} · {}",
                i.name.as_deref().unwrap_or("Drucker"),
                s.error().map_or("bereit".to_string(), |e| e.to_string())
            );
            match &i.material {
                Some(m) => line += &format!(
                    " · Etiketten {} × {} mm, Abstand {} mm{}",
                    m.width_mm,
                    m.length_mm,
                    m.gap_mm,
                    m.remaining.map_or(String::new(), |r| format!(", Zähler {r}"))
                ),
                None => line += " · keine Etiketten erkannt",
            }
            ui.label(RichText::new(line).weak());
        } else if !valid_addr(self.addr.trim()) {
            ui.label(RichText::new("Bluetooth-Adresse des Druckers eintragen (bluetoothctl devices).").weak());
        }
    }

    fn design_panel(&mut self, ui: &mut egui::Ui) {
        let d = &mut self.design;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut d.mode, Mode::Text, "Text");
            ui.selectable_value(&mut d.mode, Mode::Image, "Bild");
            ui.selectable_value(&mut d.mode, Mode::Test, "Testmuster");
        });
        ui.add_space(6.0);
        match d.mode {
            Mode::Text => {
                ui.add(egui::TextEdit::multiline(&mut d.text).desired_rows(2).desired_width(f32::INFINITY));
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("family")
                        .selected_text(&d.family)
                        .width(200.0)
                        .height(400.0)
                        .show_ui(ui, |ui| {
                            for f in &self.fonts.families {
                                ui.selectable_value(&mut d.family, f.clone(), f);
                            }
                        });
                    ui.checkbox(&mut d.bold, "Fett");
                    ui.checkbox(&mut d.auto_size, "Größe automatisch");
                    ui.add_enabled(!d.auto_size, egui::Slider::new(&mut d.size_px, 8.0..=110.0).suffix(" px"));
                });
            }
            Mode::Image => {
                ui.horizontal(|ui| {
                    let name = d.image.as_ref().map_or("Bilddatei ins Fenster ziehen (PNG/JPEG)".into(), |p| p.display().to_string());
                    ui.label(name);
                    if d.image.is_some() && ui.button("Entfernen").clicked() {
                        d.image = None;
                    }
                });
                ui.add(egui::Slider::new(&mut d.threshold, 1..=254).text("Schwelle"));
            }
            Mode::Test => {
                ui.label("Rahmen, X und Striche alle 5 mm.");
            }
        }
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut d.length_mm).range(5.0..=200.0).speed(0.5).suffix(" mm")).on_hover_text("Länge des Etiketts");
            ui.label("Länge");
            if d.mode != Mode::Test {
                ui.checkbox(&mut d.frame, "Rahmen");
                ui.checkbox(&mut d.invert, "Invertieren");
            }
        });
    }

    fn print_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut self.density, 1..=7).text("Dichte"));
            ui.checkbox(&mut self.rotate, "180° drehen");
            ui.checkbox(&mut self.mirror, "Spiegeln");
        });
        ui.horizontal(|ui| {
            let can = valid_addr(self.addr.trim()) && !self.busy && self.page.is_some();
            let btn = egui::Button::new(RichText::new("Drucken").strong()).min_size(egui::vec2(110.0, 30.0));
            if ui.add_enabled(can, btn).clicked()
                && let Some(page) = &self.page
            {
                let mut page = page.clone();
                if self.rotate {
                    page = page.rotated_180();
                }
                if self.mirror {
                    page = page.mirrored();
                }
                self.addr = self.addr.trim().to_uppercase();
                self.request(Cmd::Print { addr: self.addr.clone(), page, density: self.density });
            }
            if self.busy {
                ui.spinner();
            }
            if let Some((m, err)) = &self.message {
                let t = RichText::new(m);
                ui.label(if *err { t.color(ui.visuals().error_fg_color) } else { t });
            }
        });
    }
}

fn invert(page: &Page) -> Page {
    let mut p = Page::blank_dots(page.width());
    for x in 0..page.width() {
        for y in 0..HEAD_DOTS {
            if !page.get(x, y) {
                p.set(x, y);
            }
        }
    }
    p
}

fn preview_image(page: &Page) -> ColorImage {
    let (w, h) = (page.width(), HEAD_DOTS);
    let mut px = vec![Color32::WHITE; w * h];
    for x in 0..w {
        for y in 0..h {
            if page.get(x, y) {
                px[y * w + x] = Color32::BLACK;
            }
        }
    }
    ColorImage { size: [w, h], source_size: egui::vec2(w as f32, h as f32), pixels: px }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker();

        // Image files dropped onto the window.
        if let Some(path) = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone())) {
            self.design.image = Some(path);
            self.design.mode = Mode::Image;
        }
        self.update_page(ctx);

        egui::TopBottomPanel::top("printer").show(ctx, |ui| {
            ui.add_space(6.0);
            self.printer_panel(ui);
            ui.add_space(4.0);
        });
        egui::TopBottomPanel::bottom("print").show(ctx, |ui| {
            ui.add_space(6.0);
            self.print_panel(ui);
            ui.add_space(6.0);
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            self.design_panel(ui);
            ui.add_space(10.0);
            if let (Some(t), Some(page)) = (&self.preview, &self.page) {
                let size = egui::vec2(page.width() as f32, HEAD_DOTS as f32) * PREVIEW_SCALE;
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    egui::Frame::new().stroke(ui.visuals().widgets.noninteractive.bg_stroke).show(ui, |ui| {
                        ui.add(egui::Image::new((t.id(), size)));
                    });
                });
                ui.label(
                    RichText::new(format!(
                        "Vorschau in Druckauflösung: {} × {} Punkte = {:.0} × 12 mm (8 Punkte/mm)",
                        page.width(),
                        HEAD_DOTS,
                        page.width() as f32 / 8.0
                    ))
                    .weak()
                    .small(),
                );
            }
        });
    }
}
