use crate::font::FontConfiguration;
use crate::mux::Mux;
use crate::term::StableRowIndex;
use crate::window::*;
use std::rc::Rc;

mod glyphcache;
mod header;
mod quad;
mod renderstate;
pub mod selection;
mod spritesheet;
mod utilsprites;
mod window;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderableDimensions {
    pub cols: usize,
    pub viewport_rows: usize,
    pub scrollback_rows: usize,
    pub physical_top: StableRowIndex,
    pub scrollback_top: StableRowIndex,
}

pub struct GuiFrontEnd {
    connection: Rc<Connection>,
}

pub fn new() -> anyhow::Result<Rc<dyn FrontEnd>> {
    let front_end = GuiFrontEnd::new()?;
    Ok(front_end)
}

impl GuiFrontEnd {
    pub fn new() -> anyhow::Result<Rc<dyn FrontEnd>> {
        let connection = Connection::init()?;
        let front_end = Rc::new(GuiFrontEnd { connection });
        Ok(front_end)
    }
}

pub trait FrontEnd {
    fn run_forever(&self) -> anyhow::Result<()>;
    fn spawn_new_window(&self, fontconfig: &Rc<FontConfiguration>) -> anyhow::Result<()>;
}

impl FrontEnd for GuiFrontEnd {
    fn run_forever(&self) -> anyhow::Result<()> {
        self.connection.schedule_timer(std::time::Duration::from_millis(200), move || {
            let mux = Mux::get().unwrap();
            if mux.can_close() {
                Connection::get().unwrap().terminate_message_loop();
            }
        });

        self.connection.run_message_loop()
    }

    fn spawn_new_window(&self, fontconfig: &Rc<FontConfiguration>) -> anyhow::Result<()> {
        window::TermWindow::new_window(fontconfig)
    }
}
