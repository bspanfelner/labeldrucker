//! Katasymbol desktop app: design a label (text or image, pictograms, frame),
//! preview it at print resolution and print it on an E16 over BLE.

mod preview;
mod render;
mod scan;
mod sidebar;
mod style;
mod worker;

use eframe::egui::{self, Color32, ColorImage, RichText, TextureHandle, TextureOptions};
use katasymbol_core::label::{Label, Picture};
use katasymbol_core::page::{DOTS_PER_MM, HEAD_DOTS};
use katasymbol_core::printer::Info;
use katasymbol_core::text::{FontRef, FontVec};
use katasymbol_core::{Page, icons};
use render::Fonts;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use style::*;
use worker::{Cmd, Ev, Worker};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Katasymbol")
            .with_app_id("katasymbol")
            .with_inner_size([1240.0, 900.0])
            .with_min_inner_size([820.0, 560.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("Katasymbol", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Mode {
    #[default]
    Text,
    Image,
    Test,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Slot {
    #[default]
    Left,
    Right,
}

/// Everything that determines the printed page.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Design {
    pub mode: Mode,
    pub label: Label,
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    pub image: Option<PathBuf>,
    pub threshold: u8,
    /// Length follows the content.
    pub auto_length: bool,
}

impl Default for Design {
    fn default() -> Self {
        Self {
            mode: Mode::Text,
            label: Label { text: "Hallo E16".into(), ..Label::default() },
            family: String::new(),
            bold: true,
            italic: false,
            image: None,
            threshold: 128,
            auto_length: false,
        }
    }
}

impl Design {
    pub fn length_mm(&self) -> f32 {
        self.label.length as f32 / DOTS_PER_MM as f32
    }
    pub fn set_length_mm(&mut self, mm: f32) {
        self.label.length = (mm.clamp(MIN_MM, MAX_MM).round() as usize) * DOTS_PER_MM;
    }
}

pub const MIN_MM: f32 = 10.0;
pub const MAX_MM: f32 = 200.0;

/// What survives a restart.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    design: Design,
    addr: String,
    density: u8,
    copies: u32,
}

impl Default for Saved {
    fn default() -> Self {
        Self { design: Design::default(), addr: String::new(), density: 4, copies: 1 }
    }
}

pub struct App {
    worker: Worker,
    fonts: Fonts,
    font: Option<((String, bool, bool), FontVec)>,
    icon_font: FontRef<'static>,
    picture: Option<(PathBuf, Picture)>,
    pub design: Design,
    rendered: Option<Design>,
    page: Option<Page>,
    preview: Option<TextureHandle>,
    thumbs: Vec<TextureHandle>,
    thumbs_key: Option<(katasymbol_core::label::Frame, String)>,
    pub slot: Slot,
    pub icon_group: usize,
    pub font_filter: String,
    file_rx: Option<Receiver<Option<PathBuf>>>,
    /// Drag of the length handle: (length at start, px per mm at start, accumulated px).
    drag: Option<(f32, f32, f32)>,
    addr: String,
    density: u8,
    copies: u32,
    connected: bool,
    busy: bool,
    info: Option<Info>,
    found: Vec<scan::Found>,
    message: Option<(String, bool)>,
    /// `KATASYMBOL_SCREENSHOT=file.png`: save a screenshot after a few frames and quit.
    screenshot: Option<(PathBuf, u32)>,
}

fn valid_addr(a: &str) -> bool {
    let parts: Vec<&str> = a.split(':').collect();
    parts.len() == 6 && parts.iter().all(|p| p.len() == 2 && u8::from_str_radix(p, 16).is_ok())
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        style::setup(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let worker = Worker::spawn(move || ctx.request_repaint());
        let fonts = Fonts::system();
        let screenshot = std::env::var_os("KATASYMBOL_SCREENSHOT").map(|p| (PathBuf::from(p), 0));
        let saved: Saved = if screenshot.is_some() {
            // A design that shows off frames and icons.
            let mut s = Saved::default();
            s.design.label.text = "Gewürze\nOregano".into();
            s.design.label.icon_left = Some('\u{f0f5}');
            s.design.label.separator = true;
            s.design.label.frame.style = katasymbol_core::label::FrameStyle::Double;
            s.design.set_length_mm(50.0);
            s
        } else {
            cc.storage.and_then(|s| eframe::get_value(s, eframe::APP_KEY)).unwrap_or_default()
        };
        let mut design = saved.design;
        if !fonts.has(&design.family) {
            design.family = fonts.default_family();
        }
        let addr = std::env::var("KATASYMBOL_ADDR").ok().unwrap_or(saved.addr);
        Self {
            worker,
            fonts,
            font: None,
            icon_font: icons::font(),
            picture: None,
            design,
            rendered: None,
            page: None,
            preview: None,
            thumbs: Vec::new(),
            thumbs_key: None,
            slot: Slot::Left,
            icon_group: 0,
            font_filter: String::new(),
            file_rx: None,
            drag: None,
            addr,
            density: saved.density.clamp(1, 7),
            copies: saved.copies.clamp(1, 99),
            connected: false,
            busy: false,
            info: None,
            found: Vec::new(),
            message: None,
            screenshot,
        }
    }

    fn ensure_font(&mut self) -> bool {
        let key = (self.design.family.clone(), self.design.bold, self.design.italic);
        if self.font.as_ref().map(|(k, _)| k) != Some(&key) {
            self.font = self.fonts.load(&key.0, key.1, key.2).map(|f| (key, f));
        }
        self.font.is_some()
    }

    fn ensure_picture(&mut self) -> Result<(), String> {
        match &self.design.image {
            None => {
                self.picture = None;
                Ok(())
            }
            Some(p) if self.picture.as_ref().is_some_and(|(q, _)| q == p) => Ok(()),
            Some(p) => {
                let pic = render::load_picture(p)?;
                self.picture = Some((p.clone(), pic));
                Ok(())
            }
        }
    }

    /// The material's label length in mm, if the printer reported one.
    pub fn material_length(&self) -> Option<f32> {
        self.info.as_ref()?.material.as_ref().map(|m| f32::from(m.length_mm)).filter(|l| *l > 0.0)
    }

    pub fn material_width(&self) -> f32 {
        self.info
            .as_ref()
            .and_then(|i| i.material.as_ref())
            .map(|m| f32::from(m.width_mm))
            .filter(|w| *w >= 12.0)
            .unwrap_or(15.0)
    }

    /// Re-render the page and its preview if the design changed.
    fn update_page(&mut self, ctx: &egui::Context) {
        if self.rendered.as_ref() == Some(&self.design) {
            return;
        }
        let result = (|| -> Result<Page, String> {
            match self.design.mode {
                Mode::Test => Ok(Page::test_pattern(self.design.length_mm().round() as usize)),
                Mode::Text | Mode::Image => {
                    if !self.ensure_font() {
                        return Err(format!("Schrift „{}“ nicht ladbar", self.design.family));
                    }
                    let picture = if self.design.mode == Mode::Image {
                        self.ensure_picture()?;
                        self.picture.as_mut().map(|(_, p)| {
                            p.threshold = self.design.threshold;
                            &*p
                        })
                    } else {
                        None
                    };
                    let font = &self.font.as_ref().unwrap().1;
                    if self.design.auto_length {
                        let natural = self.design.label.natural_length(font, picture);
                        self.design.set_length_mm((natural as f32 / DOTS_PER_MM as f32).ceil());
                    }
                    Ok(self.design.label.render(font, &self.icon_font, picture))
                }
            }
        })();
        match result {
            Ok(page) => {
                let img = page_image(&page);
                match &mut self.preview {
                    Some(t) => t.set(img, TextureOptions::NEAREST),
                    None => self.preview = Some(ctx.load_texture("preview", img, TextureOptions::NEAREST)),
                }
                self.page = Some(page);
                if matches!(&self.message, Some((_, true))) && !self.busy {
                    self.message = None;
                }
            }
            Err(e) => {
                self.page = None;
                self.message = Some((e, true));
            }
        }
        self.rendered = Some(self.design.clone());
    }

    /// Frame-gallery thumbnails, re-rendered when the frame settings change.
    pub fn frame_thumbs(&mut self, ctx: &egui::Context) -> &[TextureHandle] {
        use katasymbol_core::label::FrameStyle;
        let mut base = self.design.label.frame;
        base.style = FrameStyle::None;
        let key = (base, self.design.family.clone());
        if self.thumbs_key.as_ref() != Some(&key) && self.ensure_font() {
            let font = &self.font.as_ref().unwrap().1;
            self.thumbs = FrameStyle::ALL
                .iter()
                .map(|&style| {
                    let label = Label {
                        length: 208,
                        text: "Abc".into(),
                        frame: katasymbol_core::label::Frame { style, ..base },
                        ..Label::default()
                    };
                    let img = page_image(&label.render(font, &self.icon_font, None));
                    ctx.load_texture(format!("thumb-{style:?}"), img, TextureOptions::LINEAR)
                })
                .collect();
            self.thumbs_key = Some(key);
        }
        &self.thumbs
    }

    pub fn pick_image(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new()
                .set_title("Bild wählen")
                .add_filter("Bilder", &["png", "jpg", "jpeg"])
                .pick_file();
            let _ = tx.send(file);
        });
        self.file_rx = Some(rx);
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.file_rx
            && let Ok(file) = rx.try_recv()
        {
            self.file_rx = None;
            if let Some(f) = file {
                self.design.image = Some(f);
                self.design.mode = Mode::Image;
            }
        }
        if let Some(path) = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone())) {
            self.design.image = Some(path);
            self.design.mode = Mode::Image;
        }
        while let Ok(ev) = self.worker.rx.try_recv() {
            match ev {
                Ev::Connected(c) => {
                    self.connected = c;
                    if !c {
                        self.info = None;
                    }
                }
                Ev::Info(info) => {
                    let first = self.info.is_none();
                    self.info = Some(info);
                    if first && let Some(len) = self.material_length() && !self.design.auto_length {
                        self.design.set_length_mm(len);
                    }
                    self.busy = false;
                    if matches!(&self.message, Some((m, false)) if m.ends_with('…')) {
                        self.message = None;
                    }
                }
                Ev::Found(found) => {
                    self.busy = false;
                    let printers: Vec<_> = found.iter().filter(|f| f.likely).collect();
                    self.message = Some(match printers.as_slice() {
                        [] if found.is_empty() => ("Nichts gefunden. Ist der Drucker an?".into(), true),
                        [] => ("Kein Drucker erkannt – bitte unter dem Zahnrad aus der Liste wählen.".into(), true),
                        [p] => {
                            self.addr = p.addr.clone();
                            self.busy = true;
                            self.worker.send(Cmd::Probe { addr: self.addr.clone() });
                            (format!("Drucker gefunden: {}", p.name), false)
                        }
                        _ => ("Mehrere Drucker gefunden – bitte unter dem Zahnrad wählen.".into(), false),
                    });
                    self.found = found;
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

    fn connect(&mut self) {
        self.addr = self.addr.trim().to_uppercase();
        if valid_addr(&self.addr) {
            self.busy = true;
            self.message = None;
            self.worker.send(Cmd::Probe { addr: self.addr.clone() });
        } else {
            self.scan();
        }
    }

    fn scan(&mut self) {
        self.busy = true;
        self.message = None;
        self.worker.send(Cmd::Scan);
    }

    fn print(&mut self) {
        let Some(page) = self.page.clone() else { return };
        self.addr = self.addr.trim().to_uppercase();
        if !valid_addr(&self.addr) {
            self.message = Some(("Kein Drucker gewählt – erst „Suchen“.".into(), true));
            return;
        }
        self.busy = true;
        self.message = None;
        self.worker.send(Cmd::Print { addr: self.addr.clone(), page, density: self.density, copies: self.copies });
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{I_TAG}  Katasymbol")).heading().strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Address and found devices, tucked away.
                ui.menu_button(RichText::new(I_COG).size(16.0), |ui| {
                    ui.set_min_width(300.0);
                    ui.label(RichText::new("Bluetooth-Adresse").small().weak());
                    ui.add(egui::TextEdit::singleline(&mut self.addr).hint_text("AA:BB:CC:DD:EE:FF").desired_width(280.0));
                    if !self.found.is_empty() {
                        ui.separator();
                        ui.label(RichText::new("Gefundene Geräte").small().weak());
                        for f in &self.found {
                            let label = format!("{}{}   {}", if f.likely { "★ " } else { "" }, f.name, f.addr);
                            if ui.selectable_label(self.addr == f.addr, label).clicked() {
                                self.addr = f.addr.clone();
                                ui.close();
                            }
                        }
                    }
                });
                if self.connected {
                    if ui.add_enabled(!self.busy, egui::Button::new(format!("{I_UNLINK}  Trennen"))).clicked() {
                        self.worker.send(Cmd::Disconnect);
                    }
                } else if ui.add_enabled(!self.busy, egui::Button::new(format!("{I_BLUETOOTH}  Verbinden"))).clicked() {
                    self.connect();
                }
                if ui.add_enabled(!self.busy, egui::Button::new((ic(I_SEARCH), "Suchen"))).clicked() {
                    self.scan();
                }
                ui.add_space(6.0);
                self.status_chip(ui);
            });
        });
    }

    fn status_chip(&self, ui: &mut egui::Ui) {
        let (color, text) = match &self.info {
            Some(i) if self.connected => {
                let state = i.status.error().map_or("bereit".to_string(), |e| e.to_string());
                let labels = i.material.as_ref().map_or(String::new(), |m| {
                    format!("  ·  {} × {} mm{}", m.width_mm, m.length_mm, m.remaining.map_or(String::new(), |r| format!("  ·  Zähler {r}")))
                });
                let color = if i.status.error().is_some() { Color32::from_rgb(0xd9, 0x82, 0x1e) } else { OK };
                (color, format!("{}  ·  {state}{labels}", i.name.as_deref().unwrap_or("Drucker")))
            }
            _ if valid_addr(self.addr.trim()) => (Color32::GRAY, format!("Getrennt  ·  {}", self.addr.trim())),
            _ => (Color32::GRAY, "Kein Drucker – „Suchen“ klicken".into()),
        };
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .corner_radius(14)
            .inner_margin(egui::Margin::symmetric(10, 4))
            .show(ui, |ui| {
                // Inside the right-to-left top bar: added last = leftmost.
                ui.horizontal(|ui| {
                    ui.label(RichText::new(text).size(13.0));
                    ui.label(RichText::new(I_CIRCLE).color(color).size(10.0));
                });
            });
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{I_ADJUST}  Dichte")).strong());
            ui.add(egui::Slider::new(&mut self.density, 1..=7).show_value(true))
                .on_hover_text("1 = hell, 7 = dunkel (Standard 4)");
            ui.add_space(12.0);
            ui.label(RichText::new(format!("{I_COPY}  Kopien")).strong());
            if ui.add_enabled(self.copies > 1, egui::Button::new(I_MINUS)).clicked() {
                self.copies -= 1;
            }
            ui.add(egui::DragValue::new(&mut self.copies).range(1..=99));
            if ui.add_enabled(self.copies < 99, egui::Button::new(I_PLUS)).clicked() {
                self.copies += 1;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can = !self.busy && self.page.is_some();
                let label = if self.copies > 1 { format!("{I_PRINT}  {} × drucken", self.copies) } else { format!("{I_PRINT}  Drucken") };
                if ui.add_enabled(can, primary(label)).clicked() {
                    self.print();
                }
                ui.add_space(8.0);
                if let Some((m, err)) = &self.message {
                    let t = RichText::new(m);
                    ui.label(if *err { t.color(ui.visuals().error_fg_color) } else { t });
                }
                if self.busy {
                    ui.spinner();
                }
            });
        });
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some((path, frames)) = &mut self.screenshot else { return };
        *frames += 1;
        if *frames == 8 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let [w, h] = img.size;
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                let _ = buf.save(&*path);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

/// The page as black-on-white pixels.
pub fn page_image(page: &Page) -> ColorImage {
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
        self.poll(ctx);
        self.update_page(ctx);

        egui::TopBottomPanel::top("top")
            .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(14, 10)))
            .show(ctx, |ui| self.top_bar(ui));
        egui::TopBottomPanel::bottom("bottom")
            .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(14, 10)))
            .show(ctx, |ui| self.bottom_bar(ui));
        egui::SidePanel::left("side")
            .resizable(true)
            .default_width(370.0)
            .width_range(320.0..=520.0)
            .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(12, 10)))
            .show(ctx, |ui| self.sidebar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(egui::Margin::same(18)))
            .show(ctx, |ui| self.preview(ui));

        // Re-render right away if a control changed the design this frame.
        if self.rendered.as_ref() != Some(&self.design) {
            ctx.request_repaint();
        }
        self.handle_screenshot(ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if self.screenshot.is_some() {
            return;
        }
        let saved = Saved { design: self.design.clone(), addr: self.addr.trim().to_owned(), density: self.density, copies: self.copies };
        eframe::set_value(storage, eframe::APP_KEY, &saved);
    }
}
