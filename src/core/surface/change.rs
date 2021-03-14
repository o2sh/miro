use crate::core::cell::{AttributeChange, CellAttributes};
use crate::core::color::ColorAttribute;
use crate::core::surface::{CursorShape, Position};
use serde_derive::*;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Change {
    Attribute(AttributeChange),

    AllAttributes(CellAttributes),

    Text(String),

    ClearScreen(ColorAttribute),

    ClearToEndOfLine(ColorAttribute),

    ClearToEndOfScreen(ColorAttribute),

    CursorPosition { x: Position, y: Position },

    CursorColor(ColorAttribute),

    CursorShape(CursorShape),

    ScrollRegionUp { first_row: usize, region_size: usize, scroll_count: usize },

    ScrollRegionDown { first_row: usize, region_size: usize, scroll_count: usize },

    Title(String),
}

impl<S: Into<String>> From<S> for Change {
    fn from(s: S) -> Self {
        Change::Text(s.into())
    }
}

impl From<AttributeChange> for Change {
    fn from(c: AttributeChange) -> Self {
        Change::Attribute(c)
    }
}
