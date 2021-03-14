use serde_derive::*;

use failure::Error;
use std::ops::{Deref, DerefMut, Range};
use std::str;

pub mod input;
pub use input::*;

pub mod clipboard;
pub mod keyassignment;

pub use crate::core::cell::{self, *};

pub use crate::core::surface::line::*;

pub mod screen;
pub use screen::*;

pub mod selection;
use selection::{SelectionCoordinate, SelectionRange};

use crate::core::hyperlink::Hyperlink;

pub mod terminal;
pub use terminal::*;

pub mod terminalstate;
pub use terminalstate::*;

pub type PhysRowIndex = usize;

pub type VisibleRowIndex = i64;

pub type ScrollbackOrVisibleRowIndex = i32;

pub fn intersects_range<T: Ord + Copy>(r1: Range<T>, r2: Range<T>) -> bool {
    use std::cmp::{max, min};
    let start = max(r1.start, r2.start);
    let end = min(r1.end, r2.end);

    end > start
}

#[derive(Debug)]
pub enum Position {
    Absolute(VisibleRowIndex),
    Relative(i64),
}

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct CursorPosition {
    pub x: usize,
    pub y: VisibleRowIndex,
}

pub mod color;

pub const DEVICE_IDENT: &[u8] = b"\x1b[?6c";
