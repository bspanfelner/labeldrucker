//! Printer I/O on its own thread: BLE calls block or need their own runtime,
//! the UI must not wait.
//!
//! The worker connects on demand, keeps the connection, and reconnects once
//! when a command fails on a stale link (the E16 powers off when idle).

use super::{Cmd, Ev, Found, progress_text};
use katasymbol_core::{Error, Link, Printer};
use std::future::Future;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

/// The platform's way to find and connect printers and to run futures.
trait Transport {
    type Link: Link;
    fn scan(&mut self, secs: u64) -> Result<Vec<Found>, String>;
    fn connect(&mut self, addr: &str) -> Result<Self::Link, String>;
    fn block_on<F: Future>(&mut self, f: F) -> F::Output;
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use katasymbol_linux::AttLink;

    pub struct Linux;

    impl Transport for Linux {
        type Link = AttLink;
        fn scan(&mut self, secs: u64) -> Result<Vec<Found>, String> {
            crate::backend::linux_scan::scan(secs as u32)
        }
        fn connect(&mut self, addr: &str) -> Result<AttLink, String> {
            AttLink::connect(addr).map_err(|e| e.to_string())
        }
        fn block_on<F: Future>(&mut self, f: F) -> F::Output {
            pollster::block_on(f)
        }
    }

    pub fn transport() -> Linux {
        Linux
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod platform {
    use super::*;
    use crate::backend::likely_printer;
    use katasymbol_btle::BtleLink;

    pub struct Btle(tokio::runtime::Runtime);

    impl Transport for Btle {
        type Link = BtleLink;
        fn scan(&mut self, secs: u64) -> Result<Vec<Found>, String> {
            let seen = self.0.block_on(katasymbol_btle::scan(secs)).map_err(|e| e.to_string())?;
            let mut found: Vec<Found> =
                seen.into_iter().map(|s| Found { likely: likely_printer(&s.name), addr: s.id, name: s.name }).collect();
            found.sort_by_key(|f| (!f.likely, f.name.to_lowercase()));
            Ok(found)
        }
        fn connect(&mut self, addr: &str) -> Result<BtleLink, String> {
            self.0.block_on(BtleLink::connect(addr)).map_err(|e| e.to_string())
        }
        fn block_on<F: Future>(&mut self, f: F) -> F::Output {
            self.0.block_on(f)
        }
    }

    pub fn transport() -> Btle {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
        Btle(rt)
    }
}

pub struct Worker {
    tx: Sender<Cmd>,
    pub rx: Receiver<Ev>,
}

impl Worker {
    /// `wake` is called after every event so the UI repaints.
    pub fn spawn(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, cmd_rx) = channel::<Cmd>();
        let (ev_tx, rx) = channel::<Ev>();
        thread::Builder::new()
            .name("printer".into())
            .spawn(move || {
                let send = |e: Ev| {
                    let _ = ev_tx.send(e);
                    wake();
                };
                let mut state = State { transport: platform::transport(), conn: None };
                for cmd in cmd_rx {
                    state.handle(cmd, &send);
                }
            })
            .expect("spawn printer thread");
        Self { tx, rx }
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }
}

struct State<T: Transport> {
    transport: T,
    conn: Option<(String, Printer<T::Link>)>,
}

impl<T: Transport> State<T> {
    fn handle(&mut self, cmd: Cmd, send: &dyn Fn(Ev)) {
        match cmd {
            Cmd::Scan { kind: super::Kind::Brother, .. } => {
                send(Ev::Error("Der Brother-Drucker geht bisher nur in der Web-App (Chrome oder Edge am Computer).".into()));
            }
            Cmd::Scan { .. } => {
                send(Ev::Progress("Suche Drucker (8 s) …".into()));
                match self.transport.scan(8) {
                    Ok(found) => send(Ev::Found(found)),
                    Err(e) => send(Ev::Error(e)),
                }
            }
            Cmd::Disconnect => {
                self.conn = None;
                send(Ev::Connected(false));
            }
            Cmd::Probe { addr, .. } => match self.with_printer(&addr, send, |t, p| t.block_on(p.probe())) {
                Ok(info) => send(Ev::Info(info.into())),
                Err(e) => send(Ev::Error(e)),
            },
            Cmd::Print { addr, page, density, copies, .. } => {
                let copies = copies.max(1);
                let mut r = Ok(());
                for n in 1..=copies {
                    let prefix = if copies > 1 { format!("Kopie {n}/{copies}: ") } else { String::new() };
                    r = self.with_printer(&addr, send, |t, p| {
                        t.block_on(p.print(&page, density, |pr| send(Ev::Progress(progress_text(&prefix, pr)))))
                    });
                    if r.is_err() {
                        break;
                    }
                }
                match r {
                    Ok(()) => {
                        send(Ev::Printed);
                        // Refresh the label counter and status.
                        if let Ok(info) = self.with_printer(&addr, send, |t, p| t.block_on(p.probe())) {
                            send(Ev::Info(info.into()));
                        }
                    }
                    Err(e) => send(Ev::Error(e)),
                }
            }
        }
    }

    fn connect(&mut self, addr: &str, send: &dyn Fn(Ev)) -> Result<(), String> {
        self.conn = None;
        send(Ev::Progress("Verbinde …".into()));
        let link = self.transport.connect(addr).map_err(|e| format!("Verbindung fehlgeschlagen: {e}"))?;
        self.conn = Some((addr.to_owned(), Printer::new(link)));
        send(Ev::Connected(true));
        Ok(())
    }

    /// Run `op` on a connected printer; on a link error with an older
    /// connection, reconnect once and retry.
    fn with_printer<R>(
        &mut self,
        addr: &str,
        send: &dyn Fn(Ev),
        mut op: impl FnMut(&mut T, &mut Printer<T::Link>) -> katasymbol_core::Result<R>,
    ) -> Result<R, String> {
        let fresh = !matches!(&self.conn, Some((a, _)) if a == addr);
        if fresh {
            self.connect(addr, send)?;
        }
        let printer = &mut self.conn.as_mut().expect("connected").1;
        match op(&mut self.transport, printer) {
            Ok(v) => Ok(v),
            Err(Error::Link(_)) if !fresh => {
                self.connect(addr, send)?;
                let printer = &mut self.conn.as_mut().expect("connected").1;
                op(&mut self.transport, printer).map_err(|e| self.fail(e, send))
            }
            Err(e) => Err(self.fail(e, send)),
        }
    }

    fn fail(&mut self, e: Error, send: &dyn Fn(Ev)) -> String {
        if matches!(e, Error::Link(_)) {
            self.conn = None;
            send(Ev::Connected(false));
        }
        e.to_string()
    }
}
