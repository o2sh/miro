use crate::window::os::Connection;
use crate::window::spawn;
use std::cell::RefCell;
use std::rc::Rc;

pub const FPS: u32 = 60;

thread_local! {
    static CONN: RefCell<Option<Rc<Connection>>> = RefCell::new(None);
}

pub trait ConnectionOps {
    fn get() -> Option<Rc<Connection>> {
        let mut res = None;
        CONN.with(|m| {
            if let Some(mux) = &*m.borrow() {
                res = Some(Rc::clone(mux));
            }
        });
        res
    }

    fn init() -> anyhow::Result<Rc<Connection>> {
        let conn = Rc::new(Connection::create_new()?);
        CONN.with(|m| *m.borrow_mut() = Some(Rc::clone(&conn)));
        spawn::SPAWN_QUEUE.register_promise_schedulers();
        Ok(conn)
    }

    fn terminate_message_loop(&self);
    fn run_message_loop(&self) -> anyhow::Result<()>;
    fn schedule_timer<F: FnMut() + 'static>(&self, interval: std::time::Duration, callback: F);
}
