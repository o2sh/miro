use mio::event::Evented;
use mio::{Poll, PollOpt, Ready, Registration, Token};

use std::io;
use std::thread;
use std::time::{Duration, SystemTime};

pub const FPS: u32 = 60;
pub const MIN_FPS: u32 = 15;

pub struct Deadline {
    registration: Registration,
}

impl Deadline {
    pub fn new() -> Deadline {
        let (registration, set_readiness) = Registration::new2();
        let mut last_update_time = SystemTime::now();
        thread::spawn(move || loop {
            let _ = set_readiness.set_readiness(Ready::readable());
            let duration = Duration::from_micros(1_000_000 / FPS as u64);
            let next_update_time = last_update_time + duration;

            let now = SystemTime::now();
            if now < next_update_time {
                let wait = next_update_time.duration_since(now).expect("");
                thread::sleep(wait);
                last_update_time = next_update_time;
            } else {
                let late = now.duration_since(next_update_time).expect("");
                let skip_count =
                    (late.as_millis() as f32 / duration.as_millis() as f32).floor() as u32;
                if skip_count <= FPS / MIN_FPS {
                    last_update_time = next_update_time + duration * skip_count;
                } else {
                    last_update_time = now;
                }
            }
        });

        Deadline { registration: registration }
    }
}

impl Evented for Deadline {
    fn register(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        self.registration.register(poll, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        self.registration.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.registration.deregister(poll)
    }
}
