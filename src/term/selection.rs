#![cfg_attr(feature = "cargo-clippy", allow(clippy::range_plus_one))]
use super::{ScrollbackOrVisibleRowIndex, VisibleRowIndex};
use serde_derive::*;
use std::ops::Range;

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelectionCoordinate {
    pub x: usize,
    pub y: ScrollbackOrVisibleRowIndex,
}

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelectionRange {
    pub start: SelectionCoordinate,
    pub end: SelectionCoordinate,
}

impl SelectionRange {
    pub fn start(start: SelectionCoordinate) -> Self {
        let end = start;
        Self { start, end }
    }

    pub fn clip_to_viewport(
        &self,
        viewport_offset: VisibleRowIndex,
        height: usize,
    ) -> SelectionRange {
        let offset = -viewport_offset as ScrollbackOrVisibleRowIndex;
        SelectionRange {
            start: SelectionCoordinate { x: self.start.x, y: self.start.y.max(offset) - offset },
            end: SelectionCoordinate {
                x: self.end.x,
                y: self.end.y.min(offset + height as ScrollbackOrVisibleRowIndex) - offset,
            },
        }
    }

    pub fn extend(&self, end: SelectionCoordinate) -> Self {
        Self { start: self.start, end }
    }

    pub fn normalize(&self) -> Self {
        if self.start.y <= self.end.y {
            *self
        } else {
            Self { start: self.end, end: self.start }
        }
    }

    pub fn rows(&self) -> Range<ScrollbackOrVisibleRowIndex> {
        debug_assert!(self.start.y <= self.end.y, "you forgot to normalize a SelectionRange");
        self.start.y..self.end.y + 1
    }

    pub fn cols_for_row(&self, row: ScrollbackOrVisibleRowIndex) -> Range<usize> {
        debug_assert!(self.start.y <= self.end.y, "you forgot to normalize a SelectionRange");
        if row < self.start.y || row > self.end.y {
            0..0
        } else if self.start.y == self.end.y {
            if self.start.x <= self.end.x {
                self.start.x..self.end.x.saturating_add(1)
            } else {
                self.end.x..self.start.x.saturating_add(1)
            }
        } else if row == self.end.y {
            0..self.end.x.saturating_add(1)
        } else if row == self.start.y {
            self.start.x..usize::max_value()
        } else {
            0..usize::max_value()
        }
    }
}
