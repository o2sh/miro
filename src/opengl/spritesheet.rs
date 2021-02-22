use crate::config::SpriteSheetConfig;
use crate::x_window::Point;
use glium::texture::CompressedSrgbTexture2d;

pub struct SpriteSheetTexture {
    pub tex: CompressedSrgbTexture2d,
    pub width: f32,
    pub height: f32,
}

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
