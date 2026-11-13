//! Find nearby printers with a short BLE scan through `bluetoothctl`.
//!
//! Only discovery goes through BlueZ; connecting uses the raw LE socket (see
//! `katasymbol-linux` for why).

use std::process::Command;

#[derive(Clone, Debug)]
pub struct Found {
    pub addr: String,
    pub name: String,
    /// Looks like a Supvan/Katasymbol printer.
    pub likely: bool,
}

/// Supvan printers advertise a serial such as `T0105A2310082067`.
fn likely_printer(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let serial = name.is_ascii()
        && name.len() >= 10
        && name.starts_with('T')
        && name[1..].chars().all(|c| c.is_ascii_alphanumeric())
        && name[1..3].chars().all(|c| c.is_ascii_digit());
    serial || ["katasymbol", "supvan", "e16", "t16"].iter().any(|k| n.contains(k))
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else if c != '\r' && c != '\x01' && c != '\x02' {
            out.push(c);
        }
    }
    out
}

/// Parse `[NEW] Device AA:BB:CC:DD:EE:FF Name` / `Device AA:… Name` lines.
fn parse(output: &str, found: &mut Vec<Found>) {
    for line in strip_ansi(output).lines() {
        let Some(rest) = line.split("Device ").nth(1) else { continue };
        if line.contains("[CHG]") || line.contains("[DEL]") || rest.len() < 19 || !rest.is_char_boundary(17) {
            continue;
        }
        let (addr, name) = (&rest[..17], rest[17..].trim());
        if addr.split(':').count() != 6 || name.is_empty() || name == addr.replace(':', "-") {
            continue; // unnamed devices
        }
        if !found.iter().any(|f| f.addr == addr) {
            found.push(Found { addr: addr.to_owned(), name: name.to_owned(), likely: likely_printer(name) });
        }
    }
}

/// Scan for `secs` seconds; likely printers first.
pub fn scan(secs: u32) -> Result<Vec<Found>, String> {
    let out = Command::new("bluetoothctl")
        .args(["--timeout", &secs.to_string(), "scan", "le"])
        .output()
        .map_err(|e| format!("bluetoothctl nicht startbar: {e}"))?;
    let mut found = Vec::new();
    parse(&String::from_utf8_lossy(&out.stdout), &mut found);
    // Devices BlueZ already knows (seen earlier, so not "[NEW]" this time).
    if let Ok(out) = Command::new("bluetoothctl").arg("devices").output() {
        parse(&String::from_utf8_lossy(&out.stdout), &mut found);
    }
    found.sort_by_key(|f| (!f.likely, f.name.to_lowercase()));
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scan_output() {
        let out = "\x1b[0;93m[NEW]\x1b[0m Device 6C:02:3C:2E:BD:27 6C-02-3C-2E-BD-27\n\
                   [NEW] Device A4:93:40:3E:7B:0C T0105A2310082067\n\
                   [NEW] Device 94:C9:60:4D:C0:43 ZenP1_94\n\
                   [CHG] Device A4:93:40:3E:7B:0C RSSI: 0xffffffb1 (-79)\n\
                   Device A4:93:40:3E:7B:0C T0105A2310082067\n";
        let mut f = Vec::new();
        parse(out, &mut f);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].addr, "A4:93:40:3E:7B:0C");
        assert!(f[0].likely);
        assert!(!f[1].likely);
        assert!(!likely_printer("Shelly2PMG4-D885ACF491DC"));
        assert!(!likely_printer("S22 E5B8 LE"));
    }
}
