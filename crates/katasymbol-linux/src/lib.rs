//! Linux [`Link`] over a raw LE L2CAP socket on the ATT channel (CID 4).
//!
//! Why not BlueZ's D-Bus API (and so `btleplug`/`bluer`)? The E16 is dual-mode,
//! and BlueZ's `Device1.Connect()` picks BR/EDR for it, after which its LE GATT
//! services are never resolved. Forcing LE over D-Bus (`ConnectDevice`,
//! `PreferredBearer`) needs bluetoothd's experimental mode. A kernel L2CAP LE
//! socket — what `gatttool` uses — needs neither root nor configuration.
//!
//! The link is blocking underneath; the async methods simply block, which is
//! fine for a CLI or a dedicated thread.

use katasymbol_core::{Error, Link, Result};
use std::io;
use std::time::{Duration, Instant};

const AF_BLUETOOTH: libc::c_int = 31;
const BTPROTO_L2CAP: libc::c_int = 0;
const ATT_CID: u16 = 4;
const BDADDR_LE_PUBLIC: u8 = 1;

const ATT_ERROR_RSP: u8 = 0x01;
const ATT_MTU_REQ: u8 = 0x02;
const ATT_MTU_RSP: u8 = 0x03;
const ATT_READ_BY_TYPE_REQ: u8 = 0x08;
const ATT_READ_BY_TYPE_RSP: u8 = 0x09;
const ATT_WRITE_REQ: u8 = 0x12;
const ATT_WRITE_RSP: u8 = 0x13;
const ATT_NOTIFY: u8 = 0x1B;
const ATT_WRITE_CMD: u8 = 0x52;

/// 16-bit UUIDs of the Supvan `e0ff` service's characteristics.
const UUID_NOTIFY: u16 = 0xFFE1;
const UUID_WRITE: u16 = 0xFFE9;
/// First 12 bytes of the Bluetooth base UUID in wire (little-endian) order.
const BT_BASE_LE: [u8; 12] = [0xfb, 0x34, 0x9b, 0x5f, 0x80, 0x00, 0x00, 0x80, 0x00, 0x10, 0x00, 0x00];

#[repr(C)]
struct SockaddrL2 {
    l2_family: libc::sa_family_t,
    l2_psm: u16,
    l2_bdaddr: [u8; 6],
    l2_cid: u16,
    l2_bdaddr_type: u8,
}

fn io_err(e: io::Error) -> Error {
    Error::Link(e.to_string())
}

fn last_err() -> Error {
    io_err(io::Error::last_os_error())
}

fn parse_addr(s: &str) -> Result<[u8; 6]> {
    let bytes: Vec<u8> = s
        .split(':')
        .map(|p| u8::from_str_radix(p, 16))
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| Error::Link(format!("invalid address {s}")))?;
    let mut a: [u8; 6] = bytes.try_into().map_err(|_| Error::Link(format!("invalid address {s}")))?;
    a.reverse();
    Ok(a)
}

/// An LE ATT connection to one printer.
pub struct AttLink {
    fd: libc::c_int,
    write_handle: u16,
    notify_handle: u16,
}

impl Drop for AttLink {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}

impl AttLink {
    /// Connect over LE to `address` (`AA:BB:CC:DD:EE:FF`), discover the
    /// `ffe1`/`ffe9` characteristics and enable notifications.
    pub fn connect(address: &str) -> Result<Self> {
        let peer = parse_addr(address)?;
        let fd = unsafe { libc::socket(AF_BLUETOOTH, libc::SOCK_SEQPACKET, BTPROTO_L2CAP) };
        if fd < 0 {
            return Err(last_err());
        }
        let mut link = Self { fd, write_handle: 0, notify_handle: 0 };
        let local = SockaddrL2 {
            l2_family: AF_BLUETOOTH as _,
            l2_psm: 0,
            l2_bdaddr: [0; 6],
            l2_cid: ATT_CID.to_le(),
            l2_bdaddr_type: BDADDR_LE_PUBLIC,
        };
        let remote = SockaddrL2 { l2_bdaddr: peer, ..local };
        let len = std::mem::size_of::<SockaddrL2>() as libc::socklen_t;
        unsafe {
            if libc::bind(fd, (&raw const local).cast(), len) < 0 {
                return Err(last_err());
            }
            if libc::connect(fd, (&raw const remote).cast(), len) < 0 {
                return Err(last_err());
            }
        }

        link.send(&[ATT_MTU_REQ, 0x0F, 0x02])?; // ask for 527, as the iOS app does
        link.expect(ATT_MTU_RSP)?;
        link.discover()?;
        let [lo, hi] = (link.notify_handle + 1).to_le_bytes(); // CCCD follows the value
        link.send(&[ATT_WRITE_REQ, lo, hi, 0x01, 0x00])?;
        link.expect(ATT_WRITE_RSP)?;
        Ok(link)
    }

    /// Find the value handles of ffe1/ffe9 with Read By Type (characteristic
    /// declarations, 0x2803) over the whole handle range.
    fn discover(&mut self) -> Result<()> {
        let mut start: u16 = 1;
        while self.write_handle == 0 || self.notify_handle == 0 {
            let [s0, s1] = start.to_le_bytes();
            self.send(&[ATT_READ_BY_TYPE_REQ, s0, s1, 0xFF, 0xFF, 0x03, 0x28])?;
            let rsp = self.wait(|p| matches!(p[0], ATT_READ_BY_TYPE_RSP | ATT_ERROR_RSP), Duration::from_secs(4))?
                .ok_or_else(|| Error::Link("GATT discovery timed out".into()))?;
            if rsp[0] == ATT_ERROR_RSP {
                break; // end of database
            }
            if std::env::var_os("KATASYMBOL_DEBUG").is_some() {
                eprintln!("discover {start:#06x}: {rsp:02x?}");
            }
            let item = usize::from(rsp[1]);
            let mut last = start;
            for e in rsp[2..].chunks_exact(item) {
                // handle(2) props(1) value_handle(2) uuid(2 or 16)
                let decl = u16::from_le_bytes([e[0], e[1]]);
                let value = u16::from_le_bytes([e[3], e[4]]);
                // 16-bit UUID, or a 128-bit one on the Bluetooth base
                // (0000xxxx-0000-1000-8000-00805f9b34fb, little-endian on the wire).
                let short = match item {
                    7 => Some(u16::from_le_bytes([e[5], e[6]])),
                    21 if e[5..17] == BT_BASE_LE[..12] && e[19..21] == [0, 0] => {
                        Some(u16::from_le_bytes([e[17], e[18]]))
                    }
                    _ => None,
                };
                match short {
                    Some(UUID_NOTIFY) => self.notify_handle = value,
                    Some(UUID_WRITE) => self.write_handle = value,
                    _ => {}
                }
                last = decl;
            }
            if last == u16::MAX {
                break;
            }
            start = last + 1;
        }
        if self.write_handle == 0 || self.notify_handle == 0 {
            return Err(Error::Link("printer has no ffe1/ffe9 characteristics".into()));
        }
        Ok(())
    }

    fn send(&self, pdu: &[u8]) -> Result<()> {
        if unsafe { libc::send(self.fd, pdu.as_ptr().cast(), pdu.len(), 0) } < 0 {
            return Err(last_err());
        }
        Ok(())
    }

    fn expect(&self, op: u8) -> Result<Vec<u8>> {
        self.wait(|p| p[0] == op, Duration::from_secs(4))?
            .ok_or_else(|| Error::Link(format!("no ATT response {op:#04x}")))
    }

    /// Read PDUs until one matches `pred` or `timeout` passes.
    fn wait(&self, pred: impl Fn(&[u8]) -> bool, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 600];
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            let mut pfd = libc::pollfd { fd: self.fd, events: libc::POLLIN, revents: 0 };
            let r = unsafe { libc::poll(&mut pfd, 1, left.as_millis().clamp(1, i32::MAX as u128) as i32) };
            if r < 0 {
                return Err(last_err());
            }
            if r == 0 {
                return Ok(None);
            }
            let n = unsafe { libc::recv(self.fd, buf.as_mut_ptr().cast(), buf.len(), 0) };
            if n <= 0 {
                return Err(Error::Link("printer disconnected".into()));
            }
            let pdu = &buf[..n as usize];
            if pdu[0] == ATT_MTU_REQ {
                self.send(&[ATT_MTU_RSP, 0x0F, 0x02])?;
                continue;
            }
            if pred(pdu) {
                return Ok(Some(pdu.to_vec()));
            }
        }
    }
}

impl Link for AttLink {
    async fn write(&mut self, chunk: &[u8]) -> Result<()> {
        let mut pdu = Vec::with_capacity(3 + chunk.len());
        pdu.push(ATT_WRITE_CMD);
        pdu.extend_from_slice(&self.write_handle.to_le_bytes());
        pdu.extend_from_slice(chunk);
        self.send(&pdu)
    }

    async fn recv(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let h = self.notify_handle.to_le_bytes();
        let r = self.wait(|p| p.len() > 3 && p[0] == ATT_NOTIFY && p[1..3] == h, timeout)?;
        Ok(r.map(|p| p[3..].to_vec()))
    }

    async fn sleep(&mut self, d: Duration) {
        std::thread::sleep(d);
    }
}
