use super::quad::Quad;
use super::renderer::OpenGLRenderer;
use super::utilsprites::RenderMetrics;
use crate::config::TextStyle;
use crate::core::color::RgbColor;
use crate::font::FontConfiguration;
use crate::term::color::{ColorAttribute, ColorPalette};
use crate::window::bitmaps::atlas::SpriteSlice;
use crate::window::bitmaps::Texture2d;
use crate::window::color::Color;
use crate::window::Dimensions;
use chrono::{DateTime, Utc};
use failure::Fallible;
use glium::{uniform, Surface};
use sysinfo::{ProcessorExt, System, SystemExt};

pub struct Header {
    pub offset: usize,
    sys: System,
    count: u32,
}

impl Header {
    pub fn new() -> Self {
        let sys = System::new();

        Self { offset: 2, count: 0, sys }
    }

    pub fn paint(
        &mut self,
        gl_state: &OpenGLRenderer,
        palette: &ColorPalette,
        dimensions: &Dimensions,
        frame_count: u32,
        render_metrics: &RenderMetrics,
        fonts: &FontConfiguration,
        frame: &mut glium::Frame,
    ) -> Fallible<()> {
        let w = dimensions.pixel_width as f32 as f32 / 2.0;
        if frame_count % 6 == 0 {
            self.count += 1;
            gl_state.slide_sprite(w);
        }

        if frame_count % 30 == 0 {
            self.sys.refresh_system();
        }

        let projection = euclid::Transform3D::<f32, f32, f32>::ortho(
            -(dimensions.pixel_width as f32) / 2.0,
            dimensions.pixel_width as f32 / 2.0,
            dimensions.pixel_height as f32 / 2.0,
            -(dimensions.pixel_height as f32) / 2.0,
            -1.0,
            1.0,
        )
        .to_arrays();

        let draw_params =
            glium::DrawParameters { blend: glium::Blend::alpha_blending(), ..Default::default() };

        frame.draw(
            &*gl_state.header_rect_vertex_buffer.borrow(),
            &gl_state.header_rect_index_buffer,
            &gl_state.header_program,
            &uniform! {
                projection: projection,
            },
            &draw_params,
        )?;

        self.render_header_line_opengl(gl_state, render_metrics, fonts, palette)?;

        let tex = gl_state.glyph_cache.borrow().atlas.texture();

        frame.draw(
            &*gl_state.header_glyph_vertex_buffer.borrow(),
            &gl_state.header_glyph_index_buffer,
            &gl_state.glyph_program,
            &uniform! {
                projection: projection,
                glyph_tex: &*tex,
                bg_and_line_layer: false,
            },
            &draw_params,
        )?;

        let number_of_sprites = gl_state.spritesheet.sprites.len();
        let sprite =
            &gl_state.spritesheet.sprites[(self.count % number_of_sprites as u32) as usize];
        frame.draw(
            &*gl_state.sprite_vertex_buffer.borrow(),
            &gl_state.sprite_index_buffer,
            &gl_state.sprite_program,
            &uniform! {
                projection: projection,
                tex: &gl_state.player_texture.tex,
                source_dimensions: sprite.size,
                source_position: sprite.position,
                source_texture_dimensions: [gl_state.player_texture.width, gl_state.player_texture.height]
            },
            &draw_params,
        )?;

        Ok(())
    }

    fn render_header_line_opengl(
        &self,
        gl_state: &OpenGLRenderer,
        render_metrics: &RenderMetrics,
        fonts: &FontConfiguration,
        palette: &ColorPalette,
    ) -> Fallible<()> {
        let now: DateTime<Utc> = Utc::now();
        let current_time = now.format("%H:%M:%S").to_string();
        let cpu_load = format!("{}", self.sys.get_global_processor_info().get_cpu_usage().round());
        let mut vb = gl_state.header_glyph_vertex_buffer.borrow_mut();
        let mut vertices = vb
            .slice_mut(..)
            .ok_or_else(|| format_err!("we're confused about the screen size"))?
            .map();

        let style = TextStyle::default();

        let indent = 3 - cpu_load.len();

        let glyph_info = {
            let font = fonts.cached_font(&style)?;
            let mut font = font.borrow_mut();
            font.shape(&format!(
                "CPU:{}%{:indent$}{}",
                cpu_load,
                "",
                current_time,
                indent = indent
            ))?
        };

        let glyph_color = palette.resolve_fg(ColorAttribute::PaletteIndex(0xff));
        let bg_color = palette.resolve_bg(ColorAttribute::Default);

        for (glyph_idx, info) in glyph_info.iter().enumerate() {
            let glyph = gl_state.glyph_cache.borrow_mut().cached_glyph(info, &style)?;

            let left = (glyph.x_offset + glyph.bearing_x) as f32;
            let top = ((render_metrics.cell_size.height as f64 + render_metrics.descender)
                - (glyph.y_offset + glyph.bearing_y)) as f32;

            let texture = glyph.texture.as_ref().unwrap_or(&gl_state.util_sprites.white_space);

            let slice = SpriteSlice {
                cell_idx: glyph_idx,
                num_cells: info.num_cells as usize,
                cell_width: render_metrics.cell_size.width as usize,
                scale: glyph.scale as f32,
                left_offset: left,
            };

            let pixel_rect = slice.pixel_rect(texture);
            let texture_rect = texture.texture.to_texture_coords(pixel_rect);

            let bottom = (pixel_rect.size.height as f32 * glyph.scale as f32) + top
                - render_metrics.cell_size.height as f32;
            let right = pixel_rect.size.width as f32 + left - render_metrics.cell_size.width as f32;

            let mut quad = Quad::for_cell(glyph_idx, &mut vertices);

            quad.set_fg_color(rgbcolor_to_window_color(glyph_color));
            quad.set_bg_color(rgbcolor_to_window_color(bg_color));
            quad.set_texture(texture_rect);
            quad.set_texture_adjust(left, top, right, bottom);
            quad.set_has_color(glyph.has_color);
        }

        Ok(())
    }
}

fn rgbcolor_to_window_color(color: RgbColor) -> Color {
    Color::rgba(color.red, color.green, color.blue, 0xff)
}
