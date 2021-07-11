use crate::core::promise;
use crate::mux::Mux;
use crate::pty::{Child, MasterPty, PtySize};
use crate::term::color::ColorPalette;
use crate::term::{KeyCode, KeyModifiers, MouseEvent, Terminal, TerminalHost};
use std::cell::{RefCell, RefMut};
use std::sync::{Arc, Mutex};

const PASTE_CHUNK_SIZE: usize = 1024;

struct Paste {
    text: String,
    offset: usize,
}

fn schedule_next_paste(paste: &Arc<Mutex<Paste>>) {
    let paste = Arc::clone(paste);
    promise::spawn(async move {
        let mut locked = paste.lock().unwrap();
        let mux = Mux::get().unwrap();
        let tab = mux.get_tab();

        let remain = locked.text.len() - locked.offset;
        let chunk = remain.min(PASTE_CHUNK_SIZE);
        let text_slice = &locked.text[locked.offset..locked.offset + chunk];
        tab.send_paste(text_slice).unwrap();

        if chunk < remain {
            locked.offset += chunk;
            schedule_next_paste(&paste);
        }
    });
}

pub struct Tab {
    terminal: RefCell<Terminal>,
    process: RefCell<Box<dyn Child>>,
    pty: RefCell<Box<dyn MasterPty>>,
    can_close: bool,
}

impl Tab {
    pub fn renderer(&self) -> RefMut<Terminal> {
        RefMut::map(self.terminal.borrow_mut(), |t| &mut *t)
    }

    pub fn trickle_paste(&self, text: String) -> anyhow::Result<()> {
        if text.len() <= PASTE_CHUNK_SIZE {
            self.send_paste(&text)?;
        } else {
            self.send_paste(&text[0..PASTE_CHUNK_SIZE])?;

            let paste = Arc::new(Mutex::new(Paste { text, offset: PASTE_CHUNK_SIZE }));
            schedule_next_paste(&paste);
        }
        Ok(())
    }

    pub fn advance_bytes(&self, buf: &[u8], host: &mut dyn TerminalHost) {
        self.terminal.borrow_mut().advance_bytes(buf, host)
    }

    pub fn mouse_event(
        &self,
        event: MouseEvent,
        host: &mut dyn TerminalHost,
    ) -> anyhow::Result<()> {
        self.terminal.borrow_mut().mouse_event(event, host)
    }

    pub fn key_down(&self, key: KeyCode, mods: KeyModifiers) -> anyhow::Result<()> {
        self.terminal.borrow_mut().key_down(key, mods, &mut *self.pty.borrow_mut())
    }

    pub fn resize(&self, size: PtySize) -> anyhow::Result<()> {
        self.pty.borrow_mut().resize(size)?;
        self.terminal.borrow_mut().resize(
            size.rows as usize,
            size.cols as usize,
            size.pixel_width as usize,
            size.pixel_height as usize,
        );
        Ok(())
    }

    pub fn writer(&self) -> RefMut<dyn std::io::Write> {
        self.pty.borrow_mut()
    }

    pub fn reader(&self) -> anyhow::Result<Box<dyn std::io::Read + Send>> {
        self.pty.borrow_mut().try_clone_reader()
    }

    fn send_paste(&self, text: &str) -> anyhow::Result<()> {
        self.terminal.borrow_mut().send_paste(text, &mut *self.pty.borrow_mut())
    }

    pub fn get_title(&self) -> String {
        self.terminal.borrow_mut().get_title().to_string()
    }

    pub fn palette(&self) -> ColorPalette {
        self.terminal.borrow().palette().clone()
    }

    pub fn close(&mut self) {
        self.can_close = true;
    }

    pub fn can_close(&self) -> bool {
        self.can_close || self.is_dead()
    }

    pub fn is_dead(&self) -> bool {
        if let Ok(None) = self.process.borrow_mut().try_wait() {
            false
        } else {
            true
        }
    }

    pub fn new(terminal: Terminal, process: Box<dyn Child>, pty: Box<dyn MasterPty>) -> Self {
        Self {
            terminal: RefCell::new(terminal),
            process: RefCell::new(process),
            pty: RefCell::new(pty),
            can_close: false,
        }
    }
}

impl Drop for Tab {
    fn drop(&mut self) {
        self.process.borrow_mut().kill().ok();
        self.process.borrow_mut().wait().ok();
    }
}
