//! On-screen keyboard for text fields in mobile browsers.
//!
//! iOS opens the keyboard only when an input gets focus *while* a tap is
//! being handled. egui focuses its hidden text input a frame later, so on
//! iPhones (Safari, Bluefy) tapping a text field showed no keyboard. Each
//! frame records where the text fields are; the page asks [`text_field_at`]
//! in its own `touchend` handler and focuses eframe's text input right away.

use eframe::egui;
use std::cell::{Cell, RefCell};

thread_local! {
    static FIELDS: RefCell<Vec<egui::Rect>> = const { RefCell::new(Vec::new()) };
    static SCREEN_WIDTH: Cell<f32> = const { Cell::new(0.0) };
}

/// Call at the start of each frame.
pub fn begin_frame(ctx: &egui::Context) {
    FIELDS.with(|f| f.borrow_mut().clear());
    SCREEN_WIDTH.with(|w| w.set(ctx.screen_rect().width()));
}

/// Register a text field (its visible part).
pub fn field(ui: &egui::Ui, response: &egui::Response) {
    let r = response.rect.intersect(ui.clip_rect());
    if r.is_positive() {
        FIELDS.with(|f| f.borrow_mut().push(r));
    }
}

/// Whether the position (`x`, `y`) in CSS pixels of a canvas `css_width`
/// pixels wide lies on a text field.
pub fn text_field_at(x: f32, y: f32, css_width: f32) -> bool {
    let points = SCREEN_WIDTH.with(Cell::get);
    let scale = if points > 0.0 && css_width > 0.0 { points / css_width } else { 1.0 };
    let p = egui::pos2(x * scale, y * scale);
    FIELDS.with(|f| f.borrow().iter().any(|r| r.expand(4.0).contains(p)))
}
