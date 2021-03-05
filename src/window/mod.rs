use std::ops::Deref;

pub mod glium_window;

#[derive(Copy, Clone, Debug)]
pub struct Point(pub euclid::Point2D<f32, f32>);

impl Default for Point {
    fn default() -> Point {
        Point::new(0.0, 0.0)
    }
}

impl Deref for Point {
    type Target = euclid::Point2D<f32, f32>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

unsafe impl glium::vertex::Attribute for Point {
    #[inline]
    fn get_type() -> glium::vertex::AttributeType {
        glium::vertex::AttributeType::F32F32
    }
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { 0: euclid::point2(x, y) }
    }
}

impl std::ops::AddAssign for Point {
    fn add_assign(&mut self, other: Self) {
        self.0.x += other.0.x;
        self.0.y += other.0.y;
    }
}
