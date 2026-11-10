//! Turning a [`Page`] into the compressed print buffers the E16 expects
//! (`T16Print.initLZMAData`, confirmed byte-for-byte against the iOS app).
//!
//! Each buffer is 4000 bytes: a 14-byte header followed by up to 332 columns
//! of 12 bytes. Every buffer is LZMA-compressed on its own (props `0x5D`,
//! dictionary 8192, exact size in the header, no end marker).

use crate::page::{BYTES_PER_COL, Page};
use crate::{Error, Result};
use lzma_rust2::{LzmaOptions, LzmaWriter};
use std::io::Write;

const BUF_LEN: usize = 4000;
const HDR: usize = 14;
const MAX_COLS: usize = 332;
/// The first 6 mm are pulled back with PAPER_BACK instead of being sent blank.
const LEAD_IN: usize = 48;

/// A page ready to send.
#[derive(Debug, Clone)]
pub struct EncodedPage {
    pub buffers: Vec<Vec<u8>>,
    /// Columns to reverse-feed before printing (PAPER_BACK parameter).
    pub paper_back: u16,
}

/// Page-control bits (`PAGE_REG_BITS.toByteArray()`, Mat at bit 4).
fn page_reg(first: bool, last: bool, density: u8) -> [u8; 2] {
    let mut b0 = 0;
    if first {
        b0 |= 0x02;
    }
    if last {
        b0 |= 0x04 | 0x08; // PageEnd | PrtEnd
    }
    [b0, (density & 0x0F) << 2 | 1 << 4]
}

pub fn compress(data: &[u8]) -> Result<Vec<u8>> {
    let mut opts = LzmaOptions::with_preset(6);
    opts.dict_size = 8192;
    opts.lc = 3;
    opts.lp = 0;
    opts.pb = 2;
    let err = |e: std::io::Error| Error::Encode(e.to_string());
    let mut w = LzmaWriter::new_use_header(Vec::new(), &opts, Some(data.len() as u64)).map_err(err)?;
    w.write_all(data).map_err(err)?;
    w.finish().map_err(err)
}

/// Encode one page (the job's only/last page) at `density` (1-7).
pub fn encode_page(page: &Page, density: u8) -> Result<EncodedPage> {
    let cols = page.columns();
    if cols.is_empty() {
        return Err(Error::Encode("empty page".into()));
    }
    let skip = page.leading_blank(LEAD_IN).min(cols.len() - 1);
    let paper_back = if skip >= LEAD_IN - 1 { 0 } else { (LEAD_IN - skip) as u16 };

    let mut buffers = Vec::new();
    let mut pos = skip;
    let mut first = true;
    while pos < cols.len() || first {
        let n = (cols.len() - pos).min(MAX_COLS);
        let last = pos + n >= cols.len();
        let mut buf = vec![0u8; BUF_LEN];
        for (i, col) in cols[pos..pos + n].iter().enumerate() {
            buf[HDR + i * BYTES_PER_COL..HDR + (i + 1) * BYTES_PER_COL].copy_from_slice(col);
        }
        buf[2..4].copy_from_slice(&page_reg(first, last, density));
        buf[4..6].copy_from_slice(&(n as u16).to_le_bytes());
        buf[6] = BYTES_PER_COL as u8;
        buf[10] = 1;
        buf[12] = if first { skip as u8 } else { 0 };

        let len = n * BYTES_PER_COL + HDR;
        let mut sum: u32 = buf[2..HDR].iter().map(|&b| u32::from(b)).sum();
        for k in 1..=len / 256 {
            sum += u32::from(buf[k * 256 - 1]);
        }
        buf[0..2].copy_from_slice(&(sum as u16).to_le_bytes());
        buffers.push(compress(&buf)?);
        pos += n;
        first = false;
    }
    Ok(EncodedPage { buffers, paper_back })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lzma_rust2::LzmaReader;
    use std::io::Read;

    fn decompress(c: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        LzmaReader::new_mem_limit(c, u32::MAX, None).unwrap().read_to_end(&mut out).unwrap();
        out
    }

    #[test]
    fn compressed_stream_has_vendor_header_and_no_end_marker() {
        let data = vec![0u8; BUF_LEN];
        let c = compress(&data).unwrap();
        assert_eq!(c[0], 0x5D);
        assert_eq!(u32::from_le_bytes(c[1..5].try_into().unwrap()), 8192);
        assert_eq!(u64::from_le_bytes(c[5..13].try_into().unwrap()), BUF_LEN as u64);
        assert_eq!(decompress(&c), data);
    }

    #[test]
    fn header_matches_ios_capture_layout() {
        // The iOS app's 20 mm "3x40" label: content starts at column 24.
        let mut p = Page::blank(20);
        p.fill_rect(25, 40, 30, 50);
        let e = encode_page(&p, 4).unwrap();
        assert_eq!(e.paper_back, 24);
        assert_eq!(e.buffers.len(), 1);
        let b = decompress(&e.buffers[0]);
        // iOS: e3 00 0e 10 a0 00 0c 00 00 00 01 00 18 00 (cols differ: 160 - 24 = 136 here)
        assert_eq!(b[2..4], [0x0e, 0x10]);
        assert_eq!(u16::from_le_bytes([b[4], b[5]]), 136);
        assert_eq!(b[6..14], [0x0c, 0, 0, 0, 1, 0, 24, 0]);
    }

    #[test]
    fn long_page_splits_at_332_columns() {
        let mut p = Page::blank(50);
        p.fill_rect(0, 10, 400, 11);
        let e = encode_page(&p, 4).unwrap();
        assert_eq!(e.paper_back, 48);
        let b: Vec<_> = e.buffers.iter().map(|c| decompress(c)).collect();
        assert_eq!(b.len(), 2);
        assert_eq!((b[0][2], u16::from_le_bytes([b[0][4], b[0][5]])), (0x02, 332));
        assert_eq!((b[1][2], u16::from_le_bytes([b[1][4], b[1][5]])), (0x0c, 68));
    }
}
