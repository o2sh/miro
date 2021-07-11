use anyhow::anyhow;
use clipboard::{ClipboardContext, ClipboardProvider};
use std::sync::Mutex;

pub trait Clipboard {
    fn get_contents(&self) -> anyhow::Result<String>;
    fn set_contents(&self, data: Option<String>) -> anyhow::Result<()>;
}

pub struct SystemClipboard {
    inner: Mutex<Inner>,
}

struct Inner {
    clipboard: Option<ClipboardContext>,
}

impl Inner {
    fn new() -> Self {
        Self { clipboard: None }
    }

    fn clipboard(&mut self) -> anyhow::Result<&mut ClipboardContext> {
        if self.clipboard.is_none() {
            self.clipboard = Some(ClipboardContext::new().map_err(|e| anyhow!("{}", e))?);
        }
        Ok(self.clipboard.as_mut().unwrap())
    }
}

impl SystemClipboard {
    pub fn new() -> Self {
        Self { inner: Mutex::new(Inner::new()) }
    }
}

impl Clipboard for SystemClipboard {
    fn get_contents(&self) -> anyhow::Result<String> {
        let mut inner = self.inner.lock().unwrap();
        inner.clipboard()?.get_contents().map_err(|e| anyhow!("{}", e))
    }

    fn set_contents(&self, data: Option<String>) -> anyhow::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let clip = inner.clipboard()?;
        clip.set_contents(data.unwrap_or_else(|| "".into())).map_err(|e| anyhow!("{}", e))?;

        clip.get_contents().map(|_| ()).map_err(|e| anyhow!("{}", e))
    }
}
