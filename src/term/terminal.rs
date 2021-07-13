use super::*;
use crate::core::escape::parser::Parser;
use crate::term::clipboard::Clipboard;
use std::sync::Arc;

pub trait TerminalHost {
    fn writer(&mut self) -> &mut dyn std::io::Write;
    fn get_clipboard(&mut self) -> anyhow::Result<Arc<dyn Clipboard>>;
    fn set_title(&mut self, title: &str);
    fn click_link(&mut self, link: &Arc<Hyperlink>);
}

pub struct Terminal {
    state: TerminalState,
    parser: Parser,
}

impl Deref for Terminal {
    type Target = TerminalState;

    fn deref(&self) -> &TerminalState {
        &self.state
    }
}

impl DerefMut for Terminal {
    fn deref_mut(&mut self) -> &mut TerminalState {
        &mut self.state
    }
}

impl Terminal {
    pub fn new(
        physical_rows: usize,
        physical_cols: usize,
        pixel_width: usize,
        pixel_height: usize,
        scrollback_size: usize,
        writer: Box<dyn std::io::Write>,
    ) -> Terminal {
        Terminal {
            state: TerminalState::new(
                physical_rows,
                physical_cols,
                pixel_height,
                pixel_width,
                scrollback_size,
                writer,
            ),
            parser: Parser::new(),
        }
    }

    pub fn advance_bytes<B: AsRef<[u8]>>(&mut self, bytes: B) {
        let bytes = bytes.as_ref();
        let mut performer = Performer::new(&mut self.state);
        self.parser.parse(bytes, |action| performer.perform(action));
    }
}
