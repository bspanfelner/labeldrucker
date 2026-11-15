//! Render every frame style and the icon set into a PNG for a visual check:
//! `cargo run -p katasymbol-core --example sheet -- out.png [font.ttf]`
use katasymbol_core::icons;
use katasymbol_core::label::{Frame, FrameStyle, Label};
use katasymbol_core::page::HEAD_DOTS;
use katasymbol_core::text::{self, Align};

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "sheet.png".into());
    let font_path = args.next().unwrap_or_else(|| "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf".into());
    let font = text::load_font(std::fs::read(font_path).unwrap(), 0).unwrap();
    let icon_font = icons::font();
    let (w, gap) = (360usize, 12usize);
    let mut pages = Vec::new();
    for (i, style) in FrameStyle::ALL.iter().enumerate() {
        let label = Label {
            length: w,
            text: style.name().into(),
            frame: Frame { style: *style, ..Frame::default() },
            icon_left: (i % 3 == 1).then_some('\u{f015}'),
            icon_right: (i % 3 == 2).then_some('\u{f0e7}'),
            separator: i % 2 == 0,
            align: Align::Center,
            ..Label::default()
        };
        pages.push(label.render(&font, &icon_font, None));
    }
    // Icon rows: 16 per row in a 360-dot strip.
    for g in icons::GROUPS {
        let mut c = katasymbol_core::draw::Canvas::for_page(w);
        for (k, ic) in g.icons.iter().enumerate() {
            let (col, row) = (k % 8, k / 8);
            let x = col as f32 * 45.0;
            let y = row as f32 * 48.0;
            text::draw_glyph(&mut c, &icon_font, ic.ch, katasymbol_core::draw::Rect::new(x + 4.0, y + 4.0, x + 40.0, y + 44.0));
        }
        pages.push(c.to_page());
    }
    let cols = 2;
    let rows = pages.len().div_ceil(cols);
    let (iw, ih) = (cols * (w + gap) + gap, rows * (HEAD_DOTS + gap) + gap);
    let mut img = image::GrayImage::from_pixel(iw as u32, ih as u32, image::Luma([170]));
    for (n, p) in pages.iter().enumerate() {
        let (ox, oy) = (gap + (n % cols) * (w + gap), gap + (n / cols) * (HEAD_DOTS + gap));
        for x in 0..p.width() {
            for y in 0..HEAD_DOTS {
                img.put_pixel((ox + x) as u32, (oy + y) as u32, image::Luma([if p.get(x, y) { 0 } else { 255 }]));
            }
        }
    }
    img.save(&out).unwrap();
    println!("{out}");
}
