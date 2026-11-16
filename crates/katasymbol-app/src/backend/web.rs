//! Web Bluetooth backend: everything runs on the browser's event loop.
//!
//! `Cmd::Scan` opens the browser's device chooser (it must follow a click,
//! which egui's button handling does within the activation window) and
//! connects; later commands use that connection. The chosen device is
//! remembered: when the printer drops the connection (it powers off when
//! idle), the next command reconnects without the chooser.

use super::{Cmd, Ev, progress_text};
use katasymbol_core::{Error, Printer};
use katasymbol_web::{BluetoothDevice, WebLink};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};

#[derive(Default)]
struct State {
    printer: Option<Printer<WebLink>>,
    /// The device picked in the chooser, until the user disconnects.
    device: Option<BluetoothDevice>,
}

type Shared = Rc<RefCell<State>>;
type Send = Rc<dyn Fn(Ev)>;

pub struct Worker {
    tx: Sender<Ev>,
    wake: Rc<dyn Fn()>,
    printer: Shared,
    pub rx: Receiver<Ev>,
}

impl Worker {
    pub fn spawn(wake: impl Fn() + 'static) -> Self {
        let (tx, rx) = channel();
        Self { tx, wake: Rc::new(wake), printer: Rc::new(RefCell::new(State::default())), rx }
    }

    pub fn send(&self, cmd: Cmd) {
        let send = self.sender();
        let printer = self.printer.clone();
        wasm_bindgen_futures::spawn_local(async move { handle(cmd, printer, &send).await });
    }

    fn sender(&self) -> Send {
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        Rc::new(move |e: Ev| {
            let _ = tx.send(e);
            wake();
        })
    }
}

async fn handle(cmd: Cmd, shared: Shared, send: &Send) {
    match cmd {
        Cmd::Scan { all } => {
            forget(&shared);
            send(Ev::Progress("Drucker im Browser-Dialog wählen …".into()));
            match WebLink::request(all).await {
                Ok(link) => {
                    let name = link.name().unwrap_or_else(|| "Drucker".into());
                    send(Ev::Device { addr: name.clone(), name });
                    watch(&link, send);
                    shared.borrow_mut().device = Some(link.device());
                    send(Ev::Connected(true));
                    let mut p = Printer::new(link);
                    let mut r = p.probe().await;
                    // Not a single answer: try the other ATT write type, keep it if it works.
                    if r.is_err() && katasymbol_web::diagnostics().events == 0 {
                        let other = !katasymbol_web::write_with_response();
                        katasymbol_web::set_write_with_response(other);
                        send(Ev::Progress("Keine Antwort – versuche andere Sendeart …".into()));
                        r = p.probe().await;
                        if r.is_err() {
                            katasymbol_web::set_write_with_response(!other);
                        }
                    }
                    shared.borrow_mut().printer = Some(p);
                    match r {
                        Ok(info) => send(Ev::Info(info)),
                        Err(e) => send(Ev::Error(diagnose(&e))),
                    }
                }
                Err(katasymbol_core::Error::Link(m)) if m == katasymbol_web::NOT_A_PRINTER => {
                    send(Ev::Error(format!("{m}.")));
                }
                Err(e) => send(Ev::Error(format!(
                    "Keine Verbindung: {e}{}",
                    if all { "" } else { " – Drucker nicht in der Liste? Ist er an und mit keinem anderen Gerät verbunden? Sonst ⚙ → „Alle Geräte zeigen“." }
                ))),
            }
        }
        Cmd::Disconnect => {
            forget(&shared);
            send(Ev::Connected(false));
        }
        Cmd::Probe { .. } => match with_printer(&shared, send, Job::Probe).await {
            Ok(Done::Info(info)) => send(Ev::Info(info)),
            Ok(Done::Printed) => {}
            Err(e) => send(Ev::Error(e)),
        },
        Cmd::Print { page, density, copies, .. } => {
            katasymbol_web::trace(format!("== Druck: {} Spalten, Dichte {density}, {copies}×", page.width()));
            let copies = copies.max(1);
            let mut r = Ok(());
            for n in 1..=copies {
                let prefix = if copies > 1 { format!("Kopie {n}/{copies}: ") } else { String::new() };
                r = with_printer(&shared, send, Job::Print { page: &page, density, prefix: &prefix }).await.map(drop);
                if r.is_err() {
                    break;
                }
            }
            match r {
                Ok(()) => {
                    send(Ev::Printed);
                    // Refresh the label counter and status (may fail while
                    // the printer finishes; that is fine).
                    let quiet: Send = Rc::new(|_| {});
                    if let Ok(Done::Info(info)) = with_printer(&shared, &quiet, Job::Probe).await {
                        send(Ev::Info(info));
                    }
                }
                Err(e) => send(Ev::Error(e)),
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Job<'a> {
    Probe,
    Print { page: &'a katasymbol_core::page::Page, density: u8, prefix: &'a str },
}

enum Done {
    Info(katasymbol_core::printer::Info),
    Printed,
}

async fn run(p: &mut Printer<WebLink>, job: Job<'_>, send: &Send) -> katasymbol_core::Result<Done> {
    match job {
        Job::Probe => p.probe().await.map(Done::Info),
        Job::Print { page, density, prefix } => {
            p.print(page, density, |pr| send(Ev::Progress(progress_text(prefix, pr)))).await.map(|()| Done::Printed)
        }
    }
}

/// Run `job` on the printer, connecting to the remembered device first if
/// needed; after a link error on an older connection, reconnect once and
/// retry.
async fn with_printer(shared: &Shared, send: &Send, job: Job<'_>) -> Result<Done, String> {
    let existing = shared.borrow_mut().printer.take().filter(|p| p.link().is_connected());
    let fresh = existing.is_none();
    let mut p = match existing {
        Some(p) => p,
        None => reconnect(shared, send).await?,
    };
    let mut r = run(&mut p, job, send).await;
    if matches!(r, Err(Error::Link(_))) && !fresh {
        drop(p);
        p = reconnect(shared, send).await?;
        r = run(&mut p, job, send).await;
    }
    match r {
        Ok(v) => {
            shared.borrow_mut().printer = Some(p);
            Ok(v)
        }
        Err(e) => {
            if matches!(e, Error::Link(_)) {
                drop(p);
                send(Ev::Connected(false));
            } else {
                shared.borrow_mut().printer = Some(p);
            }
            Err(diagnose(&e))
        }
    }
}

/// Connect to the remembered device again (no chooser needed).
async fn reconnect(shared: &Shared, send: &Send) -> Result<Printer<WebLink>, String> {
    let device = shared.borrow().device.clone().ok_or_else(|| "Nicht verbunden – erst „Verbinden“ klicken.".to_string())?;
    send(Ev::Progress("Verbinde erneut mit dem Drucker …".into()));
    match WebLink::connect(device).await {
        Ok(link) => {
            watch(&link, send);
            send(Ev::Connected(true));
            Ok(Printer::new(link))
        }
        Err(e) => {
            send(Ev::Connected(false));
            Err(format!("Drucker nicht erreichbar – ist er eingeschaltet? ({e})"))
        }
    }
}

/// Report it to the UI when the printer drops this connection.
fn watch(link: &WebLink, send: &Send) {
    let send = send.clone();
    link.on_lost(move || send(Ev::Connected(false)));
}

/// Disconnect and forget the device (the user chose to, or picks another).
fn forget(shared: &Shared) {
    let mut st = shared.borrow_mut();
    if let Some(p) = st.printer.take() {
        p.into_link().disconnect();
    }
    st.device = None;
}

/// The error text, with how many answers the printer sent so far: tells a
/// browser that drops notifications apart from a printer that is silent.
fn diagnose(e: &katasymbol_core::Error) -> String {
    let d = katasymbol_web::diagnostics();
    let mode = if katasymbol_web::write_with_response() { "mit Antwort" } else { "ohne Antwort" };
    let note = if d.note.is_empty() { String::new() } else { format!(", {}", d.note) };
    format!(
        "{e} (Diagnose: Ereignisse {}, gelesen {}, gesendet {}, Sendefehler {}, Schreiben {mode}, Paket {} B{note})",
        d.events, d.decoded, d.writes_ok, d.writes_failed, katasymbol_web::chunk_size()
    )
}
