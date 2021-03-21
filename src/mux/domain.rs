use crate::config::Config;
use crate::mux::tab::{LocalTab, Tab};
use crate::mux::Mux;
use crate::pty::unix;
use crate::pty::{PtySize, PtySystem};
use downcast_rs::{impl_downcast, Downcast};
use failure::{Error, Fallible};
use log::info;
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;

static DOMAIN_ID: ::std::sync::atomic::AtomicUsize = ::std::sync::atomic::AtomicUsize::new(0);
pub type DomainId = usize;

pub fn alloc_domain_id() -> DomainId {
    DOMAIN_ID.fetch_add(1, ::std::sync::atomic::Ordering::Relaxed)
}

pub trait Domain: Downcast {
    fn spawn(&self, size: PtySize) -> Result<Rc<dyn Tab>, Error>;
    fn domain_id(&self) -> DomainId;
    fn domain_name(&self) -> &str;
    fn detach(&self) -> Fallible<()>;
}
impl_downcast!(Domain);

pub struct LocalDomain {
    pty_system: Box<dyn PtySystem>,
    config: Arc<Config>,
    id: DomainId,
    name: String,
}

impl LocalDomain {
    pub fn new(name: &str, config: &Arc<Config>) -> Result<Self, Error> {
        let pty_system = Box::new(unix::UnixPtySystem);
        Ok(Self::with_pty_system(name, config, pty_system))
    }

    pub fn with_pty_system(
        name: &str,
        config: &Arc<Config>,
        pty_system: Box<dyn PtySystem>,
    ) -> Self {
        let config = Arc::clone(config);
        let id = alloc_domain_id();
        Self { pty_system, config, id, name: name.to_string() }
    }
}

impl Domain for LocalDomain {
    fn spawn(&self, size: PtySize) -> Result<Rc<dyn Tab>, Error> {
        let pair = self.pty_system.openpty(size)?;
        let child = pair.slave.spawn_command(Command::new(crate::pty::get_shell()?))?;
        info!("spawned: {:?}", child);

        let mut terminal = crate::term::Terminal::new(
            size.rows as usize,
            size.cols as usize,
            size.pixel_width as usize,
            size.pixel_height as usize,
            self.config.scrollback_lines.unwrap_or(3500),
            self.config.hyperlink_rules.clone(),
        );

        let mux = Mux::get().unwrap();

        if let Some(palette) = mux.config().colors.as_ref() {
            *terminal.palette_mut() = palette.clone().into();
        }

        let tab: Rc<dyn Tab> = Rc::new(LocalTab::new(terminal, child, pair.master, self.id));

        mux.add_tab(&tab)?;
        mux.add_tab_to_window(&tab)?;

        Ok(tab)
    }

    fn domain_id(&self) -> DomainId {
        self.id
    }

    fn domain_name(&self) -> &str {
        &self.name
    }

    fn detach(&self) -> Fallible<()> {
        failure::bail!("detach not implemented");
    }
}
