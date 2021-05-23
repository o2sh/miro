use crate::core::promise::{self, SpawnFunc};
#[cfg(target_os = "macos")]
use core_foundation::runloop::*;
use failure::Fallible;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
#[cfg(all(unix, not(target_os = "macos")))]
use {
    filedescriptor::{FileDescriptor, Pipe},
    mio::unix::EventedFd,
    mio::{Evented, Poll, PollOpt, Ready, Token},
    std::os::unix::io::AsRawFd,
};

lazy_static::lazy_static! {
    pub(crate) static ref SPAWN_QUEUE: Arc<SpawnQueue> = Arc::new(SpawnQueue::new().expect("failed to create SpawnQueue"));
}

pub(crate) struct SpawnQueue {
    spawned_funcs: Mutex<VecDeque<SpawnFunc>>,
    spawned_funcs_low_pri: Mutex<VecDeque<SpawnFunc>>,
    #[cfg(all(unix, not(target_os = "macos")))]
    write: Mutex<FileDescriptor>,
    #[cfg(all(unix, not(target_os = "macos")))]
    read: Mutex<FileDescriptor>,
}

impl SpawnQueue {
    pub fn new() -> Fallible<Self> {
        Self::new_impl()
    }

    pub fn register_promise_schedulers(&self) {
        promise::set_schedulers(
            Box::new(|task| {
                SPAWN_QUEUE.spawn_impl(Box::new(move || task.run()), true);
            }),
            Box::new(|low_pri_task| {
                SPAWN_QUEUE.spawn_impl(Box::new(move || low_pri_task.run()), false);
            }),
        );
    }

    pub fn run(&self) -> bool {
        self.run_impl()
    }

    fn pop_func(&self) -> Option<SpawnFunc> {
        if let Some(func) = self.spawned_funcs.lock().unwrap().pop_front() {
            Some(func)
        } else if let Some(func) = self.spawned_funcs_low_pri.lock().unwrap().pop_front() {
            Some(func)
        } else {
            None
        }
    }

    fn queue_func(&self, f: SpawnFunc, high_pri: bool) {
        if high_pri {
            self.spawned_funcs.lock().unwrap()
        } else {
            self.spawned_funcs_low_pri.lock().unwrap()
        }
        .push_back(f);
    }

    fn has_any_queued(&self) -> bool {
        !self.spawned_funcs.lock().unwrap().is_empty()
            || !self.spawned_funcs_low_pri.lock().unwrap().is_empty()
    }
}

#[cfg(not(target_os = "macos"))]
impl SpawnQueue {
    fn new_impl() -> Fallible<Self> {
        let mut pipe = match Pipe::new() {
            Ok(v) => v,
            Err(_) => bail!(""),
        };
        pipe.write.set_non_blocking(true).unwrap();
        pipe.read.set_non_blocking(true).unwrap();
        Ok(Self {
            spawned_funcs: Mutex::new(VecDeque::new()),
            spawned_funcs_low_pri: Mutex::new(VecDeque::new()),
            write: Mutex::new(pipe.write),
            read: Mutex::new(pipe.read),
        })
    }

    fn spawn_impl(&self, f: SpawnFunc, high_pri: bool) {
        use std::io::Write;

        self.queue_func(f, high_pri);
        self.write.lock().unwrap().write(b"x").ok();
    }

    fn run_impl(&self) -> bool {
        if let Some(func) = self.pop_func() {
            func();
        }

        let mut byte = [0u8; 64];
        use std::io::Read;
        self.read.lock().unwrap().read(&mut byte).ok();

        self.has_any_queued()
    }

    pub(crate) fn raw_fd(&self) -> std::os::unix::io::RawFd {
        self.read.lock().unwrap().as_raw_fd()
    }
}

#[cfg(not(target_os = "macos"))]
impl Evented for SpawnQueue {
    fn register(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> std::io::Result<()> {
        EventedFd(&self.raw_fd()).register(poll, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> std::io::Result<()> {
        EventedFd(&self.raw_fd()).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> std::io::Result<()> {
        EventedFd(&self.raw_fd()).deregister(poll)
    }
}

#[cfg(target_os = "macos")]
impl SpawnQueue {
    fn new_impl() -> Fallible<Self> {
        let spawned_funcs = Mutex::new(VecDeque::new());
        let spawned_funcs_low_pri = Mutex::new(VecDeque::new());

        let observer = unsafe {
            CFRunLoopObserverCreate(
                std::ptr::null(),
                kCFRunLoopAllActivities,
                1,
                0,
                SpawnQueue::trigger,
                std::ptr::null_mut(),
            )
        };
        unsafe {
            CFRunLoopAddObserver(CFRunLoopGetMain(), observer, kCFRunLoopCommonModes);
        }

        Ok(Self { spawned_funcs, spawned_funcs_low_pri })
    }

    extern "C" fn trigger(
        _observer: *mut __CFRunLoopObserver,
        _: CFRunLoopActivity,
        _: *mut std::ffi::c_void,
    ) {
        if SPAWN_QUEUE.run() {
            Self::queue_wakeup();
        }
    }

    fn queue_wakeup() {
        unsafe {
            CFRunLoopWakeUp(CFRunLoopGetMain());
        }
    }

    fn spawn_impl(&self, f: SpawnFunc, high_pri: bool) {
        self.queue_func(f, high_pri);
        Self::queue_wakeup();
    }

    fn run_impl(&self) -> bool {
        if let Some(func) = self.pop_func() {
            func();
        }
        self.has_any_queued()
    }
}
