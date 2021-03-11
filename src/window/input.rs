use bitflags::*;

/// Which key is pressed.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeyCode {
    /// The decoded unicode character
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
    /// F1-F24 are possible
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
    pub x: u16,
    pub y: u16,
    pub mouse_buttons: MouseButtons,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    /// Which key was pressed.
    /// This is the potentially processed/composed version
    /// of the input.
    pub key: KeyCode,

    /// The raw unprocessed key press if it was different from
    /// the processed/composed version
    pub raw_key: Option<KeyCode>,

    /// Which modifiers are down
    pub modifiers: Modifiers,

    /// How many times this key repeats
    pub repeat_count: u16,

    /// If true, this is a key down rather than a key up event
    pub key_is_down: bool,
}
