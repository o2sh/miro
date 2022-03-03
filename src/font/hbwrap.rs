use freetype;

use anyhow::ensure;
pub use harfbuzz::*;
use harfbuzz_sys as harfbuzz;

use std::mem;
use std::ptr;
use std::slice;

extern "C" {
    fn hb_ft_font_set_load_flags(font: *mut hb_font_t, load_flags: i32);
}

extern "C" {
    pub fn hb_ft_font_create_referenced(face: freetype::freetype::FT_Face) -> *mut hb_font_t;
}

pub fn language_from_string(s: &str) -> anyhow::Result<hb_language_t> {
    unsafe {
        let lang = hb_language_from_string(s.as_ptr() as *const i8, s.len() as i32);
        ensure!(!lang.is_null(), "failed to convert {} to language", s);
        Ok(lang)
    }
}

pub fn feature_from_string(s: &str) -> anyhow::Result<hb_feature_t> {
    unsafe {
        let mut feature = mem::zeroed();
        ensure!(
            hb_feature_from_string(s.as_ptr() as *const i8, s.len() as i32, &mut feature as *mut _,)
                != 0,
            "failed to create feature from {}",
            s
        );
        Ok(feature)
    }
}

pub struct Font {
    font: *mut hb_font_t,
}

impl Drop for Font {
    fn drop(&mut self) {
        unsafe {
            hb_font_destroy(self.font);
        }
    }
}

impl Font {
    pub fn new(face: freetype::freetype::FT_Face) -> Font {
        Font { font: unsafe { hb_ft_font_create_referenced(face as _) } }
    }

    pub fn set_load_flags(&mut self, load_flags: freetype::freetype::FT_Int32) {
        unsafe {
            hb_ft_font_set_load_flags(self.font, load_flags);
        }
    }

    pub fn shape(&mut self, buf: &mut Buffer, features: Option<&[hb_feature_t]>) {
        unsafe {
            if let Some(features) = features {
                hb_shape(self.font, buf.buf, features.as_ptr(), features.len() as u32)
            } else {
                hb_shape(self.font, buf.buf, ptr::null(), 0)
            }
        }
    }
}

pub struct Buffer {
    buf: *mut hb_buffer_t,
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            hb_buffer_destroy(self.buf);
        }
    }
}

impl Buffer {
    pub fn new() -> anyhow::Result<Buffer> {
        let buf = unsafe { hb_buffer_create() };
        ensure!(unsafe { hb_buffer_allocation_successful(buf) } != 0, "hb_buffer_create failed");
        Ok(Buffer { buf })
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        unsafe {
            hb_buffer_reset(self.buf);
        }
    }

    pub fn set_direction(&mut self, direction: hb_direction_t) {
        unsafe {
            hb_buffer_set_direction(self.buf, direction);
        }
    }

    pub fn set_script(&mut self, script: hb_script_t) {
        unsafe {
            hb_buffer_set_script(self.buf, script);
        }
    }

    pub fn set_language(&mut self, lang: hb_language_t) {
        unsafe {
            hb_buffer_set_language(self.buf, lang);
        }
    }

    #[allow(dead_code)]
    pub fn add(&mut self, codepoint: hb_codepoint_t, cluster: u32) {
        unsafe {
            hb_buffer_add(self.buf, codepoint, cluster);
        }
    }

    pub fn add_utf8(&mut self, buf: &[u8]) {
        unsafe {
            hb_buffer_add_utf8(
                self.buf,
                buf.as_ptr() as *const i8,
                buf.len() as i32,
                0,
                buf.len() as i32,
            );
        }
    }

    pub fn add_str(&mut self, s: &str) {
        self.add_utf8(s.as_bytes())
    }

    pub fn glyph_infos(&self) -> &[hb_glyph_info_t] {
        unsafe {
            let mut len: u32 = 0;
            let info = hb_buffer_get_glyph_infos(self.buf, &mut len as *mut _);
            slice::from_raw_parts(info, len as usize)
        }
    }

    pub fn glyph_positions(&self) -> &[hb_glyph_position_t] {
        unsafe {
            let mut len: u32 = 0;
            let pos = hb_buffer_get_glyph_positions(self.buf, &mut len as *mut _);
            slice::from_raw_parts(pos, len as usize)
        }
    }
}
