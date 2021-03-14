use crate::gui::executor;
use crate::mux::domain::DomainId;
use crate::mux::renderable::Renderable;
use crate::mux::Mux;
use crate::pty::{Child, MasterPty, PtySize};
use crate::term::color::ColorPalette;
use crate::term::selection::SelectionRange;
use crate::term::{KeyCode, KeyModifiers, MouseEvent, Terminal, TerminalHost};
use downcast_rs::{impl_downcast, Downcast};
use failure::{Error, Fallible};
use std::cell::{RefCell, RefMut};
use std::sync::{Arc, Mutex};

static TAB_ID: ::std::sync::atomic::AtomicUsize = ::std::sync::atomic::AtomicUsize::new(0);
pub type TabId = usize;

pub fn alloc_tab_id() -> TabId {
    TAB_ID.fetch_add(1, ::std::sync::atomic::Ordering::Relaxed)
}

const PASTE_CHUNK_SIZE: usize = 1024;

struct Paste {
    tab_id: TabId,
    text: String,
    offset: usize,
}

fn schedule_next_paste(paste: &Arc<Mutex<Paste>>) {
    let paste = Arc::clone(paste);
    crate::core::promise::Future::with_executor(executor(), move || {
        let mut locked = paste.lock().unwrap();
        let mux = Mux::get().unwrap();
        let tab = mux.get_tab(locked.tab_id).unwrap();

        let remain = locked.text.len() - locked.offset;
        let chunk = remain.min(PASTE_CHUNK_SIZE);
        let text_slice = &locked.text[locked.offset..locked.offset + chunk];
        tab.send_paste(text_slice).unwrap();

        if chunk < remain {
            locked.offset += chunk;
            schedule_next_paste(&paste);
        }

        Ok(())
    });
}

pub trait Tab: Downcast {
    fn tab_id(&self) -> TabId;
    fn renderer(&self) -> RefMut<dyn Renderable>;
    fn get_title(&self) -> String;
    fn send_paste(&self, text: &str) -> Fallible<()>;
    fn reader(&self) -> Fallible<Box<dyn std::io::Read + Send>>;
    fn writer(&self) -> RefMut<dyn std::io::Write>;
    fn resize(&self, size: PtySize) -> Fallible<()>;
    fn key_down(&self, key: KeyCode, mods: KeyModifiers) -> Fallible<()>;
    fn mouse_event(&self, event: MouseEvent, host: &mut dyn TerminalHost) -> Fallible<()>;
    fn advance_bytes(&self, buf: &[u8], host: &mut dyn TerminalHost);
    fn is_dead(&self) -> bool;
    fn palette(&self) -> ColorPalette;
    fn domain_id(&self) -> DomainId;

    fn selection_range(&self) -> Option<SelectionRange>;

    fn trickle_paste(&self, text: String) -> Fallible<()> {
        if text.len() <= PASTE_CHUNK_SIZE {
            self.send_paste(&text)?;
        } else {
            self.send_paste(&text[0..PASTE_CHUNK_SIZE])?;

            let paste = Arc::new(Mutex::new(Paste {
                tab_id: self.tab_id(),
                text,
                offset: PASTE_CHUNK_SIZE,
            }));
            schedule_next_paste(&paste);
        }
        Ok(())
    }
}
impl_downcast!(Tab);

pub struct LocalTab {
    tab_id: TabId,
    terminal: RefCell<Terminal>,
    process: RefCell<Box<dyn Child>>,
    pty: RefCell<Box<dyn MasterPty>>,
    domain_id: DomainId,
}

impl Tab for LocalTab {
    #[inline]
    fn tab_id(&self) -> TabId {
        self.tab_id
    }

    fn renderer(&self) -> RefMut<dyn Renderable> {
        RefMut::map(self.terminal.borrow_mut(), |t| &mut *t)
    }

    fn is_dead(&self) -> bool {
        if let Ok(None) = self.process.borrow_mut().try_wait() {
            false
        } else {
            log::error!("is_dead: {:?}", self.tab_id);
            true
        }
    }

    fn advance_bytes(&self, buf: &[u8], host: &mut dyn TerminalHost) {
        self.terminal.borrow_mut().advance_bytes(buf, host)
    }

    fn mouse_event(&self, event: MouseEvent, host: &mut dyn TerminalHost) -> Result<(), Error> {
        self.terminal.borrow_mut().mouse_event(event, host)
    }

    fn key_down(&self, key: KeyCode, mods: KeyModifiers) -> Result<(), Error> {
        self.terminal.borrow_mut().key_down(key, mods, &mut *self.pty.borrow_mut())
    }

    fn resize(&self, size: PtySize) -> Result<(), Error> {
        self.pty.borrow_mut().resize(size)?;
        self.terminal.borrow_mut().resize(
            size.rows as usize,
            size.cols as usize,
            size.pixel_width as usize,
            size.pixel_height as usize,
        );
        Ok(())
    }

    fn writer(&self) -> RefMut<dyn std::io::Write> {
        self.pty.borrow_mut()
    }

    fn reader(&self) -> Result<Box<dyn std::io::Read + Send>, Error> {
        self.pty.borrow_mut().try_clone_reader()
    }

    fn send_paste(&self, text: &str) -> Result<(), Error> {
        self.terminal.borrow_mut().send_paste(text, &mut *self.pty.borrow_mut())
    }

    fn get_title(&self) -> String {
        self.terminal.borrow_mut().get_title().to_string()
    }

    fn palette(&self) -> ColorPalette {
        self.terminal.borrow().palette().clone()
    }

    fn domain_id(&self) -> DomainId {
        self.domain_id
    }

    fn selection_range(&self) -> Option<SelectionRange> {
        let terminal = self.terminal.borrow();
        let rows = terminal.screen().physical_rows;
        terminal.selection_range().map(|r| r.clip_to_viewport(terminal.get_viewport_offset(), rows))
    }
}

impl LocalTab {
    pub fn new(
        terminal: Terminal,
        process: Box<dyn Child>,
        pty: Box<dyn MasterPty>,
        domain_id: DomainId,
    ) -> Self {
        let tab_id = alloc_tab_id();
        Self {
            tab_id,
            terminal: RefCell::new(terminal),
            process: RefCell::new(process),
            pty: RefCell::new(pty),
            domain_id,
        }
    }
}

impl Drop for LocalTab {
    fn drop(&mut self) {
        self.process.borrow_mut().kill().ok();
        self.process.borrow_mut().wait().ok();
    }
}
