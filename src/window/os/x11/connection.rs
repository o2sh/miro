use super::keyboard::Keyboard;
use crate::core::promise::BasicExecutor;
use crate::window::connection::{ConnectionOps, FPS};
use crate::window::os::x11::WindowInner;
use crate::window::spawn::*;
use crate::window::tasks::{Task, Tasks};
use failure::Fallible;
use mio::unix::EventedFd;
use mio::{Evented, Events, Poll, PollOpt, Ready, Token};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};
use std::os::unix::io::AsRawFd;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use xcb_util::ffi::keysyms::{xcb_key_symbols_alloc, xcb_key_symbols_free, xcb_key_symbols_t};

struct TimerEntry {
    callback: Box<dyn FnMut()>,
    due: Instant,
    interval: Duration,
}

#[derive(Default)]
struct TimerList {
    timers: VecDeque<TimerEntry>,
}

impl TimerList {
    pub fn new() -> Self {
        Default::default()
    }

    fn find_index_after(&self, due: &Instant) -> usize {
        for (idx, entry) in self.timers.iter().enumerate() {
            if entry.due.cmp(due) == Ordering::Greater {
                return idx;
            }
        }
        self.timers.len()
    }

    pub fn insert(&mut self, mut entry: TimerEntry) {
        entry.due = Instant::now() + entry.interval;
        let idx = self.find_index_after(&entry.due);
        self.timers.insert(idx, entry);
    }

    pub fn time_until_due(&self, now: Instant) -> Option<Duration> {
        self.timers.front().map(|entry| {
            if entry.due <= now {
                Duration::from_secs(0)
            } else {
                entry.due - now
            }
        })
    }

    fn first_is_ready(&self, now: Instant) -> bool {
        if let Some(first) = self.timers.front() {
            first.due <= now
        } else {
            false
        }
    }

    pub fn run_ready(&mut self) {
        let now = Instant::now();
        let mut requeue = vec![];
        while self.first_is_ready(now) {
            let mut first = self.timers.pop_front().expect("first_is_ready");
            (first.callback)();
            requeue.push(first);
        }

        for entry in requeue.into_iter() {
            self.insert(entry);
        }
    }
}

pub struct Connection {
    pub display: *mut x11::xlib::Display,
    conn: xcb::Connection,
    screen_num: i32,
    pub keyboard: Keyboard,
    pub kbd_ev: u8,
    pub atom_protocols: xcb::Atom,
    pub cursor_font_id: xcb::ffi::xcb_font_t,
    pub atom_delete: xcb::Atom,
    pub atom_utf8_string: xcb::Atom,
    pub atom_xsel_data: xcb::Atom,
    pub atom_targets: xcb::Atom,
    pub atom_clipboard: xcb::Atom,
    keysyms: *mut xcb_key_symbols_t,
    pub(crate) windows: RefCell<HashMap<xcb::xproto::Window, Arc<Mutex<WindowInner>>>>,
    should_terminate: RefCell<bool>,
    tasks: Tasks,
    timers: RefCell<TimerList>,
    pub(crate) visual: xcb::xproto::Visualtype,
}

impl std::ops::Deref for Connection {
    type Target = xcb::Connection;

    fn deref(&self) -> &xcb::Connection {
        &self.conn
    }
}

impl Evented for Connection {
    fn register(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> std::io::Result<()> {
        EventedFd(&self.conn.as_raw_fd()).register(poll, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> std::io::Result<()> {
        EventedFd(&self.conn.as_raw_fd()).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> std::io::Result<()> {
        EventedFd(&self.conn.as_raw_fd()).deregister(poll)
    }
}

#[link(name = "X11-xcb")]
extern "C" {
    fn XGetXCBConnection(display: *mut x11::xlib::Display) -> *mut xcb::ffi::xcb_connection_t;
    fn XSetEventQueueOwner(display: *mut x11::xlib::Display, owner: i32);
}

fn window_id_from_event(event: &xcb::GenericEvent) -> Option<xcb::xproto::Window> {
    match event.response_type() & 0x7f {
        xcb::CONFIGURE_NOTIFY => {
            let cfg: &xcb::ConfigureNotifyEvent = unsafe { xcb::cast_event(event) };
            Some(cfg.window())
        }
        xcb::KEY_PRESS | xcb::KEY_RELEASE => {
            let key_press: &xcb::KeyPressEvent = unsafe { xcb::cast_event(event) };
            Some(key_press.event())
        }
        xcb::MOTION_NOTIFY => {
            let motion: &xcb::MotionNotifyEvent = unsafe { xcb::cast_event(event) };
            Some(motion.event())
        }
        xcb::BUTTON_PRESS | xcb::BUTTON_RELEASE => {
            let button_press: &xcb::ButtonPressEvent = unsafe { xcb::cast_event(event) };
            Some(button_press.event())
        }
        xcb::CLIENT_MESSAGE => {
            let msg: &xcb::ClientMessageEvent = unsafe { xcb::cast_event(event) };
            Some(msg.window())
        }
        xcb::DESTROY_NOTIFY => {
            let msg: &xcb::DestroyNotifyEvent = unsafe { xcb::cast_event(event) };
            Some(msg.window())
        }
        xcb::FOCUS_IN => {
            let msg: &xcb::FocusInEvent = unsafe { xcb::cast_event(event) };
            Some(msg.event())
        }
        xcb::FOCUS_OUT => {
            let msg: &xcb::FocusOutEvent = unsafe { xcb::cast_event(event) };
            Some(msg.event())
        }
        _ => None,
    }
}

impl ConnectionOps for Connection {
    fn spawn_task<F: std::future::Future<Output = ()> + 'static>(&self, future: F) {
        let id = self.tasks.add_task(Task(Box::pin(future)));
        Self::wake_task_by_id(id);
    }

    fn wake_task_by_id(slot: usize) {
        SpawnQueueExecutor {}.execute(Box::new(move || {
            let conn = Connection::get().unwrap();
            conn.tasks.poll_by_slot(slot);
        }));
    }

    fn terminate_message_loop(&self) {
        *self.should_terminate.borrow_mut() = true;
    }

    fn run_message_loop(&self) -> Fallible<()> {
        self.conn.flush();

        const TOK_XCB: usize = 0xffff_fffc;
        const TOK_SPAWN: usize = 0xffff_fffd;
        let tok_xcb = Token(TOK_XCB);
        let tok_spawn = Token(TOK_SPAWN);

        let poll = Poll::new()?;
        let mut events = Events::with_capacity(8);
        poll.register(self, tok_xcb, Ready::readable(), PollOpt::level())?;
        poll.register(&*SPAWN_QUEUE, tok_spawn, Ready::readable(), PollOpt::level())?;

        let paint_interval = Duration::from_micros(1_000_000 / FPS as u64);
        let mut last_interval = Instant::now();

        while !*self.should_terminate.borrow() {
            self.timers.borrow_mut().run_ready();

            let now = Instant::now();
            let diff = now - last_interval;
            let period = if diff >= paint_interval {
                self.do_paint();
                last_interval = now;
                paint_interval
            } else {
                paint_interval - diff
            };

            self.process_queued_xcb()?;

            let period = self
                .timers
                .borrow()
                .time_until_due(Instant::now())
                .map(|duration| duration.min(period))
                .unwrap_or(period);

            match poll.poll(&mut events, Some(period)) {
                Ok(_) => {
                    for event in &events {
                        let t = event.token();
                        if t == tok_xcb {
                            self.process_queued_xcb()?;
                        } else if t == tok_spawn {
                            SPAWN_QUEUE.run();
                        } else {
                        }
                    }
                }

                Err(err) => {
                    failure::bail!("polling for events: {:?}", err);
                }
            }
        }

        Ok(())
    }

    fn schedule_timer<F: FnMut() + 'static>(&self, interval: std::time::Duration, callback: F) {
        self.timers.borrow_mut().insert(TimerEntry {
            callback: Box::new(callback),
            due: Instant::now(),
            interval,
        });
    }
}

impl Connection {
    fn process_queued_xcb(&self) -> Fallible<()> {
        match self.conn.poll_for_event() {
            None => match self.conn.has_error() {
                Ok(_) => (),
                Err(err) => {
                    failure::bail!("X11 connection is broken: {:?}", err);
                }
            },
            Some(event) => {
                if let Err(err) = self.process_xcb_event(&event) {
                    return Err(err);
                }
            }
        }
        self.conn.flush();

        loop {
            match self.conn.poll_for_queued_event() {
                None => return Ok(()),
                Some(event) => self.process_xcb_event(&event)?,
            }
            self.conn.flush();
        }
    }

    fn process_xcb_event(&self, event: &xcb::GenericEvent) -> Fallible<()> {
        if let Some(window_id) = window_id_from_event(event) {
            self.process_window_event(window_id, event)?;
        } else {
            let r = event.response_type() & 0x7f;
            if r == self.kbd_ev {
                self.keyboard.process_xkb_event(&self.conn, event)?;
            }
        }
        Ok(())
    }

    fn window_by_id(&self, window_id: xcb::xproto::Window) -> Option<Arc<Mutex<WindowInner>>> {
        self.windows.borrow().get(&window_id).map(Arc::clone)
    }

    fn process_window_event(
        &self,
        window_id: xcb::xproto::Window,
        event: &xcb::GenericEvent,
    ) -> Fallible<()> {
        if let Some(window) = self.window_by_id(window_id) {
            let mut inner = window.lock().unwrap();
            inner.dispatch_event(event)?;
        }
        Ok(())
    }

    pub(crate) fn create_new() -> Fallible<Connection> {
        let display = unsafe { x11::xlib::XOpenDisplay(std::ptr::null()) };
        if display.is_null() {
            failure::bail!("failed to open display");
        }
        let screen_num = unsafe { x11::xlib::XDefaultScreen(display) };
        let conn = unsafe { xcb::Connection::from_raw_conn(XGetXCBConnection(display)) };
        unsafe { XSetEventQueueOwner(display, 1) };

        let atom_protocols = xcb::intern_atom(&conn, false, "WM_PROTOCOLS").get_reply()?.atom();
        let atom_delete = xcb::intern_atom(&conn, false, "WM_DELETE_WINDOW").get_reply()?.atom();
        let atom_utf8_string = xcb::intern_atom(&conn, false, "UTF8_STRING").get_reply()?.atom();
        let atom_xsel_data = xcb::intern_atom(&conn, false, "XSEL_DATA").get_reply()?.atom();
        let atom_targets = xcb::intern_atom(&conn, false, "TARGETS").get_reply()?.atom();
        let atom_clipboard = xcb::intern_atom(&conn, false, "CLIPBOARD").get_reply()?.atom();

        let keysyms = unsafe { xcb_key_symbols_alloc(conn.get_raw_conn()) };

        let screen = conn
            .get_setup()
            .roots()
            .nth(screen_num as usize)
            .ok_or_else(|| failure::err_msg("no screen?"))?;

        let visual = screen
            .allowed_depths()
            .filter(|depth| depth.depth() == 24)
            .flat_map(|depth| depth.visuals())
            .filter_map(|vis| {
                if vis.class() == xcb::xproto::VISUAL_CLASS_TRUE_COLOR as u8 {
                    Some(vis.clone())
                } else {
                    None
                }
            })
            .nth(0)
            .ok_or_else(|| failure::err_msg("did not find 24-bit visual"))?;
        eprintln!(
            "picked visual {:x}, screen root visual is {:x}",
            visual.visual_id(),
            screen.root_visual()
        );

        let (keyboard, kbd_ev) = Keyboard::new(&conn)?;

        let cursor_font_id = conn.generate_id();
        let cursor_font_name = "cursor";
        xcb::open_font_checked(&conn, cursor_font_id, cursor_font_name);

        let conn = Connection {
            display,
            conn,
            cursor_font_id,
            screen_num,
            atom_protocols,
            atom_clipboard,
            atom_delete,
            keysyms,
            keyboard,
            kbd_ev,
            atom_utf8_string,
            atom_xsel_data,
            atom_targets,
            windows: RefCell::new(HashMap::new()),
            should_terminate: RefCell::new(false),
            tasks: Default::default(),
            timers: RefCell::new(TimerList::new()),
            visual,
        };

        Ok(conn)
    }

    pub fn conn(&self) -> &xcb::Connection {
        &self.conn
    }

    pub fn screen_num(&self) -> i32 {
        self.screen_num
    }

    pub fn atom_delete(&self) -> xcb::Atom {
        self.atom_delete
    }

    fn do_paint(&self) {
        for window in self.windows.borrow().values() {
            window.lock().unwrap().paint().unwrap();
        }
        self.conn.flush();
    }

    pub fn executor() -> impl BasicExecutor {
        SpawnQueueExecutor {}
    }

    pub(crate) fn with_window_inner<F: FnMut(&mut WindowInner) + Send + 'static>(
        window: xcb::xproto::Window,
        mut f: F,
    ) {
        SpawnQueueExecutor {}.execute(Box::new(move || {
            if let Some(handle) = Connection::get().unwrap().window_by_id(window) {
                let mut inner = handle.lock().unwrap();
                f(&mut inner);
            }
        }));
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            xcb_key_symbols_free(self.keysyms);
        }
    }
}
