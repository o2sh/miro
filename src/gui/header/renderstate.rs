use crate::config::Theme;
use crate::gui::quad::*;
use crate::gui::spritesheet::*;
use crate::gui::utilsprites::RenderMetrics;
use crate::window::color::Color;
use failure::Fallible;
use glium::backend::Context as GliumContext;
use glium::{IndexBuffer, VertexBuffer};
use std::cell::RefCell;
use std::rc::Rc;

const SPRITE_SPEED: f32 = 10.0;

fn rect_vertex_shader(version: &str) -> String {
    format!(
        "#version {}\n{}",
        version,
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shaders/h_vertex.glsl"))
    )
}

fn rect_fragment_shader(version: &str) -> String {
    format!(
        "#version {}\n{}",
        version,
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets//shaders/h_fragment.glsl"))
    )
}

fn sprite_vertex_shader(version: &str) -> String {
    format!(
        "#version {}\n{}",
        version,
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shaders/s_vertex.glsl"))
    )
}

fn sprite_fragment_shader(version: &str) -> String {
    format!(
        "#version {}\n{}",
        version,
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shaders/s_fragment.glsl"))
    )
}

pub struct HeaderRenderState {
    pub context: Rc<GliumContext>,
    pub rect_program: glium::Program,
    pub sprite_program: glium::Program,
    pub sprite_vertex_buffer: RefCell<VertexBuffer<SpriteVertex>>,
    pub sprite_index_buffer: IndexBuffer<u32>,
    pub rect_vertex_buffer: RefCell<VertexBuffer<RectVertex>>,
    pub rect_index_buffer: IndexBuffer<u32>,
    pub glyph_vertex_buffer: RefCell<VertexBuffer<Vertex>>,
    pub glyph_index_buffer: IndexBuffer<u32>,
    pub player_texture: SpriteSheetTexture,
    pub color: (f32, f32, f32, f32),
    pub height: f32,
    pub sprite_size: (f32, f32),
    pub sprite_speed: f32,
    pub spritesheet: SpriteSheet,
    pub dpi: f32,
}

impl HeaderRenderState {
    pub fn new(
        context: Rc<GliumContext>,
        theme: &Theme,
        metrics: &RenderMetrics,
        pixel_width: usize,
        pixel_height: usize,
    ) -> Fallible<Self> {
        let spritesheet = get_spritesheet(&theme.spritesheet_path);
        let sprite_size = (spritesheet.sprite_width, spritesheet.sprite_height);
        let height = spritesheet.sprite_height;
        let (glyph_vertex_buffer, glyph_index_buffer) = Self::compute_glyph_vertices(
            &context,
            height,
            pixel_width as f32,
            pixel_height as f32,
            metrics,
        )?;

        let mut header_errors = vec![];
        let mut rect_program = None;
        for version in &["330", "300 es"] {
            let rect_source = glium::program::ProgramCreationInput::SourceCode {
                vertex_shader: &rect_vertex_shader(version),
                fragment_shader: &rect_fragment_shader(version),
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
                    rect_program = Some(prog);
                    break;
                }
                Err(err) => header_errors.push(err.to_string()),
            };
        }

        let rect_program = rect_program.ok_or_else(|| {
            failure::format_err!("Failed to compile shaders: {}", header_errors.join("\n"))
        })?;

        let color = Color::rgba(theme.color.red, theme.color.green, theme.color.blue, 0xff);

        let color = color.to_tuple_rgba();

        let (rect_vertex_buffer, rect_index_buffer) = Self::compute_rect_vertices(
            &context,
            color,
            height,
            pixel_width as f32,
            pixel_height as f32,
        )?;

        let mut sprite_errors = vec![];
        let mut sprite_program = None;
        for version in &["330", "300 es"] {
            let sprite_source = glium::program::ProgramCreationInput::SourceCode {
                vertex_shader: &sprite_vertex_shader(version),
                fragment_shader: &sprite_fragment_shader(version),
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
            rect_program,
            sprite_program,
            sprite_vertex_buffer: RefCell::new(sprite_vertex_buffer),
            sprite_index_buffer,
            rect_vertex_buffer: RefCell::new(rect_vertex_buffer),
            rect_index_buffer,
            glyph_vertex_buffer: RefCell::new(glyph_vertex_buffer),
            glyph_index_buffer,
            player_texture,
            color,
            sprite_size,
            height,
            sprite_speed: SPRITE_SPEED,
            spritesheet,
            dpi: 1.0,
        })
    }

    pub fn advise_of_window_size_change(
        &mut self,
        metrics: &RenderMetrics,
        pixel_width: usize,
        pixel_height: usize,
    ) -> Fallible<()> {
        let (rect_vertex_buffer, rect_index_buffer) = Self::compute_rect_vertices(
            &self.context,
            self.color,
            self.height,
            pixel_width as f32,
            pixel_height as f32,
        )?;

        *self.rect_vertex_buffer.borrow_mut() = rect_vertex_buffer;
        self.rect_index_buffer = rect_index_buffer;

        let (glyph_vertex_buffer, glyph_index_buffer) = Self::compute_glyph_vertices(
            &self.context,
            self.height,
            pixel_width as f32,
            pixel_height as f32,
            metrics,
        )?;

        *self.glyph_vertex_buffer.borrow_mut() = glyph_vertex_buffer;
        self.glyph_index_buffer = glyph_index_buffer;

        self.reset_sprite_pos(pixel_height as f32 / 2.0);

        Ok(())
    }

    pub fn change_scaling(
        &mut self,
        new_dpi: f32,
        pixel_width: usize,
        pixel_height: usize,
    ) -> Fallible<()> {
        self.dpi = new_dpi;
        self.sprite_size = (self.sprite_size.0 * self.dpi, self.sprite_size.1 * self.dpi);
        self.height = self.height * self.dpi;
        self.sprite_speed = self.sprite_speed * self.dpi;

        let (rect_vertex_buffer, rect_index_buffer) = Self::compute_rect_vertices(
            &self.context,
            self.color,
            self.height,
            pixel_width as f32,
            pixel_height as f32,
        )?;

        *self.rect_vertex_buffer.borrow_mut() = rect_vertex_buffer;
        self.rect_index_buffer = rect_index_buffer;

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

    pub fn reset_sprite_pos(&mut self, top: f32) {
        let mut vb = self.sprite_vertex_buffer.borrow_mut();
        let mut vert = { vb.slice_mut(0..4).unwrap().map() };

        vert[V_TOP_LEFT].position.1 = -top;
        vert[V_TOP_RIGHT].position.1 = -top;
        vert[V_BOT_LEFT].position.1 = -top + self.sprite_size.1;
        vert[V_BOT_RIGHT].position.1 = -top + self.sprite_size.1;
    }

    fn compute_glyph_vertices(
        context: &Rc<GliumContext>,
        header_height: f32,
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

        let num_cols = (width - header_width_padding * 2.) / cell_width;

        for x in 0..num_cols as usize {
            let x_pos = (width / -2.0) + header_width_padding + (x as f32 * cell_width);

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

    pub fn compute_rect_vertices(
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
            vert[V_TOP_LEFT].position.0 = -width - sprite_width;
            vert[V_TOP_RIGHT].position.0 = -width;
            vert[V_BOT_LEFT].position.0 = -width - sprite_width;
            vert[V_BOT_RIGHT].position.0 = -width;
        } else {
            vert[V_TOP_LEFT].position.0 += delta;
            vert[V_TOP_RIGHT].position.0 += delta;
            vert[V_BOT_LEFT].position.0 += delta;
            vert[V_BOT_RIGHT].position.0 += delta;
        }
    }
}
