use super::quad::{MappedQuads, VERTICES_PER_CELL};
use super::renderstate::RenderState;
use super::utilsprites::RenderMetrics;
use crate::config::TextStyle;
use crate::core::color::RgbColor;
use crate::font::FontConfiguration;
use crate::term::color::{ColorAttribute, ColorPalette};
use crate::window::bitmaps::atlas::SpriteSlice;
use crate::window::bitmaps::Texture2d;
use crate::window::color::Color;
use crate::window::Dimensions;
use crate::window::PixelLength;
use chrono::{DateTime, Utc};
use failure::Fallible;
use glium::{uniform, Surface};
use sysinfo::{ProcessorExt, System, SystemExt};

pub mod renderstate;

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
        gl_state: &RenderState,
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
            gl_state.header.slide_sprite(w);
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
            &*gl_state.header.rect_vertex_buffer.borrow(),
            &gl_state.header.rect_index_buffer,
            &gl_state.header.rect_program,
            &uniform! {
                projection: projection,
            },
            &draw_params,
        )?;

        let mut vb = gl_state.header.glyph_vertex_buffer.borrow_mut();
        let mut quads = gl_state.header.quads.map(&mut vb);

        self.render_line(gl_state, render_metrics, fonts, palette, &mut quads)?;

        let tex = gl_state.glyph_cache.borrow().atlas.texture();
        drop(quads);
        frame.draw(
            &*vb,
            &gl_state.header.glyph_index_buffer,
            &gl_state.glyph_program,
            &uniform! {
                projection: projection,
                glyph_tex: &*tex,
                bg_and_line_layer: false,
            },
            &draw_params,
        )?;

        let number_of_sprites = gl_state.header.spritesheet.sprites.len();
        let sprite =
            &gl_state.header.spritesheet.sprites[(self.count % number_of_sprites as u32) as usize];
        frame.draw(
            &*gl_state.header.sprite_vertex_buffer.borrow(),
            &gl_state.header.sprite_index_buffer,
            &gl_state.header.sprite_program,
            &uniform! {
                projection: projection,
                tex: &gl_state.header.player_texture.tex,
                source_dimensions: sprite.size,
                source_position: sprite.position,
                source_texture_dimensions: [gl_state.header.player_texture.width, gl_state.header.player_texture.height]
            },
            &draw_params,
        )?;

        Ok(())
    }

    fn render_line(
        &self,
        gl_state: &RenderState,
        render_metrics: &RenderMetrics,
        fonts: &FontConfiguration,
        palette: &ColorPalette,
        quads: &mut MappedQuads,
    ) -> Fallible<()> {
        let header_text = self.compute_header_text(quads.cols());
        let style = TextStyle::default();
        let glyph_info = {
            let font = fonts.resolve_font(&style)?;
            font.shape(&header_text)?
        };

        let glyph_color = palette.resolve_fg(ColorAttribute::PaletteIndex(0xff));
        let bg_color = palette.resolve_bg(ColorAttribute::Default);

        for (glyph_idx, info) in glyph_info.iter().enumerate() {
            let glyph = gl_state.glyph_cache.borrow_mut().cached_glyph(info, &style)?;

            let left = (glyph.x_offset + glyph.bearing_x).get() as f32;
            let top = ((PixelLength::new(render_metrics.cell_size.to_f64().height)
                + render_metrics.descender)
                - (glyph.y_offset + glyph.bearing_y))
                .get() as f32;
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

            let mut quad = quads.cell(glyph_idx, 0)?;

            quad.set_fg_color(rgbcolor_to_window_color(glyph_color));
            quad.set_bg_color(rgbcolor_to_window_color(bg_color));
            quad.set_texture(texture_rect);
            quad.set_texture_adjust(left, top, right, bottom);
            quad.set_has_color(glyph.has_color);
        }

        Ok(())
    }

    fn compute_header_text(&self, number_of_vertices: usize) -> String {
        let now: DateTime<Utc> = Utc::now();
        let current_time = now.format("%H:%M:%S").to_string();
        let cpu_load =
            format!("CPU:{}%", self.sys.get_global_processor_info().get_cpu_usage().round());
        let indent = (number_of_vertices / VERTICES_PER_CELL) as usize
            - (current_time.len() + cpu_load.len())
            - 2;

        format!(" {}{:indent$}{} ", cpu_load, "", current_time, indent = indent)
    }
}

fn rgbcolor_to_window_color(color: RgbColor) -> Color {
    Color::rgba(color.red, color.green, color.blue, 0xff)
}
