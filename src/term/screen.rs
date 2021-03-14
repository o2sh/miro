use super::*;
use log::debug;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct Screen {
    pub lines: VecDeque<Line>,

    pub scrollback_size: usize,

    pub physical_rows: usize,

    pub physical_cols: usize,
}

impl Screen {
    pub fn new(physical_rows: usize, physical_cols: usize, scrollback_size: usize) -> Screen {
        let physical_rows = physical_rows.max(1);
        let physical_cols = physical_cols.max(1);

        let mut lines = VecDeque::with_capacity(physical_rows + scrollback_size);
        for _ in 0..physical_rows {
            lines.push_back(Line::with_width(physical_cols));
        }

        Screen { lines, scrollback_size, physical_rows, physical_cols }
    }

    pub fn resize(&mut self, physical_rows: usize, physical_cols: usize) {
        let physical_rows = physical_rows.max(1);
        let physical_cols = physical_cols.max(1);

        let capacity = physical_rows + self.scrollback_size;
        let current_capacity = self.lines.capacity();
        if capacity > current_capacity {
            self.lines.reserve(capacity - current_capacity);
        }

        if physical_rows > self.physical_rows {
            for _ in self.physical_rows..physical_rows {
                self.lines.push_back(Line::with_width(physical_cols));
            }
        }
        self.physical_rows = physical_rows;
        self.physical_cols = physical_cols;
    }

    #[inline]
    pub fn line_mut(&mut self, idx: PhysRowIndex) -> &mut Line {
        &mut self.lines[idx]
    }

    #[inline]
    pub fn dirty_line(&mut self, idx: VisibleRowIndex) {
        let line_idx = self.phys_row(idx);
        if line_idx < self.lines.len() {
            self.lines[line_idx].set_dirty();
        }
    }

    pub fn insert_cell(&mut self, x: usize, y: VisibleRowIndex) {
        let phys_cols = self.physical_cols;

        let line_idx = self.phys_row(y);
        let line = self.line_mut(line_idx);
        line.insert_cell(x, Cell::default());
        if line.cells().len() > phys_cols {
            line.resize(phys_cols);
        }
    }

    pub fn erase_cell(&mut self, x: usize, y: VisibleRowIndex) {
        let line_idx = self.phys_row(y);
        let line = self.line_mut(line_idx);
        line.erase_cell(x);
    }

    pub fn set_cell(&mut self, x: usize, y: VisibleRowIndex, cell: &Cell) -> &Cell {
        let line_idx = self.phys_row(y);

        let line = self.line_mut(line_idx);
        line.set_cell(x, cell.clone())
    }

    pub fn clear_line(
        &mut self,
        y: VisibleRowIndex,
        cols: impl Iterator<Item = usize>,
        attr: &CellAttributes,
    ) {
        let physical_cols = self.physical_cols;
        let line_idx = self.phys_row(y);
        let line = self.line_mut(line_idx);
        line.resize(physical_cols);
        line.fill_range(cols, &Cell::new(' ', attr.clone()));
    }

    #[inline]
    pub fn phys_row(&self, row: VisibleRowIndex) -> PhysRowIndex {
        assert!(row >= 0, "phys_row called with negative row {}", row);
        (self.lines.len() - self.physical_rows) + row as usize
    }

    #[inline]
    pub fn scrollback_or_visible_row(&self, row: ScrollbackOrVisibleRowIndex) -> PhysRowIndex {
        ((self.lines.len() - self.physical_rows) as ScrollbackOrVisibleRowIndex + row).max(0)
            as usize
    }

    #[inline]
    pub fn scrollback_or_visible_range(
        &self,
        range: &Range<ScrollbackOrVisibleRowIndex>,
    ) -> Range<PhysRowIndex> {
        self.scrollback_or_visible_row(range.start)..self.scrollback_or_visible_row(range.end)
    }

    #[inline]
    pub fn phys_range(&self, range: &Range<VisibleRowIndex>) -> Range<PhysRowIndex> {
        self.phys_row(range.start)..self.phys_row(range.end)
    }

    pub fn scroll_up(&mut self, scroll_region: &Range<VisibleRowIndex>, num_rows: usize) {
        let phys_scroll = self.phys_range(scroll_region);
        let num_rows = num_rows.min(phys_scroll.end - phys_scroll.start);

        debug!("scroll_up {:?} num_rows={} phys_scroll={:?}", scroll_region, num_rows, phys_scroll);

        for y in phys_scroll.clone() {
            self.line_mut(y).set_dirty();
        }

        let lines_removed = if scroll_region.start > 0 {
            num_rows
        } else {
            let max_allowed = self.physical_rows + self.scrollback_size;
            if self.lines.len() + num_rows >= max_allowed {
                (self.lines.len() + num_rows) - max_allowed
            } else {
                0
            }
        };

        let remove_idx = if scroll_region.start == 0 { 0 } else { phys_scroll.start };

        let to_move = lines_removed.min(num_rows);
        let (to_remove, to_add) = {
            for _ in 0..to_move {
                let mut line = self.lines.remove(remove_idx).unwrap();

                line.resize_and_clear(self.physical_cols);
                if scroll_region.end as usize == self.physical_rows {
                    self.lines.push_back(line);
                } else {
                    self.lines.insert(phys_scroll.end - 1, line);
                }
            }

            (lines_removed - to_move, num_rows - to_move)
        };

        for _ in 0..to_remove {
            self.lines.remove(remove_idx);
        }

        if scroll_region.end as usize == self.physical_rows {
            for _ in 0..to_add {
                self.lines.push_back(Line::with_width(self.physical_cols));
            }
        } else {
            for _ in 0..to_add {
                self.lines.insert(phys_scroll.end, Line::with_width(self.physical_cols));
            }
        }
    }

    pub fn scroll_down(&mut self, scroll_region: &Range<VisibleRowIndex>, num_rows: usize) {
        debug!("scroll_down {:?} {}", scroll_region, num_rows);
        let phys_scroll = self.phys_range(scroll_region);
        let num_rows = num_rows.min(phys_scroll.end - phys_scroll.start);

        let middle = phys_scroll.end - num_rows;

        for y in phys_scroll.start..middle {
            self.line_mut(y).set_dirty();
        }

        for _ in 0..num_rows {
            self.lines.remove(middle);
        }

        for _ in 0..num_rows {
            self.lines.insert(phys_scroll.start, Line::with_width(self.physical_cols));
        }
    }
}
