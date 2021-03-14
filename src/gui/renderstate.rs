use super::glyphcache::GlyphCache;
use super::quad::*;
use super::spritesheet::*;
use super::utilsprites::{RenderMetrics, UtilSprites};
use crate::config::Theme;
use crate::font::FontConfiguration;
use crate::window::bitmaps::ImageTexture;
use crate::window::color::Color;
use failure::Fallible;
use glium::backend::Context as GliumContext;
use glium::texture::SrgbTexture2d;
use glium::{IndexBuffer, VertexBuffer};
use lazy_static::lazy_static;
use std::cell::RefCell;
use std::rc::Rc;

const SPRITE_SPEED: f32 = 10.0;

lazy_static! {
    static ref CURRENT_TIME_LENGTH: usize = "00:00:00".chars().count();
    static ref CPU_LOAD_LENGTH: usize = "CPU:000%".chars().count();
}

pub struct SoftwareRenderState {
    pub glyph_cache: RefCell<GlyphCache<ImageTexture>>,
    pub util_sprites: UtilSprites<ImageTexture>,
}

impl SoftwareRenderState {
    pub fn new(
        fonts: &Rc<FontConfiguration>,
        metrics: &RenderMetrics,
        size: usize,
    ) -> Fallible<Self> {
        let glyph_cache = RefCell::new(GlyphCache::new(fonts, size));
        let util_sprites = UtilSprites::new(&mut glyph_cache.borrow_mut(), metrics)?;
        Ok(Self { glyph_cache, util_sprites })
    }
}

pub struct OpenGLRenderState {
    pub context: Rc<GliumContext>,
    pub glyph_cache: RefCell<GlyphCache<SrgbTexture2d>>,
    pub util_sprites: UtilSprites<SrgbTexture2d>,
    pub glyph_program: glium::Program,
    pub header_program: glium::Program,
    pub sprite_program: glium::Program,
    pub glyph_vertex_buffer: RefCell<VertexBuffer<Vertex>>,
    pub glyph_index_buffer: IndexBuffer<u32>,
    pub sprite_vertex_buffer: RefCell<VertexBuffer<SpriteVertex>>,
    pub sprite_index_buffer: IndexBuffer<u32>,
    pub header_rect_vertex_buffer: RefCell<VertexBuffer<RectVertex>>,
    pub header_rect_index_buffer: IndexBuffer<u32>,
    pub header_glyph_vertex_buffer: RefCell<VertexBuffer<Vertex>>,
    pub header_glyph_index_buffer: IndexBuffer<u32>,
    pub player_texture: SpriteSheetTexture,
    pub header_color: (f32, f32, f32, f32),
    pub header_height: f32,
    pub sprite_size: (f32, f32),
    pub sprite_speed: f32,
    pub spritesheet: SpriteSheet,
    pub dpi: f32,
}

impl OpenGLRenderState {
    pub fn new(
        context: Rc<GliumContext>,
        fonts: &Rc<FontConfiguration>,
        metrics: &RenderMetrics,
        size: usize,
        pixel_width: usize,
        pixel_height: usize,
        theme: &Theme,
    ) -> Fallible<Self> {
        let glyph_cache = RefCell::new(GlyphCache::new_gl(&context, fonts, size)?);
        let util_sprites = UtilSprites::new(&mut *glyph_cache.borrow_mut(), metrics)?;
        let spritesheet = get_spritesheet(&theme.spritesheet_path);
        let sprite_size = (spritesheet.sprite_width, spritesheet.sprite_height);
        let header_height = spritesheet.sprite_height;

        let mut glyph_errors = vec![];
        let mut glyph_program = None;
        for version in &["330", "300 es"] {
            let glyph_source = glium::program::ProgramCreationInput::SourceCode {
                vertex_shader: &Self::glyph_vertex_shader(version),
                fragment_shader: &Self::glyph_fragment_shader(version),
                outputs_srgb: true,
                tessellation_control_shader: None,
                tessellation_evaluation_shader: None,
                transform_feedback_varyings: None,
                uses_point_size: false,
                geometry_shader: None,
            };
            log::error!("compiling a prog with version {}", version);
            match glium::Program::new(&context, glyph_source) {
                Ok(prog) => {
                    glyph_program = Some(prog);
                    break;
                }
                Err(err) => glyph_errors.push(err.to_string()),
            };
        }

        let glyph_program = glyph_program.ok_or_else(|| {
            failure::format_err!("Failed to compile shaders: {}", glyph_errors.join("\n"))
        })?;

        let (glyph_vertex_buffer, glyph_index_buffer) = Self::compute_glyph_vertices(
            &context,
            header_height + 1.0,
            metrics,
            pixel_width as f32,
            pixel_height as f32,
        )?;

        let (header_glyph_vertex_buffer, header_glyph_index_buffer) =
            Self::compute_header_glyph_vertices(
                &context,
                header_height,
                *CPU_LOAD_LENGTH,
                *CURRENT_TIME_LENGTH,
                pixel_width as f32,
                pixel_height as f32,
                metrics,
            )?;

        let mut header_errors = vec![];
        let mut header_program = None;
        for version in &["330", "300 es"] {
            let rect_source = glium::program::ProgramCreationInput::SourceCode {
                vertex_shader: &Self::header_vertex_shader(version),
                fragment_shader: &Self::header_fragment_shader(version),
                outputs_srgb: true,
                tessellation_control_shader: None,
                tessellation_evaluation_shader: None,
                transform_feedback_varyings: None,
                uses_point_size: false,
                geometry_shader: None,
            };
            log::error!("compiling a prog with version {}", version);
            match glium::Program::new(&context, rect_source) {
                Ok(prog) => {
                    header_program = Some(prog);
                    break;
                }
                Err(err) => header_errors.push(err.to_string()),
            };
        }

        let header_program = header_program.ok_or_else(|| {
            failure::format_err!("Failed to compile shaders: {}", header_errors.join("\n"))
        })?;

        let color = Color::rgba(
            theme.header_color.red,
            theme.header_color.green,
            theme.header_color.blue,
            0xff,
        );

        let header_color = color.to_tuple_rgba();

        let (header_rect_vertex_buffer, header_rect_index_buffer) =
            Self::compute_header_rect_vertices(
                &context,
                header_color,
                header_height,
                pixel_width as f32,
                pixel_height as f32,
            )?;

        let mut sprite_errors = vec![];
        let mut sprite_program = None;
        for version in &["330", "300 es"] {
            let sprite_source = glium::program::ProgramCreationInput::SourceCode {
                vertex_shader: &Self::sprite_vertex_shader(version),
                fragment_shader: &Self::sprite_fragment_shader(version),
                outputs_srgb: true,
                tessellation_control_shader: None,
                tessellation_evaluation_shader: None,
                transform_feedback_varyings: None,
                uses_point_size: false,
                geometry_shader: None,
            };
            log::error!("compiling a prog with version {}", version);
            match glium::Program::new(&context, sprite_source) {
                Ok(prog) => {
                    sprite_program = Some(prog);
                    break;
                }
                Err(err) => sprite_errors.push(err.to_string()),
            };
        }

        let sprite_program = sprite_program.ok_or_else(|| {
            failure::format_err!("Failed to compile shaders: {}", sprite_errors.join("\n"))
        })?;

        let (sprite_vertex_buffer, sprite_index_buffer) = Self::compute_sprite_vertices(
            &context,
            sprite_size.0,
            sprite_size.1,
            pixel_width as f32,
            pixel_height as f32,
        )?;

        let image = image::open(&spritesheet.image_path).unwrap().to_rgba8();
        let image_dimensions = image.dimensions();
        let image =
            glium::texture::RawImage2d::from_raw_rgba_reversed(&image.into_raw(), image_dimensions);

        let player_texture = SpriteSheetTexture {
            tex: glium::texture::CompressedSrgbTexture2d::new(&context, image).unwrap(),
            width: image_dimensions.0 as f32,
            height: image_dimensions.1 as f32,
        };

        Ok(Self {
            context,
            glyph_cache,
            util_sprites,
            glyph_program,
            header_program,
            sprite_program,
            glyph_vertex_buffer: RefCell::new(glyph_vertex_buffer),
            glyph_index_buffer,
            sprite_vertex_buffer: RefCell::new(sprite_vertex_buffer),
            sprite_index_buffer,
            header_rect_vertex_buffer: RefCell::new(header_rect_vertex_buffer),
            header_rect_index_buffer,
            header_glyph_vertex_buffer: RefCell::new(header_glyph_vertex_buffer),
            header_glyph_index_buffer,
            player_texture,
            header_color,
            sprite_size,
            header_height,
            sprite_speed: SPRITE_SPEED,
            spritesheet,
            dpi: 1.0,
        })
    }

    pub fn change_header_scaling(
        &mut self,
        dpi: f32,
        metrics: &RenderMetrics,
        pixel_width: usize,
        pixel_height: usize,
    ) -> Fallible<()> {
        self.dpi = dpi;
        let dpi = dpi / 96.;
        self.sprite_size = (self.sprite_size.0 * dpi, self.sprite_size.1 * dpi);
        self.header_height = self.header_height * dpi;
        self.sprite_speed = self.sprite_speed * dpi;

        let (header_rect_vertex_buffer, header_rect_index_buffer) =
            Self::compute_header_rect_vertices(
                &self.context,
                self.header_color,
                self.header_height,
                pixel_width as f32,
                pixel_height as f32,
            )?;

        *self.header_rect_vertex_buffer.borrow_mut() = header_rect_vertex_buffer;
        self.header_rect_index_buffer = header_rect_index_buffer;

        let (glyph_vertex_buffer, glyph_index_buffer) = Self::compute_glyph_vertices(
            &self.context,
            self.header_height + 1.0,
            metrics,
            pixel_width as f32,
            pixel_height as f32,
        )?;

        *self.glyph_vertex_buffer.borrow_mut() = glyph_vertex_buffer;
        self.glyph_index_buffer = glyph_index_buffer;

        let (sprite_vertex_buffer, sprite_index_buffer) = Self::compute_sprite_vertices(
            &self.context,
            self.sprite_size.0,
            self.sprite_size.1,
            pixel_width as f32,
            pixel_height as f32,
        )?;

        *self.sprite_vertex_buffer.borrow_mut() = sprite_vertex_buffer;
        self.sprite_index_buffer = sprite_index_buffer;

        Ok(())
    }

    pub fn advise_of_window_size_change(
        &mut self,
        metrics: &RenderMetrics,
        pixel_width: usize,
        pixel_height: usize,
    ) -> Fallible<()> {
        let (glyph_vertex_buffer, glyph_index_buffer) = Self::compute_glyph_vertices(
            &self.context,
            self.header_height + 1.0,
            metrics,
            pixel_width as f32,
            pixel_height as f32,
        )?;

        *self.glyph_vertex_buffer.borrow_mut() = glyph_vertex_buffer;
        self.glyph_index_buffer = glyph_index_buffer;

        let (header_rect_vertex_buffer, header_rect_index_buffer) =
            Self::compute_header_rect_vertices(
                &self.context,
                self.header_color,
                self.header_height,
                pixel_width as f32,
                pixel_height as f32,
            )?;

        *self.header_rect_vertex_buffer.borrow_mut() = header_rect_vertex_buffer;
        self.header_rect_index_buffer = header_rect_index_buffer;

        let (header_glyph_vertex_buffer, header_glyph_index_buffer) =
            Self::compute_header_glyph_vertices(
                &self.context,
                self.header_height,
                *CPU_LOAD_LENGTH,
                *CURRENT_TIME_LENGTH,
                pixel_width as f32,
                pixel_height as f32,
                metrics,
            )?;

        *self.header_glyph_vertex_buffer.borrow_mut() = header_glyph_vertex_buffer;
        self.header_glyph_index_buffer = header_glyph_index_buffer;

        self.reset_sprite_pos(pixel_height as f32 / 2.0);

        Ok(())
    }

    pub fn reset_sprite_pos(&mut self, top: f32) {
        let mut vb = self.sprite_vertex_buffer.borrow_mut();
        let mut vert = { vb.slice_mut(0..4).unwrap().map() };

        vert[V_TOP_LEFT].position.1 = -top;
        vert[V_TOP_RIGHT].position.1 = -top;
        vert[V_BOT_LEFT].position.1 = -top + self.sprite_size.1;
        vert[V_BOT_RIGHT].position.1 = -top + self.sprite_size.1;
    }

    fn glyph_vertex_shader(version: &str) -> String {
        format!("#version {}\n{}", version, include_str!("shaders/g_vertex.glsl"))
    }

    fn glyph_fragment_shader(version: &str) -> String {
        format!("#version {}\n{}", version, include_str!("shaders/g_fragment.glsl"))
    }

    fn header_vertex_shader(version: &str) -> String {
        format!("#version {}\n{}", version, include_str!("shaders/h_vertex.glsl"))
    }

    fn header_fragment_shader(version: &str) -> String {
        format!("#version {}\n{}", version, include_str!("shaders/h_fragment.glsl"))
    }

    fn sprite_vertex_shader(version: &str) -> String {
        format!("#version {}\n{}", version, include_str!("shaders/s_vertex.glsl"))
    }

    fn sprite_fragment_shader(version: &str) -> String {
        format!("#version {}\n{}", version, include_str!("shaders/s_fragment.glsl"))
    }

    fn compute_glyph_vertices(
        context: &Rc<GliumContext>,
        top_padding: f32,
        metrics: &RenderMetrics,
        width: f32,
        height: f32,
    ) -> Fallible<(VertexBuffer<Vertex>, IndexBuffer<u32>)> {
        let cell_width = metrics.cell_size.width as f32;
        let cell_height = metrics.cell_size.height as f32;
        let mut verts = Vec::new();
        let mut indices = Vec::new();

        let num_cols = width as usize / cell_width as usize;
        let num_rows = height as usize / cell_height as usize;

        for y in 0..num_rows {
            for x in 0..num_cols {
                let y_pos = top_padding + (height / -2.0) + (y as f32 * cell_height);
                let x_pos = (width / -2.0) + (x as f32 * cell_width);

                let idx = verts.len() as u32;
                verts.push(Vertex { position: (x_pos, y_pos), ..Default::default() });
                verts.push(Vertex { position: (x_pos + cell_width, y_pos), ..Default::default() });
                verts.push(Vertex { position: (x_pos, y_pos + cell_height), ..Default::default() });
                verts.push(Vertex {
                    position: (x_pos + cell_width, y_pos + cell_height),
                    ..Default::default()
                });

                indices.push(idx + V_TOP_LEFT as u32);
                indices.push(idx + V_TOP_RIGHT as u32);
                indices.push(idx + V_BOT_LEFT as u32);

                indices.push(idx + V_TOP_RIGHT as u32);
                indices.push(idx + V_BOT_LEFT as u32);
                indices.push(idx + V_BOT_RIGHT as u32);
            }
        }

        Ok((
            VertexBuffer::dynamic(context, &verts)?,
            IndexBuffer::new(context, glium::index::PrimitiveType::TrianglesList, &indices)?,
        ))
    }

    fn compute_header_glyph_vertices(
        context: &Rc<GliumContext>,
        header_height: f32,
        left_num_cols: usize,
        right_num_cols: usize,
        width: f32,
        height: f32,
        metrics: &RenderMetrics,
    ) -> Fallible<(VertexBuffer<Vertex>, IndexBuffer<u32>)> {
        let mut verts = Vec::new();
        let mut indices = Vec::new();

        let cell_width = metrics.cell_size.width as f32;
        let cell_height = metrics.cell_size.height as f32;

        let header_width_padding = cell_width;

        let top_padding = (header_height - cell_height) / 2.0;
        let y_pos = (height / -2.0) + top_padding;
        for x in 0..(left_num_cols + right_num_cols) {
            let x_pos = if x < left_num_cols {
                (width / -2.0) + header_width_padding + (x as f32 * cell_width)
            } else {
                (width / 2.0)
                    - header_width_padding
                    - ((left_num_cols + right_num_cols - x) as f32 * cell_width)
                    + 5.0
            };

            let idx = verts.len() as u32;
            verts.push(Vertex { position: (x_pos, y_pos), ..Default::default() });
            verts.push(Vertex { position: (x_pos + cell_width, y_pos), ..Default::default() });
            verts.push(Vertex { position: (x_pos, y_pos + cell_height), ..Default::default() });
            verts.push(Vertex {
                position: (x_pos + cell_width, y_pos + cell_height),
                ..Default::default()
            });

            indices.push(idx);
            indices.push(idx + 1);
            indices.push(idx + 2);
            indices.push(idx + 1);
            indices.push(idx + 2);
            indices.push(idx + 3);
        }
        Ok((
            VertexBuffer::dynamic(context, &verts)?,
            IndexBuffer::new(context, glium::index::PrimitiveType::TrianglesList, &indices)?,
        ))
    }

    pub fn compute_sprite_vertices(
        context: &Rc<GliumContext>,
        sprite_width: f32,
        sprite_height: f32,
        width: f32,
        height: f32,
    ) -> Fallible<(VertexBuffer<SpriteVertex>, IndexBuffer<u32>)> {
        let mut verts = Vec::new();

        let (w, h) = { (width / 2.0, height / 2.0) };

        verts.push(SpriteVertex {
            tex_coords: (0.0, 1.0),
            position: (-w, -h),
            ..Default::default()
        });
        verts.push(SpriteVertex {
            tex_coords: (1.0, 1.0),
            position: (-w + sprite_width, -h),
            ..Default::default()
        });
        verts.push(SpriteVertex {
            tex_coords: (0.0, 0.0),
            position: (-w, -h + sprite_height),
            ..Default::default()
        });
        verts.push(SpriteVertex {
            tex_coords: (1.0, 0.0),
            position: (-w + sprite_width, -h + sprite_height),
            ..Default::default()
        });

        Ok((
            VertexBuffer::dynamic(context, &verts)?,
            IndexBuffer::new(
                context,
                glium::index::PrimitiveType::TrianglesList,
                &[0, 1, 2, 1, 3, 2],
            )?,
        ))
    }

    pub fn compute_header_rect_vertices(
        context: &Rc<GliumContext>,
        color: (f32, f32, f32, f32),
        banner_height: f32,
        width: f32,
        height: f32,
    ) -> Fallible<(VertexBuffer<RectVertex>, IndexBuffer<u32>)> {
        let mut verts = Vec::new();

        let (w, h) = ((width / 2.0), (height / 2.0));

        verts.push(RectVertex { position: (-w, -h), color });
        verts.push(RectVertex { position: (w, -h), color });
        verts.push(RectVertex { position: (-w, -h + banner_height), color });
        verts.push(RectVertex { position: (w, -h + banner_height), color });

        Ok((
            VertexBuffer::dynamic(context, &verts)?,
            IndexBuffer::new(
                context,
                glium::index::PrimitiveType::TrianglesList,
                &[0, 1, 2, 1, 3, 2],
            )?,
        ))
    }

    pub fn slide_sprite(&self, width: f32) {
        let mut vb = self.sprite_vertex_buffer.borrow_mut();
        let mut vert = { vb.slice_mut(0..4).unwrap().map() };

        let delta = self.sprite_speed;
        let sprite_width = self.sprite_size.0;

        if vert[V_TOP_LEFT].position.0 > width {
            vert[V_TOP_LEFT].position.0 = -width;
            vert[V_TOP_RIGHT].position.0 = -width + sprite_width;
            vert[V_BOT_LEFT].position.0 = -width;
            vert[V_BOT_RIGHT].position.0 = -width + sprite_width;
        } else {
            vert[V_TOP_LEFT].position.0 += delta;
            vert[V_TOP_RIGHT].position.0 += delta;
            vert[V_BOT_LEFT].position.0 += delta;
            vert[V_BOT_RIGHT].position.0 += delta;
        }
    }
}

#[allow(clippy::large_enum_variant)]
pub enum RenderState {
    Software(SoftwareRenderState),
    GL(OpenGLRenderState),
}

impl RenderState {
    pub fn recreate_texture_atlas(
        &mut self,
        fonts: &Rc<FontConfiguration>,
        metrics: &RenderMetrics,
        size: Option<usize>,
    ) -> Fallible<()> {
        match self {
            RenderState::Software(software) => {
                let size = size.unwrap_or_else(|| software.glyph_cache.borrow().atlas.size());
                let mut glyph_cache = GlyphCache::new(fonts, size);
                software.util_sprites = UtilSprites::new(&mut glyph_cache, metrics)?;
                *software.glyph_cache.borrow_mut() = glyph_cache;
            }
            RenderState::GL(gl) => {
                let size = size.unwrap_or_else(|| gl.glyph_cache.borrow().atlas.size());
                let mut glyph_cache = GlyphCache::new_gl(&gl.context, fonts, size)?;
                gl.util_sprites = UtilSprites::new(&mut glyph_cache, metrics)?;
                *gl.glyph_cache.borrow_mut() = glyph_cache;
            }
        };
        Ok(())
    }

    pub fn advise_of_window_size_change(
        &mut self,
        metrics: &RenderMetrics,
        pixel_width: usize,
        pixel_height: usize,
    ) -> Fallible<()> {
        if let RenderState::GL(gl) = self {
            gl.advise_of_window_size_change(metrics, pixel_width, pixel_height)?;
        }
        Ok(())
    }

    pub fn change_header_scaling(
        &mut self,
        dpi: f32,
        metrics: &RenderMetrics,
        pixel_width: usize,
        pixel_height: usize,
    ) -> Fallible<()> {
        if let RenderState::GL(gl) = self {
            gl.change_header_scaling(dpi, metrics, pixel_width, pixel_height)?;
        }
        Ok(())
    }

    pub fn opengl(&self) -> &OpenGLRenderState {
        match self {
            RenderState::GL(gl) => gl,
            _ => panic!("only valid for opengl render mode"),
        }
    }
}
