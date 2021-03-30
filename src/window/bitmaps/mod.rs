use crate::window::color::Color;
use crate::window::{Operator, Point, Rect};
use glium::texture::SrgbTexture2d;
use palette::LinSrgba;
use rgb::FromSlice;

pub mod atlas;

pub struct TextureUnit;
pub type TextureCoord = euclid::Point2D<f32, TextureUnit>;
pub type TextureRect = euclid::Rect<f32, TextureUnit>;
pub type TextureSize = euclid::Size2D<f32, TextureUnit>;

pub trait Texture2d {
    fn write(&self, rect: Rect, im: &dyn BitmapImage);
    fn read(&self, rect: Rect, im: &mut dyn BitmapImage);
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn to_texture_coords(&self, coords: Rect) -> TextureRect {
        let coords = coords.to_f32();
        let width = self.width() as f32;
        let height = self.height() as f32;
        TextureRect::new(
            TextureCoord::new(coords.min_x() / width, coords.min_y() / height),
            TextureSize::new(coords.size.width / width, coords.size.height / height),
        )
    }
}

impl Texture2d for SrgbTexture2d {
    fn write(&self, rect: Rect, im: &dyn BitmapImage) {
        let (im_width, im_height) = im.image_dimensions();

        let source = glium::texture::RawImage2d {
            data: im
                .pixels()
                .iter()
                .map(|&p| {
                    let (r, g, b, a) = Color(p).as_rgba();

                    fn conv(v: u8) -> u8 {
                        let f = (v as f32) / 255.;
                        let c = if f <= 0.0031308 {
                            f * 12.92
                        } else {
                            f.powf(1.0 / 2.4) * 1.055 - 0.055
                        };
                        (c * 255.).ceil() as u8
                    }
                    Color::rgba(conv(b), conv(g), conv(r), conv(a)).0
                })
                .collect(),
            width: im_width as u32,
            height: im_height as u32,
            format: glium::texture::ClientFormat::U8U8U8U8,
        };

        SrgbTexture2d::write(
            self,
            glium::Rect {
                left: rect.min_x() as u32,
                bottom: rect.min_y() as u32,
                width: rect.size.width as u32,
                height: rect.size.height as u32,
            },
            source,
        )
    }

    fn read(&self, _rect: Rect, _im: &mut dyn BitmapImage) {
        unimplemented!();
    }

    fn width(&self) -> usize {
        SrgbTexture2d::width(self) as usize
    }

    fn height(&self) -> usize {
        SrgbTexture2d::height(self) as usize
    }
}

pub trait BitmapImage {
    unsafe fn pixel_data(&self) -> *const u8;
    unsafe fn pixel_data_mut(&mut self) -> *mut u8;
    fn image_dimensions(&self) -> (usize, usize);

    #[inline]
    fn pixels(&self) -> &[u32] {
        let (width, height) = self.image_dimensions();
        unsafe {
            #[allow(clippy::cast_ptr_alignment)]
            let first = self.pixel_data() as *const u32;
            std::slice::from_raw_parts(first, width * height)
        }
    }

    #[inline]
    fn pixels_mut(&mut self) -> &mut [u32] {
        let (width, height) = self.image_dimensions();
        unsafe {
            #[allow(clippy::cast_ptr_alignment)]
            let first = self.pixel_data_mut() as *mut u32;
            std::slice::from_raw_parts_mut(first, width * height)
        }
    }

    #[inline]

    fn pixel_mut(&mut self, x: usize, y: usize) -> &mut u32 {
        let (width, height) = self.image_dimensions();
        debug_assert!(x < width && y < height, "x={} width={} y={} height={}", x, width, y, height);
        unsafe {
            let offset = (y * width * 4) + (x * 4);
            #[allow(clippy::cast_ptr_alignment)]
            &mut *(self.pixel_data_mut().add(offset) as *mut u32)
        }
    }

    #[inline]

    fn pixel(&self, x: usize, y: usize) -> &u32 {
        let (width, height) = self.image_dimensions();
        debug_assert!(x < width && y < height);
        unsafe {
            let offset = (y * width * 4) + (x * 4);
            #[allow(clippy::cast_ptr_alignment)]
            &*(self.pixel_data().add(offset) as *const u32)
        }
    }

    #[inline]
    fn horizontal_pixel_range(&self, x1: usize, x2: usize, y: usize) -> &[u32] {
        unsafe { std::slice::from_raw_parts(self.pixel(x1, y), x2 - x1) }
    }

    #[inline]
    fn horizontal_pixel_range_mut(&mut self, x1: usize, x2: usize, y: usize) -> &mut [u32] {
        unsafe { std::slice::from_raw_parts_mut(self.pixel_mut(x1, y), x2 - x1) }
    }

    fn clear(&mut self, color: Color) {
        for c in self.pixels_mut() {
            *c = color.0;
        }
    }

    fn clear_rect(&mut self, rect: Rect, color: Color) {
        let (dim_width, dim_height) = self.image_dimensions();
        let max_x = rect.max_x().min(dim_width as isize) as usize;
        let max_y = rect.max_y().min(dim_height as isize) as usize;

        let dest_x = rect.origin.x.max(0) as usize;
        if dest_x >= dim_width {
            return;
        }
        let dest_y = rect.origin.y.max(0) as usize;

        for y in dest_y..max_y {
            let range = self.horizontal_pixel_range_mut(dest_x, max_x, y);
            for c in range {
                *c = color.0;
            }
        }
    }

    fn draw_line(&mut self, start: Point, end: Point, color: Color, operator: Operator) {
        let (dim_width, dim_height) = self.image_dimensions();
        let linear: LinSrgba = color.into();
        let (red, green, blue, alpha) = linear.into_components();

        for ((x, y), value) in line_drawing::XiaolinWu::<f32, isize>::new(
            (start.x as f32, start.y as f32),
            (end.x as f32, end.y as f32),
        ) {
            if y < 0 || x < 0 {
                continue;
            }
            if y >= dim_height as isize || x >= dim_width as isize {
                continue;
            }
            let pix = self.pixel_mut(x as usize, y as usize);

            let color: Color = LinSrgba::from_components((red, green, blue, alpha * value)).into();
            *pix = color.composite(Color(*pix), operator).0;
        }
    }

    fn draw_rect(&mut self, rect: Rect, color: Color, operator: Operator) {
        let bottom_right = rect.origin.add_size(&rect.size);

        self.draw_line(rect.origin, Point::new(rect.origin.x, bottom_right.y), color, operator);
        self.draw_line(Point::new(bottom_right.x, rect.origin.y), bottom_right, color, operator);

        self.draw_line(rect.origin, Point::new(bottom_right.x, rect.origin.y), color, operator);
        self.draw_line(Point::new(rect.origin.x, bottom_right.y), bottom_right, color, operator);
    }
}

pub struct Image {
    data: Vec<u8>,
    width: usize,
    height: usize,
}

impl Into<Vec<u8>> for Image {
    fn into(self) -> Vec<u8> {
        self.data
    }
}

impl Image {
    pub fn new(width: usize, height: usize) -> Image {
        let size = height * width * 4;
        let mut data = vec![0; size];
        data.resize(size, 0);
        Image { data, width, height }
    }

    pub fn with_rgba32(width: usize, height: usize, stride: usize, data: &[u8]) -> Image {
        let mut image = Image::new(width, height);
        for y in 0..height {
            let src_offset = y * stride;
            let dest_offset = y * width * 4;
            #[allow(clippy::identity_op)]
            for x in 0..width {
                let red = data[src_offset + (x * 4) + 0];
                let green = data[src_offset + (x * 4) + 1];
                let blue = data[src_offset + (x * 4) + 2];
                let alpha = data[src_offset + (x * 4) + 3];
                image.data[dest_offset + (x * 4) + 0] = blue;
                image.data[dest_offset + (x * 4) + 1] = green;
                image.data[dest_offset + (x * 4) + 2] = red;
                image.data[dest_offset + (x * 4) + 3] = alpha;
            }
        }
        image
    }

    pub fn resize(&self, width: usize, height: usize) -> Image {
        let mut dest = Image::new(width, height);
        let algo = if (width * height) < (self.width * self.height) {
            resize::Type::Lanczos3
        } else {
            resize::Type::Mitchell
        };
        resize::new(self.width, self.height, width, height, resize::Pixel::RGBA8, algo)
            .expect("")
            .resize(self.data.as_rgba(), dest.data.as_rgba_mut())
            .expect("");
        dest
    }

    pub fn scale_by(&self, scale: f64) -> Image {
        let width = (self.width as f64 * scale) as usize;
        let height = (self.height as f64 * scale) as usize;
        self.resize(width, height)
    }
}

impl BitmapImage for Image {
    unsafe fn pixel_data(&self) -> *const u8 {
        self.data.as_ptr()
    }

    unsafe fn pixel_data_mut(&mut self) -> *mut u8 {
        self.data.as_mut_ptr()
    }

    fn image_dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }
}
