use crate::gui::selection::SelectionMode;
use crate::term::input::MouseButton;
use crate::term::{KeyCode, KeyModifiers};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum KeyAssignment {
    ToggleFullScreen,
    Copy,
    Paste,
    IncreaseFontSize,
    DecreaseFontSize,
    ResetFontSize,
    SelectTextAtMouseCursor(SelectionMode),
    ExtendSelectionToMouseCursor(Option<SelectionMode>),
    Hide,
}

/// A mouse event that can trigger an action
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MouseEventTrigger {
    /// Mouse button is pressed. streak is how many times in a row
    /// it was pressed.
    Down { streak: usize, button: MouseButton },
    /// Mouse button is held down while the cursor is moving. streak is how many times in a row
    /// it was pressed, with the last of those being held to form the drag.
    Drag { streak: usize, button: MouseButton },
    /// Mouse button is being released. streak is how many times
    /// in a row it was pressed and released.
    Up { streak: usize, button: MouseButton },
}

pub struct InputMap {
    keys: HashMap<(KeyCode, KeyModifiers), KeyAssignment>,
    mouse: HashMap<(MouseEventTrigger, KeyModifiers), KeyAssignment>,
}

impl InputMap {
    pub fn new() -> Self {
        let mut keys = HashMap::new();
        let mut mouse = HashMap::new();

        macro_rules! k {
            ($([$mod:expr, $code:expr, $action:expr]),* $(,)?) => {
                $(
                keys.entry(($code, $mod)).or_insert($action);
                )*
            };
        }

        macro_rules! m {
            ($([$mod:expr, $code:expr, $action:expr]),* $(,)?) => {
                $(
                mouse.entry(($code, $mod)).or_insert($action);
                )*
            };
        }

        use KeyAssignment::*;

        let ctrl_shift = KeyModifiers::CTRL | KeyModifiers::SHIFT;

        m!(
            [
                KeyModifiers::NONE,
                MouseEventTrigger::Down { streak: 3, button: MouseButton::Left },
                SelectTextAtMouseCursor(SelectionMode::Line)
            ],
            [
                KeyModifiers::NONE,
                MouseEventTrigger::Down { streak: 2, button: MouseButton::Left },
                SelectTextAtMouseCursor(SelectionMode::Word)
            ],
            [
                KeyModifiers::NONE,
                MouseEventTrigger::Down { streak: 1, button: MouseButton::Left },
                SelectTextAtMouseCursor(SelectionMode::Cell)
            ],
            [
                KeyModifiers::SHIFT,
                MouseEventTrigger::Down { streak: 1, button: MouseButton::Left },
                ExtendSelectionToMouseCursor(None)
            ],
            [
                KeyModifiers::NONE,
                MouseEventTrigger::Drag { streak: 1, button: MouseButton::Left },
                ExtendSelectionToMouseCursor(Some(SelectionMode::Cell))
            ],
            [
                KeyModifiers::NONE,
                MouseEventTrigger::Drag { streak: 2, button: MouseButton::Left },
                ExtendSelectionToMouseCursor(Some(SelectionMode::Word))
            ],
            [
                KeyModifiers::NONE,
                MouseEventTrigger::Drag { streak: 3, button: MouseButton::Left },
                ExtendSelectionToMouseCursor(Some(SelectionMode::Line))
            ],
        );

        k!(
            [KeyModifiers::SHIFT, KeyCode::Insert, Paste],
            [KeyModifiers::SUPER, KeyCode::Char('c'), Copy],
            [KeyModifiers::SUPER, KeyCode::Char('v'), Paste],
            [ctrl_shift, KeyCode::Char('c'), Copy],
            [ctrl_shift, KeyCode::Char('v'), Paste],
            [KeyModifiers::ALT, KeyCode::Char('\n'), ToggleFullScreen],
            [KeyModifiers::ALT, KeyCode::Char('\r'), ToggleFullScreen],
            [KeyModifiers::ALT, KeyCode::Enter, ToggleFullScreen],
            [KeyModifiers::SUPER, KeyCode::Char('m'), Hide],
            [ctrl_shift, KeyCode::Char('m'), Hide],
            [KeyModifiers::CTRL, KeyCode::Char('-'), DecreaseFontSize],
            [KeyModifiers::CTRL, KeyCode::Char('0'), ResetFontSize],
            [KeyModifiers::CTRL, KeyCode::Char('='), IncreaseFontSize],
            [KeyModifiers::SUPER, KeyCode::Char('-'), DecreaseFontSize],
            [KeyModifiers::SUPER, KeyCode::Char('0'), ResetFontSize],
            [KeyModifiers::SUPER, KeyCode::Char('='), IncreaseFontSize],
        );

        Self { keys, mouse }
    }

    pub fn lookup_key(&self, key: KeyCode, mods: KeyModifiers) -> Option<KeyAssignment> {
        self.keys.get(&(key, mods)).cloned()
    }

    pub fn lookup_mouse(
        &self,
        event: MouseEventTrigger,
        mods: KeyModifiers,
    ) -> Option<KeyAssignment> {
        self.mouse.get(&(event, mods)).cloned()
    }
}
