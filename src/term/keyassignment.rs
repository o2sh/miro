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
    Hide,
}

pub struct KeyMap(HashMap<(KeyCode, KeyModifiers), KeyAssignment>);

impl KeyMap {
    pub fn new() -> Self {
        let mut map = HashMap::new();

        macro_rules! m {
            ($([$mod:expr, $code:expr, $action:expr]),* $(,)?) => {
                $(
                map.entry(($code, $mod)).or_insert($action);
                )*
            };
        }

        use KeyAssignment::*;

        let ctrl_shift = KeyModifiers::CTRL | KeyModifiers::SHIFT;

        m!(
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

        Self(map)
    }

    pub fn lookup(&self, key: KeyCode, mods: KeyModifiers) -> Option<KeyAssignment> {
        self.0.get(&(key, mods)).cloned()
    }
}
