use crate::term::{KeyCode, KeyModifiers};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum KeyAssignment {
    SpawnTab,
    ToggleFullScreen,
    Copy,
    Paste,
    ActivateTabRelative(isize),
    IncreaseFontSize,
    DecreaseFontSize,
    ResetFontSize,
    ActivateTab(usize),
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
        };

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
            [KeyModifiers::SUPER, KeyCode::Char('t'), SpawnTab],
            [KeyModifiers::SUPER, KeyCode::Char('1'), ActivateTab(0)],
            [KeyModifiers::SUPER, KeyCode::Char('2'), ActivateTab(1)],
            [KeyModifiers::SUPER, KeyCode::Char('3'), ActivateTab(2)],
            [KeyModifiers::SUPER, KeyCode::Char('4'), ActivateTab(3)],
            [KeyModifiers::SUPER, KeyCode::Char('5'), ActivateTab(4)],
            [KeyModifiers::SUPER, KeyCode::Char('6'), ActivateTab(5)],
            [KeyModifiers::SUPER, KeyCode::Char('7'), ActivateTab(6)],
            [KeyModifiers::SUPER, KeyCode::Char('8'), ActivateTab(7)],
            [KeyModifiers::SUPER, KeyCode::Char('9'), ActivateTab(8)],
            [ctrl_shift, KeyCode::Char('1'), ActivateTab(0)],
            [ctrl_shift, KeyCode::Char('2'), ActivateTab(1)],
            [ctrl_shift, KeyCode::Char('3'), ActivateTab(2)],
            [ctrl_shift, KeyCode::Char('4'), ActivateTab(3)],
            [ctrl_shift, KeyCode::Char('5'), ActivateTab(4)],
            [ctrl_shift, KeyCode::Char('6'), ActivateTab(5)],
            [ctrl_shift, KeyCode::Char('7'), ActivateTab(6)],
            [ctrl_shift, KeyCode::Char('8'), ActivateTab(7)],
            [ctrl_shift, KeyCode::Char('9'), ActivateTab(8)],
            [
                KeyModifiers::SUPER | KeyModifiers::SHIFT,
                KeyCode::Char('['),
                ActivateTabRelative(-1)
            ],
            [
                KeyModifiers::SUPER | KeyModifiers::SHIFT,
                KeyCode::Char('{'),
                ActivateTabRelative(-1)
            ],
            [KeyModifiers::SUPER | KeyModifiers::SHIFT, KeyCode::Char(']'), ActivateTabRelative(1)],
            [KeyModifiers::SUPER | KeyModifiers::SHIFT, KeyCode::Char('}'), ActivateTabRelative(1)],
        );

        Self(map)
    }

    pub fn lookup(&self, key: KeyCode, mods: KeyModifiers) -> Option<KeyAssignment> {
        self.0.get(&(key, mods)).cloned()
    }
}
