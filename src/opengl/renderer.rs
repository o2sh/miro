use crate::config::SpriteSheetConfig;
use crate::config::{TextStyle, Theme};
use crate::font::{FontConfiguration, GlyphInfo};
use crate::opengl::spritesheet::{SpriteSheet, SpriteSheetTexture};
use crate::term::{self, CursorPosition, Line, Underline};
use chrono::{DateTime, Utc};
use euclid;
use failure::{self, Error};
use glium::backend::Facade;
use glium::texture::SrgbTexture2d;
use glium::{self, IndexBuffer, Surface, VertexBuffer};
use lazy_static::lazy_static;
use log::debug;
use std::cell::RefCell;
use std::collections::HashMap;
use std::mem;
use std::ops::Range;
use std::rc::Rc;
use sysinfo::{ProcessorExt, System, SystemExt};
use term::color::RgbaTuple;

use crate::opengl::texture_atlas::{Atlas, Sprite, SpriteSlice, TEX_SIZE};
use crate::window::Point;

type Transform3D = euclid::Transform3D<f32, f32, f32>;

/// Each cell is composed of two triangles built from 4 vertices.
/// The buffer is organized row by row.
const VERTICES_PER_CELL: usize = 4;
const V_TOP_LEFT: usize = 0;
const V_TOP_RIGHT: usize = 1;
const V_BOT_LEFT: usize = 2;
const V_BOT_RIGHT: usize = 3;

#[derive(Copy, Clone, Debug, Default)]
struct Vertex {
    // pre-computed by compute_vertices and changed only on resize
    position: Point,
    // adjustment for glyph size, recomputed each time the cell changes
    adjust: Point,
    // texture coords are updated as the screen contents change
    tex: (f32, f32),
    // cell foreground and background color
    fg_color: (f32, f32, f32, f32),
    bg_color: (f32, f32, f32, f32),
    /// Nominally a boolean, but the shader compiler hated it
    has_color: f32,
    /// Count of how many underlines there are
    underline: f32,
    strikethrough: f32,
    v_idx: f32,
}

implement_vertex!(
    Vertex,
    position,
    adjust,
    tex,
    fg_color,
    bg_color,
    has_color,
    underline,
    strikethrough,
    v_idx,
);

#[derive(Copy, Clone, Debug, Default)]
pub struct SpriteVertex {
    pub position: Point,
    tex_coords: Point,
}

implement_vertex!(SpriteVertex, position, tex_coords);

#[derive(Copy, Clone)]
pub struct RectVertex {
    position: [f32; 2],
    color: [f32; 3],
}

implement_vertex!(RectVertex, position, color);

/// How many columns the underline texture has
const U_COLS: f32 = 5.0;
/// The glyph has no underline or strikethrough
const U_NONE: f32 = 0.0;
/// The glyph has a single underline.  This value is actually the texture
/// coordinate for the right hand side of the underline.
const U_ONE: f32 = 1.0 / U_COLS;
/// Texture coord for the RHS of the double underline glyph
const U_TWO: f32 = 2.0 / U_COLS;
/// Texture coord for the RHS of the strikethrough glyph
const U_STRIKE: f32 = 3.0 / U_COLS;
/// Texture coord for the RHS of the strikethrough + single underline glyph
const U_STRIKE_ONE: f32 = 4.0 / U_COLS;
/// Texture coord for the RHS of the strikethrough + double underline glyph
const U_STRIKE_TWO: f32 = 5.0 / U_COLS;

lazy_static! {
    static ref CURRENT_TIME_LENGTH: usize = "00:00:00".chars().count();
    static ref CPU_LOAD_LENGTH: usize = "CPU:00%".chars().count();
}
const HEADER_WIDTH_PADDING: f32 = 13.0;

const GLYPH_VERTEX_SHADER: &str = include_str!("../../assets/shader/g_vertex.glsl");
const GLYPH_FRAGMENT_SHADER: &str = include_str!("../../assets/shader/g_fragment.glsl");

const PLAYER_VERTEX_SHADER: &str = include_str!("../../assets/shader/p_vertex.glsl");
const PLAYER_FRAGMENT_SHADER: &str = include_str!("../../assets/shader/p_fragment.glsl");

const RECT_VERTEX_SHADER: &str = include_str!("../../assets/shader/r_vertex.glsl");
const RECT_FRAGMENT_SHADER: &str = include_str!("../../assets/shader/r_fragment.glsl");

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    font_idx: usize,
    glyph_pos: u32,
    style: TextStyle,
}

/// Caches a rendered glyph.
/// The image data may be None for whitespace glyphs.
#[derive(Debug)]
pub struct CachedGlyph {
    has_color: bool,
    x_offset: f64,
    y_offset: f64,
    bearing_x: f64,
    bearing_y: f64,
    texture: Option<Sprite>,
    scale: f64,
}

pub struct Renderer {
    width: u16,
    height: u16,
    pub fonts: Rc<FontConfiguration>,
    header_banner_color: RgbaTuple,
    header_text_style: TextStyle,
    header_cell_height: usize,
    header_cell_width: usize,
    header_cell_descender: isize,
    cell_height: f64,
    cell_width: f64,
    descender: f64,
    glyph_cache: RefCell<HashMap<GlyphKey, Rc<CachedGlyph>>>,
    g_program: glium::Program,
    r_program: glium::Program,
    p_program: glium::Program,
    glyph_vertex_buffer: RefCell<VertexBuffer<Vertex>>,
    glyph_index_buffer: IndexBuffer<u32>,
    glyph_header_vertex_buffer: RefCell<VertexBuffer<Vertex>>,
    glyph_header_index_buffer: IndexBuffer<u32>,
    sprite_vertex_buffer: RefCell<VertexBuffer<SpriteVertex>>,
    sprite_index_buffer: IndexBuffer<u32>,
    rect_vertex_buffer: RefCell<VertexBuffer<RectVertex>>,
    rect_index_buffer: IndexBuffer<u32>,
    projection: Transform3D,
    glyph_atlas: RefCell<Atlas>,
    underline_tex: SrgbTexture2d,
    palette: term::color::ColorPalette,
    spritesheet: SpriteSheet,
    pub frame_count: u32,
    player_texture: SpriteSheetTexture,
    pub sys: System,
}

impl Renderer {
    pub fn new<F: Facade>(
        facade: &F,
        width: u16,
        height: u16,
        fonts: &Rc<FontConfiguration>,
        palette: term::color::ColorPalette,
        theme: &Theme,
        sys: System,
    ) -> Result<Self, Error> {
        let spritesheet = get_spritesheet(&theme.spritesheet_path);
        let metrics = fonts.default_font_metrics()?;
        let (cell_height, cell_width, descender) =
            (metrics.cell_height, metrics.cell_width, metrics.descender);

        let underline_tex = Self::compute_underlines(facade, cell_width, cell_height, descender)?;

        //Header Text
        let header_text_style = TextStyle {
            fontconfig_pattern: String::from("monospace:size=13"),
            ..Default::default()
        };
        let font = fonts.cached_font(&header_text_style)?;
        let (header_cell_height, header_cell_width, header_cell_descender) = {
            let metrics = font.borrow_mut().get_fallback(0)?.metrics();
            (metrics.cell_height as usize, metrics.cell_width as usize, metrics.descender)
        };
        let header_cell_descender = if header_cell_descender.is_sign_positive() {
            ((header_cell_descender as f64) / 64.0).ceil() as isize
        } else {
            ((header_cell_descender as f64) / 64.0).floor() as isize
        };

        let (glyph_header_vertex_buffer, glyph_header_index_buffer) =
            Self::compute_header_text_vertices(
                facade,
                spritesheet.sprite_height,
                HEADER_WIDTH_PADDING,
                *CPU_LOAD_LENGTH,
                *CURRENT_TIME_LENGTH,
                width as f32,
                height as f32,
                header_cell_width as f32,
                header_cell_height as f32,
            )?;

        let (glyph_vertex_buffer, glyph_index_buffer) = Self::compute_vertices(
            facade,
            spritesheet.sprite_height + 1.0,
            cell_width as f32,
            cell_height as f32,
            width as f32,
            height as f32,
        )?;

        let (sprite_vertex_buffer, sprite_index_buffer) = Self::compute_sprite_vertices(
            facade,
            spritesheet.sprite_width,
            spritesheet.sprite_height,
            width as f32,
            height as f32,
        );

        let (rect_vertex_buffer, rect_index_buffer) = Self::compute_rect_vertices(
            facade,
            theme.header_color.to_linear_tuple_rgba(),
            spritesheet.sprite_height,
            width as f32,
            height as f32,
        );

        let g_source = glium::program::ProgramCreationInput::SourceCode {
            vertex_shader: GLYPH_VERTEX_SHADER,
            fragment_shader: GLYPH_FRAGMENT_SHADER,
            outputs_srgb: true,
            tessellation_control_shader: None,
            tessellation_evaluation_shader: None,
            transform_feedback_varyings: None,
            uses_point_size: false,
            geometry_shader: None,
        };

        let r_source = glium::program::ProgramCreationInput::SourceCode {
            vertex_shader: RECT_VERTEX_SHADER,
            fragment_shader: RECT_FRAGMENT_SHADER,
            outputs_srgb: true,
            tessellation_control_shader: None,
            tessellation_evaluation_shader: None,
            transform_feedback_varyings: None,
            uses_point_size: false,
            geometry_shader: None,
        };

        let p_source = glium::program::ProgramCreationInput::SourceCode {
            vertex_shader: PLAYER_VERTEX_SHADER,
            fragment_shader: PLAYER_FRAGMENT_SHADER,
            outputs_srgb: true,
            tessellation_control_shader: None,
            tessellation_evaluation_shader: None,
            transform_feedback_varyings: None,
            uses_point_size: false,
            geometry_shader: None,
        };

        let g_program = glium::Program::new(facade, g_source)?;

        let r_program = glium::Program::new(facade, r_source)?;

        let p_program = glium::Program::new(facade, p_source)?;

        let glyph_atlas = RefCell::new(Atlas::new(facade, TEX_SIZE)?);

        let image = image::open(&spritesheet.image_path).unwrap().to_rgba8();
        let image_dimensions = image.dimensions();
        let image =
            glium::texture::RawImage2d::from_raw_rgba_reversed(&image.into_raw(), image_dimensions);

        let player_texture = SpriteSheetTexture {
            tex: glium::texture::CompressedSrgbTexture2d::new(facade, image).unwrap(),
            width: image_dimensions.0 as f32,
            height: image_dimensions.1 as f32,
        };

        Ok(Self {
            glyph_atlas,
            player_texture,
            g_program,
            r_program,
            p_program,
            glyph_vertex_buffer: RefCell::new(glyph_vertex_buffer),
            glyph_index_buffer,
            glyph_header_vertex_buffer: RefCell::new(glyph_header_vertex_buffer),
            glyph_header_index_buffer,
            sprite_vertex_buffer: RefCell::new(sprite_vertex_buffer),
            sprite_index_buffer,
            rect_vertex_buffer: RefCell::new(rect_vertex_buffer),
            rect_index_buffer,
            palette,
            width,
            height,
            fonts: Rc::clone(fonts),
            cell_height,
            cell_width,
            descender,
            header_cell_height,
            header_cell_width,
            header_cell_descender,
            header_text_style,
            glyph_cache: RefCell::new(HashMap::new()),
            projection: Self::compute_projection(width as f32, height as f32),
            underline_tex,
            spritesheet,
            frame_count: 0,
            sys,
            header_banner_color: theme.header_color.to_linear_tuple_rgba(),
        })
    }

    /// Create the texture atlas for the line decoration layer.
    /// This is a bitmap with columns to accomodate the U_XXX
    /// constants defined above.
    fn compute_underlines<F: Facade>(
        facade: &F,
        cell_width: f64,
        cell_height: f64,
        descender: f64,
    ) -> Result<SrgbTexture2d, glium::texture::TextureCreationError> {
        let cell_width = cell_width.ceil() as usize;
        let cell_height = cell_height.ceil() as usize;
        let descender = if descender.is_sign_positive() {
            (descender / 64.0).ceil() as isize
        } else {
            (descender / 64.0).floor() as isize
        };

        let width = 5 * cell_width;
        let mut underline_data = vec![0u8; width * cell_height * 4];

        let descender_row = (cell_height as isize + descender) as usize;
        let descender_plus_one = (1 + descender_row).min(cell_height - 1);
        let descender_plus_two = (2 + descender_row).min(cell_height - 1);
        let strike_row = descender_row / 2;

        // First, the single underline.
        // We place this just under the descender position.
        {
            let col = 0;
            let offset = ((width * 4) * descender_plus_one) + (col * 4 * cell_width);
            for i in 0..4 * cell_width {
                underline_data[offset + i] = 0xff;
            }
        }
        // Double underline,
        // We place this at and just below the descender
        {
            let col = 1;
            let offset_one = ((width * 4) * (descender_row)) + (col * 4 * cell_width);
            let offset_two = ((width * 4) * (descender_plus_two)) + (col * 4 * cell_width);
            for i in 0..4 * cell_width {
                underline_data[offset_one + i] = 0xff;
                underline_data[offset_two + i] = 0xff;
            }
        }
        // Strikethrough
        {
            let col = 2;
            let offset = (width * 4) * strike_row + (col * 4 * cell_width);
            for i in 0..4 * cell_width {
                underline_data[offset + i] = 0xff;
            }
        }
        // Strikethrough and single underline
        {
            let col = 3;
            let offset_one = ((width * 4) * descender_plus_one) + (col * 4 * cell_width);
            let offset_two = ((width * 4) * strike_row) + (col * 4 * cell_width);
            for i in 0..4 * cell_width {
                underline_data[offset_one + i] = 0xff;
                underline_data[offset_two + i] = 0xff;
            }
        }
        // Strikethrough and double underline
        {
            let col = 4;
            let offset_one = ((width * 4) * (descender_row)) + (col * 4 * cell_width);
            let offset_two = ((width * 4) * strike_row) + (col * 4 * cell_width);
            let offset_three = ((width * 4) * (descender_plus_two)) + (col * 4 * cell_width);
            for i in 0..4 * cell_width {
                underline_data[offset_one + i] = 0xff;
                underline_data[offset_two + i] = 0xff;
                underline_data[offset_three + i] = 0xff;
            }
        }

        glium::texture::SrgbTexture2d::new(
            facade,
            glium::texture::RawImage2d::from_raw_rgba(
                underline_data,
                (width as u32, cell_height as u32),
            ),
        )
    }

    pub fn resize<F: Facade>(&mut self, facade: &F, width: u16, height: u16) -> Result<(), Error> {
        debug!("Renderer resize {},{}", width, height);

        self.width = width;
        self.height = height;
        self.projection = Self::compute_projection(width as f32, height as f32);

        let (glyph_vertex_buffer, glyph_index_buffer) = Self::compute_vertices(
            facade,
            self.spritesheet.sprite_height + 1.0,
            self.cell_width as f32,
            self.cell_height as f32,
            width as f32,
            height as f32,
        )?;
        self.glyph_vertex_buffer = RefCell::new(glyph_vertex_buffer);
        self.glyph_index_buffer = glyph_index_buffer;

        self.reset_sprite_pos((height / 2) as f32);

        let (glyph_header_vertex_buffer, glyph_header_index_buffer) =
            Self::compute_header_text_vertices(
                facade,
                self.spritesheet.sprite_height,
                HEADER_WIDTH_PADDING,
                *CPU_LOAD_LENGTH,
                *CURRENT_TIME_LENGTH,
                width as f32,
                height as f32,
                self.header_cell_width as f32,
                self.header_cell_height as f32,
            )?;

        self.glyph_header_vertex_buffer = RefCell::new(glyph_header_vertex_buffer);
        self.glyph_header_index_buffer = glyph_header_index_buffer;

        let (rect_vertex_buffer, rect_index_buffer) = Self::compute_rect_vertices(
            facade,
            self.header_banner_color,
            self.spritesheet.sprite_height,
            width as f32,
            height as f32,
        );

        self.rect_vertex_buffer = RefCell::new(rect_vertex_buffer);
        self.rect_index_buffer = rect_index_buffer;

        Ok(())
    }

    pub fn reset_sprite_pos(&mut self, height: f32) {
        let mut vb = self.sprite_vertex_buffer.borrow_mut();
        let mut vert = { vb.slice_mut(0..4).unwrap().map() };

        vert[V_TOP_LEFT].position.0.y = -height;
        vert[V_TOP_RIGHT].position.0.y = -height;
        vert[V_BOT_LEFT].position.0.y = -height + self.spritesheet.sprite_height;
        vert[V_BOT_RIGHT].position.0.y = -height + self.spritesheet.sprite_height;
    }

    /// Resolve a glyph from the cache, rendering the glyph on-demand if
    /// the cache doesn't already hold the desired glyph.
    fn cached_glyph(&self, info: &GlyphInfo, style: &TextStyle) -> Result<Rc<CachedGlyph>, Error> {
        let key =
            GlyphKey { font_idx: info.font_idx, glyph_pos: info.glyph_pos, style: style.clone() };

        let mut cache = self.glyph_cache.borrow_mut();

        if let Some(entry) = cache.get(&key) {
            return Ok(Rc::clone(entry));
        }

        let glyph = self.load_glyph(info, style)?;
        cache.insert(key, Rc::clone(&glyph));
        Ok(glyph)
    }

    /// Perform the load and render of a glyph
    fn load_glyph(&self, info: &GlyphInfo, style: &TextStyle) -> Result<Rc<CachedGlyph>, Error> {
        let (has_color, glyph, cell_width, cell_height) = {
            let font = self.fonts.cached_font(style)?;
            let mut font = font.borrow_mut();
            let metrics = font.get_fallback(0)?.metrics();
            let active_font = font.get_fallback(info.font_idx)?;
            let has_color = active_font.has_color();
            let glyph = active_font.rasterize_glyph(info.glyph_pos)?;
            (has_color, glyph, metrics.cell_width, metrics.cell_height)
        };

        let scale = if (info.x_advance / f64::from(info.num_cells)).floor() > cell_width {
            f64::from(info.num_cells) * (cell_width / info.x_advance)
        } else if glyph.height as f64 > cell_height {
            cell_height / glyph.height as f64
        } else {
            1.0f64
        };
        #[cfg_attr(feature = "cargo-clippy", allow(clippy::float_cmp))]
        let (x_offset, y_offset) = if scale != 1.0 {
            (info.x_offset * scale, info.y_offset * scale)
        } else {
            (info.x_offset, info.y_offset)
        };

        let glyph = if glyph.width == 0 || glyph.height == 0 {
            // a whitespace glyph
            CachedGlyph {
                texture: None,
                has_color,
                x_offset,
                y_offset,
                bearing_x: 0.0,
                bearing_y: 0.0,
                scale,
            }
        } else {
            let raw_im = glium::texture::RawImage2d::from_raw_rgba(
                glyph.data,
                (glyph.width as u32, glyph.height as u32),
            );

            let tex =
                self.glyph_atlas.borrow_mut().allocate(raw_im.width, raw_im.height, raw_im)?;

            let bearing_x = glyph.bearing_x * scale;
            let bearing_y = glyph.bearing_y * scale;

            CachedGlyph {
                texture: Some(tex),
                has_color,
                x_offset,
                y_offset,
                bearing_x,
                bearing_y,
                scale,
            }
        };

        Ok(Rc::new(glyph))
    }

    /// Compute a vertex buffer to hold the quads that comprise the visible
    /// portion of the screen.   We recreate this when the screen is resized.
    /// The idea is that we want to minimize and heavy lifting and computation
    /// and instead just poke some attributes into the offset that corresponds
    /// to a changed cell when we need to repaint the screen, and then just
    /// let the GPU figure out the rest.
    fn compute_vertices<F: Facade>(
        facade: &F,
        top_padding: f32,
        cell_width: f32,
        cell_height: f32,
        width: f32,
        height: f32,
    ) -> Result<(VertexBuffer<Vertex>, IndexBuffer<u32>), Error> {
        let cell_width = cell_width.ceil();
        let cell_height = cell_height.ceil();
        let mut verts = Vec::new();
        let mut indices = Vec::new();

        let num_cols = (width as usize + 1) / cell_width as usize;
        let num_rows = (height as usize + 1) / cell_height as usize;

        for y in 0..num_rows {
            for x in 0..num_cols {
                let y_pos = (height / -2.0) + (y as f32 * cell_height);
                let x_pos = (width / -2.0) + (x as f32 * cell_width);
                // Remember starting index for this position
                let idx = verts.len() as u32;
                verts.push(Vertex {
                    // Top left
                    position: Point::new(x_pos, top_padding + y_pos),
                    v_idx: V_TOP_LEFT as f32,
                    ..Default::default()
                });
                verts.push(Vertex {
                    // Top Right
                    position: Point::new(x_pos + cell_width, top_padding + y_pos),
                    v_idx: V_TOP_RIGHT as f32,
                    ..Default::default()
                });
                verts.push(Vertex {
                    // Bottom Left
                    position: Point::new(x_pos, top_padding + y_pos + cell_height),
                    v_idx: V_BOT_LEFT as f32,
                    ..Default::default()
                });
                verts.push(Vertex {
                    // Bottom Right
                    position: Point::new(x_pos + cell_width, top_padding + y_pos + cell_height),
                    v_idx: V_BOT_RIGHT as f32,
                    ..Default::default()
                });

                // Emit two triangles to form the glyph quad
                indices.push(idx);
                indices.push(idx + 1);
                indices.push(idx + 2);
                indices.push(idx + 1);
                indices.push(idx + 2);
                indices.push(idx + 3);
            }
        }

        Ok((
            VertexBuffer::dynamic(facade, &verts)?,
            IndexBuffer::new(facade, glium::index::PrimitiveType::TrianglesList, &indices)?,
        ))
    }

    fn compute_header_text_vertices<F: Facade>(
        facade: &F,
        banner_height: f32,
        width_padding: f32,
        left_num_cols: usize,
        right_num_cols: usize,
        width: f32,
        height: f32,
        cell_width: f32,
        cell_height: f32,
    ) -> Result<(VertexBuffer<Vertex>, IndexBuffer<u32>), Error> {
        let mut verts = Vec::new();
        let mut indices = Vec::new();

        let top_padding = ((banner_height - cell_height) / 2.0) + 3.0;
        let y_pos = (height / -2.0) + top_padding;
        for x in 0..(left_num_cols + right_num_cols) {
            let x_pos = if x < left_num_cols {
                (width / -2.0) + width_padding + (x as f32 * cell_width)
            } else {
                (width / 2.0)
                    - width_padding
                    - ((left_num_cols + right_num_cols - x) as f32 * cell_width)
                    + 5.0
            };
            // Remember starting index for this position
            let idx = verts.len() as u32;
            verts.push(Vertex {
                // Top left
                position: Point::new(x_pos, y_pos),
                v_idx: V_TOP_LEFT as f32,
                ..Default::default()
            });
            verts.push(Vertex {
                // Top Right
                position: Point::new(x_pos + cell_width, y_pos),
                v_idx: V_TOP_RIGHT as f32,
                ..Default::default()
            });
            verts.push(Vertex {
                // Bottom Left
                position: Point::new(x_pos, y_pos + cell_height),
                v_idx: V_BOT_LEFT as f32,
                ..Default::default()
            });
            verts.push(Vertex {
                // Bottom Right
                position: Point::new(x_pos + cell_width, y_pos + cell_height),
                v_idx: V_BOT_RIGHT as f32,
                ..Default::default()
            });

            // Emit two triangles to form the glyph quad
            indices.push(idx);
            indices.push(idx + 1);
            indices.push(idx + 2);
            indices.push(idx + 1);
            indices.push(idx + 2);
            indices.push(idx + 3);
        }
        Ok((
            VertexBuffer::dynamic(facade, &verts)?,
            IndexBuffer::new(facade, glium::index::PrimitiveType::TrianglesList, &indices)?,
        ))
    }

    pub fn compute_sprite_vertices<F: Facade>(
        facade: &F,
        sprite_width: f32,
        sprite_height: f32,
        width: f32,
        height: f32,
    ) -> (VertexBuffer<SpriteVertex>, IndexBuffer<u32>) {
        let mut verts = Vec::new();

        let (w, h) = { (width / 2.0, height / 2.0) };

        verts.push(SpriteVertex {
            // Top left
            tex_coords: Point::new(0.0, 1.0),
            position: Point::new(-w, -h),
            ..Default::default()
        });
        verts.push(SpriteVertex {
            // Top Right
            tex_coords: Point::new(1.0, 1.0),
            position: Point::new(-w + sprite_width, -h),
            ..Default::default()
        });
        verts.push(SpriteVertex {
            // Bottom Left
            tex_coords: Point::new(0.0, 0.0),
            position: Point::new(-w, -h + sprite_height),
            ..Default::default()
        });
        verts.push(SpriteVertex {
            // Bottom Right
            tex_coords: Point::new(1.0, 0.0),
            position: Point::new(-w + sprite_width, -h + sprite_height),
            ..Default::default()
        });

        (
            VertexBuffer::dynamic(facade, &verts).unwrap(),
            IndexBuffer::new(
                facade,
                glium::index::PrimitiveType::TrianglesList,
                &[0, 1, 2, 1, 3, 2],
            )
            .unwrap(),
        )
    }

    pub fn compute_rect_vertices<F: Facade>(
        facade: &F,
        color: RgbaTuple,
        banner_height: f32,
        width: f32,
        height: f32,
    ) -> (VertexBuffer<RectVertex>, IndexBuffer<u32>) {
        let r = color.0;
        let g = color.1;
        let b = color.2;
        let mut verts = Vec::new();

        let (w, h) = ((width / 2.0), (height / 2.0));

        verts.push(RectVertex { position: [-w, -h], color: [r, g, b] });
        verts.push(RectVertex { position: [w, -h], color: [r, g, b] });
        verts.push(RectVertex { position: [-w, -h + banner_height], color: [r, g, b] });
        verts.push(RectVertex { position: [w, -h + banner_height], color: [r, g, b] });

        (
            VertexBuffer::dynamic(facade, &verts).unwrap(),
            IndexBuffer::new(
                facade,
                glium::index::PrimitiveType::TrianglesList,
                &[0, 1, 2, 1, 3, 2],
            )
            .unwrap(),
        )
    }

    pub fn paint_sprite(&mut self, target: &mut glium::Frame) -> Result<(), Error> {
        let number_of_sprites = self.spritesheet.sprites.len();
        let sprite =
            &mut self.spritesheet.sprites[(self.frame_count % number_of_sprites as u32) as usize];
        let w = self.width as f32 / 2.0;

        // Draw mario
        target.draw(
            &*self.sprite_vertex_buffer.borrow(),
            &self.sprite_index_buffer,
            &self.p_program,
            &uniform! {
                projection: self.projection.to_arrays(),
                tex: &self.player_texture.tex,
                source_dimensions: sprite.size.to_array(),
                source_position: sprite.position.to_array(),
                source_texture_dimensions: [self.player_texture.width, self.player_texture.height]
            },
            &glium::DrawParameters { blend: glium::Blend::alpha_blending(), ..Default::default() },
        )?;

        self.slide_sprite(w);
        Ok(())
    }

    pub fn slide_sprite(&mut self, width: f32) {
        let mut vb = self.sprite_vertex_buffer.borrow_mut();
        let mut vert = { vb.slice_mut(0..4).unwrap().map() };

        let delta = Point::new(10.0, 0.0);

        let sprite_width = self.spritesheet.sprite_width;

        if vert[V_TOP_LEFT].position.0.x > width {
            vert[V_TOP_LEFT].position.0.x = -width;
            vert[V_TOP_RIGHT].position.0.x = -width + sprite_width;
            vert[V_BOT_LEFT].position.0.x = -width;
            vert[V_BOT_RIGHT].position.0.x = -width + sprite_width;
        } else {
            vert[V_TOP_LEFT].position += delta;
            vert[V_TOP_RIGHT].position += delta;
            vert[V_BOT_LEFT].position += delta;
            vert[V_BOT_RIGHT].position += delta;
        }
    }

    /// The projection corrects for the aspect ratio and flips the y-axis
    fn compute_projection(width: f32, height: f32) -> Transform3D {
        Transform3D::ortho(-width / 2.0, width / 2.0, height / 2.0, -height / 2.0, -1.0, 1.0)
    }

    fn render_screen_line(
        &self,
        line_idx: usize,
        line: &Line,
        selection: Range<usize>,
        cursor: &CursorPosition,
        terminal: &term::Terminal,
    ) -> Result<(), Error> {
        let num_cols = terminal.screen().physical_cols;
        let mut vb = self.glyph_vertex_buffer.borrow_mut();
        let mut vertices = {
            let per_line = num_cols * VERTICES_PER_CELL;
            let start_pos = line_idx * per_line;
            vb.slice_mut(start_pos..start_pos + per_line)
                .ok_or_else(|| format_err!("we're confused about the screen size"))?
                .map()
        };

        let current_highlight = terminal.current_highlight();

        // Break the line into clusters of cells with the same attributes
        let cell_clusters = line.cluster();
        let mut last_cell_idx = 0;
        for cluster in cell_clusters {
            let attrs = &cluster.attrs;
            let is_highlited_hyperlink = match (&attrs.hyperlink, &current_highlight) {
                (&Some(ref this), &Some(ref highlight)) => this == highlight,
                _ => false,
            };
            let style = self.fonts.match_style(attrs);

            let bg_color = self.palette.resolve(&attrs.background);
            let fg_color = self.palette.resolve(&attrs.foreground);

            let (fg_color, bg_color) = {
                let mut fg = fg_color;
                let mut bg = bg_color;

                if attrs.reverse() {
                    mem::swap(&mut fg, &mut bg);
                }

                (fg, bg)
            };

            let glyph_color = fg_color.to_linear_tuple_rgba();
            let bg_color = bg_color.to_linear_tuple_rgba();

            // Shape the printable text from this cluster
            let glyph_info = {
                let font = self.fonts.cached_font(style)?;
                let mut font = font.borrow_mut();
                font.shape(&cluster.text)?
            };

            for info in &glyph_info {
                let cell_idx = cluster.byte_to_cell_idx[info.cluster as usize];
                let glyph = self.cached_glyph(info, style)?;

                let left = (glyph.x_offset + glyph.bearing_x) as f32;
                let top = ((self.cell_height + self.descender) - (glyph.y_offset + glyph.bearing_y))
                    as f32;

                // underline and strikethrough
                // Figure out what we're going to draw for the underline.
                // If the current cell is part of the current URL highlight
                // then we want to show the underline.
                #[cfg_attr(feature = "cargo-clippy", allow(clippy::match_same_arms))]
                let underline: f32 =
                    match (is_highlited_hyperlink, attrs.strikethrough(), attrs.underline()) {
                        (true, false, Underline::None) => U_ONE,
                        (true, false, Underline::Single) => U_TWO,
                        (true, false, Underline::Double) => U_ONE,
                        (true, true, Underline::None) => U_STRIKE_ONE,
                        (true, true, Underline::Single) => U_STRIKE_TWO,
                        (true, true, Underline::Double) => U_STRIKE_ONE,
                        (false, false, Underline::None) => U_NONE,
                        (false, false, Underline::Single) => U_ONE,
                        (false, false, Underline::Double) => U_TWO,
                        (false, true, Underline::None) => U_STRIKE,
                        (false, true, Underline::Single) => U_STRIKE_ONE,
                        (false, true, Underline::Double) => U_STRIKE_TWO,
                    };

                // Iterate each cell that comprises this glyph.  There is usually
                // a single cell per glyph but combining characters, ligatures
                // and emoji can be 2 or more cells wide.
                for glyph_idx in 0..info.num_cells as usize {
                    let cell_idx = cell_idx + glyph_idx;

                    if cell_idx >= num_cols {
                        // terminal line data is wider than the window.
                        // This happens for example while live resizing the window
                        // smaller than the terminal.
                        break;
                    }
                    last_cell_idx = cell_idx;

                    let (glyph_color, bg_color) = self.compute_cell_fg_bg(
                        line_idx,
                        cell_idx,
                        cursor,
                        &selection,
                        glyph_color,
                        bg_color,
                    );

                    let vert_idx = cell_idx * VERTICES_PER_CELL;
                    let vert = &mut vertices[vert_idx..vert_idx + VERTICES_PER_CELL];

                    vert[V_TOP_LEFT].fg_color = glyph_color;
                    vert[V_TOP_RIGHT].fg_color = glyph_color;
                    vert[V_BOT_LEFT].fg_color = glyph_color;
                    vert[V_BOT_RIGHT].fg_color = glyph_color;

                    vert[V_TOP_LEFT].bg_color = bg_color;
                    vert[V_TOP_RIGHT].bg_color = bg_color;
                    vert[V_BOT_LEFT].bg_color = bg_color;
                    vert[V_BOT_RIGHT].bg_color = bg_color;

                    vert[V_TOP_LEFT].underline = underline;
                    vert[V_TOP_RIGHT].underline = underline;
                    vert[V_BOT_LEFT].underline = underline;
                    vert[V_BOT_RIGHT].underline = underline;

                    match glyph.texture {
                        Some(ref texture) => {
                            let slice = SpriteSlice {
                                cell_idx: glyph_idx,
                                num_cells: info.num_cells as usize,
                                cell_width: self.cell_width.ceil() as usize,
                                scale: glyph.scale as f32,
                                left_offset: left,
                            };

                            // How much of the width of this glyph we can use here
                            let slice_width = texture.slice_width(&slice);

                            let left = if glyph_idx == 0 { left } else { 0.0 };
                            let right = (slice_width as f32 + left) - self.cell_width as f32;

                            let bottom = (texture.coords.height as f32 * glyph.scale as f32 + top)
                                - self.cell_height as f32;

                            vert[V_TOP_LEFT].tex = texture.top_left(&slice);
                            vert[V_TOP_LEFT].adjust = Point::new(left, top);

                            vert[V_TOP_RIGHT].tex = texture.top_right(&slice);
                            vert[V_TOP_RIGHT].adjust = Point::new(right, top);

                            vert[V_BOT_LEFT].tex = texture.bottom_left(&slice);
                            vert[V_BOT_LEFT].adjust = Point::new(left, bottom);

                            vert[V_BOT_RIGHT].tex = texture.bottom_right(&slice);
                            vert[V_BOT_RIGHT].adjust = Point::new(right, bottom);

                            let has_color = if glyph.has_color { 1.0 } else { 0.0 };
                            vert[V_TOP_LEFT].has_color = has_color;
                            vert[V_TOP_RIGHT].has_color = has_color;
                            vert[V_BOT_LEFT].has_color = has_color;
                            vert[V_BOT_RIGHT].has_color = has_color;
                        }
                        None => {
                            // Whitespace; no texture to render
                            let zero = (0.0, 0.0f32);

                            // Note: these 0 coords refer to the blank pixel
                            // in the bottom left of the underline texture!
                            vert[V_TOP_LEFT].tex = zero;
                            vert[V_TOP_RIGHT].tex = zero;
                            vert[V_BOT_LEFT].tex = zero;
                            vert[V_BOT_RIGHT].tex = zero;

                            vert[V_TOP_LEFT].adjust = Default::default();
                            vert[V_TOP_RIGHT].adjust = Default::default();
                            vert[V_BOT_LEFT].adjust = Default::default();
                            vert[V_BOT_RIGHT].adjust = Default::default();

                            vert[V_TOP_LEFT].has_color = 0.0;
                            vert[V_TOP_RIGHT].has_color = 0.0;
                            vert[V_BOT_LEFT].has_color = 0.0;
                            vert[V_BOT_RIGHT].has_color = 0.0;
                        }
                    }
                }
            }
        }

        // Clear any remaining cells to the right of the clusters we
        // found above, otherwise we leave artifacts behind.  The easiest
        // reproduction for the artifacts is to maximize the window and
        // open a vim split horizontally.  Backgrounding vim would leave
        // the right pane with its prior contents instead of showing the
        // cleared lines from the shell in the main screen.

        for cell_idx in last_cell_idx + 1..num_cols {
            let vert_idx = cell_idx * VERTICES_PER_CELL;
            let vert_slice = &mut vertices[vert_idx..vert_idx + 4];

            // Even though we don't have a cell for these, they still
            // hold the cursor or the selection so we need to compute
            // the colors in the usual way.
            let (glyph_color, bg_color) = self.compute_cell_fg_bg(
                line_idx,
                cell_idx,
                cursor,
                &selection,
                self.palette.foreground.to_linear_tuple_rgba(),
                self.palette.background.to_linear_tuple_rgba(),
            );

            for vert in vert_slice.iter_mut() {
                vert.bg_color = bg_color;
                vert.fg_color = glyph_color;
                vert.underline = U_NONE;
                // Note: these 0 coords refer to the blank pixel
                // in the bottom left of the underline texture!
                vert.tex = (0.0, 0.0);
                vert.adjust = Default::default();
                vert.has_color = 0.0;
            }
        }

        Ok(())
    }

    fn compute_cell_fg_bg(
        &self,
        line_idx: usize,
        cell_idx: usize,
        cursor: &CursorPosition,
        selection: &Range<usize>,
        fg_color: RgbaTuple,
        bg_color: RgbaTuple,
    ) -> (RgbaTuple, RgbaTuple) {
        let selected = selection.contains(&cell_idx);
        let is_cursor = line_idx as i64 == cursor.y && cursor.x == cell_idx;

        let (fg_color, bg_color) = match (selected, is_cursor) {
            // Normally, render the cell as configured
            (false, false) => (fg_color, bg_color),
            // Cursor cell overrides colors
            (_, true) => (
                self.palette.background.to_linear_tuple_rgba(),
                self.palette.cursor.to_linear_tuple_rgba(),
            ),
            // Selected text overrides colors
            (true, false) => (fg_color, self.palette.cursor.to_linear_tuple_rgba()),
        };

        (fg_color, bg_color)
    }

    pub fn render_header_text(&mut self) -> Result<(), Error> {
        let now: DateTime<Utc> = Utc::now();
        let current_time = now.format("%H:%M:%S").to_string();
        let cpu_load = self.sys.get_global_processor_info().get_cpu_usage();
        let mut vb = self.glyph_header_vertex_buffer.borrow_mut();
        let mut vertices = vb
            .slice_mut(..)
            .ok_or_else(|| format_err!("we're confused about the screen size"))?
            .map();

        let glyph_info = {
            let font = self.fonts.cached_font(&self.header_text_style)?;
            let mut font = font.borrow_mut();
            font.shape(&format!("CPU:{:02}%{}", cpu_load.round(), current_time))?
        };
        let glyph_color = self
            .palette
            .resolve(&term::color::ColorAttribute::PaletteIndex(15))
            .to_linear_tuple_rgba();
        //let glyph_color = term::color::RgbColor::new(163, 66, 15).to_linear_tuple_rgba();
        let bg_color = self.palette.background.to_linear_tuple_rgba();

        let cell_width = self.header_cell_width as f32;
        let cell_height = self.header_cell_height as f32;

        for (i, info) in glyph_info.iter().enumerate() {
            let glyph = self.cached_glyph(info, &self.header_text_style)?;
            let left: f32 = glyph.x_offset as f32 + glyph.bearing_x as f32;
            let top = (self.cell_height as f32 + self.header_cell_descender as f32)
                - (glyph.y_offset as f32 + glyph.bearing_y as f32);
            let underline: f32 = U_NONE;
            let vert_idx = i * VERTICES_PER_CELL;
            let vert = &mut vertices[vert_idx..vert_idx + VERTICES_PER_CELL];

            vert[V_TOP_LEFT].fg_color = glyph_color;
            vert[V_TOP_RIGHT].fg_color = glyph_color;
            vert[V_BOT_LEFT].fg_color = glyph_color;
            vert[V_BOT_RIGHT].fg_color = glyph_color;

            vert[V_TOP_LEFT].bg_color = bg_color;
            vert[V_TOP_RIGHT].bg_color = bg_color;
            vert[V_BOT_LEFT].bg_color = bg_color;
            vert[V_BOT_RIGHT].bg_color = bg_color;

            vert[V_TOP_LEFT].underline = underline;
            vert[V_TOP_RIGHT].underline = underline;
            vert[V_BOT_LEFT].underline = underline;
            vert[V_BOT_RIGHT].underline = underline;

            match &glyph.texture {
                &Some(ref texture) => {
                    let slice = SpriteSlice {
                        cell_idx: 0,
                        num_cells: info.num_cells as usize,
                        cell_width: self.header_cell_width,
                        scale: glyph.scale as f32,
                        left_offset: left,
                    };

                    // How much of the width of this glyph we can use here
                    let slice_width = texture.slice_width(&slice);
                    let right = (slice_width as f32 + left) - cell_width;

                    let bottom =
                        ((texture.coords.height as f32) * glyph.scale as f32 + top) - cell_height;

                    vert[V_TOP_LEFT].tex = texture.top_left(&slice);
                    vert[V_TOP_LEFT].adjust = Point::new(left, top);

                    vert[V_TOP_RIGHT].tex = texture.top_right(&slice);
                    vert[V_TOP_RIGHT].adjust = Point::new(right, top);

                    vert[V_BOT_LEFT].tex = texture.bottom_left(&slice);
                    vert[V_BOT_LEFT].adjust = Point::new(left, bottom);

                    vert[V_BOT_RIGHT].tex = texture.bottom_right(&slice);
                    vert[V_BOT_RIGHT].adjust = Point::new(right, bottom);

                    let has_color = if glyph.has_color { 1.0 } else { 0.0 };
                    vert[V_TOP_LEFT].has_color = has_color;
                    vert[V_TOP_RIGHT].has_color = has_color;
                    vert[V_BOT_LEFT].has_color = has_color;
                    vert[V_BOT_RIGHT].has_color = has_color;
                }
                &None => {
                    // Whitespace; no texture to render
                    let zero = (0.0, 0.0f32);

                    vert[V_TOP_LEFT].tex = zero;
                    vert[V_TOP_RIGHT].tex = zero;
                    vert[V_BOT_LEFT].tex = zero;
                    vert[V_BOT_RIGHT].tex = zero;

                    vert[V_TOP_LEFT].adjust = Default::default();
                    vert[V_TOP_RIGHT].adjust = Default::default();
                    vert[V_BOT_LEFT].adjust = Default::default();
                    vert[V_BOT_RIGHT].adjust = Default::default();

                    vert[V_TOP_LEFT].has_color = 0.0;
                    vert[V_TOP_RIGHT].has_color = 0.0;
                    vert[V_BOT_LEFT].has_color = 0.0;
                    vert[V_BOT_RIGHT].has_color = 0.0;
                }
            }
        }
        Ok(())
    }

    pub fn paint(
        &mut self,
        target: &mut glium::Frame,
        term: &mut term::Terminal,
    ) -> Result<(), Error> {
        let background_color = self.palette.resolve(&term::color::ColorAttribute::Background);
        let (r, g, b, a) = background_color.to_linear_tuple_rgba();
        target.clear_color(r, g, b, a);

        let cursor = term.cursor_pos();
        {
            let dirty_lines = term.get_dirty_lines();

            for (line_idx, line, selrange) in dirty_lines {
                self.render_screen_line(line_idx, line, selrange, &cursor, term)?;
            }
        }
        self.render_header_text()?;

        let tex = self.glyph_atlas.borrow().texture();

        // Pass 1: Draw backgrounds, strikethrough and underline
        target.draw(
            &*self.glyph_vertex_buffer.borrow(),
            &self.glyph_index_buffer,
            &self.g_program,
            &uniform! {
                projection: self.projection.to_arrays(),
                glyph_tex: &*tex,
                bg_and_line_layer: true,
                underline_tex: &self.underline_tex,
            },
            &glium::DrawParameters { blend: glium::Blend::alpha_blending(), ..Default::default() },
        )?;

        // Pass 2: Draw glyphs
        target.draw(
            &*self.glyph_vertex_buffer.borrow(),
            &self.glyph_index_buffer,
            &self.g_program,
            &uniform! {
                projection: self.projection.to_arrays(),
                glyph_tex: &*tex,
                bg_and_line_layer: false,
            },
            &glium::DrawParameters { blend: glium::Blend::alpha_blending(), ..Default::default() },
        )?;

        term.clean_dirty_lines();

        // Draw header background
        target.draw(
            &*self.rect_vertex_buffer.borrow(),
            &self.rect_index_buffer,
            &self.r_program,
            &uniform! {
                projection: self.projection.to_arrays(),
            },
            &glium::DrawParameters { blend: glium::Blend::alpha_blending(), ..Default::default() },
        )?;

        // Pass 3: Draw glyphs header
        target.draw(
            &*self.glyph_header_vertex_buffer.borrow(),
            &self.glyph_header_index_buffer,
            &self.g_program,
            &uniform! {
                projection: self.projection.to_arrays(),
                glyph_tex: &*tex,
                bg_fill: false,
                underlining: false,
            },
            &glium::DrawParameters { blend: glium::Blend::alpha_blending(), ..Default::default() },
        )?;

        Ok(())
    }

    pub fn scaling_changed(&mut self, fonts: &Rc<FontConfiguration>) -> Result<(), Error> {
        let metrics = fonts.default_font_metrics()?;
        self.cell_height = metrics.cell_height;
        self.cell_width = metrics.cell_width;
        self.descender = metrics.descender;
        Ok(())
    }
}

pub fn get_spritesheet(path: &str) -> SpriteSheet {
    let spritesheet_config = SpriteSheetConfig::load(path).unwrap();
    SpriteSheet::from_config(&spritesheet_config)
}
