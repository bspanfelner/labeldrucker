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
use web_sys::{BluetoothDevice, BluetoothRemoteGattCharacteristic, RequestDeviceOptions};

/// The Supvan service; `ffe1`/`ffe9` live under it with Bluetooth-base UUIDs.
pub const SERVICE: &str = "0000e0ff-3c17-d293-8e48-14fe2e4da212";
const NOTIFY: &str = "0000ffe1-0000-1000-8000-00805f9b34fb";
const WRITE: &str = "0000ffe9-0000-1000-8000-00805f9b34fb";
/// Name prefixes of Katasymbol/Supvan printers: the E16 advertises its
/// serial (`T0105A…`) and calls itself `T16` once connected.
pub const NAME_PREFIXES: &[&str] = &["T0", "T1", "E1", "Katasymbol", "KATASYMBOL", "Supvan", "SUPVAN"];
/// The error when the chosen device lacks the Supvan service.
pub const NOT_A_PRINTER: &str = "Das gewählte Gerät ist kein Katasymbol-Drucker – bitte einen anderen Eintrag wählen";
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
    ///
    /// Unless `all`, the chooser lists only devices with a printer-like
    /// name (see [`NAME_PREFIXES`]).
    pub async fn request(all: bool) -> Result<Self> {
        let bt = web_sys::window()
            .map(|w| w.navigator().bluetooth())
            .ok_or_else(|| Error::Link("no window".into()))?
            .ok_or_else(|| Error::Link("this browser has no Web Bluetooth".into()))?;
        let opts = RequestDeviceOptions::new();
        if all {
            opts.set_accept_all_devices(true);
        } else {
            // Names only: the E16 does not advertise its service, and some
            // browsers (Bluefy on iOS) pass a service filter on to the OS
            // scan, which then finds nothing at all.
            let mut filters = Vec::new();
            for prefix in NAME_PREFIXES {
                let f = web_sys::BluetoothLeScanFilterInit::new();
                f.set_name_prefix(prefix);
                filters.push(f);
            }
            opts.set_filters(&filters);
        }
        opts.set_optional_services(&[js_sys::JsString::from(SERVICE)]);
        let device: BluetoothDevice = JsFuture::from(bt.request_device(&opts)).await.map_err(js_err)?.unchecked_into();
        Self::connect(device).await
    }

    pub async fn connect(device: BluetoothDevice) -> Result<Self> {
        let gatt = device.gatt().ok_or_else(|| Error::Link("device has no GATT server".into()))?;
        let server: web_sys::BluetoothRemoteGattServer = JsFuture::from(gatt.connect()).await.map_err(js_err)?.unchecked_into();
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
