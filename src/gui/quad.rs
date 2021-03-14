use crate::window::bitmaps::TextureRect;
use crate::window::*;

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
    pub fg_color: (f32, f32, f32, f32),

    pub has_color: f32,
}
glium::implement_vertex!(Vertex, position, adjust, tex, underline, bg_color, fg_color, has_color);

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

pub struct Quad<'a> {
    vert: &'a mut [Vertex],
}

impl<'a> Quad<'a> {
    pub fn for_cell(cell_idx: usize, vertices: &'a mut [Vertex]) -> Self {
        let vert_idx = cell_idx * VERTICES_PER_CELL;
        let vert = &mut vertices[vert_idx..vert_idx + VERTICES_PER_CELL];
        Self { vert }
    }

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
}
