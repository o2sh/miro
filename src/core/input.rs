use bitflags::bitflags;
use serde_derive::*;

bitflags! {
    #[derive(Default, Serialize, Deserialize)]
    pub struct Modifiers: u8 {
        const NONE = 0;
        const SHIFT = 1<<1;
        const ALT = 1<<2;
        const CTRL = 1<<3;
        const SUPER = 1<<4;
    }
}
bitflags! {
    #[derive(Default, Serialize, Deserialize)]
    pub struct MouseButtons: u8 {
        const NONE = 0;
        const LEFT = 1<<1;
        const RIGHT = 1<<2;
        const MIDDLE = 1<<3;
        const VERT_WHEEL = 1<<4;
        const HORZ_WHEEL = 1<<5;


        const WHEEL_POSITIVE = 1<<6;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseEvent {
    pub x: u16,
    pub y: u16,
    pub mouse_buttons: MouseButtons,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyEvent {
    pub key: KeyCode,

    pub modifiers: Modifiers,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyCode {
    Char(char),

    Hyper,
    Super,
    Meta,

    Cancel,
    Backspace,
    Tab,
    Clear,
    Enter,
    Shift,
    Escape,
    LeftShift,
    RightShift,
    Control,
    LeftControl,
    RightControl,
    Alt,
    LeftAlt,
    RightAlt,
    Menu,
    LeftMenu,
    RightMenu,
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
    Select,
    Print,
    Execute,
    PrintScreen,
    Insert,
    Delete,
    Help,
    LeftWindows,
    RightWindows,
    Applications,
    Sleep,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
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
    BrowserSearch,
    BrowserFavorites,
    BrowserHome,
    VolumeMute,
    VolumeDown,
    VolumeUp,
    MediaNextTrack,
    MediaPrevTrack,
    MediaStop,
    MediaPlayPause,
    ApplicationLeftArrow,
    ApplicationRightArrow,
    ApplicationUpArrow,
    ApplicationDownArrow,

    #[doc(hidden)]
    InternalPasteStart,
    #[doc(hidden)]
    InternalPasteEnd,
}
