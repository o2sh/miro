use super::*;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct Screen {
    pub lines: VecDeque<Line>,
    stable_row_index_offset: usize,
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

        Screen { lines, scrollback_size, physical_rows, physical_cols, stable_row_index_offset: 0 }
    }

    fn rewrap_lines(
        &mut self,
        physical_cols: usize,
        physical_rows: usize,
        cursor_x: usize,
        cursor_y: PhysRowIndex,
    ) -> (usize, PhysRowIndex) {
        let mut rewrapped = VecDeque::new();
        let mut logical_line: Option<Line> = None;
        let mut logical_cursor_x: Option<usize> = None;
        let mut adjusted_cursor = (cursor_y, cursor_y);

        for (phys_idx, mut line) in self.lines.drain(..).enumerate() {
            line.invalidate_implicit_hyperlinks();
            let was_wrapped = line.last_cell_was_wrapped();

            if was_wrapped {
                line.set_last_cell_was_wrapped(false);
            }

            let line = match logical_line.take() {
                None => {
                    if phys_idx == cursor_y {
                        logical_cursor_x = Some(cursor_x);
                    }
                    line
                }
                Some(mut prior) => {
                    if phys_idx == cursor_y {
                        logical_cursor_x = Some(cursor_x + prior.cells().len());
                    }
                    prior.append_line(line);
                    prior
                }
            };

            if was_wrapped {
                logical_line.replace(line);
                continue;
            }

            if let Some(x) = logical_cursor_x.take() {
                let num_lines = x / physical_cols;
                let last_x = x - (num_lines * physical_cols);
                adjusted_cursor = (last_x, rewrapped.len() + num_lines);
            }

            if line.cells().len() <= physical_cols {
                rewrapped.push_back(line);
            } else {
                for line in line.wrap(physical_cols) {
                    rewrapped.push_back(line);
                }
            }
        }
        self.lines = rewrapped;

        let capacity = physical_rows + self.scrollback_size;
        while self.lines.len() > capacity
            && self.lines.back().map(Line::is_whitespace).unwrap_or(false)
        {
            self.lines.pop_back();
        }

        adjusted_cursor
    }

    pub fn resize(
        &mut self,
        physical_rows: usize,
        physical_cols: usize,
        cursor: CursorPosition,
    ) -> CursorPosition {
        let physical_rows = physical_rows.max(1);
        let physical_cols = physical_cols.max(1);
        if physical_rows == self.physical_rows && physical_cols == self.physical_cols {
            return cursor;
        }

        let cursor_phys = self.phys_row(cursor.y);
        for _ in cursor_phys + 1..self.lines.len() {
            if self.lines.back().map(Line::is_whitespace).unwrap_or(false) {
                self.lines.pop_back();
            }
        }

        let (cursor_x, cursor_y) = if physical_cols != self.physical_cols {
            self.rewrap_lines(physical_cols, physical_rows, cursor.x, cursor_phys)
        } else {
            (cursor.x, cursor_phys)
        };

        let capacity = physical_rows + self.scrollback_size;
        let current_capacity = self.lines.capacity();
        if capacity > current_capacity {
            self.lines.reserve(capacity - current_capacity);
        }

        while self.lines.len() < physical_rows {
            self.lines.push_back(Line::with_width(physical_cols));
        }

        let vis_cursor_y =
            cursor.y.saturating_add(cursor_y as i64).saturating_sub(cursor_phys as i64).max(0);

        let required_num_rows_after_cursor = physical_rows.saturating_sub(vis_cursor_y as usize);
        let actual_num_rows_after_cursor = self.lines.len().saturating_sub(cursor_y);
        for _ in actual_num_rows_after_cursor..required_num_rows_after_cursor {
            self.lines.push_back(Line::with_width(physical_cols));
        }

        self.physical_rows = physical_rows;
        self.physical_cols = physical_cols;
        CursorPosition { x: cursor_x, y: vis_cursor_y }
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

    pub fn stable_range(&self, range: &Range<StableRowIndex>) -> Range<PhysRowIndex> {
        let range_len = (range.end - range.start) as usize;

        let first = match self.stable_row_to_phys(range.start) {
            Some(first) => first,
            None => {
                return 0..range_len.min(self.lines.len());
            }
        };

        let last = match self.stable_row_to_phys(range.end.saturating_sub(1)) {
            Some(last) => last,
            None => {
                let last = self.lines.len() - 1;
                return last.saturating_sub(range_len)..last + 1;
            }
        };

        first..last + 1
    }

    #[inline]
    pub fn phys_range(&self, range: &Range<VisibleRowIndex>) -> Range<PhysRowIndex> {
        self.phys_row(range.start)..self.phys_row(range.end)
    }

    #[inline]
    pub fn phys_to_stable_row_index(&self, phys: PhysRowIndex) -> StableRowIndex {
        (phys + self.stable_row_index_offset) as StableRowIndex
    }

    #[inline]
    pub fn stable_row_to_phys(&self, stable: StableRowIndex) -> Option<PhysRowIndex> {
        let idx = stable - self.stable_row_index_offset as isize;
        if idx < 0 || idx >= self.lines.len() as isize {
            None
        } else {
            Some(idx as PhysRowIndex)
        }
    }

    #[inline]
    pub fn visible_row_to_stable_row(&self, vis: VisibleRowIndex) -> StableRowIndex {
        self.phys_to_stable_row_index(self.phys_row(vis))
    }

    pub fn erase_scrollback(&mut self) {
        let len = self.lines.len();
        let to_clear = len - self.physical_rows;
        for _ in 0..to_clear {
            self.lines.pop_front();
            self.stable_row_index_offset += 1;
        }
    }

    pub fn scroll_up(&mut self, scroll_region: &Range<VisibleRowIndex>, num_rows: usize) {
        let phys_scroll = self.phys_range(scroll_region);
        let num_rows = num_rows.min(phys_scroll.end - phys_scroll.start);

        if scroll_region.start != 0 || scroll_region.end as usize != self.physical_rows {
            for y in phys_scroll.clone() {
                self.line_mut(y).set_dirty();
            }
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
                line.set_dirty();
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

        if remove_idx == 0 {
            self.stable_row_index_offset += lines_removed;
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
