//! Printer communication, per platform, behind one message interface:
//! the UI sends [`Cmd`]s and polls [`Ev`]s.
//!
//! - Linux: raw LE socket (`katasymbol-linux`), scan via `bluetoothctl`.
//! - macOS / Windows: `btleplug` (`katasymbol-btle`).
//! - Web: Web Bluetooth (`katasymbol-web`); the browser's chooser replaces the scan.

#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use katasymbol_core::Page;
use katasymbol_core::printer::Info;

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

/// Whether the user picks the printer in a browser dialog (no address/scan list).
pub const BROWSER_CHOOSER: bool = cfg!(target_arch = "wasm32");

pub enum Cmd {
    /// Find printers (native) or open the device chooser and connect (web).
    /// `all`: the browser's chooser lists every device, not just printers.
    Scan {
        #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
        all: bool,
    },
    Probe { addr: String },
    Print { addr: String, page: Page, density: u8, copies: u32 },
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
