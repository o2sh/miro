use crate::config::UnixDomain;
use crate::mux::domain::{Domain, DomainId, DomainState};
use crate::mux::tab::Tab;
use crate::mux::window::WindowId;
use crate::mux::Mux;
use crate::pty::{CommandBuilder, PtySize};
use crate::server::client::Client;
use crate::server::codec::Spawn;
use crate::server::tab::ClientTab;
use failure::{err_msg, Fallible};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

pub struct ClientInner {
    pub client: Client,
    pub local_domain_id: DomainId,
    pub remote_domain_id: DomainId,
    remote_to_local_window: Mutex<HashMap<WindowId, WindowId>>,
}

impl ClientInner {
    fn record_remote_to_local_window_mapping(
        &self,
        remote_window_id: WindowId,
        local_window_id: WindowId,
    ) {
        let mut map = self.remote_to_local_window.lock().unwrap();
        map.insert(remote_window_id, local_window_id);
        log::error!(
            "record_remote_to_local_window_mapping: {} -> {}",
            remote_window_id,
            local_window_id
        );
    }

    fn local_to_remote_window(&self, local_window_id: WindowId) -> Option<WindowId> {
        let map = self.remote_to_local_window.lock().unwrap();
        for (remote, local) in map.iter() {
            if *local == local_window_id {
                return Some(*remote);
            }
        }
        None
    }
}

#[derive(Clone, Debug)]
pub enum ClientDomainConfig {
    Unix(UnixDomain),
}

impl ClientDomainConfig {
    pub fn name(&self) -> &str {
        match self {
            ClientDomainConfig::Unix(unix) => &unix.name,
        }
    }
}

pub struct ClientDomain {
    config: ClientDomainConfig,
    inner: RefCell<Option<Arc<ClientInner>>>,
    local_domain_id: DomainId,
}

impl ClientDomain {
    fn inner(&self) -> Option<Arc<ClientInner>> {
        self.inner.borrow().as_ref().map(|i| Arc::clone(i))
    }
}

impl Domain for ClientDomain {
    fn domain_id(&self) -> DomainId {
        self.local_domain_id
    }

    fn domain_name(&self) -> &str {
        self.config.name()
    }

    fn spawn(
        &self,
        size: PtySize,
        command: Option<CommandBuilder>,
        window: WindowId,
    ) -> Fallible<Rc<dyn Tab>> {
        let inner = self.inner().ok_or_else(|| err_msg("domain is not attached"))?;
        let remote_tab_id = {
            let result = inner
                .client
                .spawn(Spawn {
                    domain_id: inner.remote_domain_id,
                    window_id: inner.local_to_remote_window(window),
                    size,
                    command,
                })
                .wait()?;

            inner.record_remote_to_local_window_mapping(result.window_id, window);

            result.tab_id
        };
        let tab: Rc<dyn Tab> = Rc::new(ClientTab::new(&inner, remote_tab_id, size));
        let mux = Mux::get().unwrap();
        mux.add_tab(&tab)?;
        mux.add_tab_to_window(&tab, window)?;

        Ok(tab)
    }

    fn detach(&self) -> Fallible<()> {
        failure::bail!("detach not implemented");
    }

    fn state(&self) -> DomainState {
        if self.inner.borrow().is_some() {
            DomainState::Attached
        } else {
            DomainState::Detached
        }
    }
}
