use crate::config::Config;
use crate::core::promise::{BasicExecutor, Executor, SpawnFunc};
use crate::font::FontConfiguration;
use crate::mux::tab::Tab;
use crate::mux::window::WindowId;
use crate::mux::window::WindowId as MuxWindowId;
use crate::mux::Mux;
use crate::window::*;
use downcast_rs::{impl_downcast, Downcast};
use failure::{Error, Fallible};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod glyphcache;
mod quad;
mod renderstate;
mod spritesheet;
mod termwindow;
mod utilsprites;

pub struct GuiFrontEnd {
    connection: Rc<Connection>,
}

lazy_static::lazy_static! {
static ref USE_OPENGL: AtomicBool = AtomicBool::new(true);
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
    let front_end = GuiFrontEnd::try_new()?;

    EXECUTOR.lock().unwrap().replace(front_end.executor());
    FRONT_END.with(|f| *f.borrow_mut() = Some(Rc::clone(&front_end)));

    Ok(front_end)
}

pub trait FrontEnd: Downcast {
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

impl GuiFrontEnd {
    pub fn try_new() -> Fallible<Rc<dyn FrontEnd>> {
        let connection = Connection::init()?;
        let front_end = Rc::new(GuiFrontEnd { connection });
        Ok(front_end)
    }
}

struct GuiExecutor {}
impl BasicExecutor for GuiExecutor {
    fn execute(&self, f: SpawnFunc) {
        Connection::executor().execute(f)
    }
}

impl Executor for GuiExecutor {
    fn clone_executor(&self) -> Box<dyn Executor> {
        Box::new(GuiExecutor {})
    }
}

impl FrontEnd for GuiFrontEnd {
    fn executor(&self) -> Box<dyn Executor> {
        Box::new(GuiExecutor {})
    }

    fn run_forever(&self) -> Fallible<()> {
        struct State {
            when: Option<Instant>,
        }

        impl State {
            fn mark(&mut self, is_empty: bool) {
                if is_empty {
                    let now = Instant::now();
                    if let Some(start) = self.when.as_ref() {
                        let diff = now - *start;
                        if diff > Duration::new(5, 0) {
                            Connection::get().unwrap().terminate_message_loop();
                        }
                    } else {
                        self.when = Some(now);
                    }
                } else {
                    self.when = None;
                }
            }
        }

        let state = Arc::new(Mutex::new(State { when: None }));

        self.connection.schedule_timer(std::time::Duration::from_millis(200), move || {
            let mux = Mux::get().unwrap();
            mux.prune_dead_windows();
            state.lock().unwrap().mark(mux.is_empty());
        });

        self.connection.run_message_loop()
    }

    fn spawn_new_window(
        &self,
        config: &Arc<Config>,
        fontconfig: &Rc<FontConfiguration>,
        tab: &Rc<dyn Tab>,
        window_id: MuxWindowId,
    ) -> Fallible<()> {
        termwindow::TermWindow::new_window(config, fontconfig, tab, window_id)
    }
}
