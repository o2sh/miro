use failure::{bail, Error};
use log::{debug, error};
mod ftfont;
mod hbwrap;
use self::hbwrap as harfbuzz;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

pub mod system;
pub use self::system::*;

pub mod ftwrap;

#[cfg(not(target_os = "macos"))]
pub mod fcftwrap;
#[cfg(not(target_os = "macos"))]
pub mod fcwrap;

#[cfg(target_os = "macos")]
pub mod coretext;

use crate::config::{Config, TextStyle};
use crate::term::CellAttributes;

type FontPtr = Rc<RefCell<Box<dyn NamedFont>>>;

pub struct FontConfiguration {
    config: Arc<Config>,
    fonts: RefCell<HashMap<TextStyle, FontPtr>>,
    system: Rc<dyn FontSystem>,
    metrics: RefCell<Option<FontMetrics>>,
    dpi_scale: RefCell<f64>,
    font_scale: RefCell<f64>,
}

impl FontConfiguration {
    pub fn new(config: Arc<Config>) -> Self {
        #[cfg(target_os = "macos")]
        let system = coretext::FontSystemImpl::new();
        #[cfg(not(target_os = "macos"))]
        let system = fcftwrap::FontSystemImpl::new();

        Self {
            config,
            fonts: RefCell::new(HashMap::new()),
            system: Rc::new(system),
            metrics: RefCell::new(None),
            font_scale: RefCell::new(1.0),
            dpi_scale: RefCell::new(1.0),
        }
    }

    pub fn cached_font(&self, style: &TextStyle) -> Result<Rc<RefCell<Box<dyn NamedFont>>>, Error> {
        let mut fonts = self.fonts.borrow_mut();

        if let Some(entry) = fonts.get(style) {
            return Ok(Rc::clone(entry));
        }

        let scale = *self.dpi_scale.borrow() * *self.font_scale.borrow();
        let font = Rc::new(RefCell::new(self.system.load_font(&self.config, style, scale)?));
        fonts.insert(style.clone(), Rc::clone(&font));
        Ok(font)
    }

    pub fn change_scaling(&self, font_scale: f64, dpi_scale: f64) {
        *self.dpi_scale.borrow_mut() = dpi_scale;
        *self.font_scale.borrow_mut() = font_scale;
        self.fonts.borrow_mut().clear();
        self.metrics.borrow_mut().take();
    }

    pub fn default_font(&self) -> Result<Rc<RefCell<Box<dyn NamedFont>>>, Error> {
        self.cached_font(&self.config.font)
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
        let metrics = font.borrow_mut().get_fallback(0)?.metrics();

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

#[allow(dead_code)]
pub fn shape_with_harfbuzz(
    font: &mut dyn NamedFont,
    font_idx: system::FallbackIdx,
    s: &str,
) -> Result<Vec<GlyphInfo>, Error> {
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
        let fallback = font.get_fallback(font_idx).map_err(|e| {
            let chars: Vec<u32> = s.chars().map(|c| c as u32).collect();
            e.context(format!("while shaping {:x?}", chars))
        })?;
        fallback.harfbuzz_shape(&mut buf, Some(features.as_slice()));
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
            let mut shape = match shape_with_harfbuzz(font, font_idx + 1, substr) {
                Ok(shape) => Ok(shape),
                Err(e) => {
                    error!("{:?} for {:?}", e, substr);
                    if font_idx == 0 && s == "?" {
                        bail!("unable to find any usable glyphs for `?` in font_idx 0");
                    }
                    shape_with_harfbuzz(font, 0, "?")
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

                cluster.push(GlyphInfo::new(text, font_idx, info, &positions[i]));
            } else {
                cluster.append(&mut shape_with_harfbuzz(font, 0, "?")?);
            }
        }
    }

    if let Some(start_pos) = first_fallback_pos {
        let substr = &s[start_pos..];
        if false {
            debug!("at end {:?}-{:?} needs fallback {}", start_pos, s.len() - 1, substr,);
        }
        let mut shape = match shape_with_harfbuzz(font, font_idx + 1, substr) {
            Ok(shape) => Ok(shape),
            Err(e) => {
                error!("{:?} for {:?}", e, substr);
                if font_idx == 0 && s == "?" {
                    bail!("unable to find any usable glyphs for `?` in font_idx 0");
                }
                shape_with_harfbuzz(font, 0, "?")
            }
        }?;

        for mut info in &mut shape {
            info.cluster += start_pos as u32;
        }
        cluster.append(&mut shape);
    }

    Ok(cluster)
}
