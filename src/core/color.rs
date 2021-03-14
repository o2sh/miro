#![cfg_attr(feature = "cargo-clippy", allow(clippy::useless_attribute))]

use num_derive::*;
use palette;
use palette::{Srgb, Srgba};
use serde::{self, Deserialize, Deserializer, Serialize, Serializer};
use serde_derive::*;
use std::result::Result;

#[derive(Debug, Clone, Copy, FromPrimitive)]
#[repr(u8)]

pub enum AnsiColor {
    Black = 0,
    Maroon,
    Green,
    Olive,
    Navy,
    Purple,
    Teal,
    Silver,
    Grey,
    Red,
    Lime,
    Yellow,
    Blue,
    Fuschia,
    Aqua,
    White,
}

impl From<AnsiColor> for u8 {
    fn from(col: AnsiColor) -> u8 {
        col as u8
    }
}

pub type RgbaTuple = (f32, f32, f32, f32);

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash)]
pub struct RgbColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl RgbColor {
    pub fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    pub fn to_tuple_rgba(self) -> RgbaTuple {
        Srgba::<u8>::new(self.red, self.green, self.blue, 0xff).into_format().into_components()
    }

    pub fn from_named(name: &str) -> Option<RgbColor> {
        palette::named::from_str(&name.to_ascii_lowercase()).map(|color| {
            let color = Srgb::<u8>::from_format(color);
            Self::new(color.red, color.green, color.blue)
        })
    }

    pub fn to_rgb_string(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }

    pub fn from_rgb_str(s: &str) -> Option<RgbColor> {
        if s.as_bytes()[0] == b'#' && s.len() == 7 {
            let mut chars = s.chars().skip(1);

            macro_rules! digit {
                () => {{
                    let hi = match chars.next().unwrap().to_digit(16) {
                        Some(v) => (v as u8) << 4,
                        None => return None,
                    };
                    let lo = match chars.next().unwrap().to_digit(16) {
                        Some(v) => v as u8,
                        None => return None,
                    };
                    hi | lo
                }};
            }
            Some(Self::new(digit!(), digit!(), digit!()))
        } else {
            None
        }
    }

    pub fn from_named_or_rgb_string(s: &str) -> Option<Self> {
        RgbColor::from_rgb_str(&s).or_else(|| RgbColor::from_named(&s))
    }
}

impl Serialize for RgbColor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = self.to_rgb_string();
        s.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RgbColor {
    fn deserialize<D>(deserializer: D) -> Result<RgbColor, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        RgbColor::from_named_or_rgb_string(&s)
            .ok_or_else(|| format!("unknown color name: {}", s))
            .map_err(serde::de::Error::custom)
    }
}

pub type PaletteIndex = u8;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ColorSpec {
    Default,
    PaletteIndex(PaletteIndex),
    TrueColor(RgbColor),
}

impl Default for ColorSpec {
    fn default() -> Self {
        ColorSpec::Default
    }
}

impl From<AnsiColor> for ColorSpec {
    fn from(col: AnsiColor) -> Self {
        ColorSpec::PaletteIndex(col as u8)
    }
}

impl From<RgbColor> for ColorSpec {
    fn from(col: RgbColor) -> Self {
        ColorSpec::TrueColor(col)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum ColorAttribute {
    TrueColorWithPaletteFallback(RgbColor, PaletteIndex),
    TrueColorWithDefaultFallback(RgbColor),
    PaletteIndex(PaletteIndex),
    Default,
}

impl Default for ColorAttribute {
    fn default() -> Self {
        ColorAttribute::Default
    }
}

impl From<AnsiColor> for ColorAttribute {
    fn from(col: AnsiColor) -> Self {
        ColorAttribute::PaletteIndex(col as u8)
    }
}

impl From<ColorSpec> for ColorAttribute {
    fn from(spec: ColorSpec) -> Self {
        match spec {
            ColorSpec::Default => ColorAttribute::Default,
            ColorSpec::PaletteIndex(idx) => ColorAttribute::PaletteIndex(idx),
            ColorSpec::TrueColor(color) => ColorAttribute::TrueColorWithDefaultFallback(color),
        }
    }
}
