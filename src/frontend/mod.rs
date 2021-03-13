use crate::config::Config;
use crate::core::promise::Executor;
use crate::font::FontConfiguration;
use crate::mux::tab::Tab;
use crate::mux::window::WindowId;
use downcast_rs::{impl_downcast, Downcast};
use failure::{Error, Fallible};
use lazy_static::lazy_static;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

pub mod gui;
pub mod muxserver;

lazy_static! {
    static ref EXECUTOR: Mutex<Option<Box<dyn Executor>>> = Mutex::new(None);
}
thread_local! {
    static FRONT_END: RefCell<Option<Rc<dyn FrontEnd>>> = RefCell::new(None);
}

pub fn executor() -> Box<dyn Executor> {
    let locked = EXECUTOR.lock().unwrap();
    match locked.as_ref() {
        Some(exec) => exec.clone_executor(),
        None => panic!("executor machinery not yet configured"),
    }
}

pub fn front_end() -> Option<Rc<dyn FrontEnd>> {
    let mut res = None;
    FRONT_END.with(|f| {
        if let Some(me) = &*f.borrow() {
            res = Some(Rc::clone(me));
        }
    });
    res
}

pub fn try_new() -> Result<Rc<dyn FrontEnd>, Error> {
    let front_end = gui::GuiFrontEnd::try_new()?;

    EXECUTOR.lock().unwrap().replace(front_end.executor());
    FRONT_END.with(|f| *f.borrow_mut() = Some(Rc::clone(&front_end)));

    Ok(front_end)
}

pub trait FrontEnd: Downcast {
    /// Run the event loop.  Does not return until there is either a fatal
    /// error, or until there are no more windows left to manage.
    fn run_forever(&self) -> Result<(), Error>;

    fn spawn_new_window(
        &self,
        config: &Arc<Config>,
        fontconfig: &Rc<FontConfiguration>,
        tab: &Rc<dyn Tab>,
        window_id: WindowId,
    ) -> Fallible<()>;

    fn executor(&self) -> Box<dyn Executor>;
}
impl_downcast!(FrontEnd);
