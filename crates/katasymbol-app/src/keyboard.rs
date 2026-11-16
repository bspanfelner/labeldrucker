//! Text fields that work with the on-screen keyboard in mobile browsers.
//!
//! iOS opens the keyboard only for an input focused by the tap itself, and
//! egui (drawing everything on a canvas) focuses its hidden input a frame
//! later, so on iPhones tapping a text field showed no keyboard. On touch
//! devices in the browser [`text_edit`] therefore lays a real HTML
//! `<textarea>`/`<input>` over the (non-interactive) egui field and keeps
//! both in sync. Everywhere else it is just egui's `TextEdit`.

use eframe::egui;

/// A text field. `make` builds the egui `TextEdit` for `text` (with `hint`
/// as its hint text); `key` identifies the field across frames;
/// `multiline` picks `<textarea>` over `<input>`.
pub fn text_edit<'t>(
    ui: &mut egui::Ui,
    key: &'static str,
    text: &'t mut String,
    multiline: bool,
    hint: &str,
    make: impl FnOnce(&'t mut String) -> egui::TextEdit<'t>,
) -> egui::Response {
    #[cfg(target_arch = "wasm32")]
    if web::touch_device() {
        return web::overlay(ui, key, text, multiline, hint, make);
    }
    let _ = (key, multiline);
    ui.add(make(text).hint_text(hint))
}

/// Call at the start of each frame.
pub fn begin_frame(_ctx: &egui::Context) {
    #[cfg(target_arch = "wasm32")]
    web::begin_frame();
}

/// Call at the end of each frame: hides the overlays of fields not shown.
pub fn end_frame(_ctx: &egui::Context) {
    #[cfg(target_arch = "wasm32")]
    web::end_frame();
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::egui;
    use std::cell::{Cell, RefCell};
    use std::collections::{HashMap, HashSet};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    struct Field {
        el: web_sys::HtmlElement,
        multiline: bool,
        style_key: String,
        _on_event: Closure<dyn FnMut(web_sys::Event)>,
    }

    thread_local! {
        static TOUCH: Cell<Option<bool>> = const { Cell::new(None) };
        static FIELDS: RefCell<HashMap<&'static str, Field>> = RefCell::new(HashMap::new());
        static USED: RefCell<HashSet<&'static str>> = RefCell::new(HashSet::new());
    }

    pub fn touch_device() -> bool {
        TOUCH.with(|t| {
            let v = t.get().unwrap_or_else(|| web_sys::window().is_some_and(|w| w.navigator().max_touch_points() > 0));
            t.set(Some(v));
            v
        })
    }

    pub fn begin_frame() {
        USED.with(|u| u.borrow_mut().clear());
    }

    pub fn end_frame() {
        USED.with(|used| {
            let used = used.borrow();
            FIELDS.with(|f| {
                for (key, field) in f.borrow().iter() {
                    // A field no longer shown at all also gives up the keyboard.
                    if !used.contains(key) {
                        hide(&field.el);
                        if is_focused(&field.el) {
                            let _ = field.el.blur();
                        }
                    }
                }
            });
        });
    }

    fn hide(el: &web_sys::HtmlElement) {
        let _ = el.style().set_property("display", "none");
    }

    fn is_focused(el: &web_sys::HtmlElement) -> bool {
        web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.active_element())
            .is_some_and(|a| {
                let a: &wasm_bindgen::JsValue = a.as_ref();
                let e: &wasm_bindgen::JsValue = el.as_ref();
                a == e
            })
    }

    fn value(f: &Field) -> String {
        if f.multiline {
            f.el.unchecked_ref::<web_sys::HtmlTextAreaElement>().value()
        } else {
            f.el.unchecked_ref::<web_sys::HtmlInputElement>().value()
        }
    }

    fn set_value(f: &Field, v: &str) {
        if f.multiline {
            f.el.unchecked_ref::<web_sys::HtmlTextAreaElement>().set_value(v);
        } else {
            f.el.unchecked_ref::<web_sys::HtmlInputElement>().set_value(v);
        }
    }

    fn create(ctx: &egui::Context, multiline: bool, hint: &str) -> Option<Field> {
        let doc = web_sys::window()?.document()?;
        let el: web_sys::HtmlElement = doc.create_element(if multiline { "textarea" } else { "input" }).ok()?.unchecked_into();
        let _ = el.set_attribute("placeholder", hint);
        let _ = el.set_attribute("autocomplete", "off");
        let _ = el.set_attribute("spellcheck", "false");
        if !multiline {
            let _ = el.set_attribute("type", "text");
            let _ = el.set_attribute("enterkeyhint", "done");
            let _ = el.set_attribute("autocapitalize", "off");
            let _ = el.set_attribute("autocorrect", "off");
        }
        let s = el.style();
        for (k, v) in [
            ("position", "absolute"),
            ("display", "none"),
            ("z-index", "5"),
            ("box-sizing", "border-box"),
            ("margin", "0"),
            ("border-width", "1px"),
            ("border-style", "solid"),
            ("border-radius", "4px"),
            ("outline", "none"),
            ("resize", "none"),
            ("-webkit-appearance", "none"),
            // 16 px or more: iOS does not zoom in on focus.
            ("font", if multiline { "17px system-ui, -apple-system, sans-serif" } else { "16px system-ui, -apple-system, sans-serif" }),
            ("line-height", "1.3"),
        ] {
            let _ = s.set_property(k, v);
        }
        // Repaint on typing (the canvas sees none of these events); Enter
        // in a one-line field closes the keyboard.
        let ctx = ctx.clone();
        let target = el.clone();
        let on_event = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            if !multiline && e.dyn_ref::<web_sys::KeyboardEvent>().is_some_and(|k| k.key() == "Enter") {
                let _ = target.blur();
            }
            ctx.request_repaint();
        });
        for ev in ["input", "focus", "blur", "keydown"] {
            let _ = el.add_event_listener_with_callback(ev, on_event.as_ref().unchecked_ref());
        }
        doc.body()?.append_child(&el).ok()?;
        Some(Field { el, multiline, style_key: String::new(), _on_event: on_event })
    }

    fn css(c: egui::Color32) -> String {
        format!("rgba({},{},{},{:.3})", c.r(), c.g(), c.b(), f32::from(c.a()) / 255.0)
    }

    pub fn overlay<'t>(
        ui: &mut egui::Ui,
        key: &'static str,
        text: &'t mut String,
        multiline: bool,
        hint: &str,
        make: impl FnOnce(&'t mut String) -> egui::TextEdit<'t>,
    ) -> egui::Response {
        USED.with(|u| u.borrow_mut().insert(key));
        let ctx = ui.ctx().clone();
        // Sync first: what was typed since the last frame goes into `text`.
        let typed = FIELDS.with(|f| {
            let mut f = f.borrow_mut();
            if !f.contains_key(key) {
                if let Some(field) = create(&ctx, multiline, hint) {
                    f.insert(key, field);
                }
            }
            let field = f.get(key)?;
            if is_focused(&field.el) {
                let v = value(field);
                (v != *text).then_some(v)
            } else {
                if value(field) != *text {
                    set_value(field, text);
                }
                None
            }
        });
        let changed = typed.is_some();
        if let Some(v) = typed {
            *text = v;
        }
        let clip = ui.clip_rect();
        let visuals = ui.visuals().clone();
        // Lay out and paint the egui field as usual, but without letting egui
        // take focus: the HTML field on top handles touches and typing.
        let mut response = ui.add(make(text).hint_text(hint).interactive(false));
        if changed {
            response.mark_changed();
        }

        // Full-width fields can reach past the clip rect sideways: what
        // counts is the visible part, and that the field shows its full height.
        let shown = response.rect.intersect(clip);
        let whole = shown.height() >= response.rect.height() - 1.0 && shown.width() >= 24.0;
        let rect = if whole { shown } else { response.rect };
        let popup = egui::Popup::is_any_open(&ctx);
        let focused = FIELDS.with(|f| f.borrow().get(key).is_some_and(|field| is_focused(&field.el)));
        // While typing, never hide the field (that would close the keyboard):
        // when the keyboard shrinks the window, scroll the field into view.
        if focused && !whole {
            response.scroll_to_me(Some(egui::Align::Center));
            ctx.request_repaint();
        }
        let visible = focused || (whole && !popup);
        FIELDS.with(|f| {
            let mut f = f.borrow_mut();
            let Some(field) = f.get_mut(key) else { return };
            if !visible {
                hide(&field.el);
                return;
            }
            // egui points to CSS pixels, relative to the canvas.
            let Some(canvas) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id("app")) else {
                return;
            };
            let b = canvas.get_bounding_client_rect();
            let scale = b.width() as f32 / ctx.screen_rect().width().max(1.0);
            let s = field.el.style();
            let _ = s.set_property("left", &format!("{}px", b.left() as f32 + rect.left() * scale));
            let _ = s.set_property("top", &format!("{}px", b.top() as f32 + rect.top() * scale));
            let _ = s.set_property("width", &format!("{}px", rect.width() * scale));
            let _ = s.set_property("height", &format!("{}px", rect.height() * scale));
            let _ = s.set_property("display", "block");
            let pad = if multiline { "4px 6px" } else { "0 6px" };
            let key = format!(
                "{}{}{}{}",
                css(visuals.extreme_bg_color),
                css(visuals.text_color()),
                css(visuals.widgets.inactive.bg_stroke.color),
                css(visuals.selection.stroke.color)
            );
            if field.style_key != key {
                let _ = s.set_property("background", &css(visuals.extreme_bg_color));
                let _ = s.set_property("color", &css(visuals.text_color()));
                let _ = s.set_property("border-color", &css(visuals.widgets.inactive.bg_stroke.color));
                let _ = s.set_property("caret-color", &css(visuals.selection.stroke.color));
                let _ = s.set_property("padding", pad);
                field.style_key = key;
            }
        });
        response
    }
}
