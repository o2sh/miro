use crate::font::locator::FontDataHandle;
use crate::window::PixelLength;
use failure::{format_err, Error, Fallible};
use serde_derive::*;
use std::sync::Mutex;

pub mod harfbuzz;

#[derive(Clone, Debug)]
pub struct GlyphInfo {
    #[cfg(debug_assertions)]
    pub text: String,
    pub cluster: u32,
    pub num_cells: u8,
    pub font_idx: FallbackIdx,
    pub glyph_pos: u32,
    pub x_advance: PixelLength,
    pub y_advance: PixelLength,
    pub x_offset: PixelLength,
    pub y_offset: PixelLength,
}

pub type FallbackIdx = usize;

#[derive(Copy, Clone, Debug)]
pub struct FontMetrics {
    pub cell_width: PixelLength,
    pub cell_height: PixelLength,
    pub descender: PixelLength,
    pub underline_thickness: PixelLength,
    pub underline_position: PixelLength,
}

pub trait FontShaper {
    fn shape(&self, text: &str, size: f64, dpi: u32) -> Fallible<Vec<GlyphInfo>>;
    fn metrics(&self, size: f64, dpi: u32) -> Fallible<FontMetrics>;
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub enum FontShaperSelection {
    Harfbuzz,
}

lazy_static::lazy_static! {
    static ref DEFAULT_SHAPER: Mutex<FontShaperSelection> = Mutex::new(Default::default());
}

impl Default for FontShaperSelection {
    fn default() -> Self {
        FontShaperSelection::Harfbuzz
    }
}

impl FontShaperSelection {
    pub fn get_default() -> Self {
        let def = DEFAULT_SHAPER.lock().unwrap();
        *def
    }

    pub fn variants() -> Vec<&'static str> {
        vec!["Harfbuzz"]
    }

    pub fn new_shaper(self, handles: &[FontDataHandle]) -> Fallible<Box<dyn FontShaper>> {
        match self {
            Self::Harfbuzz => Ok(Box::new(harfbuzz::HarfbuzzShaper::new(handles)?)),
        }
    }
}

impl std::str::FromStr for FontShaperSelection {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_ref() {
            "harfbuzz" => Ok(Self::Harfbuzz),
            _ => Err(format_err!(
                "{} is not a valid FontShaperSelection variant, possible values are {:?}",
                s,
                Self::variants()
            )),
        }
    }
}
