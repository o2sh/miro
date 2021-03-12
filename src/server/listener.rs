use crate::mux::tab::TabId;
use crate::server::codec::*;
use crate::server::pollable::*;
use crate::term::terminal::Clipboard;
use failure::Fallible;
use std::sync::Arc;

struct RemoteClipboard {
    sender: PollableSender<DecodedPdu>,
    tab_id: TabId,
}

impl Clipboard for RemoteClipboard {
    fn get_contents(&self) -> Fallible<String> {
        Ok("".to_owned())
    }

    fn set_contents(&self, clipboard: Option<String>) -> Fallible<()> {
        self.sender.send(DecodedPdu {
            serial: 0,
            pdu: Pdu::SetClipboard(SetClipboard { tab_id: self.tab_id, clipboard }),
        })?;
        Ok(())
    }
}

struct BufferedTerminalHost<'a> {
    tab_id: TabId,
    write: std::cell::RefMut<'a, dyn std::io::Write>,
    title: Option<String>,
    sender: PollableSender<DecodedPdu>,
}

impl<'a> crate::term::TerminalHost for BufferedTerminalHost<'a> {
    fn writer(&mut self) -> &mut dyn std::io::Write {
        &mut *self.write
    }

    fn click_link(&mut self, link: &Arc<crate::term::cell::Hyperlink>) {
        self.sender
            .send(DecodedPdu {
                serial: 0,
                pdu: Pdu::OpenURL(OpenURL { tab_id: self.tab_id, url: link.uri().to_string() }),
            })
            .ok();
    }

    fn get_clipboard(&mut self) -> Fallible<Arc<dyn Clipboard>> {
        Ok(Arc::new(RemoteClipboard { tab_id: self.tab_id, sender: self.sender.clone() }))
    }

    fn set_title(&mut self, title: &str) {
        self.title.replace(title.to_owned());
    }
}
