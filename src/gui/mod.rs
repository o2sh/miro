use crate::core::promise::{BasicExecutor, Executor, SpawnFunc};
use crate::font::FontConfiguration;
use crate::mux::Mux;
use crate::window::*;
use downcast_rs::{impl_downcast, Downcast};
use failure::{Error, Fallible};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

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

pub fn new() -> Result<Rc<dyn FrontEnd>, Error> {
    let front_end = GuiFrontEnd::new()?;

    EXECUTOR.lock().unwrap().replace(front_end.executor());
    FRONT_END.with(|f| *f.borrow_mut() = Some(Rc::clone(&front_end)));

    Ok(front_end)
}

pub trait FrontEnd: Downcast {
    fn run_forever(&self) -> Result<(), Error>;

    fn spawn_new_window(&self, fontconfig: &Rc<FontConfiguration>) -> Fallible<()>;

    fn executor(&self) -> Box<dyn Executor>;
}

impl_downcast!(FrontEnd);

impl GuiFrontEnd {
    pub fn new() -> Fallible<Rc<dyn FrontEnd>> {
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
        self.connection.schedule_timer(std::time::Duration::from_millis(200), move || {
            let mux = Mux::get().unwrap();
            if mux.is_empty() {
                Connection::get().unwrap().terminate_message_loop();
            }
        });

        self.connection.run_message_loop()
    }

    fn spawn_new_window(&self, fontconfig: &Rc<FontConfiguration>) -> Fallible<()> {
        termwindow::TermWindow::new_window(fontconfig)
    }
}
