use num::Integer;
use serde_derive::*;
use std::cmp::{max, min};
use std::fmt::Debug;
use std::ops::{Deref, DerefMut, Range};
use std::str;

pub mod input;
pub use input::*;

pub mod clipboard;
pub mod keyassignment;

pub use crate::core::cell::{self, *};

pub use crate::core::surface::line::*;

pub mod screen;
pub use screen::*;

pub mod selection;
use selection::{SelectionCoordinate, SelectionRange};

use crate::core::hyperlink::Hyperlink;

pub mod terminal;
pub use terminal::*;

pub mod terminalstate;
pub use terminalstate::*;

pub type PhysRowIndex = usize;
pub type StableRowIndex = isize;

pub type VisibleRowIndex = i64;

pub type ScrollbackOrVisibleRowIndex = i32;

pub fn intersects_range2<T: Ord + Copy>(r1: Range<T>, r2: Range<T>) -> bool {
    let start = max(r1.start, r2.start);
    let end = min(r1.end, r2.end);

    end > start
}

#[derive(Debug)]
pub enum Position {
    Absolute(VisibleRowIndex),
    Relative(i64),
}

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct CursorPosition {
    pub x: usize,
    pub y: VisibleRowIndex,
}

pub mod color;

pub const DEVICE_IDENT: &[u8] = b"\x1b[?6c";

pub struct RangeSet<T: Integer + Copy> {
    ranges: Vec<Range<T>>,
}

pub fn range_is_empty<T: Integer>(range: &Range<T>) -> bool {
    range.start == range.end
}

pub fn intersects_range<T: Integer + Copy + Debug>(r1: &Range<T>, r2: &Range<T>) -> bool {
    let start = max(r1.start, r2.start);
    let end = min(r1.end, r2.end);

    end > start
}

pub fn range_intersection<T: Integer + Copy + Debug>(
    r1: &Range<T>,
    r2: &Range<T>,
) -> Option<Range<T>> {
    let start = max(r1.start, r2.start);
    let end = min(r1.end, r2.end);

    if end > start {
        Some(start..end)
    } else {
        None
    }
}

pub fn range_subtract<T: Integer + Copy + Debug>(
    r1: &Range<T>,
    r2: &Range<T>,
) -> (Option<Range<T>>, Option<Range<T>>) {
    let i_start = max(r1.start, r2.start);
    let i_end = min(r1.end, r2.end);

    if i_end > i_start {
        let a = if i_start == r1.start { None } else { Some(r1.start..r1.end.min(i_start)) };

        let b = if i_end == r1.end { None } else { Some(r1.end.min(i_end)..r1.end) };

        (a, b)
    } else {
        (Some(r1.clone()), None)
    }
}

pub fn range_union<T: Integer>(r1: Range<T>, r2: Range<T>) -> Range<T> {
    if range_is_empty(&r1) {
        r2
    } else if range_is_empty(&r2) {
        r1
    } else {
        let start = r1.start.min(r2.start);
        let end = r1.end.max(r2.end);
        start..end
    }
}

impl<T: Integer + Copy + Debug> Into<Vec<Range<T>>> for RangeSet<T> {
    fn into(self) -> Vec<Range<T>> {
        self.ranges
    }
}

impl<T: Integer + Copy + Debug> RangeSet<T> {
    pub fn new() -> Self {
        Self { ranges: vec![] }
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    pub fn contains(&self, value: T) -> bool {
        for r in &self.ranges {
            if r.contains(&value) {
                return true;
            }
        }
        false
    }

    pub fn difference(&self, other: &Self) -> Self {
        let mut result = Self::new();

        for my_range in &self.ranges {
            for other_range in &other.ranges {
                match range_subtract(my_range, other_range) {
                    (Some(a), Some(b)) => {
                        result.add_range(a);
                        result.add_range(b);
                    }
                    (Some(a), None) | (None, Some(a)) if a != *other_range => {
                        result.add_range(a);
                    }
                    _ => {}
                }
            }
        }

        result
    }

    pub fn intersection_with_range(&self, range: Range<T>) -> Self {
        let mut result = Self::new();

        for r in &self.ranges {
            if let Some(i) = range_intersection(r, &range) {
                result.add_range(i);
            }
        }

        result
    }

    pub fn remove(&mut self, value: T) {
        self.remove_range(value..value + num::one());
    }

    pub fn remove_range(&mut self, range: Range<T>) {
        let mut to_add = vec![];
        let mut to_remove = vec![];

        for (idx, r) in self.ranges.iter().enumerate() {
            match range_subtract(r, &range) {
                (None, None) => to_remove.push(idx),
                (Some(a), Some(b)) => {
                    to_remove.push(idx);
                    to_add.push(a);
                    to_add.push(b);
                }
                (Some(a), None) | (None, Some(a)) if a != *r => {
                    to_remove.push(idx);
                    to_add.push(a);
                }
                _ => {}
            }
        }

        for idx in to_remove.into_iter().rev() {
            self.ranges.remove(idx);
        }

        for r in to_add {
            self.add_range(r);
        }
    }

    pub fn remove_set(&mut self, set: &Self) {
        for r in set.iter() {
            self.remove_range(r.clone());
        }
    }

    pub fn add(&mut self, value: T) {
        self.add_range(value..value + num::one());
    }

    pub fn add_range(&mut self, range: Range<T>) {
        if range_is_empty(&range) {
            return;
        }

        if self.ranges.is_empty() {
            self.ranges.push(range.clone());
            return;
        }

        match self.intersection_helper(&range) {
            (Some(a), Some(b)) if b == a + 1 => {
                let second = self.ranges[b].clone();
                let merged = range_union(range, second);

                self.ranges.remove(b);
                return self.add_range(merged);
            }
            (Some(a), _) => self.merge_into_range(a, range),
            (None, Some(_)) => unreachable!(),
            (None, None) => {
                let idx = self.insertion_point(&range);
                self.ranges.insert(idx, range.clone());
            }
        }
    }

    pub fn add_set(&mut self, set: &Self) {
        for r in set.iter() {
            self.add_range(r.clone());
        }
    }

    fn merge_into_range(&mut self, idx: usize, range: Range<T>) {
        let existing = self.ranges[idx].clone();
        self.ranges[idx] = range_union(existing, range);
    }

    fn intersection_helper(&self, range: &Range<T>) -> (Option<usize>, Option<usize>) {
        let mut first = None;

        for (idx, r) in self.ranges.iter().enumerate() {
            if intersects_range(r, range) || r.end == range.start {
                if first.is_some() {
                    return (first, Some(idx));
                }
                first = Some(idx);
            }
        }

        (first, None)
    }

    fn insertion_point(&self, range: &Range<T>) -> usize {
        for (idx, r) in self.ranges.iter().enumerate() {
            if range.end < r.start {
                return idx;
            }
        }

        self.ranges.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Range<T>> {
        self.ranges.iter()
    }
}
