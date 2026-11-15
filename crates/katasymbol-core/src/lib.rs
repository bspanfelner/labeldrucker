//! Platform-independent core for Katasymbol/Supvan E16 (T16) label printers.
//!
//! - [`protocol`]: command and data framing (`7E 5A` frames).
//! - [`status`]: decoding of status, material and name responses.
//! - [`page`]: the 1-bit page bitmap in printhead (column) layout.
//! - [`encode`]: splitting a page into print buffers and LZMA-compressing them.
//! - [`printer`]: the print flow over any [`Link`] (BLE on each platform).
//!
//! Protocol sources: the Katasymbol Android app (`T16Print`, v1.4.31), an iOS
//! PacketLogger capture of the official app driving an E16 over BLE, and the
//! MIT-licensed `heeen/supvan-cups` project for the shared Supvan framing.
//!
//! No async runtime is required: the [`Link`] implementation supplies I/O and
//! sleeping, so the same code runs natively and in the browser.

pub mod encode;
pub mod page;
pub mod printer;
pub mod protocol;
pub mod status;
pub mod draw;
#[cfg(feature = "text")]
pub mod icons;
#[cfg(feature = "text")]
pub mod label;
#[cfg(feature = "text")]
pub mod text;

pub use page::Page;
pub use printer::{Link, Printer};
pub use status::{Material, Status};

/// Errors from the core.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("link error: {0}")]
    Link(String),
    #[error("no response from printer to command {0:#04x}")]
    NoResponse(u8),
    #[error("printer reports: {0}")]
    Printer(String),
    #[error("timed out waiting for {0}")]
    Timeout(&'static str),
    #[error("encoding failed: {0}")]
    Encode(String),
}

pub type Result<T> = std::result::Result<T, Error>;
