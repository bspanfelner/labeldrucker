//! Browser [`Link`] over Web Bluetooth, plus a small JS-facing API.
//!
//! Build with `web/build.sh`; `web/index.html` is the UI. Web Bluetooth needs a
//! secure context (`https://` or `http://localhost`) and Chrome/Edge (desktop or
//! Android). Safari and Firefox do not implement it; on iOS use a Web Bluetooth
//! browser such as Bluefy, or the native app.
#![cfg(target_arch = "wasm32")]

use katasymbol_core::page::HEAD_DOTS;
use katasymbol_core::printer::Progress;
use katasymbol_core::{Error, Link, Page, Printer, Result};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{BluetoothDevice, BluetoothRemoteGattCharacteristic, RequestDeviceOptions};

/// The Supvan service; `ffe1`/`ffe9` live under it with Bluetooth-base UUIDs.
pub const SERVICE: &str = "0000e0ff-3c17-d293-8e48-14fe2e4da212";
const NOTIFY: &str = "0000ffe1-0000-1000-8000-00805f9b34fb";
const WRITE: &str = "0000ffe9-0000-1000-8000-00805f9b34fb";
/// How often `recv` looks at the notification queue.
const RECV_POLL_MS: i32 = 2;

fn js_err(e: JsValue) -> Error {
    Error::Link(e.as_string().unwrap_or_else(|| format!("{e:?}")))
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

/// A Web Bluetooth GATT connection to one printer.
pub struct WebLink {
    device: BluetoothDevice,
    write: BluetoothRemoteGattCharacteristic,
    queue: Rc<RefCell<VecDeque<Vec<u8>>>>,
    connected: Rc<Cell<bool>>,
    // Kept alive for as long as the link exists.
    _on_value: Closure<dyn FnMut(web_sys::Event)>,
    _on_disconnect: Closure<dyn FnMut(web_sys::Event)>,
}

impl WebLink {
    /// Show the browser's device chooser and connect to the chosen printer.
    /// Must be called from a user gesture (e.g. a click handler).
    pub async fn request() -> Result<Self> {
        let bt = web_sys::window()
            .map(|w| w.navigator().bluetooth())
            .ok_or_else(|| Error::Link("no window".into()))?
            .ok_or_else(|| Error::Link("this browser has no Web Bluetooth".into()))?;
        let opts = RequestDeviceOptions::new();
        opts.set_accept_all_devices(true);
        opts.set_optional_services(&[js_sys::JsString::from(SERVICE)]);
        let device: BluetoothDevice = JsFuture::from(bt.request_device(&opts)).await.map_err(js_err)?.unchecked_into();
        Self::connect(device).await
    }

    pub async fn connect(device: BluetoothDevice) -> Result<Self> {
        let gatt = device.gatt().ok_or_else(|| Error::Link("device has no GATT server".into()))?;
        let server: web_sys::BluetoothRemoteGattServer = JsFuture::from(gatt.connect()).await.map_err(js_err)?.unchecked_into();
        let service: web_sys::BluetoothRemoteGattService =
            JsFuture::from(server.get_primary_service_with_str(SERVICE)).await.map_err(js_err)?.unchecked_into();
        let notify: BluetoothRemoteGattCharacteristic =
            JsFuture::from(service.get_characteristic_with_str(NOTIFY)).await.map_err(js_err)?.unchecked_into();
        let write: BluetoothRemoteGattCharacteristic =
            JsFuture::from(service.get_characteristic_with_str(WRITE)).await.map_err(js_err)?.unchecked_into();

        let queue = Rc::new(RefCell::new(VecDeque::new()));
        let q = queue.clone();
        let on_value = Closure::<dyn FnMut(web_sys::Event)>::new(move |ev: web_sys::Event| {
            let ch: BluetoothRemoteGattCharacteristic = ev.target().expect("event target").unchecked_into();
            if let Some(view) = ch.value() {
                let bytes = js_sys::Uint8Array::new_with_byte_offset_and_length(
                    &view.buffer(),
                    view.byte_offset() as u32,
                    view.byte_length() as u32,
                );
                q.borrow_mut().push_back(bytes.to_vec());
            }
        });
        notify
            .add_event_listener_with_callback("characteristicvaluechanged", on_value.as_ref().unchecked_ref())
            .map_err(js_err)?;
        JsFuture::from(notify.start_notifications()).await.map_err(js_err)?;

        let connected = Rc::new(Cell::new(true));
        let c = connected.clone();
        let on_disconnect = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| c.set(false));
        device
            .add_event_listener_with_callback("gattserverdisconnected", on_disconnect.as_ref().unchecked_ref())
            .map_err(js_err)?;

        Ok(Self { device, write, queue, connected, _on_value: on_value, _on_disconnect: on_disconnect })
    }

    pub fn name(&self) -> Option<String> {
        self.device.name()
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

impl Link for WebLink {
    async fn write(&mut self, chunk: &[u8]) -> Result<()> {
        self.check()?;
        let mut buf = chunk.to_vec();
        JsFuture::from(self.write.write_value_without_response_with_u8_slice(&mut buf).map_err(js_err)?)
            .await
            .map_err(js_err)?;
        Ok(())
    }

    async fn recv(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let deadline = now_ms() + timeout.as_secs_f64() * 1000.0;
        loop {
            if let Some(p) = self.queue.borrow_mut().pop_front() {
                return Ok(Some(p));
            }
            self.check()?;
            if now_ms() >= deadline {
                return Ok(None);
            }
            sleep_ms(RECV_POLL_MS).await;
        }
    }

    async fn sleep(&mut self, d: Duration) {
        sleep_ms(d.as_millis().min(i32::MAX as u128) as i32).await;
    }
}

// ---------------------------------------------------------------- JS API

fn to_js(e: Error) -> JsValue {
    JsError::new(&e.to_string()).into()
}

/// A connected printer, for JavaScript.
///
/// Calls must not overlap; each returns a Promise and rejects while another
/// call is still running.
#[wasm_bindgen]
pub struct E16 {
    inner: Rc<RefCell<Option<Printer<WebLink>>>>,
}

#[wasm_bindgen]
impl E16 {
    /// Open the device chooser and connect. Call from a click handler.
    pub async fn connect() -> std::result::Result<E16, JsValue> {
        let link = WebLink::request().await.map_err(to_js)?;
        Ok(E16 { inner: Rc::new(RefCell::new(Some(Printer::new(link)))) })
    }

    /// Run `f` on the printer while holding it exclusively.
    fn run<F, Fut>(&self, f: F) -> js_sys::Promise
    where
        F: FnOnce(Printer<WebLink>) -> Fut + 'static,
        Fut: std::future::Future<Output = (Printer<WebLink>, Result<JsValue>)>,
    {
        let inner = self.inner.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            let printer = inner.borrow_mut().take().ok_or_else(|| JsError::new("printer is busy"))?;
            let (printer, r) = f(printer).await;
            *inner.borrow_mut() = Some(printer);
            r.map_err(to_js)
        })
    }

    /// `{ name, status, busy, printing, labels: { width, length, gap, remaining } | null }`
    pub fn probe(&self) -> js_sys::Promise {
        self.run(|mut p| async move {
            let r = p.probe().await.map(|info| {
                let o = js_sys::Object::new();
                let set = |o: &js_sys::Object, k: &str, v: JsValue| {
                    let _ = js_sys::Reflect::set(o, &k.into(), &v);
                };
                set(&o, "name", info.name.map_or(JsValue::NULL, Into::into));
                set(&o, "status", info.status.error().unwrap_or("ready").into());
                set(&o, "busy", info.status.busy.into());
                set(&o, "printing", info.status.printing.into());
                let labels = info.material.map_or(JsValue::NULL, |m| {
                    let l = js_sys::Object::new();
                    set(&l, "width", m.width_mm.into());
                    set(&l, "length", m.length_mm.into());
                    set(&l, "gap", m.gap_mm.into());
                    set(&l, "remaining", m.remaining.map_or(JsValue::NULL, Into::into));
                    l.into()
                });
                set(&o, "labels", labels);
                o.into()
            });
            (p, r)
        })
    }

    /// Print a grayscale bitmap `width` × 96 (row-major, 0 = black).
    /// `on_progress(text)` is called as the job advances.
    pub fn print_gray(
        &self,
        width: usize,
        gray: Vec<u8>,
        threshold: u8,
        density: u8,
        on_progress: js_sys::Function,
    ) -> js_sys::Promise {
        self.run(move |mut p| async move {
            if gray.len() != width * HEAD_DOTS {
                return (p, Err(Error::Encode(format!("expected {width}×{HEAD_DOTS} pixels"))));
            }
            let page = Page::from_gray(width, HEAD_DOTS, &gray, threshold);
            let r = print(&mut p, &page, density, &on_progress).await;
            (p, r)
        })
    }

    /// Print the calibration pattern (frame, X, 5 mm ticks).
    pub fn print_test(&self, length_mm: usize, density: u8, on_progress: js_sys::Function) -> js_sys::Promise {
        self.run(move |mut p| async move {
            let page = Page::test_pattern(length_mm);
            let r = print(&mut p, &page, density, &on_progress).await;
            (p, r)
        })
    }

    pub fn disconnect(&self) {
        if let Some(p) = self.inner.borrow_mut().take() {
            p.into_link().disconnect();
        }
    }
}

async fn print(p: &mut Printer<WebLink>, page: &Page, density: u8, cb: &js_sys::Function) -> Result<JsValue> {
    let report = |s: String| {
        let _ = cb.call1(&JsValue::NULL, &s.into());
    };
    p.print(page, density, |pr| match pr {
        Progress::Preparing => report("Vorbereiten …".into()),
        Progress::Sending { buffer, of } => report(format!("Sende Teil {buffer}/{of} …")),
        Progress::Finishing => report("Drucke …".into()),
    })
    .await?;
    Ok(JsValue::UNDEFINED)
}

/// The head height in dots (bitmap height for `print_gray`).
#[wasm_bindgen]
pub fn head_dots() -> usize {
    HEAD_DOTS
}
