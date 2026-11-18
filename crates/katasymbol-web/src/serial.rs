//! A byte stream to a Bluetooth Classic printer (Brother P-touch) over the
//! Web Serial API: Chrome/Edge on the desktop (from version 117) talk to
//! paired devices' RFCOMM serial ports. The printer has to be paired in the
//! operating system first. Safari, Firefox and iOS have no Web Serial.
//!
//! Bound through `js_sys::Reflect` rather than web-sys, whose bindings lack
//! the Bluetooth options.

use crate::{js_message, now_ms, sleep_ms, trace};
use katasymbol_core::brother::Stream;
use katasymbol_core::{Error, Result};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// The Serial Port Profile's service class.
pub const SPP: &str = "00001101-0000-1000-8000-00805f9b34fb";

fn get(o: &JsValue, k: &str) -> Option<JsValue> {
    js_sys::Reflect::get(o, &k.into()).ok().filter(|v| !v.is_undefined() && !v.is_null())
}

/// `obj.method(...args)`, awaited if it returns a promise.
async fn call(obj: &JsValue, method: &str, args: &[JsValue]) -> std::result::Result<JsValue, JsValue> {
    let f: js_sys::Function = get(obj, method).ok_or_else(|| JsValue::from(format!("{method} fehlt")))?.dyn_into()?;
    let r = f.apply(obj, &args.iter().collect::<js_sys::Array>())?;
    if r.is_instance_of::<js_sys::Promise>() { JsFuture::from(js_sys::Promise::from(r)).await } else { Ok(r) }
}

fn link_err(what: &str, e: &JsValue) -> Error {
    Error::Link(format!("{what}: {}", js_message(e)))
}

fn serial() -> Result<JsValue> {
    web_sys::window()
        .and_then(|w| get(&w.navigator(), "serial"))
        .ok_or_else(|| Error::Link("Dieser Browser kann kein Web Serial – bitte Chrome oder Edge auf dem Computer verwenden".into()))
}

/// Whether this browser has Web Serial at all.
pub fn available() -> bool {
    serial().is_ok()
}

/// A serial port the user picked once; reopen it with [`SerialLink::open`].
pub type SerialPort = JsValue;

pub struct SerialLink {
    port: JsValue,
    writer: JsValue,
    reader: JsValue,
    rx: Rc<RefCell<Vec<u8>>>,
    open: Rc<Cell<bool>>,
    on_lost: Rc<RefCell<Option<Box<dyn Fn()>>>>,
}

impl SerialLink {
    /// Show the browser's port chooser (paired Bluetooth serial devices)
    /// and open the chosen port. Must follow a click.
    pub async fn request() -> Result<Self> {
        let serial = serial()?;
        let spp = JsValue::from(SPP);
        let filter = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&filter, &"bluetoothServiceClassId".into(), &spp);
        let opts = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&opts, &"filters".into(), &js_sys::Array::of1(&filter));
        let _ = js_sys::Reflect::set(&opts, &"allowedBluetoothServiceClassIds".into(), &js_sys::Array::of1(&spp));
        let port = call(&serial, "requestPort", &[opts.into()]).await.map_err(|e| link_err("Keine Auswahl", &e))?;
        Self::open(port).await
    }

    /// A port picked earlier in this browser (permission granted), if any.
    pub async fn remembered() -> Option<SerialPort> {
        let ports = call(&serial().ok()?, "getPorts", &[]).await.ok()?;
        js_sys::Array::from(&ports).iter().find(|p| {
            call_info(p).is_some_and(|i| get(&i, "bluetoothServiceClassId").and_then(|s| s.as_string()).is_some_and(|s| s.eq_ignore_ascii_case(SPP)))
        })
    }

    /// Open `port` and start reading from it.
    pub async fn open(port: SerialPort) -> Result<Self> {
        trace("-- öffne Bluetooth-Seriell-Verbindung");
        let opts = js_sys::Object::new();
        // RFCOMM ignores the rate, but `open` requires one.
        let _ = js_sys::Reflect::set(&opts, &"baudRate".into(), &JsValue::from(9600));
        let _ = js_sys::Reflect::set(&opts, &"bufferSize".into(), &JsValue::from(4096));
        if let Err(e) = call(&port, "open", &[opts.into()]).await {
            let msg = js_message(&e);
            // Open already (e.g. by this page before a reload): use it as is.
            if !msg.contains("already open") {
                trace(format!("-- Öffnen fehlgeschlagen: {msg}"));
                return Err(Error::Link(format!(
                    "Verbindung fehlgeschlagen ({msg}). Ist der Drucker an, im System gekoppelt und mit keinem anderen Gerät verbunden?"
                )));
            }
        }
        let writable = get(&port, "writable").ok_or_else(|| Error::Link("Port nicht beschreibbar".into()))?;
        let readable = get(&port, "readable").ok_or_else(|| Error::Link("Port nicht lesbar".into()))?;
        let writer = call(&writable, "getWriter", &[]).await.map_err(|e| link_err("Schreiben", &e))?;
        let reader = call(&readable, "getReader", &[]).await.map_err(|e| link_err("Lesen", &e))?;

        let link = Self {
            port,
            writer,
            reader: reader.clone(),
            rx: Rc::new(RefCell::new(Vec::new())),
            open: Rc::new(Cell::new(true)),
            on_lost: Rc::new(RefCell::new(None)),
        };
        // Read until the port closes, collecting the bytes.
        let (rx, open, lost) = (link.rx.clone(), link.open.clone(), link.on_lost.clone());
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                match call(&reader, "read", &[]).await {
                    Ok(r) => {
                        if get(&r, "done").and_then(|d| d.as_bool()) == Some(true) {
                            break;
                        }
                        if let Some(v) = get(&r, "value") {
                            let bytes = js_sys::Uint8Array::new(&v).to_vec();
                            trace(format!("R  {:3} {}", bytes.len(), crate::hex_short(&bytes)));
                            rx.borrow_mut().extend_from_slice(&bytes);
                        }
                    }
                    Err(e) => {
                        trace(format!("-- Lesefehler: {}", js_message(&e)));
                        break;
                    }
                }
            }
            if open.replace(false) {
                trace("-- Verbindung beendet");
                if let Some(f) = lost.borrow().as_ref() {
                    f();
                }
            }
        });
        trace("-- verbunden");
        Ok(link)
    }

    pub fn port(&self) -> SerialPort {
        self.port.clone()
    }

    pub fn is_connected(&self) -> bool {
        self.open.get()
    }

    /// Called when the connection ends without [`Self::close`].
    pub fn on_lost(&self, f: impl Fn() + 'static) {
        *self.on_lost.borrow_mut() = Some(Box::new(f));
    }

    /// Release the port (it can be reopened later without the chooser).
    pub async fn close(self) {
        self.open.set(false);
        let _ = call(&self.reader, "cancel", &[]).await;
        let _ = call(&self.reader, "releaseLock", &[]).await;
        let _ = call(&self.writer, "releaseLock", &[]).await;
        let _ = call(&self.port, "close", &[]).await;
        trace("-- getrennt");
    }
}

fn call_info(port: &JsValue) -> Option<JsValue> {
    let f: js_sys::Function = get(port, "getInfo")?.dyn_into().ok()?;
    f.call0(port).ok()
}

impl Stream for SerialLink {
    async fn write(&mut self, data: &[u8]) -> Result<()> {
        if !self.open.get() {
            return Err(Error::Link("printer disconnected".into()));
        }
        let buf = js_sys::Uint8Array::from(data);
        match call(&self.writer, "write", &[buf.into()]).await {
            Ok(_) => {
                trace(format!("W  {:3} {}", data.len(), crate::hex_short(data)));
                Ok(())
            }
            Err(e) => {
                trace(format!("W! {:3} -> {}", data.len(), js_message(&e)));
                Err(link_err("Senden", &e))
            }
        }
    }

    async fn read(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let deadline = now_ms() + timeout.as_secs_f64() * 1000.0;
        loop {
            if !self.rx.borrow().is_empty() {
                return Ok(Some(std::mem::take(&mut *self.rx.borrow_mut())));
            }
            if !self.open.get() {
                return Err(Error::Link("printer disconnected".into()));
            }
            if now_ms() >= deadline {
                return Ok(None);
            }
            sleep_ms(5).await;
        }
    }

    async fn sleep(&mut self, d: Duration) {
        sleep_ms(d.as_millis().min(i32::MAX as u128) as i32).await;
    }
}
