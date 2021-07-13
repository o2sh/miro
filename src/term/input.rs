#![cfg_attr(
    feature = "cargo-clippy",
    allow(clippy::suspicious_arithmetic_impl, clippy::redundant_field_names)
)]

use super::VisibleRowIndex;
use serde_derive::*;
use std::time::{Duration, Instant};

pub use crate::core::input::KeyCode;
pub use crate::core::input::Modifiers as KeyModifiers;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize, Hash)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp(usize),
    WheelDown(usize),
    None,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum MouseEventKind {
    Press,
    Release,
    Move,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub x: usize,
    pub y: VisibleRowIndex,
    pub button: MouseButton,
    pub modifiers: KeyModifiers,
}

#[derive(Debug)]
pub struct LastMouseClick {
    pub button: MouseButton,
    time: Instant,
    pub streak: usize,
}

const CLICK_INTERVAL: u64 = 500;

impl LastMouseClick {
    pub fn new(button: MouseButton) -> Self {
        Self { button, time: Instant::now(), streak: 1 }
    }

    pub fn add(&self, button: MouseButton) -> Self {
        let now = Instant::now();
        let streak = if button == self.button
            && now.duration_since(self.time) <= Duration::from_millis(CLICK_INTERVAL)
        {
            self.streak + 1
        } else {
            1
        };
        Self { button, time: now, streak }
    }
}
