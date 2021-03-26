use failure::{format_err, Error, Fallible};
mod hbwrap;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

pub mod ftwrap;
pub mod locator;
pub mod rasterizer;
pub mod shaper;

#[cfg(all(unix, not(target_os = "macos")))]
pub mod fcwrap;

use crate::font::locator::{FontLocator, FontLocatorSelection};
pub use crate::font::rasterizer::RasterizedGlyph;
use crate::font::rasterizer::{FontRasterizer, FontRasterizerSelection};
pub use crate::font::shaper::{FallbackIdx, FontMetrics, GlyphInfo};
use crate::font::shaper::{FontShaper, FontShaperSelection};

use super::config::{Config, TextStyle};
use crate::term::CellAttributes;

pub struct LoadedFont {
    rasterizers: Vec<Box<dyn FontRasterizer>>,
    shaper: Box<dyn FontShaper>,
    metrics: FontMetrics,
    font_size: f64,
    dpi: u32,
}

impl LoadedFont {
    pub fn metrics(&self) -> FontMetrics {
        self.metrics
    }

    pub fn shape(&self, text: &str) -> Fallible<Vec<GlyphInfo>> {
        self.shaper.shape(text, self.font_size, self.dpi)
    }

    pub fn rasterize_glyph(
        &self,
        glyph_pos: u32,
        fallback: FallbackIdx,
    ) -> Fallible<RasterizedGlyph> {
        let rasterizer = self
            .rasterizers
            .get(fallback)
            .ok_or_else(|| format_err!("no such fallback index: {}", fallback))?;
        rasterizer.rasterize_glyph(glyph_pos, self.font_size, self.dpi)
    }
}

pub struct FontConfiguration {
    fonts: RefCell<HashMap<TextStyle, Rc<LoadedFont>>>,
    metrics: RefCell<Option<FontMetrics>>,
    dpi_scale: RefCell<f64>,
    font_scale: RefCell<f64>,
    config: Arc<Config>,
    locator: Box<dyn FontLocator>,
}

impl FontConfiguration {
    pub fn new(config: Arc<Config>) -> Self {
        let locator = FontLocatorSelection::get_default().new_locator();
        Self {
            fonts: RefCell::new(HashMap::new()),
            locator,
            metrics: RefCell::new(None),
            font_scale: RefCell::new(1.0),
            dpi_scale: RefCell::new(1.0),
            config,
        }
    }

    pub fn resolve_font(&self, style: &TextStyle) -> Fallible<Rc<LoadedFont>> {
        let mut fonts = self.fonts.borrow_mut();

        if let Some(entry) = fonts.get(style) {
            return Ok(Rc::clone(entry));
        }

        let attributes = style.font_with_fallback();
        let handles = self.locator.load_fonts(&attributes)?;
        let mut rasterizers = vec![];
        for handle in &handles {
            rasterizers.push(FontRasterizerSelection::get_default().new_rasterizer(&handle)?);
        }
        let shaper = FontShaperSelection::get_default().new_shaper(&handles)?;

        let font_size = self.config.font_size * *self.font_scale.borrow();
        let dpi = *self.dpi_scale.borrow() as u32 * self.config.dpi as u32;
        let metrics = shaper.metrics(font_size, dpi)?;

        let loaded = Rc::new(LoadedFont { rasterizers, shaper, metrics, font_size, dpi });

        fonts.insert(style.clone(), Rc::clone(&loaded));

        Ok(loaded)
    }

    pub fn change_scaling(&self, font_scale: f64, dpi_scale: f64) {
        *self.dpi_scale.borrow_mut() = dpi_scale;
        *self.font_scale.borrow_mut() = font_scale;
        self.fonts.borrow_mut().clear();
        self.metrics.borrow_mut().take();
    }

    pub fn default_font(&self) -> Fallible<Rc<LoadedFont>> {
        self.resolve_font(&self.config.font)
    }

    pub fn get_font_scale(&self) -> f64 {
        *self.font_scale.borrow()
    }

    pub fn default_font_metrics(&self) -> Result<FontMetrics, Error> {
        {
            let metrics = self.metrics.borrow();
            if let Some(metrics) = metrics.as_ref() {
                return Ok(*metrics);
            }
        }

        let font = self.default_font()?;
        let metrics = font.metrics();

        *self.metrics.borrow_mut() = Some(metrics);

        Ok(metrics)
    }

    pub fn match_style(&self, attrs: &CellAttributes) -> &TextStyle {
        macro_rules! attr_match {
            ($ident:ident, $rule:expr) => {
                if let Some($ident) = $rule.$ident {
                    if $ident != attrs.$ident() {
                        continue;
                    }
                }
            };
        };

        for rule in &self.config.font_rules {
            attr_match!(intensity, &rule);
            attr_match!(underline, &rule);
            attr_match!(italic, &rule);
            attr_match!(blink, &rule);
            attr_match!(reverse, &rule);
            attr_match!(strikethrough, &rule);
            attr_match!(invisible, &rule);

            return &rule.font;
        }
        &self.config.font
    }
}
