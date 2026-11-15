//! Pictograms from the bundled Font Awesome 4.7 font (SIL OFL 1.1, see
//! `assets/FONT-AWESOME-LICENSE.txt`), grouped for a picker.

use ab_glyph::FontRef;

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

#[derive(Debug, Clone, Copy)]
pub struct Group {
    pub name: &'static str,
    pub icons: &'static [Icon],
}

macro_rules! icons {
    ($($c:literal $n:literal),* $(,)?) => { &[$(Icon { ch: $c, name: $n }),*] };
}

pub const GROUPS: &[Group] = &[
    Group {
        name: "Hinweise",
        icons: icons![
            '\u{f071}' "Warnung", '\u{f05a}' "Info", '\u{f06a}' "Achtung", '\u{f059}' "Frage",
            '\u{f00c}' "Haken", '\u{f00d}' "Kreuz", '\u{f05e}' "Verboten", '\u{f0e7}' "Strom",
            '\u{f06d}' "Feuer", '\u{f2dc}' "Kälte", '\u{f043}' "Flüssig", '\u{f023}' "Schloss",
            '\u{f09c}' "Offen", '\u{f084}' "Schlüssel", '\u{f0f3}' "Glocke", '\u{f06e}' "Auge",
        ],
    },
    Group {
        name: "Haushalt",
        icons: icons![
            '\u{f015}' "Haus", '\u{f236}' "Bett", '\u{f2cd}' "Bad", '\u{f2cc}' "Dusche",
            '\u{f0f4}' "Kaffee", '\u{f0f5}' "Besteck", '\u{f000}' "Glas", '\u{f0fc}' "Bier",
            '\u{f1fd}' "Kuchen", '\u{f07a}' "Einkauf", '\u{f06b}' "Geschenk", '\u{f1f8}' "Müll",
            '\u{f1b8}' "Recycling", '\u{f0eb}' "Licht", '\u{f2c9}' "Temperatur", '\u{f0c4}' "Schere",
        ],
    },
    Group {
        name: "Büro & Ordnung",
        icons: icons![
            '\u{f02b}' "Etikett", '\u{f02c}' "Etiketten", '\u{f07b}' "Ordner", '\u{f0f6}' "Dokument",
            '\u{f187}' "Archiv", '\u{f1b2}' "Kiste", '\u{f1b3}' "Kisten", '\u{f02d}' "Buch",
            '\u{f040}' "Stift", '\u{f02f}' "Drucker", '\u{f073}' "Kalender", '\u{f017}' "Uhr",
            '\u{f0b1}' "Koffer", '\u{f0d6}' "Geld", '\u{f153}' "Euro", '\u{f0e0}' "Post",
        ],
    },
    Group {
        name: "Technik",
        icons: icons![
            '\u{f1e6}' "Stecker", '\u{f011}' "Ein/Aus", '\u{f1eb}' "WLAN", '\u{f293}' "Bluetooth",
            '\u{f287}' "USB", '\u{f240}' "Batterie", '\u{f108}' "Monitor", '\u{f109}' "Laptop",
            '\u{f10b}' "Handy", '\u{f095}' "Telefon", '\u{f030}' "Kamera", '\u{f025}' "Kopfhörer",
            '\u{f001}' "Musik", '\u{f013}' "Zahnrad", '\u{f0ad}' "Werkzeug", '\u{f076}' "Magnet",
        ],
    },
    Group {
        name: "Natur & Freizeit",
        icons: icons![
            '\u{f185}' "Sonne", '\u{f186}' "Mond", '\u{f0c2}' "Wolke", '\u{f0e9}' "Schirm",
            '\u{f06c}' "Blatt", '\u{f1bb}' "Baum", '\u{f1b0}' "Pfote", '\u{f005}' "Stern",
            '\u{f004}' "Herz", '\u{f118}' "Smiley", '\u{f164}' "Daumen", '\u{f1b9}' "Auto",
            '\u{f206}' "Fahrrad", '\u{f072}' "Flugzeug", '\u{f13d}' "Anker", '\u{f135}' "Rakete",
        ],
    },
    Group {
        name: "Personen & Orte",
        icons: icons![
            '\u{f007}' "Person", '\u{f0c0}' "Gruppe", '\u{f1ae}' "Kind", '\u{f182}' "Frau",
            '\u{f183}' "Mann", '\u{f193}' "Rollstuhl", '\u{f0fa}' "Erste Hilfe", '\u{f21e}' "Puls",
            '\u{f19d}' "Schule", '\u{f041}' "Ort", '\u{f0ac}' "Welt", '\u{f024}' "Flagge",
            '\u{f060}' "Links", '\u{f061}' "Rechts", '\u{f062}' "Hoch", '\u{f063}' "Runter",
        ],
    },
];

/// Look up an icon's name.
pub fn name(ch: char) -> Option<&'static str> {
    GROUPS.iter().flat_map(|g| g.icons).find(|i| i.ch == ch).map(|i| i.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_glyph::Font;

    #[test]
    fn every_icon_exists_in_the_font() {
        let f = font();
        for i in GROUPS.iter().flat_map(|g| g.icons) {
            assert_ne!(f.glyph_id(i.ch).0, 0, "{} {:x}", i.name, i.ch as u32);
        }
    }
}
