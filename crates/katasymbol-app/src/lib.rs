//! Katasymbol app: design a label (text or image, pictograms, frame),
//! preview it at print resolution and print it on an E16 over BLE.
//! Runs natively (Linux, macOS, Windows) and in the browser (WebAssembly).

mod backend;
pub mod keyboard;
mod preview;
mod render;
mod sidebar;
mod style;

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
use backend::{BROWSER_CHOOSER, Cmd, Ev, Found, Worker};

/// Shown in the top bar: the build (commit and time, set by the build
/// scripts in `KATASYMBOL_BUILD`), else the crate version.
const BUILD: &str = match option_env!("KATASYMBOL_BUILD") {
    Some(b) => b,
    None => concat!("v", env!("CARGO_PKG_VERSION")),
};

/// An image the user opened, with its bytes when it cannot be re-read from
/// disk (web).
type ImageFile = (PathBuf, Option<Vec<u8>>);

/// Start the desktop app.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_native() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Katasymbol")
            .with_app_id("katasymbol")
            .with_inner_size([1240.0, 900.0])
            .with_min_inner_size([820.0, 560.0])
            .with_drag_and_drop(true)
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icon-256.png")).unwrap_or_default()),
        ..Default::default()
    };
    eframe::run_native("Katasymbol", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

/// Start the app in the `<canvas>` with id `canvas_id`.
#[cfg(target_arch = "wasm32")]
pub async fn run_web(canvas_id: &str) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;
    let canvas = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(canvas_id))
        .ok_or("canvas not found")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    eframe::WebRunner::new()
        .start(canvas, eframe::WebOptions::default(), Box::new(|cc| Ok(Box::new(App::new(cc)))))
        .await
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
/// Length of the frame-gallery thumbnails, dots.
pub const THUMB_LEN: usize = 240;
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
    /// Bytes of an image opened in the browser (no file system there).
    image_bytes: Option<(PathBuf, Vec<u8>)>,
    pub design: Design,
    rendered: Option<Design>,
    page: Option<Page>,
    preview: Option<TextureHandle>,
    thumbs: Vec<TextureHandle>,
    thumbs_key: Option<(katasymbol_core::label::Frame, String)>,
    pub slot: Slot,
    pub icon_group: usize,
    pub icon_groups: Vec<icons::Group>,
    pub icon_filter: String,
    icon_tex: std::collections::HashMap<char, TextureHandle>,
    pub frame_cat: katasymbol_core::label::Category,
    pub font_filter: String,
    file_rx: Option<Receiver<Option<ImageFile>>>,
    /// Drag of the length handle: (length at start, px per mm at start, accumulated px).
    drag: Option<(f32, f32, f32)>,
    addr: String,
    density: u8,
    copies: u32,
    connected: bool,
    busy: bool,
    info: Option<Info>,
    /// Single-column layout for phones.
    narrow: bool,
    device_name: Option<String>,
    found: Vec<Found>,
    message: Option<(String, bool)>,
    /// `KATASYMBOL_SCREENSHOT=file.png`: save a screenshot after a few frames and quit.
    screenshot: Option<(PathBuf, u32)>,
}

/// Linux needs a Bluetooth address; macOS/Windows use btleplug's device id.
#[cfg(target_os = "linux")]
fn valid_addr(a: &str) -> bool {
    let parts: Vec<&str> = a.split(':').collect();
    parts.len() == 6 && parts.iter().all(|p| p.len() == 2 && u8::from_str_radix(p, 16).is_ok())
}

#[cfg(not(target_os = "linux"))]
fn valid_addr(a: &str) -> bool {
    !a.trim().is_empty()
}

fn normalize_addr(a: &str) -> String {
    if cfg!(target_os = "linux") { a.trim().to_uppercase() } else { a.trim().to_owned() }
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        style::setup(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let worker = Worker::spawn(move || ctx.request_repaint());
        let fonts = Fonts::system();
        #[cfg(not(target_arch = "wasm32"))]
        let screenshot = std::env::var_os("KATASYMBOL_SCREENSHOT").map(|p| (PathBuf::from(p), 0));
        #[cfg(target_arch = "wasm32")]
        let screenshot: Option<(PathBuf, u32)> = None;
        let saved: Saved = if screenshot.is_some() {
            // A design that shows off frames and icons.
            let mut s = Saved::default();
            s.design.label.text = "Holzschrauben\n4 × 40".into();
            s.design.label.icon_left = Some('\u{e010}');
            s.design.label.separator = true;
            s.design.label.frame.style = katasymbol_core::label::FrameStyle::Nameplate;
            s.design.set_length_mm(50.0);
            s
        } else {
            cc.storage.and_then(|s| eframe::get_value(s, eframe::APP_KEY)).unwrap_or_default()
        };
        let mut design = saved.design;
        if !fonts.has(&design.family) {
            design.family = fonts.default_family();
        }
        let frame_cat = design.label.frame.style.category();
        let addr = if BROWSER_CHOOSER { String::new() } else { std::env::var("KATASYMBOL_ADDR").ok().unwrap_or(saved.addr) };
        Self {
            worker,
            fonts,
            font: None,
            icon_font: icons::font(),
            picture: None,
            image_bytes: None,
            design,
            rendered: None,
            page: None,
            preview: None,
            thumbs: Vec::new(),
            thumbs_key: None,
            slot: Slot::Left,
            icon_group: 0,
            icon_groups: icons::groups(),
            icon_filter: String::new(),
            icon_tex: Default::default(),
            frame_cat,
            font_filter: String::new(),
            file_rx: None,
            drag: None,
            addr,
            density: saved.density.clamp(1, 7),
            copies: saved.copies.clamp(1, 99),
            connected: false,
            busy: false,
            info: None,
            narrow: false,
            device_name: None,
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
                let pic = match &self.image_bytes {
                    Some((q, bytes)) if q == p => render::picture_from_bytes(bytes)?,
                    _ if cfg!(target_arch = "wasm32") => {
                        // A remembered name without its bytes: ask again.
                        self.design.image = None;
                        return Ok(());
                    }
                    _ => render::load_picture(p)?,
                };
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
                        let natural = self.design.label.natural_length(font, &self.icon_font, picture);
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
                        length: THUMB_LEN,
                        end_margin: 0.0,
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

    /// A white-on-transparent texture of an icon, for tinting in the UI.
    pub fn icon_texture(&mut self, ctx: &egui::Context, ch: char) -> TextureHandle {
        if let Some(t) = self.icon_tex.get(&ch) {
            return t.clone();
        }
        // Wide pictograms get a wide texture of the same height.
        const N: usize = 64;
        let w = (N as f32 * icons::aspect(ch)).round() as usize;
        let mut c = katasymbol_core::draw::Canvas::new(w, N);
        icons::draw(&mut c, &self.icon_font, ch, katasymbol_core::draw::Rect::new(2.0, 2.0, w as f32 - 2.0, N as f32 - 2.0));
        let pixels = (0..w * N).map(|i| Color32::from_white_alpha((c.get(i % w, i / w) * 255.0) as u8)).collect();
        let img = ColorImage { size: [w, N], source_size: egui::vec2(w as f32, N as f32), pixels };
        let t = ctx.load_texture(format!("icon-{:x}", ch as u32), img, TextureOptions::LINEAR);
        self.icon_tex.insert(ch, t.clone());
        t
    }

    pub fn pick_image(&mut self, ctx: &egui::Context) {
        let (tx, rx) = std::sync::mpsc::channel::<Option<ImageFile>>();
        let dialog = rfd::AsyncFileDialog::new().set_title("Bild wählen").add_filter("Bilder", &["png", "jpg", "jpeg"]);
        let ctx = ctx.clone();
        let task = async move {
            let file = match dialog.pick_file().await {
                Some(f) if cfg!(target_arch = "wasm32") => Some((PathBuf::from(f.file_name()), Some(f.read().await))),
                #[cfg(not(target_arch = "wasm32"))]
                Some(f) => Some((f.path().to_path_buf(), None)),
                #[cfg(target_arch = "wasm32")]
                Some(_) => None,
                None => None,
            };
            let _ = tx.send(file);
            ctx.request_repaint();
        };
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(task);
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || pollster::block_on(task));
        self.file_rx = Some(rx);
    }

    fn open_image(&mut self, (path, bytes): ImageFile) {
        if let Some(b) = bytes {
            self.image_bytes = Some((path.clone(), b));
        }
        self.picture = None;
        self.design.image = Some(path);
        self.design.mode = Mode::Image;
        self.rendered = None;
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.file_rx
            && let Ok(file) = rx.try_recv()
        {
            self.file_rx = None;
            if let Some(f) = file {
                self.open_image(f);
            }
        }
        // Files dropped onto the window (native: a path; web: the bytes).
        let dropped = ctx.input(|i| {
            i.raw.dropped_files.iter().find_map(|f| match (&f.path, &f.bytes) {
                (Some(p), _) => Some((p.clone(), None)),
                (None, Some(b)) => Some((PathBuf::from(&f.name), Some(b.to_vec()))),
                _ => None,
            })
        });
        if let Some(f) = dropped {
            self.open_image(f);
        }
        while let Ok(ev) = self.worker.rx.try_recv() {
            match ev {
                Ev::Device { addr, name } => {
                    self.addr = addr;
                    self.device_name = Some(name);
                }
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
                Ev::Printed => {
                    // Ready for the next job even if the status refresh after
                    // printing fails (the printer may still be finishing).
                    self.busy = false;
                    self.message = Some(("Gedruckt.".into(), false));
                }
                Ev::Error(e) => {
                    self.message = Some((e, true));
                    self.busy = false;
                }
            }
        }
    }

    fn connect(&mut self) {
        self.addr = normalize_addr(&self.addr);
        if !BROWSER_CHOOSER && valid_addr(&self.addr) {
            self.busy = true;
            self.message = None;
            self.worker.send(Cmd::Probe { addr: self.addr.clone() });
        } else {
            self.scan(false);
        }
    }

    /// Native: look for printers. Web: open the chooser (`all`: every device).
    fn scan(&mut self, all: bool) {
        self.busy = true;
        self.message = None;
        self.worker.send(Cmd::Scan { all });
    }

    fn print(&mut self) {
        let Some(page) = self.page.clone() else { return };
        self.addr = normalize_addr(&self.addr);
        // In the browser a remembered printer reconnects by itself.
        if BROWSER_CHOOSER && !self.connected && self.device_name.is_none() {
            self.message = Some(("Nicht verbunden – erst „Verbinden“ klicken.".into(), true));
            return;
        }
        if !BROWSER_CHOOSER && !valid_addr(&self.addr) {
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
            if !self.narrow {
                ui.label(RichText::new(BUILD).small().weak()).on_hover_text("Version dieses Programms");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Address and found devices, tucked away (not in the browser,
                // where the browser's chooser picks the device).
                if !BROWSER_CHOOSER {
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
                } else {
                    ui.menu_button(RichText::new(I_COG).size(16.0), |ui| {
                        ui.set_min_width(260.0);
                        if !self.connected {
                            ui.label(RichText::new("Drucker nicht in der Liste?").small().weak());
                            if ui.add_enabled(!self.busy, egui::Button::new(format!("{I_BLUETOOTH}  Alle Geräte zeigen"))).clicked() {
                                self.scan(true);
                                ui.close();
                            }
                            ui.separator();
                        }
                        ui.label(RichText::new("Für die Fehlersuche").small().weak());
                        if ui.button("Protokoll kopieren").clicked() {
                            let text = format!("Katasymbol {BUILD}\n{}", backend::link_trace());
                            ui.ctx().copy_text(text);
                            self.message = Some(("Protokoll kopiert – jetzt z. B. in eine Nachricht einfügen.".into(), false));
                            ui.close();
                        }
                    });
                }
                if self.connected {
                    if ui.add_enabled(!self.busy, egui::Button::new(format!("{I_UNLINK}  Trennen"))).clicked() {
                        self.device_name = None;
                        self.worker.send(Cmd::Disconnect);
                    }
                } else if ui.add_enabled(!self.busy, egui::Button::new(format!("{I_BLUETOOTH}  Verbinden"))).clicked() {
                    self.connect();
                }
                if !BROWSER_CHOOSER && ui.add_enabled(!self.busy, egui::Button::new((ic(I_SEARCH), "Suchen"))).clicked() {
                    self.scan(false);
                }
                if !self.narrow {
                    ui.add_space(6.0);
                    self.status_chip(ui);
                }
            });
        });
        if self.narrow {
            ui.horizontal(|ui| {
                self.status_chip(ui);
                // Just the commit on phones; the full build on hover.
                let short = BUILD.split(' ').next().unwrap_or(BUILD);
                ui.label(RichText::new(short).small().weak()).on_hover_text(BUILD);
            });
        }
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
            _ if self.connected && !self.busy => (Color32::from_rgb(0xd9, 0x82, 0x1e), "Verbunden – Drucker antwortet nicht".into()),
            _ if BROWSER_CHOOSER && self.device_name.is_some() => {
                (Color32::GRAY, format!("{} schläft – verbindet sich beim Drucken neu", self.device_name.as_deref().unwrap_or("Drucker")))
            }
            _ if BROWSER_CHOOSER => (Color32::GRAY, "Nicht verbunden – „Verbinden“ klicken".into()),
            _ if valid_addr(self.addr.trim()) => {
                let who = self.found.iter().find(|f| f.addr == self.addr.trim()).map_or(self.addr.trim(), |f| f.name.as_str());
                (Color32::GRAY, format!("Getrennt  ·  {who}"))
            }
            _ => (Color32::GRAY, "Kein Drucker – „Suchen“ klicken".into()),
        };
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .corner_radius(14)
            .inner_margin(egui::Margin::symmetric(10, 4))
            .show(ui, |ui| {
                // In the right-to-left top bar, what is added last ends up leftmost.
                let rtl = ui.layout().prefer_right_to_left();
                ui.horizontal(|ui| {
                    let dot = RichText::new(I_CIRCLE).color(color).size(10.0);
                    if !rtl {
                        ui.label(dot.clone());
                    }
                    ui.label(RichText::new(text).size(13.0));
                    if rtl {
                        ui.label(dot);
                    }
                });
            });
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        if self.narrow {
            ui.horizontal(|ui| {
                ui.label(RichText::new(I_ADJUST));
                ui.add(egui::Slider::new(&mut self.density, 1..=7)).on_hover_text("Dichte: 1 = hell, 7 = dunkel");
                ui.label(RichText::new(I_COPY));
                ui.add(egui::DragValue::new(&mut self.copies).range(1..=99).suffix(" ×"));
            });
            if let Some((m, err)) = &self.message {
                let t = RichText::new(m).small();
                ui.label(if *err { t.color(ui.visuals().error_fg_color) } else { t });
            }
            let can = !self.busy && self.page.is_some();
            let w = ui.available_width();
            if ui.add_enabled(can, primary(format!("{I_PRINT}  Drucken")).min_size(egui::vec2(w, 40.0))).clicked() {
                self.print();
            }
            return;
        }
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

    #[cfg(target_arch = "wasm32")]
    fn handle_screenshot(&mut self, _ctx: &egui::Context) {}

    #[cfg(not(target_arch = "wasm32"))]
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
        keyboard::begin_frame(ctx);
        self.poll(ctx);
        self.update_page(ctx);

        // Phones and narrow windows: one scrolling column, preview on top.
        self.narrow = ctx.screen_rect().width() < 760.0;
        let bar = egui::Margin::symmetric(if self.narrow { 10 } else { 14 }, if self.narrow { 8 } else { 10 });
        egui::TopBottomPanel::top("top")
            .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(bar))
            .show(ctx, |ui| self.top_bar(ui));
        egui::TopBottomPanel::bottom("bottom")
            .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(bar))
            .show(ctx, |ui| self.bottom_bar(ui));
        if self.narrow {
            egui::CentralPanel::default()
                .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(egui::Margin::same(10)))
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        let w = ui.available_width();
                        ui.allocate_ui(egui::vec2(w, 300.0), |ui| {
                            ui.set_min_height(300.0);
                            self.preview(ui);
                        });
                        ui.separator();
                        self.mode_tabs(ui);
                        ui.add_space(6.0);
                        self.sections(ui);
                    });
                });
        } else {
        egui::SidePanel::left("side")
            .resizable(true)
            .default_width(370.0)
            .width_range(320.0..=520.0)
            .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(12, 10)))
            .show(ctx, |ui| self.sidebar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(egui::Margin::same(18)))
            .show(ctx, |ui| self.preview(ui));
        }

        // Re-render right away if a control changed the design this frame.
        if self.rendered.as_ref() != Some(&self.design) {
            ctx.request_repaint();
        }
        keyboard::end_frame(ctx);
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
