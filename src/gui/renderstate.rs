use super::glyphcache::GlyphCache;
use super::header::renderstate::HeaderRenderState;
use super::quad::*;
use super::utilsprites::{RenderMetrics, UtilSprites};
use crate::config::Theme;
use crate::font::FontConfiguration;
use failure::Fallible;
use glium::backend::Context as GliumContext;
use glium::texture::SrgbTexture2d;
use glium::{IndexBuffer, VertexBuffer};
use std::cell::RefCell;
use std::rc::Rc;

fn glyph_vertex_shader(version: &str) -> String {
    format!(
        "#version {}\n{}",
        version,
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shaders/g_vertex.glsl"))
    )
}

fn glyph_fragment_shader(version: &str) -> String {
    format!(
        "#version {}\n{}",
        version,
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/shaders/g_fragment.glsl"))
    )
}

pub struct RenderState {
    pub context: Rc<GliumContext>,
    pub glyph_cache: RefCell<GlyphCache<SrgbTexture2d>>,
    pub util_sprites: UtilSprites<SrgbTexture2d>,
    pub glyph_program: glium::Program,
    pub glyph_vertex_buffer: RefCell<VertexBuffer<Vertex>>,
    pub glyph_index_buffer: IndexBuffer<u32>,
    pub header: HeaderRenderState,
    pub quads: Quads,
}

impl RenderState {
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
        let mut glyph_errors = vec![];
        let mut glyph_program = None;
        for version in &["330", "300 es"] {
            let glyph_source = glium::program::ProgramCreationInput::SourceCode {
                vertex_shader: &glyph_vertex_shader(version),
                fragment_shader: &glyph_fragment_shader(version),
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

        let (glyph_vertex_buffer, glyph_index_buffer, quads) = Self::compute_glyph_vertices(
            &context,
            metrics,
            pixel_width as f32,
            pixel_height as f32,
        )?;

        let header =
            HeaderRenderState::new(context.clone(), theme, metrics, pixel_width, pixel_height)?;

        Ok(Self {
            context,
            glyph_cache,
            util_sprites,
            glyph_program,
            glyph_vertex_buffer: RefCell::new(glyph_vertex_buffer),
            glyph_index_buffer,
            header,
            quads,
        })
    }

    pub fn advise_of_window_size_change(
        &mut self,
        metrics: &RenderMetrics,
        pixel_width: usize,
        pixel_height: usize,
    ) -> Fallible<()> {
        let (glyph_vertex_buffer, glyph_index_buffer, quads) = Self::compute_glyph_vertices(
            &self.context,
            metrics,
            pixel_width as f32,
            pixel_height as f32,
        )?;

        *self.glyph_vertex_buffer.borrow_mut() = glyph_vertex_buffer;
        self.glyph_index_buffer = glyph_index_buffer;
        self.quads = quads;
        self.header.advise_of_window_size_change(metrics, pixel_width, pixel_height)
    }
    pub fn recreate_texture_atlas(
        &mut self,
        fonts: &Rc<FontConfiguration>,
        metrics: &RenderMetrics,
        size: Option<usize>,
    ) -> Fallible<()> {
        let size = size.unwrap_or_else(|| self.glyph_cache.borrow().atlas.size());
        let mut glyph_cache = GlyphCache::new_gl(&self.context, fonts, size)?;
        self.util_sprites = UtilSprites::new(&mut glyph_cache, metrics)?;
        *self.glyph_cache.borrow_mut() = glyph_cache;
        Ok(())
    }
    fn compute_glyph_vertices(
        context: &Rc<GliumContext>,
        metrics: &RenderMetrics,
        width: f32,
        height: f32,
    ) -> Fallible<(VertexBuffer<Vertex>, IndexBuffer<u32>, Quads)> {
        let cell_width = metrics.cell_size.width as f32;
        let cell_height = metrics.cell_size.height as f32;
        let mut verts = Vec::new();
        let mut indices = Vec::new();

        let num_cols = width as usize / cell_width as usize;
        let num_rows = height as usize / cell_height as usize;
        let mut quads = Quads::default();
        quads.cols = num_cols;

        let mut define_quad = |left, top, right, bottom| -> u32 {
            let idx = verts.len() as u32;

            verts.push(Vertex { position: (left, top), ..Default::default() });
            verts.push(Vertex { position: (right, top), ..Default::default() });
            verts.push(Vertex { position: (left, bottom), ..Default::default() });
            verts.push(Vertex { position: (right, bottom), ..Default::default() });

            indices.push(idx + V_TOP_LEFT as u32);
            indices.push(idx + V_TOP_RIGHT as u32);
            indices.push(idx + V_BOT_LEFT as u32);

            indices.push(idx + V_TOP_RIGHT as u32);
            indices.push(idx + V_BOT_LEFT as u32);
            indices.push(idx + V_BOT_RIGHT as u32);

            idx
        };

        for y in 0..num_rows {
            let y_pos = (height / -2.0) + (y as f32 * cell_height);

            for x in 0..num_cols {
                let x_pos = (width / -2.0) + (x as f32 * cell_width);

                let idx = define_quad(x_pos, y_pos, x_pos + cell_width, y_pos + cell_height);
                if x == 0 {
                    quads.row_starts.push(idx as usize);
                }
            }
        }

        Ok((
            VertexBuffer::dynamic(context, &verts)?,
            IndexBuffer::new(context, glium::index::PrimitiveType::TrianglesList, &indices)?,
            quads,
        ))
    }
}
