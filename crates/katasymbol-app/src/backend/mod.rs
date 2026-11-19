//! Printer communication, per platform, behind one message interface:
//! the UI sends [`Cmd`]s and polls [`Ev`]s.
//!
//! - Linux: raw LE socket (`katasymbol-linux`), scan via `bluetoothctl`.
//! - macOS / Windows: `btleplug` (`katasymbol-btle`).
//! - Web: Web Bluetooth (`katasymbol-web`); the browser's chooser replaces the scan.

#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use katasymbol_core::Page;
use katasymbol_core::brother;
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::Worker;

#[cfg(target_os = "linux")]
mod linux_scan;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::Worker;

/// The browser link's trace of writes and notifications (empty natively).
pub fn link_trace() -> String {
    #[cfg(target_arch = "wasm32")]
    return katasymbol_web::trace_text();
    #[cfg(not(target_arch = "wasm32"))]
    String::new()
}

/// Whether the user picks the printer in a browser dialog (no address/scan list).
pub const BROWSER_CHOOSER: bool = cfg!(target_arch = "wasm32");

/// Which printer model the app drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Kind {
    /// Katasymbol/Supvan E16 (Bluetooth LE).
    #[default]
    E16,
    /// Brother P-touch PT-P710BT and its 128-pin siblings (Bluetooth Classic).
    Brother,
}

impl Kind {
    pub fn title(self) -> &'static str {
        match self {
            Kind::E16 => "Katasymbol E16",
            Kind::Brother => "Brother PT-P710BT",
        }
    }
}

/// What the printer reported, for either model.
#[derive(Clone, Debug)]
pub struct Info {
    pub name: Option<String>,
    /// A problem that stops printing.
    pub error: Option<String>,
    /// Width of the loaded labels or tape, mm.
    pub width_mm: Option<f32>,
    /// Length of die-cut labels, mm (none for continuous tape).
    pub length_mm: Option<f32>,
    /// Labels left (E16's counter).
    pub counter: Option<u32>,
    /// Brother: the full status (tape width code, colours).
    pub tape: Option<brother::Status>,
}

impl From<katasymbol_core::printer::Info> for Info {
    fn from(i: katasymbol_core::printer::Info) -> Self {
        let m = i.material.as_ref();
        Self {
            name: i.name.clone(),
            error: i.status.error().map(str::to_owned),
            width_mm: m.map(|m| f32::from(m.width_mm)).filter(|w| *w >= 12.0),
            length_mm: m.map(|m| f32::from(m.length_mm)).filter(|l| *l > 0.0),
            counter: m.and_then(|m| m.remaining),
            tape: None,
        }
    }
}

impl From<brother::Status> for Info {
    fn from(s: brother::Status) -> Self {
        Self {
            name: Some("Brother P-touch".into()),
            error: s.error().map(str::to_owned),
            width_mm: (s.media_width > 0).then(|| brother::tape_mm(s.media_width)),
            length_mm: None,
            counter: None,
            tape: Some(s),
        }
    }
}

pub enum Cmd {
    /// Find printers (native) or open the device chooser and connect (web).
    /// `all`: the browser's chooser lists every device, not just printers.
    Scan {
        #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
        all: bool,
        kind: Kind,
    },
    Probe {
        addr: String,
        #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
        kind: Kind,
    },
    Print {
        addr: String,
        page: Page,
        density: u8,
        copies: u32,
        #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
        kind: Kind,
        /// Brother: chain printing (the last label stays in the printer).
        #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
        chain: bool,
    },
    /// Brother: push out and cut a label held back by chain printing.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Eject,
    Disconnect,
}

pub enum Ev {
    Found(Vec<Found>),
    Connected(bool),
    /// The connected device's id and name (web: from the chooser).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Device { addr: String, name: String },
    Info(Info),
    Progress(String),
    Printed,
    Error(String),
}

#[derive(Clone, Debug)]
pub struct Found {
    pub addr: String,
    pub name: String,
    /// Looks like a Supvan/Katasymbol printer.
    pub likely: bool,
}

/// Supvan printers advertise a serial such as `T0105A2310082067`.
pub fn likely_printer(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let serial = name.is_ascii()
        && name.len() >= 10
        && name.starts_with('T')
        && name[1..].chars().all(|c| c.is_ascii_alphanumeric())
        && name[1..3].chars().all(|c| c.is_ascii_digit());
    serial || ["katasymbol", "supvan", "e16", "t16"].iter().any(|k| n.contains(k))
}

/// Progress text for a print job.
pub fn progress_text(prefix: &str, p: katasymbol_core::printer::Progress) -> String {
    use katasymbol_core::printer::Progress;
    match p {
        Progress::Preparing => format!("{prefix}Vorbereiten …"),
        Progress::Sending { buffer, of } => format!("{prefix}Sende Teil {buffer}/{of} …"),
        Progress::Finishing => format!("{prefix}Drucke …"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_printer_names() {
        assert!(likely_printer("T0105A2310082067"));
        assert!(!likely_printer("Shelly2PMG4-D885ACF491DC"));
        assert!(!likely_printer("S22 E5B8 LE"));
        assert!(!likely_printer("ZenP1_94"));
    }
}
