//! Wire framing shared by every Supvan BLE/SPP printer.
//!
//! Command frame (16 bytes, host → printer):
//! `7E 5A 0C 00 10 01 AA <cmd> <chk lo> <chk hi> 00 01 <p1 lo> <p1 hi> <p2 lo> <p2 hi>`
//! where the checksum is the byte sum of `[10..16]`.
//!
//! Response (printer → host, one BLE notification):
//! `7E 5A <len lo> <len hi> 00 03 55 <cmd> …` — status registers at `[14..18]`,
//! command-specific payload from `[22..]`.
//!
//! Data frame (512 bytes): `7E 5A FC 01 10 02` + packet
//! `AA BB <chk lo> <chk hi> <index> <count> <500 payload bytes>`, checksum over
//! packet bytes `[4..]`. The printer acks each data frame with an `0xBB` response.

pub const CMD_BUF_FULL: u8 = 0x10;
pub const CMD_INQUIRY_STA: u8 = 0x11;
pub const CMD_CHECK_DEVICE: u8 = 0x12;
pub const CMD_START_PRINT: u8 = 0x13;
pub const CMD_STOP_PRINT: u8 = 0x14;
pub const CMD_RD_DEV_NAME: u8 = 0x16;
pub const CMD_RETURN_MAT: u8 = 0x30;
pub const CMD_NEXT_ZIPPEDBULK: u8 = 0x5C;
pub const CMD_PAPER_BACK: u8 = 0xBA;
/// Echoed in the ack the printer sends for every data frame.
pub const DATA_ACK: u8 = 0xBB;
pub const CMD_READ_FWVER: u8 = 0xC5;
pub const CMD_SET_DENSITY: u8 = 0xC9;

/// Offset of the command byte in both requests and responses.
pub const CMD_OFFSET: usize = 7;

pub const DATA_FRAME_LEN: usize = 512;
pub const DATA_PAYLOAD: usize = 500;

/// Largest single BLE write the vendor apps use; frames are split into these.
pub const BLE_CHUNK: usize = 128;

fn checksum(bytes: &[u8]) -> u16 {
    bytes.iter().fold(0u16, |s, &b| s.wrapping_add(u16::from(b)))
}

/// A 16-byte command frame with two 16-bit parameters.
pub fn cmd_frame2(cmd: u8, p1: u16, p2: u16) -> [u8; 16] {
    let mut f = [0u8; 16];
    f[..7].copy_from_slice(&[0x7E, 0x5A, 0x0C, 0x00, 0x10, 0x01, 0xAA]);
    f[7] = cmd;
    f[11] = 0x01;
    f[12..14].copy_from_slice(&p1.to_le_bytes());
    f[14..16].copy_from_slice(&p2.to_le_bytes());
    let chk = checksum(&f[10..16]);
    f[8..10].copy_from_slice(&chk.to_le_bytes());
    f
}

/// A 16-byte command frame with one parameter.
pub fn cmd_frame(cmd: u8, param: u16) -> [u8; 16] {
    cmd_frame2(cmd, param, 0)
}

/// The 17-byte density frame exactly as the iOS app sends it
/// (payload length 0x0D, value at `[12..14]`).
pub fn density_frame(value: u16) -> [u8; 17] {
    let mut f = [0u8; 17];
    f[..7].copy_from_slice(&[0x7E, 0x5A, 0x0D, 0x00, 0x10, 0x01, 0xAA]);
    f[7] = CMD_SET_DENSITY;
    f[11] = 0x01;
    f[12..14].copy_from_slice(&value.to_le_bytes());
    let chk = checksum(&f[10..17]);
    f[8..10].copy_from_slice(&chk.to_le_bytes());
    f
}

/// Split compressed print data into 512-byte data frames.
pub fn data_frames(compressed: &[u8]) -> Vec<[u8; DATA_FRAME_LEN]> {
    let chunks: Vec<&[u8]> = compressed.chunks(DATA_PAYLOAD).collect();
    let count = chunks.len() as u8;
    chunks
        .iter()
        .enumerate()
        .map(|(i, chunk)| {
            let mut f = [0u8; DATA_FRAME_LEN];
            f[..6].copy_from_slice(&[0x7E, 0x5A, 0xFC, 0x01, 0x10, 0x02]);
            let p = &mut f[6..];
            p[0] = 0xAA;
            p[1] = DATA_ACK;
            p[4] = i as u8;
            p[5] = count;
            p[6..6 + chunk.len()].copy_from_slice(chunk);
            let chk = checksum(&p[4..]);
            p[2..4].copy_from_slice(&chk.to_le_bytes());
            f
        })
        .collect()
}

/// The command a response answers, if `resp` is a well-formed reply frame.
pub fn response_cmd(resp: &[u8]) -> Option<u8> {
    (resp.len() > CMD_OFFSET && resp[0] == 0x7E && resp[1] == 0x5A && resp[5] == 0x03)
        .then(|| resp[CMD_OFFSET])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_match_ios_capture() {
        // INQUIRY_STA, START_PRINT, PAPER_BACK 24, NEXT_ZIPPEDBULK(512, 1) — byte-identical to the capture.
        assert_eq!(cmd_frame(CMD_INQUIRY_STA, 0)[..], [0x7e, 0x5a, 0x0c, 0, 0x10, 1, 0xaa, 0x11, 1, 0, 0, 1, 0, 0, 0, 0]);
        assert_eq!(cmd_frame(CMD_PAPER_BACK, 24)[8..13], [0x19, 0, 0, 1, 0x18]);
        assert_eq!(cmd_frame2(CMD_NEXT_ZIPPEDBULK, 512, 1)[8..], [4, 0, 0, 1, 0, 2, 1, 0]);
        assert_eq!(
            density_frame(110)[..],
            [0x7e, 0x5a, 0x0d, 0, 0x10, 1, 0xaa, 0xc9, 0x6f, 0, 0, 1, 0x6e, 0, 0, 0, 0]
        );
    }

    #[test]
    fn data_frame_layout() {
        let f = data_frames(&[1, 2, 3]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0][..12], [0x7e, 0x5a, 0xfc, 1, 0x10, 2, 0xaa, 0xbb, 7, 0, 0, 1]);
    }
}
