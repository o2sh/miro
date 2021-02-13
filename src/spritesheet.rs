use crate::config::SpriteSheetConfig;
use crate::xgfx::Window;
use crate::xwin::Point;
use glium::{self, IndexBuffer, VertexBuffer};

pub const SPRITE_SIZE: f32 = 32.0;

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
    pub fn from_config(config: &SpriteSheetConfig) -> Self {
        let mut sprites = Vec::new();

        for (_, sprite) in &config.sheets {
            sprites.push(Sprite {
                size: Point::new(sprite.frame.w as f32, sprite.frame.h as f32),
                position: Point::new(sprite.frame.x as f32, sprite.frame.y as f32),
            });
        }

        SpriteSheet { image_path: String::from(&config.image_path), sprites }
    }
}

pub fn compute_sprite_vertices(
    window: &Window,
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
        position: Point::new(-w + SPRITE_SIZE, -h),
        ..Default::default()
    });
    verts.push(SpriteVertex {
        // Bottom Left
        tex_coords: Point::new(0.0, 0.0),
        position: Point::new(-w, -h + SPRITE_SIZE),
        ..Default::default()
    });
    verts.push(SpriteVertex {
        // Bottom Right
        tex_coords: Point::new(1.0, 0.0),
        position: Point::new(-w + SPRITE_SIZE, -h + SPRITE_SIZE),
        ..Default::default()
    });

    (
        VertexBuffer::dynamic(window, &verts).unwrap(),
        IndexBuffer::new(window, glium::index::PrimitiveType::TrianglesList, &[0, 1, 2, 1, 3, 2])
            .unwrap(),
    )
}
