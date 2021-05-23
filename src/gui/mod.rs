use crate::font::FontConfiguration;
use crate::mux::Mux;
use crate::window::*;
use failure::{Error, Fallible};
use std::rc::Rc;

mod glyphcache;
mod header;
mod quad;
mod renderstate;
mod spritesheet;
mod utilsprites;
mod window;

pub struct GuiFrontEnd {
    connection: Rc<Connection>,
}

pub fn new() -> Result<Rc<dyn FrontEnd>, Error> {
    let front_end = GuiFrontEnd::new()?;
    Ok(front_end)
}

impl GuiFrontEnd {
    pub fn new() -> Fallible<Rc<dyn FrontEnd>> {
        let connection = Connection::init()?;
        let front_end = Rc::new(GuiFrontEnd { connection });
        Ok(front_end)
    }
}

pub trait FrontEnd {
    fn run_forever(&self) -> Result<(), Error>;
    fn spawn_new_window(&self, fontconfig: &Rc<FontConfiguration>) -> Fallible<()>;
}

impl FrontEnd for GuiFrontEnd {
    fn run_forever(&self) -> Fallible<()> {
        self.connection.schedule_timer(std::time::Duration::from_millis(200), move || {
            let mux = Mux::get().unwrap();
            if mux.can_close() {
                Connection::get().unwrap().terminate_message_loop();
            }
        });

        self.connection.run_message_loop()
    }

    fn spawn_new_window(&self, fontconfig: &Rc<FontConfiguration>) -> Fallible<()> {
        window::TermWindow::new_window(fontconfig)
    }
}
