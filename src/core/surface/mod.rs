use serde_derive::*;

pub mod change;
pub mod line;

pub use self::change::Change;
pub use self::line::Line;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Position {
    NoChange,

    Relative(isize),

    Absolute(usize),

    EndRelative(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CursorShape {
    Hidden,
    Default,
    BlinkingBlock,
    SteadyBlock,
    BlinkingUnderline,
    SteadyUnderline,
    BlinkingBar,
    SteadyBar,
}

impl Default for CursorShape {
    fn default() -> CursorShape {
        CursorShape::Default
    }
}
