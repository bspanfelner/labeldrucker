//! Brother P-touch label printers with the 128-pin, 180 dpi head
//! (PT-P710BT "P-touch CUBE Plus", PT-P700/P750W, PT-E550W): raster protocol
//! and print flow over a byte stream (Bluetooth SPP/RFCOMM or USB).
//!
//! Sources: Brother's "Raster Command Reference" for the PT-E550W/P750W/
//! P710BT family, and the well-tested open-source `ptouch-print` driver for
//! the bit layout of raster lines.
//!
//! A [`Page`] for these printers is as high as the tape's print area (e.g.
//! 70 dots on 12 mm tape, see [`print_area`]); each page column becomes one
//! raster line across the head.

use crate::page::Page;
use crate::printer::Progress;
use crate::{Error, Result};
use std::time::Duration;

/// Pins on the print head.
pub const HEAD_PINS: usize = 128;
/// Bytes per raster line.
pub const LINE_BYTES: usize = HEAD_PINS / 8;
/// Dots per millimetre (180 dpi), along and across the tape.
pub const DOTS_PER_MM: f32 = 180.0 / 25.4;
/// Tape widths these printers take, mm (3.5 mm tape reports as 4).
pub const TAPE_WIDTHS: [u8; 6] = [4, 6, 9, 12, 18, 24];

/// The printable pins for a tape `width_mm` wide: (first pin, number of
/// pins), centred on the head (Brother's raster reference, "print area").
pub fn print_area(width_mm: u8) -> Option<(usize, usize)> {
    Some(match width_mm {
        3 | 4 => (52, 24),
        6 => (48, 32),
        9 => (39, 50),
        12 => (29, 70),
        18 => (8, 112),
        24 => (0, 128),
        _ => return None,
    })
}

/// Tape width in mm as people say it (3.5 for the code 4).
pub fn tape_mm(width_code: u8) -> f32 {
    if width_code == 4 { 3.5 } else { f32::from(width_code) }
}

/// Status frames start with these bytes (print head mark, size, 'B', '0').
const STATUS_SYNC: [u8; 4] = [0x80, 0x20, 0x42, 0x30];
pub const STATUS_LEN: usize = 32;

/// What the printer reports in a 32-byte status frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Status {
    pub model: u8,
    pub battery: u8,
    pub error1: u8,
    pub error2: u8,
    /// Tape width in mm (4 for 3.5 mm), 0 without a tape.
    pub media_width: u8,
    pub media_type: u8,
    /// 0 reply, 1 printing completed, 2 error, 5 notification, 6 phase change.
    pub status_type: u8,
    /// 0 editing (idle), 1 printing.
    pub phase: u8,
    pub notification: u8,
    pub tape_color: u8,
    pub text_color: u8,
}

pub const STATUS_REPLY: u8 = 0x00;
pub const STATUS_DONE: u8 = 0x01;
pub const STATUS_ERROR: u8 = 0x02;

impl Status {
    pub fn parse(f: &[u8]) -> Option<Self> {
        if f.len() < STATUS_LEN || f[..4] != STATUS_SYNC {
            return None;
        }
        Some(Self {
            model: f[4],
            battery: f[6],
            error1: f[8],
            error2: f[9],
            media_width: f[10],
            media_type: f[11],
            status_type: f[18],
            phase: f[19],
            notification: f[22],
            tape_color: f[24],
            text_color: f[25],
        })
    }

    /// A problem that stops printing, if any.
    pub fn error(&self) -> Option<&'static str> {
        let (e1, e2) = (self.error1, self.error2);
        Some(if e1 & 0x01 != 0 || self.media_width == 0 {
            "no tape cassette"
        } else if e2 & 0x10 != 0 {
            "cover open"
        } else if e1 & 0x04 != 0 {
            "cutter jammed"
        } else if e2 & 0x01 != 0 || self.media_type == 0xFF {
            "unsupported tape cassette"
        } else if e2 & 0x20 != 0 {
            "print head too hot"
        } else if e1 & 0x08 != 0 {
            "battery too weak"
        } else if e1 & 0x40 != 0 {
            "wrong power adapter"
        } else if e2 & 0x80 != 0 {
            "system error"
        } else if e1 | e2 != 0 {
            "printer error"
        } else {
            return None;
        })
    }

    /// The tape cassette in words, e.g. "12 mm schwarz auf weiß".
    pub fn tape_text(&self) -> String {
        let colour = |c: u8| match c {
            0x01 => "weiß",
            0x02 => "andere",
            0x03 => "klar",
            0x04 => "rot",
            0x05 => "blau",
            0x06 => "gelb",
            0x07 => "grün",
            0x08 => "schwarz",
            0x09 => "klar (weiße Schrift)",
            0x0A => "gold",
            0x20 => "matt weiß",
            0x21 => "matt klar",
            0x22 => "matt silber",
            0x23 => "satin gold",
            0x24 => "satin silber",
            0x30 | 0x31 | 0x32 | 0x33 | 0x34 | 0x35 => "farbig",
            0x40 | 0x41 | 0x42 | 0x43 | 0x44 | 0x45 => "fluoreszierend",
            0x50 | 0x51 | 0x52 | 0x53 | 0x54 => "Stoff",
            0x60 | 0x61 | 0x62 => "Flexi",
            0x70 => "Schrumpfschlauch",
            _ => "",
        };
        let w = tape_mm(self.media_width);
        let w = if w.fract() == 0.0 { format!("{w:.0}") } else { format!("{w:.1}").replace('.', ",") };
        match (colour(self.text_color), colour(self.tape_color)) {
            ("", "") => format!("{w} mm"),
            (t, b) if !t.is_empty() && !b.is_empty() => format!("{w} mm {t} auf {b}"),
            (_, b) => format!("{w} mm {b}{}", if self.media_type == 0x11 || self.media_type == 0x17 { " (Schlauch)" } else { "" }),
        }
    }
}

/// TIFF PackBits, as the printer's `M 02` mode expects.
pub fn packbits(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 2);
    let mut i = 0;
    while i < data.len() {
        // A run of at least 2 equal bytes?
        let mut run = 1;
        while i + run < data.len() && data[i + run] == data[i] && run < 128 {
            run += 1;
        }
        if run >= 2 {
            out.push((1i16 - run as i16) as i8 as u8);
            out.push(data[i]);
            i += run;
            continue;
        }
        // Literals up to the next run of 2 (or 128 bytes).
        let start = i;
        while i < data.len() && i - start < 128 && !(i + 1 < data.len() && data[i] == data[i + 1]) {
            i += 1;
        }
        out.push((i - start - 1) as u8);
        out.extend_from_slice(&data[start..i]);
    }
    out
}

/// The raster line for page column `x`: bit `first_pin + y`, counted from
/// the most significant bit of byte 0, set for a black dot at `y`.
fn raster_line(page: &Page, x: usize, first_pin: usize) -> [u8; LINE_BYTES] {
    let mut line = [0u8; LINE_BYTES];
    for y in 0..page.height() {
        if page.get(x, y) {
            let b = first_pin + y;
            if b < HEAD_PINS {
                line[b / 8] |= 0x80 >> (b % 8);
            }
        }
    }
    line
}

/// How a job is printed.
///
/// The cutter sits about 24 mm before the print head, so a job that feeds
/// its last label out (the default) leaves that much blank tape at the
/// cutter, and the next job cuts it off first. Several labels in one job
/// pay for it once; [`chain`](Self::chain) not at all, but keeps the last
/// label in the printer until the next job pushes it out (or
/// [`eject_page`] does).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobOptions {
    /// Cut after each label.
    pub cut: bool,
    /// Chain printing: no feed and cut after the last label.
    pub chain: bool,
    /// Blank tape before and after the print, dots (at least 14, ~2 mm).
    pub margin_dots: u16,
}

impl Default for JobOptions {
    fn default() -> Self {
        Self { cut: true, chain: false, margin_dots: 14 }
    }
}

/// Shortest page the printer takes (4.4 mm), dots.
pub const MIN_LINES: usize = 31;

/// A blank page that, printed without chaining, pushes a label held back
/// by chain printing out and cuts it off (fits every tape).
pub fn eject_page() -> Page {
    Page::blank_sized(MIN_LINES, 24)
}

/// Clears whatever a previous, interrupted job left in the printer's buffer.
pub const INVALIDATE: [u8; 100] = [0; 100];
pub const INITIALIZE: [u8; 2] = [0x1B, 0x40];
pub const STATUS_REQUEST: [u8; 3] = [0x1B, 0x69, 0x53];

/// All bytes of a one-label job for `page` on the tape described by
/// `status` (width and type as the printer reported them).
pub fn job(page: &Page, status: &Status, opts: JobOptions) -> Result<Vec<u8>> {
    Ok(job_pages(&[page], status, opts)?.concat())
}

/// A job of several labels, split where the host has to wait for the
/// printer: part `i` ends with label `i`'s print command, and the next part
/// may only be sent after "printing completed" for it.
pub fn job_pages(pages: &[&Page], status: &Status, opts: JobOptions) -> Result<Vec<Vec<u8>>> {
    let (first_pin, pins) = print_area(status.media_width)
        .ok_or_else(|| Error::Encode(format!("unsupported tape width code {}", status.media_width)))?;
    if pages.is_empty() {
        return Err(Error::Encode("no pages".into()));
    }
    let mut parts = Vec::with_capacity(pages.len());
    for (i, page) in pages.iter().enumerate() {
        if page.height() > pins {
            return Err(Error::Encode(format!("page is {} dots high, the tape prints {pins}", page.height())));
        }
        if page.width() == 0 {
            return Err(Error::Encode("empty page".into()));
        }
        let last = i + 1 == pages.len();
        let mut out = Vec::with_capacity(200 + page.width() * 8);
        if i == 0 {
            out.extend_from_slice(&INVALIDATE);
            out.extend_from_slice(&INITIALIZE);
        }
        // Centre a lower page in the print area.
        let first_pin = first_pin + (pins - page.height()) / 2;
        let lines = page.width() as u32;
        // Raster mode.
        out.extend_from_slice(&[0x1B, 0x69, 0x61, 0x01]);
        // Print information: media type, width and line count are valid; recover on error.
        let media_type = if status.media_type == 0xFF { 0x00 } else { status.media_type };
        out.extend_from_slice(&[0x1B, 0x69, 0x7A, 0x80 | 0x04 | 0x02, media_type, status.media_width, 0x00]);
        out.extend_from_slice(&lines.to_le_bytes());
        out.extend_from_slice(&[if i == 0 { 0x00 } else { 0x01 }, 0x00]);
        // Mode: auto cut.
        out.extend_from_slice(&[0x1B, 0x69, 0x4D, if opts.cut { 0x40 } else { 0x00 }]);
        // Advanced mode: bit 3 = no chain printing (feed and cut after the last label).
        out.extend_from_slice(&[0x1B, 0x69, 0x4B, if opts.chain { 0x00 } else { 0x08 }]);
        // Margin (feed amount).
        let m = opts.margin_dots.max(14);
        out.extend_from_slice(&[0x1B, 0x69, 0x64, m as u8, (m >> 8) as u8]);
        // TIFF (PackBits) compression.
        out.extend_from_slice(&[0x4D, 0x02]);
        for x in 0..page.width() {
            let line = raster_line(page, x, first_pin);
            if line.iter().all(|&b| b == 0) {
                out.push(0x5A);
            } else {
                let packed = packbits(&line);
                out.push(0x47);
                out.extend_from_slice(&(packed.len() as u16).to_le_bytes());
                out.extend_from_slice(&packed);
            }
        }
        // Print, and after the last page also feed (chain printing decides
        // whether that feed happens).
        out.push(if last { 0x1A } else { 0x0C });
        parts.push(out);
    }
    Ok(parts)
}

/// A byte-stream connection to the printer, implemented per platform.
#[allow(async_fn_in_trait)]
pub trait Stream {
    async fn write(&mut self, data: &[u8]) -> Result<()>;
    /// Bytes received since the last call, or `None` once `timeout` passes
    /// without any.
    async fn read(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>>;
    async fn sleep(&mut self, d: Duration);
}

/// Bytes per write while sending a job.
const SEND_CHUNK: usize = 512;
const STATUS_TIMEOUT: Duration = Duration::from_secs(3);
/// From the end of the data to "printing completed" (long labels, cutting).
const PRINT_TIMEOUT: Duration = Duration::from_secs(40);

pub struct PtPrinter<S: Stream> {
    stream: S,
    rx: Vec<u8>,
}

impl<S: Stream> PtPrinter<S> {
    pub fn new(stream: S) -> Self {
        Self { stream, rx: Vec::new() }
    }

    pub fn stream(&self) -> &S {
        &self.stream
    }

    pub fn into_stream(self) -> S {
        self.stream
    }

    /// The next status frame within `timeout`.
    async fn next_status(&mut self, timeout: Duration) -> Result<Option<Status>> {
        let step = Duration::from_millis(100);
        let mut waited = Duration::ZERO;
        loop {
            // A frame in what arrived so far? Drop bytes before a sync mark.
            while !self.rx.is_empty() {
                match self.rx.windows(4).position(|w| w == STATUS_SYNC) {
                    Some(p) => {
                        self.rx.drain(..p);
                        if self.rx.len() < STATUS_LEN {
                            break;
                        }
                        let frame: Vec<u8> = self.rx.drain(..STATUS_LEN).collect();
                        if let Some(s) = Status::parse(&frame) {
                            return Ok(Some(s));
                        }
                    }
                    None => {
                        let keep = self.rx.len().min(3);
                        self.rx.drain(..self.rx.len() - keep);
                        break;
                    }
                }
            }
            if waited >= timeout {
                return Ok(None);
            }
            if let Some(b) = self.stream.read(step).await? {
                self.rx.extend_from_slice(&b);
            } else {
                waited += step;
            }
        }
    }

    /// Ask for the status (clearing any half-sent job first).
    pub async fn status(&mut self) -> Result<Status> {
        self.rx.clear();
        let mut req = Vec::with_capacity(INVALIDATE.len() + 5);
        req.extend_from_slice(&INVALIDATE);
        req.extend_from_slice(&INITIALIZE);
        req.extend_from_slice(&STATUS_REQUEST);
        self.stream.write(&req).await?;
        for _ in 0..8 {
            match self.next_status(STATUS_TIMEOUT).await? {
                Some(s) if s.status_type == STATUS_REPLY => return Ok(s),
                Some(_) => continue,
                None => break,
            }
        }
        Err(Error::NoResponse(STATUS_REQUEST[2]))
    }

    /// Print `page` (as high as the loaded tape's print area, see
    /// [`print_area`]), checking the tape first.
    pub async fn print(&mut self, page: &Page, opts: JobOptions, progress: impl FnMut(Progress)) -> Result<Status> {
        self.print_pages(&[page], opts, progress).await
    }

    /// Print several labels in one job (one blank lead for all of them).
    pub async fn print_pages(&mut self, pages: &[&Page], opts: JobOptions, mut progress: impl FnMut(Progress)) -> Result<Status> {
        progress(Progress::Preparing);
        let st = self.status().await?;
        if let Some(e) = st.error() {
            return Err(Error::Printer(e.into()));
        }
        if st.phase != 0 {
            return Err(Error::Printer("printer is still busy".into()));
        }
        let parts = job_pages(pages, &st, opts)?;
        let of: usize = parts.iter().map(|p| p.len().div_ceil(SEND_CHUNK)).sum();
        let mut sent = 0;
        let mut last = st;
        for (n, part) in parts.iter().enumerate() {
            for chunk in part.chunks(SEND_CHUNK) {
                sent += 1;
                progress(Progress::Sending { buffer: sent, of });
                self.stream.write(chunk).await?;
            }
            if n + 1 == parts.len() {
                progress(Progress::Finishing);
            }
            // "Printing completed" (or an error) comes by itself; only then
            // may the next label follow.
            last = self.wait_done().await?;
        }
        Ok(last)
    }

    async fn wait_done(&mut self) -> Result<Status> {
        let mut waited = Duration::ZERO;
        let step = Duration::from_secs(1);
        while waited < PRINT_TIMEOUT {
            match self.next_status(step).await? {
                Some(s) if s.status_type == STATUS_DONE => return Ok(s),
                Some(s) if s.status_type == STATUS_ERROR => {
                    return Err(Error::Printer(s.error().unwrap_or("printer error").into()));
                }
                Some(_) => {}
                None => waited += step,
            }
        }
        Err(Error::Timeout("the printer to finish"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unpack(mut p: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        while let Some((&h, rest)) = p.split_first() {
            let h = h as i8;
            if h >= 0 {
                let n = h as usize + 1;
                out.extend_from_slice(&rest[..n]);
                p = &rest[n..];
            } else if h != -128 {
                out.extend(std::iter::repeat_n(rest[0], (1 - h as i16) as usize));
                p = &rest[1..];
            } else {
                p = rest;
            }
        }
        out
    }

    #[test]
    fn packbits_round_trips() {
        for data in [
            vec![0u8; 16],
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            vec![0, 0, 0, 0xff, 0xff, 1, 2, 2, 3, 0, 0, 0, 0, 0, 0, 7],
            vec![5],
        ] {
            assert_eq!(unpack(&packbits(&data)), data, "{data:?}");
        }
        // The reference's example: twenty zeros are `ED 00`.
        assert_eq!(packbits(&[0; 20]), [0xED, 0x00]);
    }

    #[test]
    fn print_areas_fill_the_head_symmetrically() {
        for w in TAPE_WIDTHS {
            let (first, pins) = print_area(w).unwrap();
            assert_eq!(first * 2 + pins, HEAD_PINS, "{w} mm");
        }
    }

    #[test]
    fn job_places_the_page_in_the_print_area() {
        let st = Status { media_width: 12, media_type: 1, ..Status::default() };
        let mut page = Page::blank_sized(3, 70);
        page.set(1, 0); // top dot -> pin 29
        page.set(1, 69); // bottom dot -> pin 98
        let data = job(&page, &st, JobOptions::default()).unwrap();
        // Header: invalidate, init, raster mode, print info with 3 lines.
        assert_eq!(&data[100..106], &[0x1B, 0x40, 0x1B, 0x69, 0x61, 0x01]);
        assert_eq!(&data[106..119], &[0x1B, 0x69, 0x7A, 0x86, 1, 12, 0, 3, 0, 0, 0, 0, 0]);
        let raster = data.windows(2).position(|w| w == [0x4D, 0x02]).unwrap() + 2;
        assert_eq!(data[raster], 0x5A); // blank first line
        assert_eq!(data[raster + 1], 0x47);
        let n = u16::from_le_bytes([data[raster + 2], data[raster + 3]]) as usize;
        let line = unpack(&data[raster + 4..raster + 4 + n]);
        let mut want = [0u8; LINE_BYTES];
        want[29 / 8] |= 0x80 >> (29 % 8);
        want[98 / 8] |= 0x80 >> (98 % 8);
        assert_eq!(line, want);
        assert_eq!(data[raster + 4 + n], 0x5A);
        assert_eq!(*data.last().unwrap(), 0x1A);
    }

    #[test]
    fn several_labels_share_one_job() {
        let st = Status { media_width: 12, media_type: 1, ..Status::default() };
        let page = Page::blank_sized(40, 70);
        let parts = job_pages(&[&page, &page, &page], &st, JobOptions { chain: true, ..JobOptions::default() }).unwrap();
        assert_eq!(parts.len(), 3);
        // Invalidate and initialize once, then per page: raster mode, print info.
        assert!(parts[0].starts_with(&INVALIDATE));
        assert!(parts[1].starts_with(&[0x1B, 0x69, 0x61, 0x01, 0x1B, 0x69, 0x7A]));
        // Page flag: first 0, others 1.
        assert_eq!(parts[0][100 + 2 + 4 + 11], 0);
        assert_eq!(parts[1][4 + 11], 1);
        // Chain printing clears bit 3 of ESC i K.
        let k = parts[2].windows(3).position(|w| w == [0x1B, 0x69, 0x4B]).unwrap();
        assert_eq!(parts[2][k + 3], 0x00);
        assert_eq!(*parts[0].last().unwrap(), 0x0C);
        assert_eq!(*parts[1].last().unwrap(), 0x0C);
        assert_eq!(*parts[2].last().unwrap(), 0x1A);
    }

    #[test]
    fn parses_status() {
        let mut f = [0u8; 32];
        f[..4].copy_from_slice(&STATUS_SYNC);
        f[10] = 12;
        f[11] = 1;
        f[24] = 0x01;
        f[25] = 0x08;
        let s = Status::parse(&f).unwrap();
        assert_eq!(s.error(), None);
        assert_eq!(s.tape_text(), "12 mm schwarz auf weiß");
        f[8] = 0x01;
        assert_eq!(Status::parse(&f).unwrap().error(), Some("no tape cassette"));
    }
}
