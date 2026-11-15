# Katasymbol

Rust software for the Katasymbol (Supvan) **E16** label printer over Bluetooth Low Energy.
Targets: Linux, macOS, iOS and Web.

## Platforms

| Platform | How to run | Build |
|---|---|---|
| **Linux** (x86_64, glibc ≥ 2.30) | unpack, run `katasymbol-app` (or `./install-linux.sh` for a menu entry) — no packages needed | `scripts/package-linux.sh` |
| **macOS** 11+ (Apple Silicon + Intel) | `Katasymbol.app` (zip or dmg), allow Bluetooth on first start | `scripts/package-macos.sh` (on a Mac) |
| **Web** (Chrome/Edge desktop + Android; iPhone/iPad via the Bluefy browser) | open the hosted page — nothing to install | `scripts/package-web.sh` |
| **iOS native** | not yet — use the web app in Bluefy | — |
| Windows | the btleplug link supports it; not packaged yet | — |

All platforms run the same app (`katasymbol-app`, egui) over the same core; only
the Bluetooth link differs: raw LE L2CAP socket on Linux (`katasymbol-linux`),
CoreBluetooth via btleplug on macOS (`katasymbol-btle`), Web Bluetooth in the
browser (`katasymbol-web`, started by `katasymbol-webapp`).

`.github/workflows/build.yml` builds Linux, macOS and web on GitHub (manually or on a
`v*` tag), attaches them to a release and publishes the web app on GitHub Pages.

### Crates

| Crate | Purpose |
|------|-------|
| `katasymbol-core` | protocol, page bitmap, LZMA, label layout (text, pictograms, 45 frames), print flow — platform independent, builds for WASM and iOS |
| `katasymbol-linux` | BLE link via a raw LE L2CAP ATT socket |
| `katasymbol-btle` | BLE link via btleplug (macOS, Windows) |
| `katasymbol-web` | BLE link via Web Bluetooth |
| `katasymbol-app` | the app (desktop binary + library for the web build) |
| `katasymbol-webapp` | WebAssembly entry point |
| `katasymbol-cli` | `katasymbol probe | test | image` (Linux) |

## Usage (CLI, Linux)

```sh
cargo build --release
export KATASYMBOL_ADDR=A4:93:40:3E:7B:0C
./target/release/katasymbol probe
./target/release/katasymbol test --length 30
./target/release/katasymbol image label.png --threshold 128
```

No root and no pairing are needed. Turn off Bluetooth on phones that may hold the
printer's connection; the E16 accepts one connection at a time and powers off
after a few idle minutes.

## App

```sh
cargo run --release -p katasymbol-app
```

Click *Suchen* (desktop) or *Verbinden* (web) to find the printer; the app
connects on demand and reconnects after the printer has powered off. Labels are
text (any system font — bundled DejaVu/Liberation on the web — bold/italic,
alignment, auto-fit) or an image (open or drag a PNG/JPEG), with up to two
pictograms (~250 Font Awesome icons plus hand-drawn screws, threads, drives, nuts
and fixings) and one of 45 frame styles in six categories (classic, decorative,
work, birthday, Christmas, storage). Length: −/+, presets, the loaded label's
length, *Automatisch* (fits the content), or drag the handle on the label's end;
a blank *Schnittrand* (default 2 mm) keeps the print away from the cut edges.
Design, printer, density and copies are remembered. Narrow windows (phones) get
a single-column layout.

`cargo run -p katasymbol-core --example sheet -- DIR` writes contact sheets of all
frames and pictograms. `KATASYMBOL_SCREENSHOT=out.png` renders a demo design, saves
a screenshot and quits (use the X11 backend, e.g. `env -u WAYLAND_DISPLAY`, when the
window is hidden).

## Protocol summary (E16 / T16)

- BLE service `0000e0ff-3c17-d293-8e48-14fe2e4da212`; write `ffe9` with **Write Command**
  (no response) in 128-byte chunks, ~12 ms apart; responses are notifications on `ffe1`.
- Command frames `7E 5A 0C 00 10 01 AA <cmd> <chk> 00 01 <p1> <p2>`; see `protocol.rs`.
- Print flow: density `0xC9` → `CHECK_DEVICE` → wait idle → `START_PRINT 0` →
  `PAPER_BACK n` → per buffer: wait `BufSta == 0`, `NEXT_ZIPPEDBULK(512, frames)`,
  data frames (each acked with `0xBB`), `BUF_FULL 0` → wait until idle.
- Page: 96-dot head (12 bytes per column, 8 dots/mm); 4000-byte buffers of ≤ 332 columns,
  each LZMA-compressed alone (`0x5D`, dict 8192, exact size, no end marker).

Linux note: BlueZ's D-Bus `Connect()` links the dual-mode E16 over BR/EDR, so its
GATT services never show up; hence the raw L2CAP socket instead of `bluer`/`btleplug`.

## Sources and credits

- Pictograms: Font Awesome 4.7 by Dave Gandy (SIL OFL 1.1), bundled in
  `crates/katasymbol-core/assets/`.
- Web fonts: DejaVu Sans (Bitstream Vera license) and Liberation (SIL OFL 1.1), bundled
  in `crates/katasymbol-app/assets/fonts/` (see `LICENSES.txt`).

- Shared Supvan framing and command set: [heeen/supvan-cups](https://github.com/heeen/supvan-cups)
  (MIT, © 2026 Florian Hänel).
- E16 print flow: `T16Print` in the Katasymbol Android app (v1.4.31), analysed for
  interoperability.
- BLE transport details: Apple PacketLogger capture of the official iOS app printing on an E16.
