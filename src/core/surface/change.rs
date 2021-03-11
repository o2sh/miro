use crate::core::cell::{AttributeChange, CellAttributes};
use crate::core::color::ColorAttribute;
use crate::core::surface::{CursorShape, Position};
use serde_derive::*;
use std::sync::Arc;

/// `Change` describes an update operation to be applied to a `Surface`.
/// Changes to the active attributes (color, style), moving the cursor
/// and outputting text are examples of some of the values.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Change {
    /// Change a single attribute
    Attribute(AttributeChange),
    /// Change all possible attributes to the given set of values
    AllAttributes(CellAttributes),
    /// Add printable text.
    /// Control characters are rendered inert by transforming them
    /// to space.  CR and LF characters are interpreted by moving
    /// the cursor position.  CR moves the cursor to the start of
    /// the line and LF moves the cursor down to the next line.
    /// You typically want to use both together when sending in
    /// a line break.
    Text(String),
    /// Clear the screen to the specified color.
    /// Implicitly clears all attributes prior to clearing the screen.
    /// Moves the cursor to the home position (top left).
    ClearScreen(ColorAttribute),
    /// Clear from the current cursor X position to the rightmost
    /// edge of the screen.  The background color is set to the
    /// provided color.  The cursor position remains unchanged.
    ClearToEndOfLine(ColorAttribute),
    /// Clear from the current cursor X position to the rightmost
    /// edge of the screen on the current line.  Clear all of the
    /// lines below the current cursor Y position.  The background
    /// color is set ot the provided color.  The cursor position
    /// remains unchanged.
    ClearToEndOfScreen(ColorAttribute),
    /// Move the cursor to the specified `Position`.
    CursorPosition { x: Position, y: Position },
    /// Change the cursor color.
    CursorColor(ColorAttribute),
    /// Change the cursor shape
    CursorShape(CursorShape),
    /// Scroll the `region_size` lines starting at `first_row` upwards
    /// by `scroll_count` lines.  The `scroll_count` lines at the top of
    /// the region are overwritten.  The `scroll_count` lines at the
    /// bottom of the region will become blank.
    ///
    /// After a region is scrolled, the cursor position is undefined,
    /// and the terminal's scroll region is set to the range specified.
    /// To restore scrolling behaviour to the full terminal window, an
    /// additional `Change::ScrollRegionUp { first_row: 0, region_size:
    /// height, scroll_count: 0 }`, where `height` is the height of the
    /// terminal, should be emitted.
    ScrollRegionUp { first_row: usize, region_size: usize, scroll_count: usize },
    /// Scroll the `region_size` lines starting at `first_row` downwards
    /// by `scroll_count` lines.  The `scroll_count` lines at the bottom
    /// the region are overwritten.  The `scroll_count` lines at the top
    /// of the region will become blank.
    ///
    /// After a region is scrolled, the cursor position is undefined,
    /// and the terminal's scroll region is set to the range specified.
    /// To restore scrolling behaviour to the full terminal window, an
    /// additional `Change::ScrollRegionDown { first_row: 0,
    /// region_size: height, scroll_count: 0 }`, where `height` is the
    /// height of the terminal, should be emitted.
    ScrollRegionDown { first_row: usize, region_size: usize, scroll_count: usize },
    /// Change the title of the window in which the surface will be
    /// rendered.
    Title(String),
}

impl Change {
    pub fn is_text(&self) -> bool {
        match self {
            Change::Text(_) => true,
            _ => false,
        }
    }
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
