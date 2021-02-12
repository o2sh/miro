use crate::config::SpriteSheetConfig;
use crate::glium::backend::Backend;
use crate::xgfx::Window;
use crate::xwin::Point;
use glium::{self, IndexBuffer, VertexBuffer};
use std::cell::RefCell;

const V_TOP_LEFT: usize = 0;
const V_TOP_RIGHT: usize = 1;
const V_BOT_LEFT: usize = 2;
const V_BOT_RIGHT: usize = 3;

#[derive(Copy, Clone, Debug, Default)]
pub struct SpriteVertex {
    pub position: Point,
    tex_coords: Point,
}

implement_vertex!(SpriteVertex, position, tex_coords);

pub struct Sprite {
    pub size: Point,
    pub position: Point,
}

pub struct SpriteSheet {
    pub image_path: String,
    pub sprites: Vec<Sprite>,
}

impl SpriteSheet {
    pub fn from_config(window: &Window, config: &SpriteSheetConfig) -> Self {
        let mut sprites = Vec::new();
        let (w, h) = {
            let (width, height) = window.gl.get_framebuffer_dimensions();
            ((width / 2) as f32, (height / 2) as f32)
        };
        for (_, sprite) in &config.sheets {
            sprites.push(Sprite {
                size: Point::new(sprite.frame.w as f32, sprite.frame.h as f32),
                position: Point::new(sprite.frame.x as f32, sprite.frame.y as f32),
            });
        }

        SpriteSheet { image_path: String::from(&config.image_path), sprites }
    }
}

pub fn compute_player_vertices(
    window: &Window,
) -> (RefCell<VertexBuffer<SpriteVertex>>, IndexBuffer<u32>) {
    let mut verts = Vec::new();

    let (w, h) = {
        let (width, height) = window.gl.get_framebuffer_dimensions();
        ((width / 2) as f32, (height / 2) as f32)
    };

    let size = 32.0;

    verts.push(SpriteVertex {
        // Top left
        tex_coords: Point::new(0.0, 0.0),
        position: Point::new(-w, -h + size),
        ..Default::default()
    });
    verts.push(SpriteVertex {
        // Top Right
        tex_coords: Point::new(1.0, 0.0),
        position: Point::new(-w + size, -h + size),
        ..Default::default()
    });
    verts.push(SpriteVertex {
        // Bottom Left
        tex_coords: Point::new(0.0, 1.0),
        position: Point::new(-w, -h),
        ..Default::default()
    });
    verts.push(SpriteVertex {
        // Bottom Right
        tex_coords: Point::new(1.0, 1.0),
        position: Point::new(-w + size, -h),
        ..Default::default()
    });

    (
        RefCell::new(VertexBuffer::dynamic(window, &verts).unwrap()),
        IndexBuffer::new(window, glium::index::PrimitiveType::TrianglesList, &[0, 1, 2, 1, 3, 2])
            .unwrap(),
    )
}
