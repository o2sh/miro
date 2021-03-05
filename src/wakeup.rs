use failure::Error;
use glium::glutin::event_loop::EventLoopProxy;
use std::sync::mpsc::{channel, Receiver, Sender};

#[derive(Debug, Copy, Clone)]
pub enum WakeupMsg {
    PtyReadable,
    SigChld,
    Paint,
    Paste,
}

#[derive(Clone)]
pub struct Wakeup {
    sender: Sender<WakeupMsg>,
    proxy: EventLoopProxy<WakeupMsg>,
}

impl Wakeup {
    pub fn new(proxy: EventLoopProxy<WakeupMsg>) -> (Receiver<WakeupMsg>, Self) {
        let (sender, receiver) = channel();
        (receiver, Self { sender, proxy })
    }
    pub fn send(&mut self, what: WakeupMsg) -> Result<(), Error> {
        self.sender.send(what)?;
        self.proxy.send_event(what)?;
        Ok(())
    }
}
