use crate::core::hyperlink::Hyperlink;
use crate::term::{CursorPosition, Line, Terminal, TerminalState};
use downcast_rs::{impl_downcast, Downcast};
use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;

pub trait Renderable: Downcast {
    fn get_cursor_position(&self) -> CursorPosition;

    fn get_dirty_lines(&self) -> Vec<(usize, Cow<Line>, Range<usize>)>;

    fn has_dirty_lines(&self) -> bool;

    fn make_all_lines_dirty(&mut self);

    fn clean_dirty_lines(&mut self);

    fn current_highlight(&self) -> Option<Arc<Hyperlink>>;

    fn physical_dimensions(&self) -> (usize, usize);
}
impl_downcast!(Renderable);

impl Renderable for Terminal {
    fn get_cursor_position(&self) -> CursorPosition {
        self.cursor_pos()
    }

    fn get_dirty_lines(&self) -> Vec<(usize, Cow<Line>, Range<usize>)> {
        TerminalState::get_dirty_lines(self)
            .into_iter()
            .map(|(idx, line, range)| (idx, Cow::Borrowed(line), range))
            .collect()
    }

    fn clean_dirty_lines(&mut self) {
        TerminalState::clean_dirty_lines(self)
    }

    fn make_all_lines_dirty(&mut self) {
        TerminalState::make_all_lines_dirty(self)
    }

    fn current_highlight(&self) -> Option<Arc<Hyperlink>> {
        TerminalState::current_highlight(self)
    }

    fn physical_dimensions(&self) -> (usize, usize) {
        let screen = self.screen();
        (screen.physical_rows, screen.physical_cols)
    }

    fn has_dirty_lines(&self) -> bool {
        TerminalState::has_dirty_lines(self)
    }
}
