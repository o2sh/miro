use crate::font::locator::FontDataHandle;
use crate::window::PixelLength;
use failure::{format_err, Error, Fallible};
use serde_derive::*;
use std::sync::Mutex;

pub mod freetype;

pub struct RasterizedGlyph {
    pub data: Vec<u8>,
    pub height: usize,
    pub width: usize,
    pub bearing_x: PixelLength,
    pub bearing_y: PixelLength,
    pub has_color: bool,
}

pub trait FontRasterizer {
    fn rasterize_glyph(&self, glyph_pos: u32, size: f64, dpi: u32) -> Fallible<RasterizedGlyph>;
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub enum FontRasterizerSelection {
    FreeType,
}

lazy_static::lazy_static! {
    static ref DEFAULT_RASTER: Mutex<FontRasterizerSelection> = Mutex::new(Default::default());
}

impl Default for FontRasterizerSelection {
    fn default() -> Self {
        FontRasterizerSelection::FreeType
    }
}

impl FontRasterizerSelection {
    pub fn get_default() -> Self {
        let def = DEFAULT_RASTER.lock().unwrap();
        *def
    }

    pub fn variants() -> Vec<&'static str> {
        vec!["FreeType"]
    }

    pub fn new_rasterizer(self, handle: &FontDataHandle) -> Fallible<Box<dyn FontRasterizer>> {
        match self {
            Self::FreeType => Ok(Box::new(freetype::FreeTypeRasterizer::from_locator(handle)?)),
        }
    }
}

impl std::str::FromStr for FontRasterizerSelection {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_ref() {
            "freetype" => Ok(Self::FreeType),
            _ => Err(format_err!(
                "{} is not a valid FontRasterizerSelection variant, possible values are {:?}",
                s,
                Self::variants()
            )),
        }
    }
}
