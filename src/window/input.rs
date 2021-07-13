use crate::window::{Point, ScreenPoint};
use bitflags::*;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Char(char),
    Super,
    Clear,
    Shift,
    Control,
    Composed(String),
    Alt,
    Pause,
    CapsLock,
    PageUp,
    PageDown,
    End,
    Home,
    LeftArrow,
    RightArrow,
    UpArrow,
    DownArrow,
    Print,
    Insert,
    Help,
    Applications,
    Numpad(u8),
    Multiply,
    Add,
    Separator,
    Subtract,
    Decimal,
    Divide,
    Function(u8),
    NumLock,
    ScrollLock,
    BrowserBack,
    BrowserForward,
    BrowserRefresh,
    BrowserStop,
    BrowserFavorites,
    BrowserHome,
    VolumeMute,
    VolumeDown,
    VolumeUp,
    PrintScreen,
    Cancel,
}

bitflags! {
    #[derive(Default)]
    pub struct Modifiers: u8 {
        const NONE = 0;
        const SHIFT = 1<<1;
        const ALT = 1<<2;
        const CTRL = 1<<3;
        const SUPER = 1<<4;
    }
}
bitflags! {
    #[derive(Default)]
    pub struct MouseButtons: u8 {
        const NONE = 0;
        #[allow(clippy::identity_op)]
        const LEFT = 1<<0;
        const RIGHT = 1<<1;
        const MIDDLE = 1<<2;
        const X1 = 1<<3;
        const X2 = 1<<4;
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MousePress {
    Left,
    Right,
    Middle,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MouseEventKind {
    Move,
    Press(MousePress),
    Release(MousePress),
    VertWheel(i16),
    HorzWheel(i16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub coords: Point,
    pub screen_coords: ScreenPoint,
    pub mouse_buttons: MouseButtons,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: KeyCode,
    pub raw_key: Option<KeyCode>,
    pub modifiers: Modifiers,
    pub repeat_count: u16,
    pub key_is_down: bool,
}
