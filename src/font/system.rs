use super::super::config::{Config, TextStyle};
use super::hbwrap as harfbuzz;
use failure::Error;

pub struct RasterizedGlyph {
    pub data: Vec<u8>,
    pub height: usize,
    pub width: usize,
    pub bearing_x: f64,
    pub bearing_y: f64,
}

#[derive(Clone, Debug)]
pub struct GlyphInfo {
    #[cfg(debug_assertions)]
    pub text: String,

    pub cluster: u32,

    pub num_cells: u8,

    pub font_idx: usize,

    pub glyph_pos: u32,

    pub x_advance: f64,

    pub y_advance: f64,

    pub x_offset: f64,

    pub y_offset: f64,
}

impl GlyphInfo {
    #[allow(dead_code)]
    pub fn new(
        text: &str,
        font_idx: usize,
        info: &harfbuzz::hb_glyph_info_t,
        pos: &harfbuzz::hb_glyph_position_t,
    ) -> GlyphInfo {
        use crate::core::cell::unicode_column_width;
        let num_cells = unicode_column_width(text) as u8;
        GlyphInfo {
            #[cfg(debug_assertions)]
            text: text.into(),
            num_cells,
            font_idx,
            glyph_pos: info.codepoint,
            cluster: info.cluster,
            x_advance: f64::from(pos.x_advance) / 64.0,
            y_advance: f64::from(pos.y_advance) / 64.0,
            x_offset: f64::from(pos.x_offset) / 64.0,
            y_offset: f64::from(pos.y_offset) / 64.0,
        }
    }
}

pub type FallbackIdx = usize;

pub trait NamedFont {
    fn get_fallback(&mut self, idx: FallbackIdx) -> Result<&dyn Font, Error>;

    fn shape(&mut self, text: &str) -> Result<Vec<GlyphInfo>, Error>;
}

pub trait FontSystem {
    fn load_font(
        &self,
        config: &Config,
        style: &TextStyle,
        font_scale: f64,
    ) -> Result<Box<dyn NamedFont>, Error>;
}

#[derive(Copy, Clone, Debug, Default)]
pub struct FontMetrics {
    pub cell_width: f64,

    pub cell_height: f64,

    pub descender: f64,

    pub underline_thickness: f64,

    pub underline_position: f64,
}

pub trait Font {
    fn has_color(&self) -> bool;

    fn metrics(&self) -> FontMetrics;

    fn rasterize_glyph(&self, glyph_pos: u32) -> Result<RasterizedGlyph, Error>;

    fn harfbuzz_shape(
        &self,
        buf: &mut harfbuzz::Buffer,
        features: Option<&[harfbuzz::hb_feature_t]>,
    );
}
