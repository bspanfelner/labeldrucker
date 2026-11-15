# Katasymbol

Rust software for the Katasymbol (Supvan) **E16** label printer over Bluetooth Low Energy.
Targets: Linux, macOS, iOS and Web.

## Status

| Part | State |
|------|-------|
| `katasymbol-core` – protocol, page bitmap, LZMA, label layout (text, icons, frames), print flow | working, tested on an E16; builds for `wasm32-unknown-unknown` and `aarch64-apple-ios` |
| `katasymbol-linux` – BLE link (raw LE L2CAP ATT socket) | working |
| `katasymbol-cli` – `probe`, `test`, `image` | working |
| macOS/iOS link (CoreBluetooth, e.g. via `btleplug`) | todo |
| `katasymbol-web` – Web Bluetooth link + `web/` page (text, image, test pattern) | builds; hardware test pending |
| `katasymbol-app` – desktop app (egui): text, images, pictograms, 13 frame styles, length control, copies | working (printed on an E16) |

## Usage (Linux)

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

## Desktop app (Linux)

```sh
cargo run --release -p katasymbol-app
```

Click *Suchen* to find the printer (a short LE scan via `bluetoothctl`); the app
connects on demand and reconnects after the printer has powered off. Labels are
text (any system font, bold/italic, alignment, auto-fit) or an image (open or drag
a PNG/JPEG), with up to two pictograms and one of 13 frame styles (thickness,
margin, padding, radius). Length: −/+, presets, the loaded label's length,
*Automatisch* (fits the content), or drag the handle on the label's end. The
design, printer address, density and copies are remembered.

`KATASYMBOL_SCREENSHOT=out.png` renders a demo design, saves a screenshot and quits
(use the X11 backend, e.g. `env -u WAYLAND_DISPLAY`, when the window is hidden).

## Usage (Web)

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130   # must match Cargo.lock
./web/build.sh
python3 -m http.server -d web 8080                  # open http://localhost:8080 in Chrome/Edge
```

Web Bluetooth needs Chrome or Edge and a secure context (`localhost` or HTTPS). On Linux,
enable `chrome://flags/#enable-experimental-web-platform-features` if `navigator.bluetooth`
is missing; the Chromium snap also needs `sudo snap connect chromium:bluez`. Safari and
Firefox have no Web Bluetooth (iOS: Bluefy browser, or the native app).

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

- Shared Supvan framing and command set: [heeen/supvan-cups](https://github.com/heeen/supvan-cups)
  (MIT, © 2026 Florian Hänel).
- E16 print flow: `T16Print` in the Katasymbol Android app (v1.4.31), analysed for
  interoperability.
- BLE transport details: Apple PacketLogger capture of the official iOS app printing on an E16.
