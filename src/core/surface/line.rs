use crate::core::cell::{Cell, CellAttributes};
use crate::core::cellcluster::CellCluster;
use crate::core::hyperlink::Rule;
use bitflags::bitflags;
use serde_derive::*;
use std::ops::Range;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

bitflags! {
    #[derive(Serialize, Deserialize)]
    struct LineBits : u8 {
        const NONE = 0;
        const DIRTY = 1;
        const HAS_HYPERLINK = 1<<1;
        const SCANNED_IMPLICIT_HYPERLINKS = 1<<2;
        const HAS_IMPLICIT_HYPERLINKS = 1<<3;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    bits: LineBits,
    cells: Vec<Cell>,
}

pub enum DoubleClickRange {
    Range(Range<usize>),
    RangeWithWrap(Range<usize>),
}

impl Line {
    pub fn with_width(width: usize) -> Self {
        let mut cells = Vec::with_capacity(width);
        cells.resize(width, Cell::default());
        let bits = LineBits::DIRTY;
        Self { bits, cells }
    }

    pub fn from_text(s: &str, attrs: &CellAttributes) -> Line {
        let mut cells = Vec::new();

        for sub in s.graphemes(true) {
            let cell = Cell::new_grapheme(sub, attrs.clone());
            let width = cell.width();
            cells.push(cell);
            for _ in 1..width {
                cells.push(Cell::new(' ', attrs.clone()));
            }
        }

        Line { cells, bits: LineBits::DIRTY }
    }

    pub fn resize_and_clear(&mut self, width: usize) {
        let blank = Cell::default();
        self.cells.clear();
        self.cells.resize(width, blank);
        self.bits = LineBits::DIRTY;
    }

    pub fn resize(&mut self, width: usize) {
        self.cells.resize(width, Cell::default());
        self.bits |= LineBits::DIRTY;
    }

    #[inline]
    pub fn is_dirty(&self) -> bool {
        (self.bits & LineBits::DIRTY) == LineBits::DIRTY
    }

    #[inline]
    pub fn set_dirty(&mut self) {
        self.bits |= LineBits::DIRTY;
    }

    #[inline]
    pub fn clear_dirty(&mut self) {
        self.bits &= !LineBits::DIRTY;
    }

    pub fn invalidate_implicit_hyperlinks(&mut self) {
        if (self.bits & (LineBits::SCANNED_IMPLICIT_HYPERLINKS | LineBits::HAS_IMPLICIT_HYPERLINKS))
            == LineBits::NONE
        {
            return;
        }

        self.bits &= !LineBits::SCANNED_IMPLICIT_HYPERLINKS;
        if (self.bits & LineBits::HAS_IMPLICIT_HYPERLINKS) == LineBits::NONE {
            return;
        }

        for cell in &mut self.cells {
            let replace = match cell.attrs().hyperlink {
                Some(ref link) if link.is_implicit() => Some(Cell::new_grapheme(
                    cell.str(),
                    cell.attrs().clone().set_hyperlink(None).clone(),
                )),
                _ => None,
            };
            if let Some(replace) = replace {
                *cell = replace;
            }
        }

        self.bits &= !LineBits::HAS_IMPLICIT_HYPERLINKS;
        self.bits |= LineBits::DIRTY;
    }

    pub fn scan_and_create_hyperlinks(&mut self, rules: &[Rule]) {
        if (self.bits & LineBits::SCANNED_IMPLICIT_HYPERLINKS)
            == LineBits::SCANNED_IMPLICIT_HYPERLINKS
        {
            return;
        }

        let line = self.as_str();
        self.bits |= LineBits::SCANNED_IMPLICIT_HYPERLINKS;
        self.bits &= !LineBits::HAS_IMPLICIT_HYPERLINKS;

        for m in Rule::match_hyperlinks(&line, rules) {
            for (cell_idx, (byte_idx, _char)) in line.char_indices().enumerate() {
                if self.cells[cell_idx].attrs().hyperlink.is_some() {
                    continue;
                }
                if m.range.contains(&byte_idx) {
                    let attrs = self.cells[cell_idx]
                        .attrs()
                        .clone()
                        .set_hyperlink(Some(Arc::clone(&m.link)))
                        .clone();
                    let cell = Cell::new_grapheme(self.cells[cell_idx].str(), attrs);
                    self.cells[cell_idx] = cell;
                    self.bits |= LineBits::HAS_IMPLICIT_HYPERLINKS;
                }
            }
        }
    }

    #[inline]
    pub fn has_hyperlink(&self) -> bool {
        (self.bits & (LineBits::HAS_HYPERLINK | LineBits::HAS_IMPLICIT_HYPERLINKS))
            != LineBits::NONE
    }

    pub fn as_str(&self) -> String {
        let mut s = String::new();
        for (_, cell) in self.visible_cells() {
            s.push_str(cell.str());
        }
        s
    }

    pub fn compute_double_click_range(
        &self,
        click_col: usize,
        is_word: fn(s: &str) -> bool,
    ) -> DoubleClickRange {
        let mut lower = click_col;
        let mut upper = click_col;

        for (idx, cell) in self.cells.iter().enumerate().skip(click_col) {
            if !is_word(cell.str()) {
                break;
            }
            upper = idx + 1;
        }
        for (idx, cell) in self.cells.iter().enumerate().rev() {
            if idx > click_col {
                continue;
            }
            if !is_word(cell.str()) {
                break;
            }
            lower = idx;
        }

        if upper > lower && self.cells[upper - 1].attrs().wrapped() {
            DoubleClickRange::RangeWithWrap(lower..upper)
        } else {
            DoubleClickRange::Range(lower..upper)
        }
    }

    pub fn columns_as_str(&self, range: Range<usize>) -> String {
        let mut s = String::new();
        for (n, c) in self.visible_cells() {
            if n < range.start {
                continue;
            }
            if n >= range.end {
                break;
            }
            s.push_str(c.str());
        }
        s
    }

    pub fn set_cell(&mut self, idx: usize, cell: Cell) -> &Cell {
        let width = cell.width();

        if idx + width >= self.cells.len() {
            self.cells.resize(idx + width, Cell::default());
        }

        self.invalidate_implicit_hyperlinks();
        self.bits |= LineBits::DIRTY;
        if cell.attrs().hyperlink.is_some() {
            self.bits |= LineBits::HAS_HYPERLINK;
        }
        self.invalidate_grapheme_at_or_before(idx);

        for i in 1..=width.saturating_sub(1) {
            self.cells[idx + i] = Cell::new(' ', cell.attrs().clone());
        }

        self.cells[idx] = cell;
        &self.cells[idx]
    }

    fn invalidate_grapheme_at_or_before(&mut self, idx: usize) {
        if idx > 0 {
            let prior = idx - 1;
            let width = self.cells[prior].width();
            if width > 1 {
                let attrs = self.cells[prior].attrs().clone();
                for nerf in prior..prior + width {
                    self.cells[nerf] = Cell::new(' ', attrs.clone());
                }
            }
        }
    }

    pub fn insert_cell(&mut self, x: usize, cell: Cell) {
        self.invalidate_implicit_hyperlinks();

        let width = cell.width();
        for _ in 1..=width.saturating_sub(1) {
            self.cells.insert(x, Cell::new(' ', cell.attrs().clone()));
        }

        self.cells.insert(x, cell);
    }

    pub fn erase_cell(&mut self, x: usize) {
        self.invalidate_implicit_hyperlinks();
        self.invalidate_grapheme_at_or_before(x);
        self.cells.remove(x);
        self.cells.push(Cell::default());
    }

    pub fn fill_range(&mut self, cols: impl Iterator<Item = usize>, cell: &Cell) {
        let max_col = self.cells.len();
        for x in cols {
            if x >= max_col {
                break;
            }

            self.set_cell(x, cell.clone());
        }
    }

    pub fn visible_cells(&self) -> impl Iterator<Item = (usize, &Cell)> {
        let mut skip_width = 0;
        self.cells.iter().enumerate().filter(move |(_idx, cell)| {
            if skip_width > 0 {
                skip_width -= 1;
                false
            } else {
                skip_width = cell.width().saturating_sub(1);
                true
            }
        })
    }

    pub fn cluster(&self) -> Vec<CellCluster> {
        CellCluster::make_cluster(self.visible_cells())
    }

    pub fn is_whitespace(&self) -> bool {
        self.cells.iter().all(|c| c.str() == " ")
    }

    pub fn set_last_cell_was_wrapped(&mut self, wrapped: bool) {
        if let Some(cell) = self.cells.last_mut() {
            cell.attrs_mut().set_wrapped(wrapped);
        }
    }

    pub fn last_cell_was_wrapped(&self) -> bool {
        self.cells.last().map(|c| c.attrs().wrapped()).unwrap_or(false)
    }

    pub fn wrap(mut self, width: usize) -> Vec<Self> {
        if let Some(end_idx) = self.cells.iter().rposition(|c| c.str() != " ") {
            self.cells.resize(end_idx + 1, Cell::default());

            let mut lines: Vec<_> = self
                .cells
                .chunks_mut(width)
                .map(|chunk| {
                    let mut line = Line { cells: chunk.to_vec(), bits: LineBits::DIRTY };
                    if line.cells.len() == width {
                        // Ensure that we don't forget that we wrapped
                        line.set_last_cell_was_wrapped(true);
                    }
                    line
                })
                .collect();
            // The last of the chunks wasn't actually wrapped
            lines.last_mut().map(|line| line.set_last_cell_was_wrapped(false));
            lines
        } else {
            vec![self]
        }
    }

    pub fn append_line(&mut self, mut other: Line) {
        self.cells.append(&mut other.cells);
        self.set_dirty();
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }
}

impl<'a> From<&'a str> for Line {
    fn from(s: &str) -> Line {
        Line::from_text(s, &CellAttributes::default())
    }
}
