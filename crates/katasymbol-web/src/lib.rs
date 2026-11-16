//! Browser [`Link`] over Web Bluetooth.
//!
//! Web Bluetooth needs a secure context (`https://` or `http://localhost`) and
//! Chrome/Edge (desktop or Android). Safari and Firefox do not implement it; on
//! iOS use a Web Bluetooth browser such as Bluefy.
#![cfg(target_arch = "wasm32")]

use katasymbol_core::{Error, Link, Result};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
pub use web_sys::BluetoothDevice;
use web_sys::{BluetoothRemoteGattCharacteristic, RequestDeviceOptions};

/// The Supvan service; `ffe1`/`ffe9` live under it with Bluetooth-base UUIDs.
pub const SERVICE: &str = "0000e0ff-3c17-d293-8e48-14fe2e4da212";
const NOTIFY: &str = "0000ffe1-0000-1000-8000-00805f9b34fb";
const WRITE: &str = "0000ffe9-0000-1000-8000-00805f9b34fb";
/// Name prefixes of Katasymbol/Supvan printers: the E16 advertises its
/// serial (`T0105A…`) and calls itself `T16` once connected.
pub const NAME_PREFIXES: &[&str] = &["T0", "T1", "E1", "Katasymbol", "KATASYMBOL", "Supvan", "SUPVAN"];
/// The error when the chosen device lacks the Supvan service.
pub const NOT_A_PRINTER: &str = "Das gewählte Gerät ist kein Katasymbol-Drucker – bitte einen anderen Eintrag wählen";
/// Default write size in the browser: 128 bytes, like the vendor app. (The
/// printer drops the connection when a data frame arrives in 20-byte
/// writes.) Override with `?chunk=N` in the page address.
pub const WEB_CHUNK: usize = 128;

/// The write size: `?chunk=N` in the page address, else [`WEB_CHUNK`].
pub fn chunk_size() -> usize {
    web_sys::window()
        .and_then(|w| w.location().search().ok())
        .and_then(|q| q.trim_start_matches('?').split('&').find_map(|kv| kv.strip_prefix("chunk=").map(str::to_owned)))
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| (1..=512).contains(n))
        .unwrap_or(WEB_CHUNK)
}

/// How long `gatt.connect()` may take.
const CONNECT_TIMEOUT_MS: i32 = 15_000;

/// `promise`, or a rejection with "timeout" after `ms`.
async fn with_timeout(promise: js_sys::Promise, ms: i32) -> std::result::Result<JsValue, JsValue> {
    let timeout = js_sys::Promise::new(&mut |_, reject| {
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_1(&reject, ms, &"timeout".into());
        }
    });
    JsFuture::from(js_sys::Promise::race(&js_sys::Array::of2(&promise, &timeout))).await
}

/// The message for a failed GATT connect, with the Linux cause spelled out.
fn connect_failed(e: JsValue) -> String {
    let linux = web_sys::window()
        .and_then(|w| w.navigator().user_agent().ok())
        .is_some_and(|ua| ua.contains("Linux") && !ua.contains("Android"));
    let what = if e.as_string().as_deref() == Some("timeout") { "keine Verbindung nach 15 s".to_string() } else { js_message(&e) };
    if linux {
        format!(
            "Verbindung fehlgeschlagen ({what}). Unter Linux verbindet BlueZ diesen Drucker meist über klassisches \
             Bluetooth statt über Bluetooth LE. Abhilfe: die Linux-App nehmen, oder BlueZ auf LE festlegen (siehe README)."
        )
    } else {
        format!("Verbindung fehlgeschlagen ({what}). Ist der Drucker an und mit keinem anderen Gerät verbunden?")
    }
}

/// Identical notifications this close together are one, delivered twice.
const DUPLICATE_MS: f64 = 40.0;

/// How often `recv` looks at the notification queue.
const RECV_POLL_MS: i32 = 2;

fn js_err(e: JsValue) -> Error {
    Error::Link(js_message(&e))
}

/// A JS error's text: the string itself, or an Error's `name: message`.
fn js_message(e: &JsValue) -> String {
    if let Some(s) = e.as_string() {
        return s;
    }
    let get = |k: &str| js_sys::Reflect::get(e, &k.into()).ok().and_then(|v| v.as_string());
    match (get("name"), get("message")) {
        (Some(n), Some(m)) => format!("{n}: {m}"),
        (None, Some(m)) => m,
        _ => format!("{e:?}"),
    }
}

fn now_ms() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map_or_else(js_sys::Date::now, |p| p.now())
}

async fn sleep_ms(ms: i32) {
    let p = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    let _ = JsFuture::from(p).await;
}

thread_local! {
    static DIAG: RefCell<Diag> = RefCell::new(Diag::default());
    static WITH_RESPONSE: Cell<Option<bool>> = const { Cell::new(None) };
    static TRACE: RefCell<VecDeque<String>> = const { RefCell::new(VecDeque::new()) };
}

const TRACE_LINES: usize = 600;

/// Record one line of the link trace (time in ms since page load).
pub fn trace(line: impl AsRef<str>) {
    let t = web_sys::window().and_then(|w| w.performance()).map_or(0.0, |p| p.now());
    TRACE.with(|tr| {
        let mut tr = tr.borrow_mut();
        if tr.len() >= TRACE_LINES {
            tr.pop_front();
        }
        tr.push_back(format!("{t:9.1} {}", line.as_ref()));
    });
    // Also in the browser console (DevTools, or remote debugging).
    web_sys::console::debug_1(&format!("[katasymbol] {}", line.as_ref()).into());
}

/// The link trace so far, one event per line, for bug reports.
pub fn trace_text() -> String {
    TRACE.with(|tr| tr.borrow().iter().cloned().collect::<Vec<_>>().join("\n"))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A write for the trace: short ones in full; long ones as head, tail and a
/// checksum (enough to spot truncation, reordering or mangling).
fn hex_short(b: &[u8]) -> String {
    if b.len() <= 32 {
        return hex(b);
    }
    let sum = b.iter().enumerate().fold(0u32, |a, (i, &x)| a.wrapping_add((i as u32 + 1) * u32::from(x)));
    format!("{}…{} sum {sum}", hex(&b[..12]), hex(&b[b.len() - 4..]))
}

/// What happened on the link so far, to tell a browser that drops
/// notifications apart from a printer that stays silent.
#[derive(Default, Clone)]
pub struct Diag {
    /// Notification events delivered by the browser.
    pub events: u32,
    /// Of those, the ones whose value could be read.
    pub decoded: u32,
    pub writes_ok: u32,
    pub writes_failed: u32,
    /// The kind of the last value that could not be read, or of the last write error.
    pub note: String,
}

pub fn diagnostics() -> Diag {
    DIAG.with(|d| d.borrow().clone())
}

fn diag(f: impl FnOnce(&mut Diag)) {
    DIAG.with(|d| f(&mut d.borrow_mut()));
}

/// Write with response (ATT Write Request) or the faster Write Command.
///
/// iOS (Bluefy) silently drops Write Commands sent in quick succession,
/// which leaves the printed label blank, so there every write waits for its
/// acknowledgement. Elsewhere (Chromium on Linux) that wait is slow enough
/// that the printer gives up on a 512-byte data frame half-way and never
/// acks it; Write Commands, as the vendor app sends them, work there.
/// `?write=req` or `?write=cmd` in the page address overrides the choice.
pub fn set_write_with_response(on: bool) {
    WITH_RESPONSE.with(|w| w.set(Some(on)));
}

pub fn write_with_response() -> bool {
    WITH_RESPONSE.with(|w| {
        let v = w.get().unwrap_or_else(default_write_with_response);
        w.set(Some(v));
        v
    })
}

fn default_write_with_response() -> bool {
    let Some(w) = web_sys::window() else { return true };
    let param = w.location().search().ok().and_then(|q| {
        q.trim_start_matches('?').split('&').find_map(|kv| kv.strip_prefix("write=").map(str::to_owned))
    });
    match param.as_deref() {
        Some("req" | "request" | "response") => return true,
        Some("cmd" | "command" | "noresponse") => return false,
        _ => {}
    }
    let nav = w.navigator();
    let ua = nav.user_agent().unwrap_or_default();
    // iPadOS reports itself as a Mac; a touch screen gives it away.
    ua.contains("iPhone") || ua.contains("iPad") || ua.contains("iPod") || (ua.contains("Macintosh") && nav.max_touch_points() > 1)
}

/// A short description of a JS value's kind, e.g. `DataView`.
fn kind(v: &JsValue) -> String {
    js_sys::Reflect::get(&js_sys::Object::new(), &"toString".into())
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .and_then(|f| f.call0(v).ok())
        .and_then(|s| s.as_string())
        .map_or_else(|| format!("{:?}", v.js_typeof()), |s| s.replace("[object ", "").replace(']', ""))
}

/// The bytes of a notification value (a `DataView`, or any buffer).
fn bytes_of(v: &JsValue) -> Option<Vec<u8>> {
    if v.is_undefined() || v.is_null() {
        return None;
    }
    if let Some(view) = v.dyn_ref::<js_sys::DataView>() {
        let a = js_sys::Uint8Array::new_with_byte_offset_and_length(
            &view.buffer(),
            view.byte_offset() as u32,
            view.byte_length() as u32,
        );
        return Some(a.to_vec());
    }
    if v.is_instance_of::<js_sys::ArrayBuffer>() || v.is_instance_of::<js_sys::Uint8Array>() || js_sys::Array::is_array(v) {
        return Some(js_sys::Uint8Array::new(v).to_vec());
    }
    // A view from another realm: use its buffer, offset and length.
    let get = |k: &str| js_sys::Reflect::get(v, &k.into()).ok();
    let buffer = get("buffer").filter(|b| b.is_instance_of::<js_sys::ArrayBuffer>())?;
    let offset = get("byteOffset").and_then(|x| x.as_f64()).unwrap_or(0.0) as u32;
    let len = get("byteLength").and_then(|x| x.as_f64())? as u32;
    Some(js_sys::Uint8Array::new_with_byte_offset_and_length(&buffer, offset, len).to_vec())
}

/// Chooser options: every device, or printer-like ones by name and (with
/// `maker`) by the E16's manufacturer data (company 0x0000, value = its
/// address). No service filter: the E16 does not advertise its service, and
/// Bluefy on iOS passes service filters on to the OS scan, which then finds
/// nothing at all.
fn options(all: bool, maker: bool) -> RequestDeviceOptions {
    let opts = RequestDeviceOptions::new();
    if all {
        opts.set_accept_all_devices(true);
    } else {
        let mut filters = Vec::new();
        for prefix in NAME_PREFIXES {
            let f = web_sys::BluetoothLeScanFilterInit::new();
            f.set_name_prefix(prefix);
            filters.push(f);
        }
        if maker {
            let company = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&company, &"companyIdentifier".into(), &0.into());
            let f = web_sys::BluetoothLeScanFilterInit::new();
            f.set_manufacturer_data(&js_sys::Array::of1(&company));
            filters.push(f);
        }
        opts.set_filters(&filters);
    }
    opts.set_optional_services(&[js_sys::JsString::from(SERVICE)]);
    opts
}

async fn choose(bt: &web_sys::Bluetooth, opts: &RequestDeviceOptions) -> std::result::Result<BluetoothDevice, JsValue> {
    Ok(JsFuture::from(bt.request_device(opts)).await?.unchecked_into())
}

/// A Web Bluetooth GATT connection to one printer.
pub struct WebLink {
    device: BluetoothDevice,
    notify: BluetoothRemoteGattCharacteristic,
    write: BluetoothRemoteGattCharacteristic,
    queue: Rc<RefCell<VecDeque<Vec<u8>>>>,
    connected: Rc<Cell<bool>>,
    on_lost: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    // Kept alive for as long as the link exists.
    _on_value: Closure<dyn FnMut(web_sys::Event)>,
    _on_disconnect: Closure<dyn FnMut(web_sys::Event)>,
}

impl WebLink {
    /// Show the browser's device chooser and connect to the chosen printer.
    /// Must be called from a user gesture (e.g. a click handler).
    ///
    /// Unless `all`, the chooser lists only printer-like devices (see [`options`]).
    pub async fn request(all: bool) -> Result<Self> {
        let bt = web_sys::window()
            .map(|w| w.navigator().bluetooth())
            .ok_or_else(|| Error::Link("no window".into()))?
            .ok_or_else(|| Error::Link("this browser has no Web Bluetooth".into()))?;
        let device = if all {
            choose(&bt, &options(true, false)).await.map_err(js_err)?
        } else {
            match choose(&bt, &options(false, true)).await {
                // A browser that does not know manufacturer-data filters
                // rejects the request: try again with names only.
                Err(e) if e.dyn_ref::<js_sys::TypeError>().is_some() => {
                    choose(&bt, &options(false, false)).await.map_err(js_err)?
                }
                r => r.map_err(js_err)?,
            }
        };
        Self::connect(device).await
    }

    pub async fn connect(device: BluetoothDevice) -> Result<Self> {
        trace(format!(
            "-- verbinde {} (Paket {} B, {})",
            device.name().unwrap_or_default(),
            chunk_size(),
            if write_with_response() { "mit Antwort" } else { "ohne Antwort" }
        ));
        let gatt = device.gatt().ok_or_else(|| Error::Link("device has no GATT server".into()))?;
        let server: web_sys::BluetoothRemoteGattServer = match with_timeout(gatt.connect().unchecked_into(), CONNECT_TIMEOUT_MS).await {
            Ok(s) => s.unchecked_into(),
            Err(e) => {
                gatt.disconnect();
                trace(format!("-- Verbindung fehlgeschlagen: {}", js_message(&e)));
                return Err(Error::Link(connect_failed(e)));
            }
        };
        let service: web_sys::BluetoothRemoteGattService =
            match JsFuture::from(server.get_primary_service_with_str(SERVICE)).await {
                Ok(s) => s.unchecked_into(),
                Err(_) => {
                    gatt.disconnect();
                    return Err(Error::Link(NOT_A_PRINTER.into()));
                }
            };
        let notify: BluetoothRemoteGattCharacteristic =
            JsFuture::from(service.get_characteristic_with_str(NOTIFY)).await.map_err(js_err)?.unchecked_into();
        let write: BluetoothRemoteGattCharacteristic =
            JsFuture::from(service.get_characteristic_with_str(WRITE)).await.map_err(js_err)?.unchecked_into();

        let queue = Rc::new(RefCell::new(VecDeque::new()));
        let q = queue.clone();
        let source = notify.clone();
        let last = RefCell::new(JsValue::UNDEFINED);
        let last_value = RefCell::new((Vec::<u8>::new(), f64::NEG_INFINITY));
        // Tolerant of how the browser delivers the value: Bluefy (iOS) is a
        // polyfill and need not match Chrome's event exactly. Never panic
        // here: a trap in an event handler would silently drop every answer.
        let on_value = Closure::<dyn FnMut(web_sys::Event)>::new(move |ev: web_sys::Event| {
            // The same event may arrive through both the listener and the
            // `on…` property.
            let ev_val: &JsValue = ev.as_ref();
            if js_sys::Object::is(&last.borrow(), ev_val) {
                return;
            }
            *last.borrow_mut() = ev_val.clone();
            diag(|d| d.events += 1);
            let target = ev.target().map(JsValue::from);
            // Events bubble up to the device: skip other characteristics.
            if let Some(t) = &target
                && let Some(uuid) = js_sys::Reflect::get(t, &"uuid".into()).ok().and_then(|u| u.as_string())
                && !uuid.eq_ignore_ascii_case(NOTIFY)
            {
                return;
            }
            let value = |o: &JsValue| js_sys::Reflect::get(o, &"value".into()).ok().filter(|v| !v.is_undefined() && !v.is_null());
            let v = target.as_ref().and_then(value).or_else(|| value(ev_val)).or_else(|| value(&source));
            match v.as_ref().and_then(bytes_of) {
                Some(b) => {
                    diag(|d| d.decoded += 1);
                    // BlueZ hands each notification to every subscribed
                    // client and Chromium then fires once per subscription
                    // (e.g. a second tab connected to the printer): the same
                    // bytes twice within milliseconds are one notification.
                    // A genuine repeat needs a new command and a radio round
                    // trip first.
                    let t = now_ms();
                    let mut lv = last_value.borrow_mut();
                    if lv.0 == b && t - lv.1 < DUPLICATE_MS {
                        trace(format!("N= {:3} doppelt, ignoriert", b.len()));
                        return;
                    }
                    *lv = (b.clone(), t);
                    q.borrow_mut().push_back(b);
                }
                None => diag(|d| d.note = format!("Wert: {}", v.as_ref().map_or("keiner".into(), kind))),
            }
        });
        notify
            .add_event_listener_with_callback("characteristicvaluechanged", on_value.as_ref().unchecked_ref())
            .map_err(js_err)?;
        let _ = js_sys::Reflect::set(&notify, &"oncharacteristicvaluechanged".into(), on_value.as_ref());
        // Some browsers may only deliver to the device (where the event
        // bubbles to anyway); duplicates are skipped above.
        let _ = device.add_event_listener_with_callback("characteristicvaluechanged", on_value.as_ref().unchecked_ref());
        JsFuture::from(notify.start_notifications()).await.map_err(js_err)?;

        let connected = Rc::new(Cell::new(true));
        let on_lost: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let (c, lost) = (connected.clone(), on_lost.clone());
        let on_disconnect = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            trace("-- Drucker hat die Verbindung getrennt");
            c.set(false);
            if let Some(f) = lost.borrow().as_ref() {
                f();
            }
        });
        device
            .add_event_listener_with_callback("gattserverdisconnected", on_disconnect.as_ref().unchecked_ref())
            .map_err(js_err)?;

        Ok(Self { device, notify, write, queue, connected, on_lost, _on_value: on_value, _on_disconnect: on_disconnect })
    }

    pub fn name(&self) -> Option<String> {
        self.device.name()
    }

    /// The chosen device: [`WebLink::connect`] reconnects to it without the
    /// chooser (and without a user gesture).
    pub fn device(&self) -> BluetoothDevice {
        self.device.clone()
    }

    pub fn is_connected(&self) -> bool {
        self.connected.get()
    }

    /// Called when the printer drops the connection (e.g. powers off).
    pub fn on_lost(&self, f: impl Fn() + 'static) {
        *self.on_lost.borrow_mut() = Some(Box::new(f));
    }

    pub fn disconnect(&self) {
        if let Some(g) = self.device.gatt() {
            g.disconnect();
        }
    }

    fn check(&self) -> Result<()> {
        if self.connected.get() { Ok(()) } else { Err(Error::Link("printer disconnected".into())) }
    }
}

impl Drop for WebLink {
    /// Unhook the event handlers (their closures die with the link; a later
    /// connection to the same device registers its own).
    fn drop(&mut self) {
        let f: &js_sys::Function = self._on_value.as_ref().unchecked_ref();
        let _ = self.notify.remove_event_listener_with_callback("characteristicvaluechanged", f);
        let _ = js_sys::Reflect::set(&self.notify, &"oncharacteristicvaluechanged".into(), &JsValue::NULL);
        let _ = self.device.remove_event_listener_with_callback("characteristicvaluechanged", f);
        let g: &js_sys::Function = self._on_disconnect.as_ref().unchecked_ref();
        let _ = self.device.remove_event_listener_with_callback("gattserverdisconnected", g);
    }
}

impl Link for WebLink {
    async fn write(&mut self, chunk: &[u8]) -> Result<()> {
        self.check()?;
        // A standalone ArrayBuffer of exactly this chunk. A `&[u8]` would
        // reach JS as a view into the whole wasm memory, and a browser that
        // sends `view.buffer` (Bluefy on iOS, apparently) ignores the offset
        // and length and sends garbage the printer never answers.
        let buf = js_sys::Uint8Array::from(chunk).buffer();
        let promise = if write_with_response() {
            self.write.write_value_with_response_with_buffer_source(&buf)
        } else {
            self.write.write_value_without_response_with_buffer_source(&buf)
        };
        let r = match promise {
            Ok(p) => JsFuture::from(p).await.map(drop),
            Err(e) => Err(e),
        };
        match r {
            Ok(()) => {
                diag(|d| d.writes_ok += 1);
                trace(format!("W{} {:3} {}", if write_with_response() { "R" } else { "C" }, chunk.len(), hex_short(chunk)));
                Ok(())
            }
            Err(e) => {
                let e = js_err(e);
                trace(format!("W! {:3} {} -> {e}", chunk.len(), hex_short(chunk)));
                diag(|d| {
                    d.writes_failed += 1;
                    d.note = format!("Schreibfehler: {e}");
                });
                Err(e)
            }
        }
    }

    async fn recv(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let deadline = now_ms() + timeout.as_secs_f64() * 1000.0;
        loop {
            if let Some(p) = self.queue.borrow_mut().pop_front() {
                trace(format!("N  {:3} {}", p.len(), hex(&p)));
                return Ok(Some(p));
            }
            if let Err(e) = self.check() {
                trace("-- getrennt");
                return Err(e);
            }
            if now_ms() >= deadline {
                trace(format!("-- keine Antwort nach {} ms", timeout.as_millis()));
                return Ok(None);
            }
            sleep_ms(RECV_POLL_MS).await;
        }
    }

    async fn sleep(&mut self, d: Duration) {
        sleep_ms(d.as_millis().min(i32::MAX as u128) as i32).await;
    }

    fn chunk_size(&self) -> usize {
        chunk_size()
    }

    fn discard_pending(&mut self) {
        for p in self.queue.borrow_mut().drain(..) {
            trace(format!("N- {:3} {} (verworfen)", p.len(), hex(&p)));
        }
    }
}
