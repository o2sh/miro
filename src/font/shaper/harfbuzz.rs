use crate::font::ftwrap;
use crate::font::hbwrap as harfbuzz;
use crate::font::locator::FontDataHandle;
use crate::font::shaper::{FallbackIdx, FontMetrics, FontShaper, GlyphInfo};
use crate::window::PixelLength;
use anyhow::bail;
use std::cell::RefCell;

fn make_glyphinfo(
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
        x_advance: PixelLength::new(f64::from(pos.x_advance) / 64.0),
        y_advance: PixelLength::new(f64::from(pos.y_advance) / 64.0),
        x_offset: PixelLength::new(f64::from(pos.x_offset) / 64.0),
        y_offset: PixelLength::new(f64::from(pos.y_offset) / 64.0),
    }
}

struct FontPair {
    face: ftwrap::Face,
    font: harfbuzz::Font,
}

pub struct HarfbuzzShaper {
    fonts: Vec<RefCell<FontPair>>,
    _lib: ftwrap::Library,
}

impl HarfbuzzShaper {
    pub fn new(handles: &[FontDataHandle]) -> anyhow::Result<Self> {
        let lib = ftwrap::Library::new()?;
        let mut fonts = vec![];
        for handle in handles {
            let face = lib.face_from_locator(handle)?;
            let mut font = harfbuzz::Font::new(face.face);
            let render_mode = ftwrap::FT_Render_Mode::FT_RENDER_MODE_LIGHT;
            let load_flags = ftwrap::compute_load_flags_for_mode(render_mode);
            font.set_load_flags(load_flags);
            fonts.push(RefCell::new(FontPair { face, font }));
        }
        Ok(Self { fonts, _lib: lib })
    }

    fn do_shape(
        &self,
        font_idx: FallbackIdx,
        s: &str,
        font_size: f64,
        dpi: u32,
    ) -> anyhow::Result<Vec<GlyphInfo>> {
        let features = vec![
            harfbuzz::feature_from_string("kern")?,
            harfbuzz::feature_from_string("liga")?,
            harfbuzz::feature_from_string("clig")?,
        ];

        let mut buf = harfbuzz::Buffer::new()?;
        buf.set_script(harfbuzz::HB_SCRIPT_LATIN);
        buf.set_direction(harfbuzz::HB_DIRECTION_LTR);
        buf.set_language(harfbuzz::language_from_string("en")?);
        buf.add_str(s);

        {
            match self.fonts.get(font_idx) {
                Some(pair) => {
                    let mut pair = pair.borrow_mut();
                    pair.face.set_font_size(font_size, dpi)?;
                    pair.font.shape(&mut buf, Some(features.as_slice()));
                }
                None => {
                    let chars: Vec<u32> = s.chars().map(|c| c as u32).collect();
                    bail!("No more fallbacks while shaping {:x?}", chars);
                }
            }
        }

        let infos = buf.glyph_infos();
        let positions = buf.glyph_positions();

        let mut cluster = Vec::new();

        let mut last_text_pos = None;
        let mut first_fallback_pos = None;

        let mut sizes = Vec::with_capacity(s.len());
        for (i, info) in infos.iter().enumerate() {
            let pos = info.cluster as usize;
            let mut size = 1;
            if let Some(last_pos) = last_text_pos {
                let diff = pos - last_pos;
                if diff > 1 {
                    sizes[i - 1] = diff;
                }
            } else if pos != 0 {
                size = pos;
            }
            last_text_pos = Some(pos);
            sizes.push(size);
        }
        if let Some(last_pos) = last_text_pos {
            let diff = s.len() - last_pos;
            if diff > 1 {
                let last = sizes.len() - 1;
                sizes[last] = diff;
            }
        }

        for (i, info) in infos.iter().enumerate() {
            let pos = info.cluster as usize;
            if info.codepoint == 0 {
                if first_fallback_pos.is_none() {
                    first_fallback_pos = Some(pos);
                }
            } else if let Some(start_pos) = first_fallback_pos {
                let substr = &s[start_pos..pos];
                let mut shape = match self.do_shape(font_idx + 1, substr, font_size, dpi) {
                    Ok(shape) => Ok(shape),
                    Err(_) => {
                        if font_idx == 0 && s == "?" {
                            bail!("unable to find any usable glyphs for `?` in font_idx 0");
                        }
                        self.do_shape(0, "?", font_size, dpi)
                    }
                }?;

                for mut info in &mut shape {
                    info.cluster += start_pos as u32;
                }
                cluster.append(&mut shape);

                first_fallback_pos = None;
            }
            if info.codepoint != 0 {
                if s.is_char_boundary(pos) && s.is_char_boundary(pos + sizes[i]) {
                    let text = &s[pos..pos + sizes[i]];

                    cluster.push(make_glyphinfo(text, font_idx, info, &positions[i]));
                } else {
                    cluster.append(&mut self.do_shape(0, "?", font_size, dpi)?);
                }
            }
        }

        if let Some(start_pos) = first_fallback_pos {
            let substr = &s[start_pos..];
            if false {}
            let mut shape = match self.do_shape(font_idx + 1, substr, font_size, dpi) {
                Ok(shape) => Ok(shape),
                Err(_) => {
                    if font_idx == 0 && s == "?" {
                        bail!("unable to find any usable glyphs for `?` in font_idx 0");
                    }
                    self.do_shape(0, "?", font_size, dpi)
                }
            }?;

            for mut info in &mut shape {
                info.cluster += start_pos as u32;
            }
            cluster.append(&mut shape);
        }

        Ok(cluster)
    }
}

impl FontShaper for HarfbuzzShaper {
    fn shape(&self, text: &str, size: f64, dpi: u32) -> anyhow::Result<Vec<GlyphInfo>> {
        self.do_shape(0, text, size, dpi)
    }

    fn metrics(&self, size: f64, dpi: u32) -> anyhow::Result<FontMetrics> {
        let mut pair = self.fonts[0].borrow_mut();
        let (cell_width, cell_height) = pair.face.set_font_size(size, dpi)?;
        let y_scale = unsafe { (*(*pair.face.face).size).metrics.y_scale as f64 / 65536.0 };
        Ok(FontMetrics {
            cell_height: PixelLength::new(cell_height),
            cell_width: PixelLength::new(cell_width),

            descender: PixelLength::new(
                unsafe { (*(*pair.face.face).size).metrics.descender as f64 } / 64.0,
            ),
            underline_thickness: PixelLength::new(
                unsafe { (*pair.face.face).underline_thickness as f64 } * y_scale / 64.,
            ),
            underline_position: PixelLength::new(
                unsafe { (*pair.face.face).underline_position as f64 } * y_scale / 64.,
            ),
        })
    }
}
