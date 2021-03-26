use crate::config::TextStyle;
use crate::font::{FontConfiguration, GlyphInfo};
use crate::window::bitmaps::atlas::{Atlas, Sprite};
use crate::window::bitmaps::{Image, Texture2d};
use crate::window::PixelLength;
use euclid::num::Zero;
use failure::Fallible;
use glium::backend::Context as GliumContext;
use glium::texture::SrgbTexture2d;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub font_idx: usize,
    pub glyph_pos: u32,
    pub style: TextStyle,
}

pub struct CachedGlyph<T: Texture2d> {
    pub has_color: bool,
    pub x_offset: PixelLength,
    pub y_offset: PixelLength,
    pub bearing_x: PixelLength,
    pub bearing_y: PixelLength,
    pub texture: Option<Sprite<T>>,
    pub scale: f64,
}

pub struct GlyphCache<T: Texture2d> {
    glyph_cache: HashMap<GlyphKey, Rc<CachedGlyph<T>>>,
    pub atlas: Atlas<T>,
    fonts: Rc<FontConfiguration>,
}

impl GlyphCache<SrgbTexture2d> {
    pub fn new_gl(
        backend: &Rc<GliumContext>,
        fonts: &Rc<FontConfiguration>,
        size: usize,
    ) -> Fallible<Self> {
        let surface = Rc::new(SrgbTexture2d::empty_with_format(
            backend,
            glium::texture::SrgbFormat::U8U8U8U8,
            glium::texture::MipmapsOption::NoMipmap,
            size as u32,
            size as u32,
        )?);
        let atlas = Atlas::new(&surface).expect("failed to create new texture atlas");

        Ok(Self { fonts: Rc::clone(fonts), glyph_cache: HashMap::new(), atlas })
    }
}

impl<T: Texture2d> GlyphCache<T> {
    pub fn cached_glyph(
        &mut self,
        info: &GlyphInfo,
        style: &TextStyle,
    ) -> Fallible<Rc<CachedGlyph<T>>> {
        let key =
            GlyphKey { font_idx: info.font_idx, glyph_pos: info.glyph_pos, style: style.clone() };

        if let Some(entry) = self.glyph_cache.get(&key) {
            return Ok(Rc::clone(entry));
        }

        let glyph = self.load_glyph(info, style)?;
        self.glyph_cache.insert(key, Rc::clone(&glyph));
        Ok(glyph)
    }

    #[allow(clippy::float_cmp)]
    fn load_glyph(&mut self, info: &GlyphInfo, style: &TextStyle) -> Fallible<Rc<CachedGlyph<T>>> {
        let metrics;
        let glyph;

        {
            let font = self.fonts.resolve_font(style)?;
            metrics = font.metrics();
            glyph = font.rasterize_glyph(info.glyph_pos, info.font_idx)?;
        }
        let (cell_width, cell_height) = (metrics.cell_width, metrics.cell_height);

        let scale = if (info.x_advance / f64::from(info.num_cells)).get().floor() > cell_width.get()
        {
            f64::from(info.num_cells) * (cell_width / info.x_advance).get()
        } else if PixelLength::new(glyph.height as f64) > cell_height {
            cell_height.get() / glyph.height as f64
        } else {
            1.0f64
        };
        let glyph = if glyph.width == 0 || glyph.height == 0 {
            CachedGlyph {
                has_color: glyph.has_color,
                texture: None,
                x_offset: info.x_offset * scale,
                y_offset: info.y_offset * scale,
                bearing_x: PixelLength::zero(),
                bearing_y: PixelLength::zero(),
                scale,
            }
        } else {
            let raw_im = Image::with_rgba32(
                glyph.width as usize,
                glyph.height as usize,
                4 * glyph.width as usize,
                &glyph.data,
            );

            let bearing_x = glyph.bearing_x * scale;
            let bearing_y = glyph.bearing_y * scale;
            let x_offset = info.x_offset * scale;
            let y_offset = info.y_offset * scale;

            let (scale, raw_im) =
                if scale != 1.0 { (1.0, raw_im.scale_by(scale)) } else { (scale, raw_im) };

            let tex = self.atlas.allocate(&raw_im)?;

            CachedGlyph {
                has_color: glyph.has_color,
                texture: Some(tex),
                x_offset,
                y_offset,
                bearing_x,
                bearing_y,
                scale,
            }
        };

        Ok(Rc::new(glyph))
    }
}
