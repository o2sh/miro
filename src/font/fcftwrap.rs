pub use self::fcwrap::Pattern as FontPattern;
use crate::config::{Config, TextStyle};
use crate::font::ftfont::FreeTypeFontImpl;
use crate::font::{fcwrap, ftwrap};
use crate::font::{shape_with_harfbuzz, FallbackIdx, Font, FontSystem, GlyphInfo, NamedFont};
use failure::{bail, ensure, err_msg, Error};
use log::{debug, warn};

pub type FontSystemImpl = FontConfigAndFreeType;

pub struct FontConfigAndFreeType {}

impl FontConfigAndFreeType {
    pub fn new() -> Self {
        Self {}
    }
}

impl FontSystem for FontConfigAndFreeType {
    fn load_font(
        &self,
        config: &Config,
        style: &TextStyle,
        font_scale: f64,
    ) -> Result<Box<dyn NamedFont>, Error> {
        let mut fonts = vec![];
        for attr in style.font_with_fallback() {
            let mut pattern = FontPattern::new()?;
            pattern.family(&attr.family)?;
            if *attr.bold.as_ref().unwrap_or(&false) {
                pattern.add_integer("weight", 200)?;
            }
            if *attr.italic.as_ref().unwrap_or(&false) {
                pattern.add_integer("slant", 100)?;
            }
            pattern.add_double("size", config.font_size * font_scale)?;
            pattern.add_double("dpi", config.dpi)?;
            fonts.push(NamedFontImpl::new(pattern)?);
        }

        if fonts.is_empty() {
            bail!("no fonts specified!?");
        }

        Ok(Box::new(NamedFontListImpl::new(fonts)))
    }
}

pub struct NamedFontListImpl {
    fallback: Vec<NamedFontImpl>,
    fonts: Vec<FreeTypeFontImpl>,
}

impl NamedFontListImpl {
    fn new(fallback: Vec<NamedFontImpl>) -> Self {
        Self { fallback, fonts: vec![] }
    }

    fn idx_to_fallback(&mut self, idx: usize) -> Option<(&mut NamedFontImpl, usize)> {
        if idx < self.fallback.len() {
            return Some((&mut self.fallback[idx], 0));
        }
        let mut candidate = idx - self.fallback.len();

        for f in &mut self.fallback {
            if candidate < f.font_list_size {
                return Some((f, candidate));
            }
            candidate = candidate - f.font_list_size;
        }
        None
    }

    fn load_next_fallback(&mut self) -> Result<(), Error> {
        let idx = self.fonts.len();
        let (f, idx) = self.idx_to_fallback(idx).ok_or_else(|| err_msg("no more fallbacks"))?;
        let pat = f.font_list.iter().nth(idx).ok_or_else(|| err_msg("no more fallbacks"))?;
        let pat = f.pattern.render_prepare(&pat)?;
        let file = pat.get_file()?;

        debug!("load_next_fallback: file={}", file);
        debug!("{}", pat.format("%{=unparse}")?);

        let size = pat.get_double("size")?;
        let dpi = pat.get_double("dpi")? as u32;
        let face = f.lib.new_face(file, 0)?;
        self.fonts.push(FreeTypeFontImpl::with_face_size_and_dpi(face, size, dpi)?);
        Ok(())
    }

    fn get_font(&mut self, idx: usize) -> Result<&mut FreeTypeFontImpl, Error> {
        if idx >= self.fonts.len() {
            self.load_next_fallback()?;
            ensure!(
                idx < self.fonts.len(),
                "should not ask for a font later than the next prepared font"
            );
        }

        Ok(&mut self.fonts[idx])
    }
}

impl NamedFont for NamedFontListImpl {
    fn get_fallback(&mut self, idx: FallbackIdx) -> Result<&dyn Font, Error> {
        Ok(self.get_font(idx)?)
    }
    fn shape(&mut self, s: &str) -> Result<Vec<GlyphInfo>, Error> {
        shape_with_harfbuzz(self, 0, s)
    }
}

impl Drop for NamedFontListImpl {
    fn drop(&mut self) {
        self.fonts.clear();
    }
}

pub struct NamedFontImpl {
    lib: ftwrap::Library,
    pattern: fcwrap::Pattern,
    font_list: fcwrap::FontSet,
    font_list_size: usize,
}

impl NamedFontImpl {
    fn new(mut pattern: FontPattern) -> Result<Self, Error> {
        let mut lib = ftwrap::Library::new()?;

        match lib.set_lcd_filter(ftwrap::FT_LcdFilter::FT_LCD_FILTER_DEFAULT) {
            Ok(_) => (),
            Err(err) => warn!("Ignoring: FT_LcdFilter failed: {:?}", err),
        };

        pattern.monospace()?;
        pattern.config_substitute(fcwrap::MatchKind::Pattern)?;
        pattern.default_substitute();

        let font_list = pattern.sort(true)?;
        let font_list_size = font_list.iter().count();

        Ok(Self { lib, font_list, font_list_size, pattern })
    }
}
