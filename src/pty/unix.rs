use crate::pty::{Child, MasterPty, PtyPair, PtySize, PtySystem, SlavePty};
use anyhow::bail;
use filedescriptor::FileDescriptor;
use libc::{self, winsize};
use std::io;
use std::mem;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::process::Stdio;
use std::ptr;

pub struct UnixPtySystem;

impl PtySystem for UnixPtySystem {
    fn openpty(&self, size: PtySize) -> anyhow::Result<PtyPair> {
        let mut master: RawFd = -1;
        let mut slave: RawFd = -1;

        let mut size = winsize {
            ws_row: size.rows,
            ws_col: size.cols,
            ws_xpixel: size.pixel_width,
            ws_ypixel: size.pixel_height,
        };

        let result = unsafe {
            #[cfg_attr(feature = "cargo-clippy", allow(clippy::unnecessary_mut_passed))]
            libc::openpty(&mut master, &mut slave, ptr::null_mut(), ptr::null_mut(), &mut size)
        };

        if result != 0 {
            bail!("failed to openpty: {:?}", io::Error::last_os_error());
        }

        let master = UnixMasterPty { fd: unsafe { FileDescriptor::from_raw_fd(master) } };
        let slave = UnixSlavePty { fd: unsafe { FileDescriptor::from_raw_fd(slave) } };

        cloexec(master.fd.as_raw_fd())?;
        cloexec(slave.fd.as_raw_fd())?;

        Ok(PtyPair { master: Box::new(master), slave: Box::new(slave) })
    }
}

pub struct UnixMasterPty {
    fd: FileDescriptor,
}

pub struct UnixSlavePty {
    fd: FileDescriptor,
}

fn cloexec(fd: RawFd) -> anyhow::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags == -1 {
        bail!("fcntl to read flags failed: {:?}", io::Error::last_os_error());
    }
    let result = unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) };
    if result == -1 {
        bail!("fcntl to set CLOEXEC failed: {:?}", io::Error::last_os_error());
    }
    Ok(())
}

impl SlavePty for UnixSlavePty {
    fn spawn_command(&self, mut cmd: Command) -> anyhow::Result<Box<dyn Child>> {
        unsafe {
            cmd.stdin(self.as_stdio()?).stdout(self.as_stdio()?).stderr(self.as_stdio()?).pre_exec(
                move || {
                    for signo in &[
                        libc::SIGCHLD,
                        libc::SIGHUP,
                        libc::SIGINT,
                        libc::SIGQUIT,
                        libc::SIGTERM,
                        libc::SIGALRM,
                    ] {
                        libc::signal(*signo, libc::SIG_DFL);
                    }

                    if libc::setsid() == -1 {
                        return Err(io::Error::last_os_error());
                    }

                    #[cfg_attr(feature = "cargo-clippy", allow(clippy::cast_lossless))]
                    {
                        if libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
                            return Err(io::Error::last_os_error());
                        }
                    }
                    Ok(())
                },
            )
        };

        let mut child = cmd.spawn()?;

        child.stdin.take();
        child.stdout.take();
        child.stderr.take();

        Ok(Box::new(child))
    }
}

impl UnixSlavePty {
    fn as_stdio(&self) -> anyhow::Result<Stdio> {
        let dup = match self.fd.try_clone() {
            Ok(v) => v,
            Err(_) => bail!(""),
        };
        Ok(unsafe { Stdio::from_raw_fd(dup.into_raw_fd()) })
    }
}

impl MasterPty for UnixMasterPty {
    fn resize(&self, size: PtySize) -> anyhow::Result<()> {
        let ws_size = winsize {
            ws_row: size.rows,
            ws_col: size.cols,
            ws_xpixel: size.pixel_width,
            ws_ypixel: size.pixel_height,
        };

        if unsafe { libc::ioctl(self.fd.as_raw_fd(), libc::TIOCSWINSZ, &ws_size as *const _) } != 0
        {
            bail!("failed to ioctl(TIOCSWINSZ): {:?}", io::Error::last_os_error());
        }

        Ok(())
    }

    fn get_size(&self) -> anyhow::Result<PtySize> {
        let mut size: winsize = unsafe { mem::zeroed() };
        if unsafe { libc::ioctl(self.fd.as_raw_fd(), libc::TIOCGWINSZ, &mut size as *mut _) } != 0 {
            bail!("failed to ioctl(TIOCGWINSZ): {:?}", io::Error::last_os_error());
        }
        Ok(PtySize {
            rows: size.ws_row,
            cols: size.ws_col,
            pixel_width: size.ws_xpixel,
            pixel_height: size.ws_ypixel,
        })
    }

    fn try_clone_reader(&self) -> anyhow::Result<Box<dyn std::io::Read + Send>> {
        let fd = match self.fd.try_clone() {
            Ok(v) => v,
            Err(_) => bail!(""),
        };
        Ok(Box::new(fd))
    }
}

impl io::Write for UnixMasterPty {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        self.fd.write(buf)
    }
    fn flush(&mut self) -> Result<(), io::Error> {
        self.fd.flush()
    }
}
