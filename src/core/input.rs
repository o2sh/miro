use crate::core::keymap::KeyMap;
use crate::core::readbuf::ReadBuffer;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    Key(KeyEvent),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputState {
    Normal,
}

#[derive(Debug)]
pub struct InputParser {
    key_map: KeyMap<InputEvent>,
    buf: ReadBuffer,
    state: InputState,
}

impl Default for InputParser {
    fn default() -> Self {
        Self::new()
    }
}

impl InputParser {
    pub fn new() -> Self {
        Self {
            key_map: Self::build_basic_key_map(),
            buf: ReadBuffer::new(),
            state: InputState::Normal,
        }
    }

    fn build_basic_key_map() -> KeyMap<InputEvent> {
        let mut map = KeyMap::new();

        for alpha in b'A'..=b'Z' {
            let ctrl = [alpha - 0x40];
            map.insert(
                &ctrl,
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char(alpha as char),
                    modifiers: Modifiers::CTRL,
                }),
            );

            let alt = [0x1b, alpha];
            map.insert(
                &alt,
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char(alpha as char),
                    modifiers: Modifiers::ALT,
                }),
            );
        }

        for (keycode, dir) in &[
            (KeyCode::UpArrow, b'A'),
            (KeyCode::DownArrow, b'B'),
            (KeyCode::RightArrow, b'C'),
            (KeyCode::LeftArrow, b'D'),
            (KeyCode::Home, b'H'),
            (KeyCode::End, b'F'),
        ] {
            let arrow = [0x1b, b'[', *dir];
            map.insert(
                &arrow,
                InputEvent::Key(KeyEvent { key: *keycode, modifiers: Modifiers::NONE }),
            );

            for (suffix, modifiers) in &[
                (";2", Modifiers::SHIFT),
                (";3", Modifiers::ALT),
                (";4", Modifiers::ALT | Modifiers::SHIFT),
                (";5", Modifiers::CTRL),
                (";6", Modifiers::CTRL | Modifiers::SHIFT),
                (";7", Modifiers::CTRL | Modifiers::ALT),
                (";8", Modifiers::CTRL | Modifiers::ALT | Modifiers::SHIFT),
            ] {
                let key = format!("\x1b[1{}{}", suffix, *dir as char);
                map.insert(key, InputEvent::Key(KeyEvent { key: *keycode, modifiers: *modifiers }));
            }
        }

        for (keycode, dir) in &[
            (KeyCode::ApplicationUpArrow, b'A'),
            (KeyCode::ApplicationDownArrow, b'B'),
            (KeyCode::ApplicationRightArrow, b'C'),
            (KeyCode::ApplicationLeftArrow, b'D'),
        ] {
            let app = [0x1b, b'O', *dir];
            map.insert(
                &app,
                InputEvent::Key(KeyEvent { key: *keycode, modifiers: Modifiers::NONE }),
            );
        }

        for (keycode, c) in &[
            (KeyCode::Function(1), b'P'),
            (KeyCode::Function(2), b'Q'),
            (KeyCode::Function(3), b'R'),
            (KeyCode::Function(4), b'S'),
        ] {
            let key = [0x1b, b'O', *c];
            map.insert(
                &key,
                InputEvent::Key(KeyEvent { key: *keycode, modifiers: Modifiers::NONE }),
            );
        }

        for n in 1..=12 {
            for (suffix, modifiers) in &[
                ("", Modifiers::NONE),
                (";2", Modifiers::SHIFT),
                (";3", Modifiers::ALT),
                (";4", Modifiers::ALT | Modifiers::SHIFT),
                (";5", Modifiers::CTRL),
                (";6", Modifiers::CTRL | Modifiers::SHIFT),
                (";7", Modifiers::CTRL | Modifiers::ALT),
                (";8", Modifiers::CTRL | Modifiers::ALT | Modifiers::SHIFT),
            ] {
                let key = format!("\x1b[{code}{suffix}~", code = n + 10, suffix = suffix);
                map.insert(
                    key,
                    InputEvent::Key(KeyEvent { key: KeyCode::Function(n), modifiers: *modifiers }),
                );
            }
        }

        for (keycode, c) in &[
            (KeyCode::Insert, b'2'),
            (KeyCode::Home, b'1'),
            (KeyCode::End, b'4'),
            (KeyCode::PageUp, b'5'),
            (KeyCode::PageDown, b'6'),
        ] {
            let key = [0x1b, b'[', *c, b'~'];
            map.insert(
                key,
                InputEvent::Key(KeyEvent { key: *keycode, modifiers: Modifiers::NONE }),
            );
        }

        map.insert(
            &[0x7f],
            InputEvent::Key(KeyEvent { key: KeyCode::Delete, modifiers: Modifiers::NONE }),
        );

        map.insert(
            &[0x8],
            InputEvent::Key(KeyEvent { key: KeyCode::Backspace, modifiers: Modifiers::NONE }),
        );

        map.insert(
            &[0x1b],
            InputEvent::Key(KeyEvent { key: KeyCode::Escape, modifiers: Modifiers::NONE }),
        );

        map.insert(
            &[b'\t'],
            InputEvent::Key(KeyEvent { key: KeyCode::Tab, modifiers: Modifiers::NONE }),
        );

        map.insert(
            &[b'\r'],
            InputEvent::Key(KeyEvent { key: KeyCode::Enter, modifiers: Modifiers::NONE }),
        );
        map.insert(
            &[b'\n'],
            InputEvent::Key(KeyEvent { key: KeyCode::Enter, modifiers: Modifiers::NONE }),
        );

        map.insert(
            b"\x1b[200~",
            InputEvent::Key(KeyEvent {
                key: KeyCode::InternalPasteStart,
                modifiers: Modifiers::NONE,
            }),
        );
        map.insert(
            b"\x1b[201~",
            InputEvent::Key(KeyEvent {
                key: KeyCode::InternalPasteEnd,
                modifiers: Modifiers::NONE,
            }),
        );

        map
    }
}
