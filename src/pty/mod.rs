use anyhow::anyhow;
use serde_derive::*;
use std::io::Result as IoResult;
use std::process::Command;

pub mod unix;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PtySize {
    pub rows: u16,

    pub cols: u16,

    pub pixel_width: u16,

    pub pixel_height: u16,
}

impl Default for PtySize {
    fn default() -> Self {
        PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 }
    }
}

pub trait MasterPty: std::io::Write {
    fn resize(&self, size: PtySize) -> anyhow::Result<()>;

    fn get_size(&self) -> anyhow::Result<PtySize>;

    fn try_clone_reader(&self) -> anyhow::Result<Box<dyn std::io::Read + Send>>;
}

pub trait Child: std::fmt::Debug {
    fn try_wait(&mut self) -> IoResult<Option<ExitStatus>>;

    fn kill(&mut self) -> IoResult<()>;

    fn wait(&mut self) -> IoResult<ExitStatus>;
}

pub trait SlavePty {
    fn spawn_command(&self, cmd: Command) -> anyhow::Result<Box<dyn Child>>;
}

#[derive(Debug)]
pub struct ExitStatus {
    successful: bool,
}

impl From<std::process::ExitStatus> for ExitStatus {
    fn from(status: std::process::ExitStatus) -> ExitStatus {
        ExitStatus { successful: status.success() }
    }
}

pub struct PtyPair {
    pub slave: Box<dyn SlavePty>,
    pub master: Box<dyn MasterPty>,
}

pub trait PtySystem {
    fn openpty(&self, size: PtySize) -> anyhow::Result<PtyPair>;
}

impl Child for std::process::Child {
    fn try_wait(&mut self) -> IoResult<Option<ExitStatus>> {
        std::process::Child::try_wait(self).map(|s| match s {
            Some(s) => Some(s.into()),
            None => None,
        })
    }

    fn kill(&mut self) -> IoResult<()> {
        std::process::Child::kill(self)
    }

    fn wait(&mut self) -> IoResult<ExitStatus> {
        std::process::Child::wait(self).map(Into::into)
    }
}

pub fn get_shell() -> anyhow::Result<String> {
    std::env::var("SHELL").or_else(|_| {
        let ent = unsafe { libc::getpwuid(libc::getuid()) };

        if ent.is_null() {
            Ok("/bin/sh".into())
        } else {
            use std::ffi::CStr;
            use std::str;
            let shell = unsafe { CStr::from_ptr((*ent).pw_shell) };
            shell
                .to_str()
                .map(str::to_owned)
                .map_err(|e| anyhow!("failed to resolve shell: {:?}", e))
        }
    })
}
