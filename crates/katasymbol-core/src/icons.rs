//! Pictograms: the bundled Font Awesome 4.7 font (SIL OFL 1.1, see
//! `assets/FONT-AWESOME-LICENSE.txt`) plus hand-drawn ones from
//! [`crate::pictos`] (screws, nuts, fixings), grouped for a picker.

use crate::draw::{Canvas, Rect};
use crate::{pictos, text};
use ab_glyph::{Font, FontRef};

/// Font Awesome 4.7 (`fontawesome-webfont.ttf`).
pub const FONT_DATA: &[u8] = include_bytes!("../assets/fontawesome-webfont.ttf");

/// The bundled icon font.
pub fn font() -> FontRef<'static> {
    FontRef::try_from_slice(FONT_DATA).expect("bundled icon font is valid")
}

#[derive(Debug, Clone, Copy)]
pub struct Icon {
    pub ch: char,
    pub name: &'static str,
}

#[derive(Debug, Clone)]
pub struct Group {
    pub name: &'static str,
    pub icons: Vec<Icon>,
}

macro_rules! icons {
    ($($c:literal $n:literal),* $(,)?) => { &[$(Icon { ch: $c, name: $n }),*] };
}

/// Font Awesome groups.
pub const FA_GROUPS: &[(&str, &[Icon])] = &[
    ("Hinweise", icons![
        '\u{f071}' "Warnung", '\u{f05a}' "Info", '\u{f06a}' "Achtung", '\u{f059}' "Frage",
        '\u{f00c}' "Haken", '\u{f00d}' "Kreuz", '\u{f058}' "OK", '\u{f057}' "Nein",
        '\u{f05e}' "Verboten", '\u{f0e7}' "Strom", '\u{f06d}' "Feuer", '\u{f2dc}' "Kälte",
        '\u{f043}' "Flüssig", '\u{f023}' "Schloss", '\u{f09c}' "Offen", '\u{f084}' "Schlüssel",
        '\u{f0f3}' "Glocke", '\u{f06e}' "Auge", '\u{f070}' "Privat", '\u{f132}' "Schutz",
        '\u{f134}' "Feuerlöscher", '\u{f0a1}' "Megafon", '\u{f055}' "Plus", '\u{f056}' "Minus",
    ]),
    ("Haushalt", icons![
        '\u{f015}' "Haus", '\u{f236}' "Bett", '\u{f2cd}' "Bad", '\u{f2cc}' "Dusche",
        '\u{f0eb}' "Licht", '\u{f1e6}' "Stecker", '\u{f1f8}' "Müll", '\u{f1b8}' "Recycling",
        '\u{f2c9}' "Temperatur", '\u{f0c4}' "Schere", '\u{f1fc}' "Pinsel", '\u{f0e9}' "Schirm",
        '\u{f26c}' "Fernseher", '\u{f1e5}' "Fernglas", '\u{f2c2}' "Ausweis", '\u{f0f3}' "Klingel",
    ]),
    ("Küche & Vorrat", icons![
        '\u{f0f4}' "Kaffee", '\u{f0f5}' "Besteck", '\u{f1b1}' "Löffel", '\u{f000}' "Glas",
        '\u{f0fc}' "Bier", '\u{f1fd}' "Kuchen", '\u{f094}' "Zitrone", '\u{f06c}' "Kräuter",
        '\u{f07a}' "Einkauf", '\u{f291}' "Korb", '\u{f290}' "Tüte", '\u{f2cb}' "Kühl",
        '\u{f2dc}' "Gefroren", '\u{f06d}' "Scharf", '\u{f254}' "Sanduhr", '\u{f073}' "Datum",
    ]),
    ("Büro & Ordnung", icons![
        '\u{f02b}' "Etikett", '\u{f02c}' "Etiketten", '\u{f07b}' "Ordner", '\u{f07c}' "Ordner offen",
        '\u{f0f6}' "Dokument", '\u{f15b}' "Datei", '\u{f187}' "Archiv", '\u{f1b2}' "Kiste",
        '\u{f1b3}' "Kisten", '\u{f02d}' "Buch", '\u{f02e}' "Lesezeichen", '\u{f040}' "Stift",
        '\u{f0c6}' "Büroklammer", '\u{f08d}' "Pinnnadel", '\u{f02f}' "Drucker", '\u{f0ea}' "Klemmbrett",
        '\u{f073}' "Kalender", '\u{f017}' "Uhr", '\u{f0b1}' "Koffer", '\u{f0e0}' "Post",
        '\u{f01c}' "Eingang", '\u{f1ec}' "Rechner", '\u{f0d6}' "Geld", '\u{f153}' "Euro",
        '\u{f09d}' "Karte", '\u{f02a}' "Barcode", '\u{f029}' "QR-Code", '\u{f0e8}' "Struktur",
    ]),
    ("Werkstatt", icons![
        '\u{f0ad}' "Werkzeug", '\u{f013}' "Zahnrad", '\u{f085}' "Zahnräder", '\u{f0e3}' "Hammer",
        '\u{f076}' "Magnet", '\u{f0c3}' "Chemie", '\u{f1fb}' "Pipette", '\u{f0e4}' "Messgerät",
        '\u{f275}' "Industrie", '\u{f0d1}' "Lieferung", '\u{f1ad}' "Gebäude", '\u{f12e}' "Bauteil",
        '\u{f1de}' "Regler", '\u{f240}' "Akku voll", '\u{f242}' "Akku halb", '\u{f244}' "Akku leer",
    ]),
    ("Technik", icons![
        '\u{f011}' "Ein/Aus", '\u{f1eb}' "WLAN", '\u{f293}' "Bluetooth", '\u{f287}' "USB",
        '\u{f108}' "Monitor", '\u{f109}' "Laptop", '\u{f10a}' "Tablet", '\u{f10b}' "Handy",
        '\u{f095}' "Telefon", '\u{f030}' "Kamera", '\u{f03d}' "Video", '\u{f025}' "Kopfhörer",
        '\u{f130}' "Mikrofon", '\u{f001}' "Musik", '\u{f11b}' "Spiele", '\u{f11c}' "Tastatur",
        '\u{f0a0}' "Festplatte", '\u{f0c7}' "Speichern", '\u{f233}' "Server", '\u{f1c0}' "Datenbank",
        '\u{f2db}' "Chip", '\u{f012}' "Signal", '\u{f09e}' "Feed", '\u{f008}' "Film",
    ]),
    ("Natur & Wetter", icons![
        '\u{f185}' "Sonne", '\u{f186}' "Mond", '\u{f0c2}' "Wolke", '\u{f0e9}' "Regen",
        '\u{f06c}' "Blatt", '\u{f1bb}' "Baum", '\u{f1b0}' "Pfote", '\u{f188}' "Käfer",
        '\u{f0ac}' "Welt", '\u{f005}' "Stern", '\u{f006}' "Stern leer", '\u{f004}' "Herz",
        '\u{f2c7}' "Heiß", '\u{f2cb}' "Kalt", '\u{f2dc}' "Schnee", '\u{f0e7}' "Gewitter",
    ]),
    ("Feste & Freizeit", icons![
        '\u{f06b}' "Geschenk", '\u{f1fd}' "Torte", '\u{f000}' "Cocktail", '\u{f001}' "Musik",
        '\u{f005}' "Stern", '\u{f004}' "Herz", '\u{f118}' "Smiley", '\u{f0d0}' "Zauberstab",
        '\u{f091}' "Pokal", '\u{f1e3}' "Fußball", '\u{f206}' "Fahrrad", '\u{f219}' "Diamant",
        '\u{f19d}' "Abschluss", '\u{f1bb}' "Tanne", '\u{f2dc}' "Schneeflocke", '\u{f0f3}' "Glocke",
        '\u{f11e}' "Ziel", '\u{f12e}' "Puzzle", '\u{f135}' "Rakete", '\u{f13d}' "Anker",
    ]),
    ("Fahrzeuge & Orte", icons![
        '\u{f1b9}' "Auto", '\u{f0d1}' "LKW", '\u{f207}' "Bus", '\u{f1ba}' "Taxi",
        '\u{f21c}' "Motorrad", '\u{f238}' "Zug", '\u{f239}' "U-Bahn", '\u{f072}' "Flugzeug",
        '\u{f21a}' "Schiff", '\u{f0f9}' "Krankenwagen", '\u{f041}' "Ort", '\u{f279}' "Karte",
        '\u{f14e}' "Kompass", '\u{f024}' "Flagge", '\u{f19c}' "Behörde", '\u{f0f8}' "Krankenhaus",
        '\u{f018}' "Straße", '\u{f124}' "Navigation",
    ]),
    ("Personen & Gesundheit", icons![
        '\u{f007}' "Person", '\u{f0c0}' "Gruppe", '\u{f1ae}' "Kind", '\u{f182}' "Frau",
        '\u{f183}' "Mann", '\u{f0f0}' "Arzt", '\u{f193}' "Rollstuhl", '\u{f29d}' "Blind",
        '\u{f0fa}' "Erste Hilfe", '\u{f21e}' "Puls", '\u{f0f1}' "Stethoskop", '\u{f0fd}' "Klinik",
        '\u{f164}' "Daumen hoch", '\u{f165}' "Daumen runter", '\u{f119}' "Traurig", '\u{f256}' "Hand",
    ]),
    ("Pfeile", icons![
        '\u{f060}' "Links", '\u{f061}' "Rechts", '\u{f062}' "Hoch", '\u{f063}' "Runter",
        '\u{f177}' "Lang links", '\u{f178}' "Lang rechts", '\u{f176}' "Lang hoch", '\u{f175}' "Lang runter",
        '\u{f047}' "Alle Richtungen", '\u{f07e}' "Waagrecht", '\u{f07d}' "Senkrecht", '\u{f021}' "Kreislauf",
        '\u{f0e2}' "Zurück", '\u{f01e}' "Wiederholen", '\u{f0ec}' "Tauschen", '\u{f148}' "Abbiegen",
    ]),
];

/// All groups for a picker: Font Awesome first, then the hand-drawn ones.
pub fn groups() -> Vec<Group> {
    let fa = FA_GROUPS.iter().map(|(name, icons)| Group { name, icons: icons.to_vec() });
    let own = pictos::GROUPS.iter().map(|(name, ps)| Group {
        name,
        icons: ps.iter().map(|p| Icon { ch: p.ch, name: p.name }).collect(),
    });
    own.chain(fa).collect()
}

/// Look up an icon's name.
pub fn name(ch: char) -> Option<&'static str> {
    pictos::find(ch).map(|p| p.name).or_else(|| FA_GROUPS.iter().flat_map(|(_, g)| g.iter()).find(|i| i.ch == ch).map(|i| i.name))
}

/// Draw icon `ch` as large as fits into `r`.
pub fn draw(c: &mut Canvas, font: &impl Font, ch: char, r: Rect) {
    if !pictos::draw(c, ch, r) {
        text::draw_glyph(c, font, ch, r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_exists() {
        let f = font();
        for i in FA_GROUPS.iter().flat_map(|(_, g)| g.iter()) {
            assert_ne!(f.glyph_id(i.ch).0, 0, "{} {:x}", i.name, i.ch as u32);
        }
    }

    #[test]
    fn pictograms_draw_ink_inside_their_box() {
        for g in groups() {
            for i in &g.icons {
                let mut c = Canvas::new(80, 80);
                draw(&mut c, &font(), i.ch, Rect::new(8.0, 8.0, 72.0, 72.0));
                let ink: Vec<(usize, usize)> =
                    (0..80).flat_map(|y| (0..80).map(move |x| (x, y))).filter(|&(x, y)| c.get(x, y) >= 0.5).collect();
                assert!(ink.len() > 60, "{} draws only {} dots", i.name, ink.len());
                assert!(ink.iter().all(|&(x, y)| (6..74).contains(&x) && (6..74).contains(&y)), "{} leaves its box", i.name);
            }
        }
    }
}
