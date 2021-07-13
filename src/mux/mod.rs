use crate::config::Config;
use crate::core::hyperlink::Hyperlink;
use crate::core::promise;
use crate::core::ratelim::RateLimiter;
use crate::mux::tab::Tab;
use crate::pty::{unix, PtySize, PtySystem};
use crate::term::clipboard::Clipboard;
use crate::term::TerminalHost;
use anyhow::bail;
use std::cell::{Ref, RefCell};
use std::io::Read;
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;
use std::thread;

pub mod tab;

pub struct Mux {
    tab: RefCell<Tab>,
    config: Arc<Config>,
}

fn read_from_tab_pty(config: Arc<Config>, mut reader: Box<dyn std::io::Read>) {
    const BUFSIZE: usize = 32 * 1024;
    let mut buf = [0; BUFSIZE];

    let mut lim =
        RateLimiter::new(config.ratelimit_output_bytes_per_second.unwrap_or(2 * 1024 * 1024));

    loop {
        match reader.read(&mut buf) {
            Ok(size) if size == 0 => {
                break;
            }
            Err(_) => {
                break;
            }
            Ok(size) => {
                lim.blocking_admittance_check(size as u32);
                let data = buf[0..size].to_vec();
                promise::spawn_into_main_thread_with_low_priority(async move {
                    let mux = Mux::get().unwrap();
                    let tab = mux.get_tab();
                    tab.advance_bytes(&data, &mut Host { writer: &mut *tab.writer() });
                });
            }
        }
    }
}

struct Host<'a> {
    writer: &'a mut dyn std::io::Write,
}

impl<'a> TerminalHost for Host<'a> {
    fn writer(&mut self) -> &mut dyn std::io::Write {
        &mut self.writer
    }

    fn click_link(&mut self, link: &Arc<Hyperlink>) {
        match open::that(link.uri()) {
            Ok(_) => {}
            Err(_) => {}
        }
    }

    fn get_clipboard(&mut self) -> anyhow::Result<Arc<dyn Clipboard>> {
        bail!("peer requested clipboard; ignoring");
    }

    fn set_title(&mut self, _title: &str) {}
}

thread_local! {
    static MUX: RefCell<Option<Rc<Mux>>> = RefCell::new(None);
}

impl Mux {
    pub fn new(config: &Arc<Config>, size: PtySize) -> anyhow::Result<Self> {
        let pty_system = Box::new(unix::UnixPtySystem);
        let pair = pty_system.openpty(size)?;
        let child = pair.slave.spawn_command(Command::new(crate::pty::get_shell()?))?;

        let terminal = crate::term::Terminal::new(
            size.rows as usize,
            size.cols as usize,
            size.pixel_width as usize,
            size.pixel_height as usize,
            config.scrollback_lines.unwrap_or(3500),
            config.hyperlink_rules.clone(),
        );

        let tab = Tab::new(terminal, child, pair.master);

        Ok(Self { tab: RefCell::new(tab), config: Arc::clone(config) })
    }

    pub fn start(&self) -> anyhow::Result<()> {
        let reader = self.tab.borrow().reader()?;
        let config = Arc::clone(&self.config);
        thread::spawn(move || read_from_tab_pty(config, reader));

        Ok(())
    }

    pub fn config(&self) -> &Arc<Config> {
        &self.config
    }

    pub fn set_mux(mux: &Rc<Mux>) {
        MUX.with(|m| {
            *m.borrow_mut() = Some(Rc::clone(mux));
        });
    }

    pub fn get() -> Option<Rc<Mux>> {
        let mut res = None;
        MUX.with(|m| {
            if let Some(mux) = &*m.borrow() {
                res = Some(Rc::clone(mux));
            }
        });
        res
    }

    pub fn get_tab(&self) -> Ref<Tab> {
        self.tab.borrow()
    }

    pub fn close(&self) {
        self.tab.borrow_mut().close()
    }

    pub fn can_close(&self) -> bool {
        self.tab.borrow().can_close()
    }
}
