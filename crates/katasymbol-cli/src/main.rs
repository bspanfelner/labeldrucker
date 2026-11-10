//! `katasymbol` — probe and print on a Katasymbol E16 over BLE (Linux).

use clap::{Parser, Subcommand};
use katasymbol_core::page::{DOTS_PER_MM, HEAD_DOTS};
use katasymbol_core::printer::Progress;
use katasymbol_core::{Page, Printer};
use katasymbol_linux::AttLink;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Printer Bluetooth address, e.g. A4:93:40:3E:7B:0C
    #[arg(short, long, env = "KATASYMBOL_ADDR")]
    addr: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show name, status and loaded labels
    Probe,
    /// Print the calibration pattern (frame, X, 5 mm ticks)
    Test {
        /// Label length in mm
        #[arg(long, default_value_t = 30)]
        length: usize,
        /// Density 1-7
        #[arg(short, long, default_value_t = 4)]
        density: u8,
    },
    /// Print an image (PNG/JPEG); scaled to the 96-dot head height
    Image {
        file: PathBuf,
        /// Density 1-7
        #[arg(short, long, default_value_t = 4)]
        density: u8,
        /// Gray threshold 0-255; darker pixels print
        #[arg(short, long, default_value_t = 128)]
        threshold: u8,
        /// Label length in mm (image is centred); default: image width
        #[arg(long)]
        length: Option<usize>,
    },
}

fn load_image(file: &PathBuf, threshold: u8, length: Option<usize>) -> Result<Page, String> {
    let img = image::open(file).map_err(|e| format!("{}: {e}", file.display()))?.to_luma8();
    let scale = HEAD_DOTS as f32 / img.height() as f32;
    let w = ((img.width() as f32 * scale).round() as u32).max(1);
    let img = image::imageops::resize(&img, w, HEAD_DOTS as u32, image::imageops::FilterType::Triangle);
    let page_w = length.map_or(w as usize, |mm| mm * DOTS_PER_MM);
    // Centre the image on a white canvas of the page width.
    let mut canvas = vec![255u8; page_w * HEAD_DOTS];
    let off = page_w.saturating_sub(w as usize) / 2;
    for (x, y, p) in img.enumerate_pixels() {
        let cx = x as usize + off;
        if cx < page_w {
            canvas[y as usize * page_w + cx] = p.0[0];
        }
    }
    Ok(Page::from_gray(page_w, HEAD_DOTS, &canvas, threshold))
}

fn run(cli: Cli) -> Result<(), String> {
    let page = match &cli.cmd {
        Cmd::Probe => None,
        Cmd::Test { length, .. } => Some(Page::test_pattern(*length)),
        Cmd::Image { file, threshold, length, .. } => Some(load_image(file, *threshold, *length)?),
    };
    eprintln!("Connecting to {} ...", cli.addr);
    let link = AttLink::connect(&cli.addr).map_err(|e| e.to_string())?;
    let mut printer = Printer::new(link);

    pollster::block_on(async {
        match (&cli.cmd, page) {
            (Cmd::Probe, _) => {
                let info = printer.probe().await.map_err(|e| e.to_string())?;
                println!("Name:     {}", info.name.as_deref().unwrap_or("?"));
                let s = info.status;
                println!("Status:   {}", s.error().unwrap_or("ready"));
                println!("          busy={} printing={} usb={} regs={:02x?}", s.busy, s.printing, s.usb_power, s.regs);
                match info.material {
                    Some(m) => println!(
                        "Labels:   {} x {} mm, gap {} mm, {} left",
                        m.width_mm,
                        m.length_mm,
                        m.gap_mm,
                        m.remaining.map_or("?".into(), |r| r.to_string())
                    ),
                    None => println!("Labels:   unknown"),
                }
                Ok(())
            }
            (Cmd::Test { density, .. } | Cmd::Image { density, .. }, Some(page)) => {
                eprintln!("Printing {} mm page at density {density} ...", page.width() / DOTS_PER_MM);
                printer
                    .print(&page, *density, |p| match p {
                        Progress::Sending { buffer, of } => eprintln!("  sending buffer {buffer}/{of}"),
                        Progress::Finishing => eprintln!("  waiting for the printer to finish"),
                        Progress::Preparing => {}
                    })
                    .await
                    .map_err(|e| e.to_string())?;
                eprintln!("Done.");
                Ok(())
            }
            _ => unreachable!(),
        }
    })
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
