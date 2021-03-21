use crate::config::Config;
use crate::core::hyperlink::Hyperlink;
use crate::core::promise::Future;
use crate::core::ratelim::RateLimiter;
use crate::gui::executor;
use crate::mux::tab::{Tab, TabId};
use crate::mux::window::Window;
use crate::term::clipboard::Clipboard;
use crate::term::TerminalHost;
use domain::{Domain, DomainId};
use failure::{bail, Error, Fallible};
use log::{debug, error};
use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::io::Read;
use std::rc::Rc;
use std::sync::Arc;
use std::thread;

pub mod domain;
pub mod renderable;
pub mod tab;
pub mod window;

pub struct Mux {
    tabs: RefCell<HashMap<TabId, Rc<dyn Tab>>>,
    window: RefCell<Window>,
    config: Arc<Config>,
    default_domain: RefCell<Option<Arc<dyn Domain>>>,
    domains: RefCell<HashMap<DomainId, Arc<dyn Domain>>>,
}

fn read_from_tab_pty(config: Arc<Config>, tab_id: TabId, mut reader: Box<dyn std::io::Read>) {
    const BUFSIZE: usize = 32 * 1024;
    let mut buf = [0; BUFSIZE];

    let mut lim =
        RateLimiter::new(config.ratelimit_output_bytes_per_second.unwrap_or(2 * 1024 * 1024));

    loop {
        match reader.read(&mut buf) {
            Ok(size) if size == 0 => {
                error!("read_pty EOF: tab_id {}", tab_id);
                break;
            }
            Err(err) => {
                error!("read_pty failed: tab {} {:?}", tab_id, err);
                break;
            }
            Ok(size) => {
                lim.blocking_admittance_check(size as u32);
                let data = buf[0..size].to_vec();
                Future::with_executor(executor(), move || {
                    let mux = Mux::get().unwrap();
                    if let Some(tab) = mux.get_tab(tab_id) {
                        tab.advance_bytes(&data, &mut Host { writer: &mut *tab.writer() });
                    }
                    Ok(())
                });
            }
        }
    }
    Future::with_executor(executor(), move || {
        let mux = Mux::get().unwrap();
        mux.remove_tab(tab_id);
        Ok(())
    });
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
            Err(err) => error!("failed to open {}: {:?}", link.uri(), err),
        }
    }

    fn get_clipboard(&mut self) -> Fallible<Arc<dyn Clipboard>> {
        bail!("peer requested clipboard; ignoring");
    }

    fn set_title(&mut self, _title: &str) {}
}

thread_local! {
    static MUX: RefCell<Option<Rc<Mux>>> = RefCell::new(None);
}

impl Mux {
    pub fn new(config: &Arc<Config>, default_domain: Option<Arc<dyn Domain>>) -> Self {
        let mut domains = HashMap::new();
        let mut domains_by_name = HashMap::new();
        if let Some(default_domain) = default_domain.as_ref() {
            domains.insert(default_domain.domain_id(), Arc::clone(default_domain));

            domains_by_name
                .insert(default_domain.domain_name().to_string(), Arc::clone(default_domain));
        }

        Self {
            tabs: RefCell::new(HashMap::new()),
            window: RefCell::new(Window::new()),
            config: Arc::clone(config),
            default_domain: RefCell::new(default_domain),
            domains: RefCell::new(domains),
        }
    }

    pub fn default_domain(&self) -> Arc<dyn Domain> {
        self.default_domain.borrow().as_ref().map(Arc::clone).unwrap()
    }

    pub fn get_domain(&self, id: DomainId) -> Option<Arc<dyn Domain>> {
        self.domains.borrow().get(&id).cloned()
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

    pub fn get_tab(&self, tab_id: TabId) -> Option<Rc<dyn Tab>> {
        self.tabs.borrow().get(&tab_id).map(Rc::clone)
    }

    pub fn add_tab(&self, tab: &Rc<dyn Tab>) -> Result<(), Error> {
        self.tabs.borrow_mut().insert(tab.tab_id(), Rc::clone(tab));

        let reader = tab.reader()?;
        let tab_id = tab.tab_id();
        let config = Arc::clone(&self.config);
        thread::spawn(move || read_from_tab_pty(config, tab_id, reader));

        Ok(())
    }

    pub fn remove_tab(&self, tab_id: TabId) {
        debug!("removing tab {}", tab_id);
        self.tabs.borrow_mut().remove(&tab_id);
    }

    pub fn get_window(&self) -> Ref<Window> {
        self.window.borrow()
    }

    pub fn get_window_mut(&self) -> RefMut<Window> {
        self.window.borrow_mut()
    }

    pub fn get_active_tab_for_window(&self) -> Option<Rc<dyn Tab>> {
        let window = self.get_window();
        window.get_active().map(Rc::clone)
    }

    pub fn add_tab_to_window(&self, tab: &Rc<dyn Tab>) -> Fallible<()> {
        let mut window = self.get_window_mut();
        window.push(tab);
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.borrow().is_empty()
    }
}
