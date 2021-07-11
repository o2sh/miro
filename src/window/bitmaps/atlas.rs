use crate::window::bitmaps::{BitmapImage, Texture2d, TextureRect};
use crate::window::{Point, Rect, Size};
use anyhow::ensure;
use std::rc::Rc;
use thiserror::*;

#[derive(Debug, Error)]
#[error("Texture Size exceeded, need {}", size)]
pub struct OutOfTextureSpace {
    pub size: usize,
}

pub struct Atlas<T>
where
    T: Texture2d,
{
    texture: Rc<T>,
    side: usize,
    bottom: usize,
    tallest: usize,
    left: usize,
}

impl<T> Atlas<T>
where
    T: Texture2d,
{
    pub fn new(texture: &Rc<T>) -> anyhow::Result<Self> {
        ensure!(texture.width() == texture.height(), "texture must be square!");
        Ok(Self {
            texture: Rc::clone(texture),
            side: texture.width(),
            bottom: 0,
            tallest: 0,
            left: 0,
        })
    }

    #[inline]
    pub fn texture(&self) -> Rc<T> {
        Rc::clone(&self.texture)
    }

    pub fn allocate(
        &mut self,
        im: &dyn BitmapImage,
    ) -> anyhow::Result<Sprite<T>, OutOfTextureSpace> {
        let (width, height) = im.image_dimensions();

        let reserve_width = width + 2;
        let reserve_height = height + 2;

        if reserve_width > self.side || reserve_height > self.side {
            return Err(OutOfTextureSpace {
                size: reserve_width.max(reserve_height).next_power_of_two(),
            });
        }
        let x_left = self.side - self.left;
        if x_left < reserve_width {
            self.bottom += self.tallest;
            self.left = 0;
            self.tallest = 0;
        }

        let y_left = self.side - self.bottom;
        if y_left < reserve_height {
            return Err(OutOfTextureSpace {
                size: (self.side + reserve_width.max(reserve_height)).next_power_of_two(),
            });
        }

        let rect = Rect::new(
            Point::new(self.left as isize + 1, self.bottom as isize + 1),
            Size::new(width as isize, height as isize),
        );

        self.texture.write(rect, im);

        self.left += reserve_width;
        self.tallest = self.tallest.max(reserve_height);

        Ok(Sprite { texture: Rc::clone(&self.texture), coords: rect })
    }
    pub fn size(&self) -> usize {
        self.side
    }
}

pub struct Sprite<T>
where
    T: Texture2d,
{
    pub texture: Rc<T>,
    pub coords: Rect,
}

impl<T> Clone for Sprite<T>
where
    T: Texture2d,
{
    fn clone(&self) -> Self {
        Self { texture: Rc::clone(&self.texture), coords: self.coords }
    }
}

impl<T> Sprite<T>
where
    T: Texture2d,
{
    pub fn texture_coords(&self) -> TextureRect {
        self.texture.to_texture_coords(self.coords)
    }
}

pub struct SpriteSlice {
    pub cell_idx: usize,
    pub num_cells: usize,
    pub cell_width: usize,
    pub scale: f32,
    pub left_offset: f32,
}

impl SpriteSlice {
    pub fn pixel_rect<T: Texture2d>(&self, sprite: &Sprite<T>) -> Rect {
        let width = self.slice_width(sprite) as isize;
        let left = self.left_pix(sprite) as isize;

        Rect::new(
            Point::new(sprite.coords.origin.x + left, sprite.coords.origin.y),
            Size::new(width, sprite.coords.size.height),
        )
    }

    pub fn left_pix<T: Texture2d>(&self, sprite: &Sprite<T>) -> f32 {
        let width = sprite.coords.size.width as f32 * self.scale;
        if self.num_cells == 1 || self.cell_idx == 0 {
            0.0
        } else {
            let cell_0 = width.min((self.cell_width as f32) - self.left_offset);

            if self.cell_idx == self.num_cells - 1 {
                let middle = self.cell_width * (self.num_cells - 2);
                cell_0 + middle as f32
            } else {
                let prev = self.cell_width * self.cell_idx;
                cell_0 + prev as f32
            }
        }
    }

    pub fn slice_width<T: Texture2d>(&self, sprite: &Sprite<T>) -> f32 {
        let width = sprite.coords.size.width as f32 * self.scale;

        if self.num_cells == 1 {
            width
        } else if self.cell_idx == 0 {
            width.min((self.cell_width as f32) - self.left_offset)
        } else if self.cell_idx == self.num_cells - 1 {
            width - self.left_pix(sprite)
        } else {
            self.cell_width as f32
        }
    }
}
