use failure::{format_err, Fallible};
use serde_derive::*;
use std::ffi::{OsStr, OsString};

/// `CommandBuilder` is used to prepare a command to be spawned into a pty.
/// The interface is intentionally similar to that of `std::process::Command`.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandBuilder {
    args: Vec<OsString>,
    envs: Vec<(OsString, OsString)>,
}

impl CommandBuilder {
    /// Create a new builder instance with argv[0] set to the specified
    /// program.
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self { args: vec![program.as_ref().to_owned()], envs: vec![] }
    }

    /// Create a new builder instance that will run some idea of a default
    /// program.  Such a builder will panic if `arg` is called on it.
    pub fn new_default_prog() -> Self {
        Self { args: vec![], envs: vec![] }
    }

    /// Returns true if this builder was created via `new_default_prog`
    pub fn is_default_prog(&self) -> bool {
        self.args.is_empty()
    }

    /// Append an argument to the current command line.
    /// Will panic if called on a builder created via `new_default_prog`.
    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) {
        if self.is_default_prog() {
            panic!("attempted to add args to a default_prog builder");
        }
        self.args.push(arg.as_ref().to_owned());
    }

    /// Append a sequence of arguments to the current command line
    pub fn args<I, S>(&mut self, args: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for arg in args {
            self.arg(arg);
        }
    }

    /// Override the value of an environmental variable
    pub fn env<K, V>(&mut self, key: K, val: V)
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.envs.push((key.as_ref().to_owned(), val.as_ref().to_owned()));
    }
}

impl CommandBuilder {
    /// Convert the CommandBuilder to a `std::process::Command` instance.
    pub(crate) fn as_command(&self) -> Fallible<std::process::Command> {
        let mut cmd = if self.is_default_prog() {
            let mut cmd = std::process::Command::new(&Self::get_shell()?);
            cmd.current_dir(Self::get_home_dir()?);
            cmd
        } else {
            let mut cmd = std::process::Command::new(&self.args[0]);
            cmd.args(&self.args[1..]);
            cmd
        };

        for (key, val) in &self.envs {
            cmd.env(key, val);
        }

        Ok(cmd)
    }

    /// Determine which shell to run.
    /// We take the contents of the $SHELL env var first, then
    /// fall back to looking it up from the password database.
    fn get_shell() -> Fallible<String> {
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
                    .map_err(|e| format_err!("failed to resolve shell: {:?}", e))
            }
        })
    }

    fn get_home_dir() -> Fallible<String> {
        std::env::var("HOME").or_else(|_| {
            let ent = unsafe { libc::getpwuid(libc::getuid()) };

            if ent.is_null() {
                Ok("/".into())
            } else {
                use std::ffi::CStr;
                use std::str;
                let home = unsafe { CStr::from_ptr((*ent).pw_dir) };
                home.to_str()
                    .map(str::to_owned)
                    .map_err(|e| format_err!("failed to resolve home dir: {:?}", e))
            }
        })
    }
}
