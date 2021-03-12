use crate::server::UnixStream;
use crossbeam_channel::Sender;
use failure::{format_err, Fallible};
use filedescriptor::*;
use std::cell::RefCell;
use std::io::Write;

pub trait ReadAndWrite: std::io::Read + std::io::Write + Send + AsPollFd {
    fn set_non_blocking(&self, non_blocking: bool) -> Fallible<()>;
    fn has_read_buffered(&self) -> bool;
}
impl ReadAndWrite for UnixStream {
    fn set_non_blocking(&self, non_blocking: bool) -> Fallible<()> {
        self.set_nonblocking(non_blocking)?;
        Ok(())
    }
    fn has_read_buffered(&self) -> bool {
        false
    }
}

pub struct PollableSender<T> {
    sender: Sender<T>,
    write: RefCell<FileDescriptor>,
}

impl<T> PollableSender<T> {
    pub fn send(&self, item: T) -> Fallible<()> {
        self.write.borrow_mut().write_all(b"x")?;
        self.sender.send(item).map_err(|e| format_err!("{}", e))?;
        Ok(())
    }
}

impl<T> Clone for PollableSender<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            write: RefCell::new(
                self.write.borrow().try_clone().expect("failed to clone PollableSender fd"),
            ),
        }
    }
}

pub struct PollableReceiver {
    read: RefCell<FileDescriptor>,
}

impl AsPollFd for PollableReceiver {
    fn as_poll_fd(&self) -> pollfd {
        self.read.borrow().as_socket_descriptor().as_poll_fd()
    }
}

pub trait AsPollFd {
    fn as_poll_fd(&self) -> pollfd;
}

impl AsPollFd for SocketDescriptor {
    fn as_poll_fd(&self) -> pollfd {
        pollfd { fd: *self, events: POLLIN, revents: 0 }
    }
}

impl AsPollFd for UnixStream {
    fn as_poll_fd(&self) -> pollfd {
        self.as_socket_descriptor().as_poll_fd()
    }
}
