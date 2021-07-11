use crate::window::bitmaps::TextureRect;
use crate::window::Color;
use glium::VertexBuffer;
use std::cell::RefMut;

pub const VERTICES_PER_CELL: usize = 4;
pub const V_TOP_LEFT: usize = 0;
pub const V_TOP_RIGHT: usize = 1;
pub const V_BOT_LEFT: usize = 2;
pub const V_BOT_RIGHT: usize = 3;

#[derive(Copy, Clone, Default)]
pub struct Vertex {
    pub position: (f32, f32),
    pub adjust: (f32, f32),
    pub tex: (f32, f32),
    pub underline: (f32, f32),
    pub bg_color: (f32, f32, f32, f32),
    pub cursor: (f32, f32),
    pub cursor_color: (f32, f32, f32, f32),
    pub fg_color: (f32, f32, f32, f32),
    pub has_color: f32,
}

glium::implement_vertex!(
    Vertex,
    position,
    adjust,
    tex,
    underline,
    cursor,
    cursor_color,
    bg_color,
    fg_color,
    has_color
);

#[derive(Copy, Clone, Debug, Default)]
pub struct SpriteVertex {
    pub position: (f32, f32),
    pub tex_coords: (f32, f32),
}

glium::implement_vertex!(SpriteVertex, position, tex_coords);

#[derive(Copy, Clone)]
pub struct RectVertex {
    pub position: (f32, f32),
    pub color: (f32, f32, f32, f32),
}

glium::implement_vertex!(RectVertex, position, color);

#[derive(Default, Debug, Clone)]
pub struct Quads {
    pub cols: usize,
    pub row_starts: Vec<usize>,
}

pub struct MappedQuads<'a> {
    mapping: glium::buffer::Mapping<'a, [Vertex]>,
    quads: Quads,
}

impl<'a> MappedQuads<'a> {
    pub fn cell<'b>(&'b mut self, x: usize, y: usize) -> anyhow::Result<Quad<'b>> {
        if x >= self.quads.cols {
            anyhow::bail!("column {} is outside of the vertex buffer range", x);
        }

        let start = self
            .quads
            .row_starts
            .get(y)
            .ok_or_else(|| anyhow::anyhow!("line {} is outside the vertex buffer range", y))?
            + x * VERTICES_PER_CELL;

        Ok(Quad { vert: &mut self.mapping[start..start + VERTICES_PER_CELL] })
    }

    pub fn cols(&self) -> usize {
        self.quads.cols * VERTICES_PER_CELL
    }
}

impl Quads {
    pub fn map<'a>(&self, vb: &'a mut RefMut<VertexBuffer<Vertex>>) -> MappedQuads<'a> {
        let mapping = vb.slice_mut(..).expect("to map vertex buffer").map();
        MappedQuads { mapping, quads: self.clone() }
    }
}

pub struct Quad<'a> {
    vert: &'a mut [Vertex],
}

impl<'a> Quad<'a> {
    pub fn set_texture(&mut self, coords: TextureRect) {
        self.vert[V_TOP_LEFT].tex = (coords.min_x(), coords.min_y());
        self.vert[V_TOP_RIGHT].tex = (coords.max_x(), coords.min_y());
        self.vert[V_BOT_LEFT].tex = (coords.min_x(), coords.max_y());
        self.vert[V_BOT_RIGHT].tex = (coords.max_x(), coords.max_y());
    }

    pub fn set_texture_adjust(&mut self, left: f32, top: f32, right: f32, bottom: f32) {
        self.vert[V_TOP_LEFT].adjust = (left, top);
        self.vert[V_TOP_RIGHT].adjust = (right, top);
        self.vert[V_BOT_LEFT].adjust = (left, bottom);
        self.vert[V_BOT_RIGHT].adjust = (right, bottom);
    }

    pub fn set_has_color(&mut self, has_color: bool) {
        let has_color = if has_color { 1. } else { 0. };
        for v in self.vert.iter_mut() {
            v.has_color = has_color;
        }
    }

    pub fn set_fg_color(&mut self, color: Color) {
        let color = color.to_tuple_rgba();
        for v in self.vert.iter_mut() {
            v.fg_color = color;
        }
    }

    pub fn set_bg_color(&mut self, color: Color) {
        let color = color.to_tuple_rgba();
        for v in self.vert.iter_mut() {
            v.bg_color = color;
        }
    }

    pub fn set_underline(&mut self, coords: TextureRect) {
        self.vert[V_TOP_LEFT].underline = (coords.min_x(), coords.min_y());
        self.vert[V_TOP_RIGHT].underline = (coords.max_x(), coords.min_y());
        self.vert[V_BOT_LEFT].underline = (coords.min_x(), coords.max_y());
        self.vert[V_BOT_RIGHT].underline = (coords.max_x(), coords.max_y());
    }

    pub fn set_cursor(&mut self, coords: TextureRect) {
        self.vert[V_TOP_LEFT].cursor = (coords.min_x(), coords.min_y());
        self.vert[V_TOP_RIGHT].cursor = (coords.max_x(), coords.min_y());
        self.vert[V_BOT_LEFT].cursor = (coords.min_x(), coords.max_y());
        self.vert[V_BOT_RIGHT].cursor = (coords.max_x(), coords.max_y());
    }

    pub fn set_cursor_color(&mut self, color: Color) {
        let color = color.to_tuple_rgba();
        for v in self.vert.iter_mut() {
            v.cursor_color = color;
        }
    }
}
