use crate::config::SpriteSheetConfig;
use glium::texture::CompressedSrgbTexture2d;

pub struct SpriteSheetTexture {
    pub tex: CompressedSrgbTexture2d,
    pub width: f32,
    pub height: f32,
}

pub struct Sprite {
    pub size: (f32, f32),
    pub position: (f32, f32),
}

pub struct SpriteSheet {
    pub image_path: String,
    pub sprites: Vec<Sprite>,
    pub sprite_height: f32,
    pub sprite_width: f32,
}

impl SpriteSheet {
    pub fn from_config(config: &SpriteSheetConfig) -> Self {
        let mut sprites = Vec::new();

        for (_, sprite) in &config.sheets {
            sprites.push(Sprite {
                size: (sprite.frame.w as f32, sprite.frame.h as f32),
                position: (sprite.frame.x as f32, sprite.frame.y as f32),
            });
        }

        let sprite_width = sprites[0].size.0;
        let sprite_height = sprites[0].size.1;
        SpriteSheet {
            image_path: String::from(format!(
                "{}/{}",
                env!("CARGO_MANIFEST_DIR"),
                config.image_path
            )),
            sprites,
            sprite_height,
            sprite_width,
        }
    }
}

pub fn get_spritesheet(path: &str) -> SpriteSheet {
    let spritesheet_config = SpriteSheetConfig::load(path).unwrap();
    SpriteSheet::from_config(&spritesheet_config)
}
