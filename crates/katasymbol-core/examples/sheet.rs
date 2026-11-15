//! Render every frame style and every pictogram into PNG contact sheets:
//! `cargo run -p katasymbol-core --example sheet -- OUT_DIR [font.ttf]`
//! writes `frames.png` and `icons.png`.
use katasymbol_core::draw::{Canvas, Rect};
use katasymbol_core::icons;
use katasymbol_core::label::{Category, Frame, FrameStyle, Label};
use katasymbol_core::page::HEAD_DOTS;
use katasymbol_core::text;

fn save(pages: &[Canvas], cols: usize, path: &str) {
    let (w, gap) = (pages.iter().map(|p| p.width).max().unwrap(), 12usize);
    let rows = pages.len().div_ceil(cols);
    let h = pages.iter().map(|p| p.height).max().unwrap();
    let (iw, ih) = (cols * (w + gap) + gap, rows * (h + gap) + gap);
    let mut img = image::GrayImage::from_pixel(iw as u32, ih as u32, image::Luma([170]));
    for (n, p) in pages.iter().enumerate() {
        let (ox, oy) = (gap + (n % cols) * (w + gap), gap + (n / cols) * (h + gap));
        for x in 0..p.width {
            for y in 0..p.height {
                img.put_pixel((ox + x) as u32, (oy + y) as u32, image::Luma([if p.get(x, y) >= 0.5 { 0 } else { 255 }]));
            }
        }
    }
    img.save(path).unwrap();
    println!("{path}");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".".into());
    let font_path = args.next().unwrap_or_else(|| "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf".into());
    let font = text::load_font(std::fs::read(font_path).unwrap(), 0).unwrap();
    let icon_font = icons::font();

    let mut frames = Vec::new();
    for cat in Category::ALL {
        for style in FrameStyle::in_category(cat) {
            let label = Label {
                length: 400,
                text: style.name().into(),
                frame: Frame { style, ..Frame::default() },
                ..Label::default()
            };
            frames.push(label.canvas(&font, &icon_font, None));
        }
    }
    save(&frames, 3, &format!("{dir}/frames.png"));

    // Icons: 12 per strip of 96 dots.
    let mut strips = Vec::new();
    for g in icons::groups() {
        for chunk in g.icons.chunks(10) {
            let mut c = Canvas::for_page(10 * 60);
            for (k, ic) in chunk.iter().enumerate() {
                let x = k as f32 * 60.0;
                icons::draw(&mut c, &icon_font, ic.ch, Rect::new(x + 4.0, 8.0, x + 56.0, 60.0));
                text::draw(&mut c, &font, ic.name, Rect::new(x, 66.0, x + 58.0, 92.0), Some(9.0), text::Align::Center);
            }
            strips.push(c);
        }
    }
    let _ = HEAD_DOTS;

    // Hand-drawn pictograms, large: 6 per strip, 96 dots each.
    let mut big = Vec::new();
    for (_, ps) in katasymbol_core::pictos::GROUPS {
        for chunk in ps.chunks(6) {
            let mut c = Canvas::for_page(6 * 100);
            for (k, p) in chunk.iter().enumerate() {
                let x = k as f32 * 100.0;
                icons::draw(&mut c, &icon_font, p.ch, Rect::new(x + 2.0, 0.0, x + 98.0, 96.0));
            }
            big.push(c);
        }
    }
    save(&big, 1, &format!("{dir}/pictos.png"));
    save(&strips, 2, &format!("{dir}/icons.png"));
}
