//! The print flow, generic over the BLE link of each platform.

use crate::encode::{EncodedPage, encode_page};
use crate::page::Page;
use crate::protocol::*;
use crate::status::{Material, Status, ascii_payload};
use crate::{Error, Result};
use std::time::Duration;

/// How long to wait for any one notification.
pub const RESPONSE_TIMEOUT: Duration = Duration::from_secs(4);
const CHUNK_DELAY: Duration = Duration::from_millis(12);
const POLL: Duration = Duration::from_millis(100);
const BUFFER_POLL: Duration = Duration::from_millis(20);

/// A BLE connection to the printer, implemented per platform.
///
/// `write` sends one ATT *Write Command* (no response) to the `ffe9`
/// characteristic; `recv` returns the next `ffe1` notification payload.
#[allow(async_fn_in_trait)]
pub trait Link {
    async fn write(&mut self, chunk: &[u8]) -> Result<()>;
    /// Next notification, or `None` once `timeout` passes without one.
    async fn recv(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>>;
    async fn sleep(&mut self, d: Duration);
    /// Largest write this link sends at once (frames are split into chunks).
    fn chunk_size(&self) -> usize {
        BLE_CHUNK
    }
    /// Drop notifications already received but not yet read: leftovers
    /// (e.g. duplicates, which BlueZ delivers once per subscribed client)
    /// must not pass for the answer to the next command.
    fn discard_pending(&mut self) {}
}

/// What `probe` reports.
#[derive(Debug, Clone)]
pub struct Info {
    pub name: Option<String>,
    pub status: Status,
    pub material: Option<Material>,
}

/// Print-job progress, for UIs.
#[derive(Debug, Clone, Copy)]
pub enum Progress {
    Preparing,
    Sending { buffer: usize, of: usize },
    Finishing,
}

pub struct Printer<L: Link> {
    link: L,
}

impl<L: Link> Printer<L> {
    pub fn new(link: L) -> Self {
        Self { link }
    }

    pub fn into_link(self) -> L {
        self.link
    }

    pub fn link(&self) -> &L {
        &self.link
    }

    async fn write_frame(&mut self, frame: &[u8]) -> Result<()> {
        self.link.discard_pending();
        let size = self.link.chunk_size().max(1);
        for (i, chunk) in frame.chunks(size).enumerate() {
            if i > 0 {
                self.link.sleep(CHUNK_DELAY).await;
            }
            self.link.write(chunk).await?;
        }
        Ok(())
    }

    /// Wait for the response echoing `cmd`, skipping unrelated notifications.
    async fn response(&mut self, cmd: u8) -> Result<Vec<u8>> {
        for _ in 0..32 {
            match self.link.recv(RESPONSE_TIMEOUT).await? {
                Some(r) if response_cmd(&r) == Some(cmd) => return Ok(r),
                Some(_) => continue,
                None => break,
            }
        }
        Err(Error::NoResponse(cmd))
    }

    /// Send a command frame and return the printer's reply.
    pub async fn command(&mut self, frame: &[u8]) -> Result<Vec<u8>> {
        self.write_frame(frame).await?;
        self.response(frame[CMD_OFFSET]).await
    }

    pub async fn status(&mut self) -> Result<Status> {
        let r = self.command(&cmd_frame(CMD_INQUIRY_STA, 0)).await?;
        Status::from_response(&r).ok_or(Error::NoResponse(CMD_INQUIRY_STA))
    }

    pub async fn material(&mut self) -> Result<Option<Material>> {
        let r = self.command(&cmd_frame(CMD_RETURN_MAT, 0)).await?;
        Ok(Material::from_response(&r))
    }

    pub async fn probe(&mut self) -> Result<Info> {
        let name = ascii_payload(&self.command(&cmd_frame(CMD_RD_DEV_NAME, 0)).await?);
        let status = self.status().await?;
        let material = self.material().await?;
        Ok(Info { name, status, material })
    }

    async fn checked_status(&mut self) -> Result<Status> {
        let s = self.status().await?;
        match s.error() {
            Some(e) => Err(Error::Printer(e.into())),
            None => Ok(s),
        }
    }

    /// Print one page at `density` (1-7, vendor default 4).
    pub async fn print(&mut self, page: &Page, density: u8, mut progress: impl FnMut(Progress)) -> Result<()> {
        progress(Progress::Preparing);
        let density = density.clamp(1, 7);
        let EncodedPage { buffers, paper_back } = encode_page(page, density)?;

        // The order of the vendor app (iOS capture): wait for an idle
        // printer, set the density, start the job, wait until the printer
        // reports printing, and only then announce the label (PAPER_BACK).
        // Sending PAPER_BACK right after START_PRINT (and a CHECK_DEVICE,
        // which the vendor app never sends) made the printer abort the job
        // under Chromium/BlueZ, whose answers come faster than iOS's.

        // Wait (up to 6 s) for an idle printer, stopping a leftover job.
        let mut idle = false;
        for _ in 0..60 {
            let s = self.checked_status().await?;
            if s.printing {
                self.command(&cmd_frame(CMD_STOP_PRINT, 0)).await?;
            }
            if !s.busy && !s.printing {
                idle = true;
                break;
            }
            self.link.sleep(POLL).await;
        }
        if !idle {
            return Err(Error::Timeout("printer to become idle"));
        }

        // Density as the vendor maps it: ((d - 1) / 10 + 0.8) * 100.
        let value = 80 + u16::from(density - 1) * 10;
        self.command(&density_frame(value)).await?;
        self.command(&cmd_frame(CMD_START_PRINT, 0)).await?;

        // Wait (up to 3 s) for the job to run; the vendor app saw it on the
        // first poll. A printer that is neither busy nor printing dropped it.
        let mut idle_polls = 0;
        for _ in 0..30 {
            let s = self.checked_status().await?;
            if s.printing {
                break;
            }
            if !s.busy {
                idle_polls += 1;
                if idle_polls >= 3 {
                    return Err(Error::Printer("printer did not start the job".into()));
                }
            }
            self.link.sleep(POLL).await;
        }
        self.command(&cmd_frame(CMD_PAPER_BACK, paper_back)).await?;

        let of = buffers.len();
        for (i, data) in buffers.iter().enumerate() {
            // Wait (up to 15 s) for the printer to free its buffer.
            let mut ready = false;
            for _ in 0..750 {
                let s = self.checked_status().await?;
                if !s.busy && !s.printing {
                    return Err(Error::Printer("job ended before all data was sent".into()));
                }
                if !s.buffer_full {
                    ready = true;
                    break;
                }
                self.link.sleep(BUFFER_POLL).await;
            }
            if !ready {
                return Err(Error::Timeout("printer buffer"));
            }
            progress(Progress::Sending { buffer: i + 1, of });
            let frames = data_frames(data);
            self.command(&cmd_frame2(CMD_NEXT_ZIPPEDBULK, DATA_FRAME_LEN as u16, frames.len() as u16))
                .await?;
            for f in &frames {
                self.write_frame(f).await?;
                self.response(DATA_ACK).await?;
            }
            self.command(&cmd_frame(CMD_BUF_FULL, 0)).await?;
        }

        progress(Progress::Finishing);
        for _ in 0..300 {
            self.link.sleep(POLL).await;
            let s = self.checked_status().await?;
            if !s.printing && !s.busy {
                return Ok(());
            }
        }
        Err(Error::Timeout("print to finish"))
    }
}
