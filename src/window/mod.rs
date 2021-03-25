use failure::Fallible;
use std::any::Any;
pub mod bitmaps;
pub mod color;
pub mod connection;
pub mod input;
pub mod os;
pub mod spawn;
pub mod tasks;

#[cfg(all(not(target_os = "macos")))]
mod egl;

pub use bitmaps::BitmapImage;
pub use color::Color;
pub use connection::*;
pub use input::*;
pub use os::*;

#[derive(Debug, Clone, Copy)]
pub enum Operator {
    Over,
    Source,
    Multiply,
    MultiplyThenOver(Color),
}

#[derive(Debug, Clone, Copy)]
pub struct Dimensions {
    pub pixel_width: usize,
    pub pixel_height: usize,
    pub dpi: usize,
}
pub struct PixelUnit;
pub type Point = euclid::Point2D<isize, PixelUnit>;
pub type Rect = euclid::Rect<isize, PixelUnit>;
pub type Size = euclid::Size2D<isize, PixelUnit>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseCursor {
    Arrow,
    Hand,
    Text,
}

#[allow(unused_variables)]
pub trait WindowCallbacks: Any {
    fn can_close(&self) -> bool {
        true
    }
    fn destroy(&mut self) {}
    fn resize(&mut self, dimensions: Dimensions) {}
    fn paint_opengl(&mut self, frame: &mut glium::Frame) {}
    fn key_event(&mut self, key: &KeyEvent, context: &dyn WindowOps) -> bool {
        false
    }
    fn mouse_event(&mut self, event: &MouseEvent, context: &dyn WindowOps) {
        context.set_cursor(Some(MouseCursor::Arrow));
    }
    fn created(
        &mut self,
        _window: &Window,
        _context: std::rc::Rc<glium::backend::Context>,
    ) -> Fallible<()> {
        Ok(())
    }
    fn as_any(&mut self) -> &mut dyn Any;
}

pub trait WindowOps {
    fn show(&self);
    fn hide(&self);
    fn close(&self);
    fn set_cursor(&self, cursor: Option<MouseCursor>);
    fn invalidate(&self);
    fn set_title(&self, title: &str);
    fn set_inner_size(&self, width: usize, height: usize);
    fn set_text_cursor_position(&self, _cursor: Rect) {}
    fn apply<F: Send + 'static + Fn(&mut dyn Any, &dyn WindowOps)>(&self, func: F)
    where
        Self: Sized;
}

pub trait WindowOpsMut {
    fn show(&mut self);
    fn hide(&mut self);
    fn close(&mut self);
    fn set_cursor(&mut self, cursor: Option<MouseCursor>);
    fn invalidate(&mut self);
    fn set_title(&mut self, title: &str);
    fn set_inner_size(&self, width: usize, height: usize);
    fn set_text_cursor_position(&mut self, _cursor: Rect) {}
}
