//! Web Bluetooth backend: everything runs on the browser's event loop.
//!
//! `Cmd::Scan` opens the browser's device chooser (it must follow a click,
//! which egui's button handling does within the activation window) and
//! connects; later commands use that connection.

use super::{Cmd, Ev, progress_text};
use katasymbol_core::Printer;
use katasymbol_web::WebLink;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};

type Shared = Rc<RefCell<Option<Printer<WebLink>>>>;

pub struct Worker {
    tx: Sender<Ev>,
    wake: Rc<dyn Fn()>,
    printer: Shared,
    pub rx: Receiver<Ev>,
}

impl Worker {
    pub fn spawn(wake: impl Fn() + 'static) -> Self {
        let (tx, rx) = channel();
        Self { tx, wake: Rc::new(wake), printer: Rc::new(RefCell::new(None)), rx }
    }

    pub fn send(&self, cmd: Cmd) {
        let (tx, wake, printer) = (self.tx.clone(), self.wake.clone(), self.printer.clone());
        let send = move |e: Ev| {
            let _ = tx.send(e);
            wake();
        };
        wasm_bindgen_futures::spawn_local(async move { handle(cmd, printer, &send).await });
    }
}

async fn handle(cmd: Cmd, shared: Shared, send: &dyn Fn(Ev)) {
    match cmd {
        Cmd::Scan { all } => {
            if let Some(p) = shared.borrow_mut().take() {
                p.into_link().disconnect();
            }
            send(Ev::Progress("Drucker im Browser-Dialog wählen …".into()));
            match WebLink::request(all).await {
                Ok(link) => {
                    let name = link.name().unwrap_or_else(|| "Drucker".into());
                    send(Ev::Device { addr: name.clone(), name });
                    send(Ev::Connected(true));
                    let mut p = Printer::new(link);
                    let mut r = p.probe().await;
                    // Not a single answer: try the other ATT write type.
                    if r.is_err() && katasymbol_web::diagnostics().events == 0 {
                        let other = !katasymbol_web::write_with_response();
                        katasymbol_web::set_write_with_response(other);
                        send(Ev::Progress("Keine Antwort – versuche andere Sendeart …".into()));
                        r = p.probe().await;
                        if r.is_err() {
                            katasymbol_web::set_write_with_response(!other);
                        }
                    }
                    *shared.borrow_mut() = Some(p);
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
            if let Some(p) = shared.borrow_mut().take() {
                p.into_link().disconnect();
            }
            send(Ev::Connected(false));
        }
        Cmd::Probe { .. } => {
            let Some(mut p) = take(&shared, send) else { return };
            let r = p.probe().await;
            *shared.borrow_mut() = Some(p);
            match r {
                Ok(info) => send(Ev::Info(info)),
                Err(e) => fail(e, &shared, send),
            }
        }
        Cmd::Print { page, density, copies, .. } => {
            let Some(mut p) = take(&shared, send) else { return };
            let copies = copies.max(1);
            let mut r = Ok(());
            for n in 1..=copies {
                let prefix = if copies > 1 { format!("Kopie {n}/{copies}: ") } else { String::new() };
                r = p.print(&page, density, |pr| send(Ev::Progress(progress_text(&prefix, pr)))).await;
                if r.is_err() {
                    break;
                }
            }
            let info = if r.is_ok() { p.probe().await.ok() } else { None };
            *shared.borrow_mut() = Some(p);
            match r {
                Ok(()) => {
                    send(Ev::Printed);
                    if let Some(i) = info {
                        send(Ev::Info(i));
                    }
                }
                Err(e) => fail(e, &shared, send),
            }
        }
    }
}

fn take(shared: &Shared, send: &dyn Fn(Ev)) -> Option<Printer<WebLink>> {
    let p = shared.borrow_mut().take();
    if p.is_none() {
        send(Ev::Error("Nicht verbunden – erst „Verbinden“ klicken.".into()));
    }
    p
}

fn fail(e: katasymbol_core::Error, shared: &Shared, send: &dyn Fn(Ev)) {
    if matches!(e, katasymbol_core::Error::Link(_)) {
        shared.borrow_mut().take();
        send(Ev::Connected(false));
    }
    send(Ev::Error(diagnose(&e)));
}

/// The error text, with how many answers the printer sent so far: tells a
/// browser that drops notifications apart from a printer that is silent.
fn diagnose(e: &katasymbol_core::Error) -> String {
    let d = katasymbol_web::diagnostics();
    let mode = if katasymbol_web::write_with_response() { "mit Antwort" } else { "ohne Antwort" };
    let note = if d.note.is_empty() { String::new() } else { format!(", {}", d.note) };
    format!(
        "{e} (Diagnose: Ereignisse {}, gelesen {}, gesendet {}, Sendefehler {}, Schreiben {mode}{note})",
        d.events, d.decoded, d.writes_ok, d.writes_failed
    )
}
