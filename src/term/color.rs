pub use crate::core::color::{AnsiColor, ColorAttribute, RgbColor, RgbaTuple};
use std::fmt;
use std::result::Result;

#[derive(Clone)]
pub struct Palette256(pub [RgbColor; 256]);

#[derive(Clone, Debug)]
pub struct ColorPalette {
    pub colors: Palette256,
    pub foreground: RgbColor,
    pub background: RgbColor,
    pub cursor_fg: RgbColor,
    pub cursor_bg: RgbColor,
    pub selection_fg: RgbColor,
    pub selection_bg: RgbColor,
}

impl fmt::Debug for Palette256 {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "[suppressed]")
    }
}

impl ColorPalette {
    pub fn resolve_fg(&self, color: ColorAttribute) -> RgbColor {
        match color {
            ColorAttribute::Default => self.foreground,
            ColorAttribute::PaletteIndex(idx) => self.colors.0[idx as usize],
            ColorAttribute::TrueColorWithPaletteFallback(color, _)
            | ColorAttribute::TrueColorWithDefaultFallback(color) => color,
        }
    }
    pub fn resolve_bg(&self, color: ColorAttribute) -> RgbColor {
        match color {
            ColorAttribute::Default => self.background,
            ColorAttribute::PaletteIndex(idx) => self.colors.0[idx as usize],
            ColorAttribute::TrueColorWithPaletteFallback(color, _)
            | ColorAttribute::TrueColorWithDefaultFallback(color) => color,
        }
    }
}

impl Default for ColorPalette {
    fn default() -> ColorPalette {
        let mut colors = [RgbColor::default(); 256];

        static ANSI: [RgbColor; 16] = [
            RgbColor { red: 0x00, green: 0x00, blue: 0x00 },
            RgbColor { red: 0xcc, green: 0x55, blue: 0x55 },
            RgbColor { red: 0x55, green: 0xcc, blue: 0x55 },
            RgbColor { red: 0xcd, green: 0xcd, blue: 0x55 },
            RgbColor { red: 0x54, green: 0x55, blue: 0xcb },
            RgbColor { red: 0xcc, green: 0x55, blue: 0xcc },
            RgbColor { red: 0x7a, green: 0xca, blue: 0xca },
            RgbColor { red: 0xcc, green: 0xcc, blue: 0xcc },
            RgbColor { red: 0x55, green: 0x55, blue: 0x55 },
            RgbColor { red: 0xff, green: 0x55, blue: 0x55 },
            RgbColor { red: 0x55, green: 0xff, blue: 0x55 },
            RgbColor { red: 0xff, green: 0xff, blue: 0x55 },
            RgbColor { red: 0x55, green: 0x55, blue: 0xff },
            RgbColor { red: 0xff, green: 0x55, blue: 0xff },
            RgbColor { red: 0x55, green: 0xff, blue: 0xff },
            RgbColor { red: 0xff, green: 0xff, blue: 0xff },
        ];

        colors[0..16].copy_from_slice(&ANSI);

        static RAMP6: [u8; 6] = [0x00, 0x33, 0x66, 0x99, 0xCC, 0xFF];
        for idx in 0..216 {
            let blue = RAMP6[idx % 6];
            let green = RAMP6[idx / 6 % 6];
            let red = RAMP6[idx / 6 / 6 % 6];

            colors[16 + idx] = RgbColor { red, green, blue };
        }

        static GREYS: [u8; 24] = [
            0x08, 0x12, 0x1c, 0x26, 0x30, 0x3a, 0x44, 0x4e, 0x58, 0x62, 0x6c, 0x76, 0x80, 0x8a,
            0x94, 0x9e, 0xa8, 0xb2, /* Grey70 */
            0xbc, 0xc6, 0xd0, 0xda, 0xe4, 0xee,
        ];

        for idx in 0..24 {
            let grey = GREYS[idx];
            colors[232 + idx] = RgbColor::new(grey, grey, grey);
        }

        let foreground = colors[249];
        let background = colors[AnsiColor::Black as usize];

        let cursor_bg = RgbColor::new(0x52, 0xad, 0x70);
        let cursor_fg = colors[AnsiColor::Black as usize];

        let selection_fg = colors[AnsiColor::Black as usize];
        let selection_bg = RgbColor::new(0xff, 0xfa, 0xcd);

        ColorPalette {
            colors: Palette256(colors),
            foreground,
            background,
            cursor_fg,
            cursor_bg,
            selection_fg,
            selection_bg,
        }
    }
}
