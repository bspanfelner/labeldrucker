//! Printer I/O on its own thread: the BLE link blocks, the UI must not.
//!
//! The worker connects on demand, keeps the connection, and reconnects once
//! when a command fails on a stale link (the E16 powers off when idle).

use katasymbol_core::printer::{Info, Progress};
use katasymbol_core::{Error, Page, Printer};
use katasymbol_linux::AttLink;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

pub enum Cmd {
    Scan,
    Probe { addr: String },
    Print { addr: String, page: Page, density: u8, copies: u32 },
    Disconnect,
}

pub enum Ev {
    Found(Vec<crate::scan::Found>),
    Connected(bool),
    Info(Info),
    Progress(String),
    Printed,
    Error(String),
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
                let mut state = State { conn: None };
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

struct State {
    conn: Option<(String, Printer<AttLink>)>,
}

impl State {
    fn handle(&mut self, cmd: Cmd, send: &dyn Fn(Ev)) {
        match cmd {
            Cmd::Scan => {
                send(Ev::Progress("Suche Drucker (8 s) …".into()));
                match crate::scan::scan(8) {
                    Ok(found) => send(Ev::Found(found)),
                    Err(e) => send(Ev::Error(e)),
                }
            }
            Cmd::Disconnect => {
                self.conn = None;
                send(Ev::Connected(false));
            }
            Cmd::Probe { addr } => {
                match self.with_printer(&addr, send, |p| pollster::block_on(p.probe())) {
                    Ok(info) => send(Ev::Info(info)),
                    Err(e) => send(Ev::Error(e)),
                }
            }
            Cmd::Print { addr, page, density, copies } => {
                let copies = copies.max(1);
                let mut r = Ok(());
                for n in 1..=copies {
                    let prefix = if copies > 1 { format!("Kopie {n}/{copies}: ") } else { String::new() };
                    r = self.with_printer(&addr, send, |p| {
                        pollster::block_on(p.print(&page, density, |pr| {
                            send(Ev::Progress(match pr {
                                Progress::Preparing => format!("{prefix}Vorbereiten …"),
                                Progress::Sending { buffer, of } => format!("{prefix}Sende Teil {buffer}/{of} …"),
                                Progress::Finishing => format!("{prefix}Drucke …"),
                            }))
                        }))
                    });
                    if r.is_err() {
                        break;
                    }
                }
                match r {
                    Ok(()) => {
                        send(Ev::Printed);
                        // Refresh the label counter and status.
                        if let Ok(info) = self.with_printer(&addr, send, |p| pollster::block_on(p.probe())) {
                            send(Ev::Info(info));
                        }
                    }
                    Err(e) => send(Ev::Error(e)),
                }
            }
        }
    }

    fn connect(&mut self, addr: &str, send: &dyn Fn(Ev)) -> Result<(), String> {
        self.conn = None;
        send(Ev::Progress(format!("Verbinde mit {addr} …")));
        let link = AttLink::connect(addr).map_err(|e| format!("Verbindung fehlgeschlagen: {e}"))?;
        self.conn = Some((addr.to_owned(), Printer::new(link)));
        send(Ev::Connected(true));
        Ok(())
    }

    /// Run `op` on a connected printer; on a link error with an older
    /// connection, reconnect once and retry.
    fn with_printer<T>(
        &mut self,
        addr: &str,
        send: &dyn Fn(Ev),
        mut op: impl FnMut(&mut Printer<AttLink>) -> katasymbol_core::Result<T>,
    ) -> Result<T, String> {
        let fresh = !matches!(&self.conn, Some((a, _)) if a == addr);
        if fresh {
            self.connect(addr, send)?;
        }
        let printer = &mut self.conn.as_mut().expect("connected").1;
        match op(printer) {
            Ok(v) => Ok(v),
            Err(Error::Link(_)) if !fresh => {
                self.connect(addr, send)?;
                let printer = &mut self.conn.as_mut().expect("connected").1;
                op(printer).map_err(|e| self.fail(e, send))
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
