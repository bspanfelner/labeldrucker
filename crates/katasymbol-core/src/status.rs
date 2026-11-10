//! Decoding printer responses.

/// Printer status registers (`INQUIRY_STA`, and echoed in most acks).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Status {
    pub buffer_full: bool,
    pub label_rw_error: bool,
    pub label_end: bool,
    pub label_mode_error: bool,
    pub low_battery: bool,
    pub busy: bool,
    pub head_hot: bool,
    pub cover_open: bool,
    pub usb_power: bool,
    pub printing: bool,
    pub label_not_installed: bool,
    pub regs: [u8; 4],
}

impl Status {
    /// Decode the four status bytes at `[14..18]` of a response frame.
    pub fn from_response(resp: &[u8]) -> Option<Self> {
        let r: [u8; 4] = resp.get(14..18)?.try_into().ok()?;
        Some(Self {
            buffer_full: r[0] & 0x01 != 0,
            label_rw_error: r[0] & 0x02 != 0,
            label_end: r[0] & 0x04 != 0,
            label_mode_error: r[0] & 0x08 != 0,
            low_battery: r[0] & 0x40 != 0,
            busy: r[1] & 0x04 != 0,
            head_hot: r[1] & 0x08 != 0,
            cover_open: r[2] & 0x08 != 0,
            usb_power: r[2] & 0x10 != 0,
            printing: r[2] & 0x40 != 0,
            label_not_installed: r[3] & 0x01 != 0,
            regs: r,
        })
    }

    /// A human-readable error, if the printer is in a state that stops printing.
    pub fn error(&self) -> Option<&'static str> {
        if self.cover_open {
            Some("cover open")
        } else if self.label_not_installed {
            Some("no label roll installed")
        } else if self.label_rw_error {
            Some("label could not be read")
        } else if self.label_end {
            Some("labels used up")
        } else if self.head_hot {
            Some("print head too hot")
        } else {
            None
        }
    }
}

/// The loaded label roll (`RETURN_MAT`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Material {
    pub rfid_uid: [u8; 7],
    pub label_type: u8,
    /// Across the print head, mm.
    pub width_mm: u8,
    /// Along the feed, mm.
    pub length_mm: u8,
    pub gap_mm: u8,
    pub remaining: Option<u32>,
}

impl Material {
    pub fn from_response(resp: &[u8]) -> Option<Self> {
        let p = resp.get(22..)?;
        if p.len() < 21 {
            return None;
        }
        Some(Self {
            rfid_uid: p[0..7].try_into().ok()?,
            label_type: p[17],
            width_mm: p[18],
            length_mm: p[19],
            gap_mm: p[20],
            remaining: p.get(21..25).map(|b| u32::from_le_bytes(b.try_into().unwrap())),
        })
    }
}

/// ASCII payload from `[22..]` up to the frame's declared length, NUL-trimmed.
pub fn ascii_payload(resp: &[u8]) -> Option<String> {
    let declared = 4 + usize::from(u16::from_le_bytes([*resp.get(2)?, *resp.get(3)?]));
    let p = resp.get(22..declared.min(resp.len()))?;
    let s: String = p.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_captured_frames() {
        // BUF_FULL ack from the iOS capture: buffer full, busy, printing.
        let r = [0x7e, 0x5a, 0x13, 0, 0, 3, 0x55, 0x10, 0x45, 0, 0, 0, 0, 0, 0x01, 0x04, 0x40, 0, 0, 0, 0, 0, 0];
        let s = Status::from_response(&r).unwrap();
        assert!(s.buffer_full && s.busy && s.printing && s.error().is_none());

        let mut m = vec![0x7e, 0x5a, 0x38, 0, 0, 3, 0x55, 0x30];
        m.resize(22, 0);
        m.extend_from_slice(&[0x1d, 0x70, 0xeb, 0x21, 0x93, 0, 0, 0x88, 0x11, 0x74, 0xee, 0xdc, 0x54, 0x29, 0x4e, 0xa9, 0x61, 0, 0x0f, 0x32, 0, 0x77, 0x0c, 0, 0]);
        let mat = Material::from_response(&m).unwrap();
        assert_eq!((mat.width_mm, mat.length_mm, mat.remaining), (15, 50, Some(3191)));
    }
}
