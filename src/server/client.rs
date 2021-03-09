use crate::config::{Config, UnixDomain};
use crate::core::promise::{Future, Promise};
use crate::frontend::executor;
use crate::mux::domain::alloc_domain_id;
use crate::mux::domain::DomainId;
use crate::mux::Mux;
use crate::server::codec::*;
use crate::server::domain::{ClientDomain, ClientDomainConfig};
use crate::server::pollable::*;
use crate::server::tab::ClientTab;
use crate::server::UnixStream;
use crossbeam_channel::TryRecvError;
use failure::{bail, err_msg, format_err, Fallible};
use log::info;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

enum ReaderMessage {
    SendPdu { pdu: Pdu, promise: Promise<Pdu> },
}

#[derive(Clone)]
pub struct Client {
    sender: PollableSender<ReaderMessage>,
    local_domain_id: DomainId,
}

macro_rules! rpc {
    ($method_name:ident, $request_type:ident, $response_type:ident) => {
        pub fn $method_name(&self, pdu: $request_type) -> Future<$response_type> {
            self.send_pdu(Pdu::$request_type(pdu)).then(|result| match result {
                Ok(Pdu::$response_type(res)) => Ok(res),
                Ok(_) => bail!("unexpected response {:?}", result),
                Err(err) => Err(err),
            })
        }
    };

    // This variant allows omitting the request parameter; this is useful
    // in the case where the struct is empty and present only for the purpose
    // of typing the request.
    ($method_name:ident, $request_type:ident=(), $response_type:ident) => {
        #[allow(dead_code)]
        pub fn $method_name(&self) -> Future<$response_type> {
            self.send_pdu(Pdu::$request_type($request_type {})).then(|result| match result {
                Ok(Pdu::$response_type(res)) => Ok(res),
                Ok(_) => bail!("unexpected response {:?}", result),
                Err(err) => Err(err),
            })
        }
    };
}

fn process_unilateral(local_domain_id: DomainId, decoded: DecodedPdu) -> Fallible<()> {
    if let Some(tab_id) = decoded.pdu.tab_id() {
        let pdu = decoded.pdu;
        Future::with_executor(executor(), move || {
            let mux = Mux::get().unwrap();
            let client_domain = mux
                .get_domain(local_domain_id)
                .ok_or_else(|| format_err!("no such domain {}", local_domain_id))?;
            let client_domain = client_domain.downcast_ref::<ClientDomain>().ok_or_else(|| {
                format_err!("domain {} is not a ClientDomain instance", local_domain_id)
            })?;

            let local_tab_id = client_domain.remote_to_local_tab_id(tab_id).ok_or_else(|| {
                format_err!("remote tab id {} does not have a local tab id", tab_id)
            })?;
            let tab = mux
                .get_tab(local_tab_id)
                .ok_or_else(|| format_err!("no such tab {}", local_tab_id))?;
            let client_tab = tab.downcast_ref::<ClientTab>().ok_or_else(|| {
                log::error!(
                    "received unilateral PDU for tab {} which is \
                     not an instance of ClientTab: {:?}",
                    local_tab_id,
                    pdu
                );
                format_err!(
                    "received unilateral PDU for tab {} which is \
                     not an instance of ClientTab: {:?}",
                    local_tab_id,
                    pdu
                )
            })?;
            client_tab.process_unilateral(pdu)
        });
    } else {
        bail!("don't know how to handle {:?}", decoded);
    }
    Ok(())
}

fn client_thread(
    reconnectable: &mut Reconnectable,
    local_domain_id: DomainId,
    rx: &mut PollableReceiver<ReaderMessage>,
) -> Fallible<()> {
    let mut next_serial = 1u64;
    let mut promises = HashMap::new();
    let mut read_buffer = Vec::with_capacity(1024);
    loop {
        loop {
            match rx.try_recv() {
                Ok(msg) => match msg {
                    ReaderMessage::SendPdu { pdu, promise } => {
                        let serial = next_serial;
                        next_serial += 1;
                        promises.insert(serial, promise);

                        pdu.encode(reconnectable.stream(), serial)?;
                        reconnectable.stream().flush()?;
                    }
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => bail!("Client was destroyed"),
            };
        }

        let mut poll_array = [rx.as_poll_fd(), reconnectable.stream().as_poll_fd()];
        poll_for_read(&mut poll_array);

        if poll_array[1].revents != 0 || reconnectable.stream().has_read_buffered() {
            // When TLS is enabled on a stream, it may require a mixture of
            // reads AND writes in order to satisfy a given read or write.
            // As a result, we may appear ready to read a PDU, but may not
            // be able to read a complete PDU.
            // Set to non-blocking mode while we try to decode a packet to
            // avoid blocking.
            loop {
                reconnectable.stream().set_non_blocking(true)?;
                let res = Pdu::try_read_and_decode(reconnectable.stream(), &mut read_buffer);
                reconnectable.stream().set_non_blocking(false)?;
                if let Some(decoded) = res? {
                    log::trace!("decoded serial {}", decoded.serial);
                    if decoded.serial == 0 {
                        process_unilateral(local_domain_id, decoded)?;
                    } else if let Some(mut promise) = promises.remove(&decoded.serial) {
                        promise.result(Ok(decoded.pdu));
                    } else {
                        log::error!(
                            "got serial {} without a corresponding promise",
                            decoded.serial
                        );
                    }
                } else {
                    break;
                }
            }
        }
    }
}

pub fn unix_connect_with_retry(path: &Path) -> Result<UnixStream, std::io::Error> {
    let mut error = std::io::Error::last_os_error();

    for iter in 0..10 {
        if iter > 0 {
            std::thread::sleep(std::time::Duration::from_millis(iter * 10));
        }
        match UnixStream::connect(path) {
            Ok(stream) => return Ok(stream),
            Err(err) => error = err,
        }
    }

    Err(error)
}

struct Reconnectable {
    config: ClientDomainConfig,
    stream: Option<Box<dyn ReadAndWrite>>,
}

impl Reconnectable {
    fn new(config: ClientDomainConfig, stream: Option<Box<dyn ReadAndWrite>>) -> Self {
        Self { config, stream }
    }

    // Clippy thinks we should return &ReadAndWrite here, but the caller
    // needs to know the size of the returned type in a number of situations,
    // so suppress that lint
    #[allow(clippy::borrowed_box)]
    fn stream(&mut self) -> &mut Box<dyn ReadAndWrite> {
        self.stream.as_mut().unwrap()
    }

    fn reconnectable(&mut self) -> bool {
        match &self.config {
            // It doesn't make sense to reconnect to a unix socket; we only
            // get disconnected it it dies, so respawning it would not preserve
            // the set of tabs and we'd have confusing and inconsistent state
            ClientDomainConfig::Unix(_) => false,
        }
    }

    fn connect(&mut self, initial: bool) -> Fallible<()> {
        match self.config.clone() {
            ClientDomainConfig::Unix(unix_dom) => self.unix_connect(unix_dom, initial),
        }
    }

    fn unix_connect(&mut self, unix_dom: UnixDomain, initial: bool) -> Fallible<()> {
        let sock_path = unix_dom.socket_path();
        info!("connect to {}", sock_path.display());

        let stream = match unix_connect_with_retry(&sock_path) {
            Ok(stream) => stream,
            Err(e) => {
                if unix_dom.no_serve_automatically || !initial {
                    bail!("failed to connect to {}: {}", sock_path.display(), e);
                }
                log::error!(
                    "While connecting to {}: {}.  Will try spawning the server.",
                    sock_path.display(),
                    e
                );

                let argv = unix_dom.serve_command()?;

                // We need to use a pty to spawn the command because,
                // on Windows, when spawned from the gui with no pre-existing
                // conhost.exe, `wsl.exe` will fail to start up correctly.
                // This also has a nice side effect of not flashing up a
                // console window when we first spin up the wsl instance.
                let pty_system = crate::pty::PtySystemSelection::default().get()?;
                let pair = pty_system.openpty(Default::default())?;
                let mut cmd = crate::pty::CommandBuilder::new(&argv[0]);
                cmd.args(&argv[1..]);
                let mut child = pair.slave.spawn_command(cmd)?;
                let status = child.wait()?;
                if !status.success() {
                    log::error!("{:?} failed with status {:?}", argv, status);
                }
                drop(child);
                drop(pair.slave);
                // Gross bug workaround: ClosePsuedoConsole can get confused about the
                // processes attached to the console when using wsl; the scenario
                // is that we use wsl.exe to invoke miro, daemonize it (which forks
                // and detaches from the pty) and then we can discard the pty.
                // The ClosePsuedoConsole call should not need to wait for any clients,
                // but blocks forever.
                // The workaround is to leak the console handle.  The associated conhost
                // process will show up in the process tree until this instance of
                // miro is terminated, but it otherwise invisible.
                std::mem::forget(pair.master);

                unix_connect_with_retry(&sock_path).map_err(|e| {
                    format_err!("failed to connect to {}: {}", sock_path.display(), e)
                })?
            }
        };

        let stream: Box<dyn ReadAndWrite> = Box::new(stream);
        self.stream.replace(stream);
        Ok(())
    }
}

impl Client {
    fn new(local_domain_id: DomainId, mut reconnectable: Reconnectable) -> Self {
        let (sender, mut receiver) = pollable_channel().expect("failed to create pollable_channel");

        thread::spawn(move || {
            const BASE_INTERVAL: Duration = Duration::from_secs(1);
            const MAX_INTERVAL: Duration = Duration::from_secs(10);

            let mut backoff = BASE_INTERVAL;
            loop {
                if let Err(e) = client_thread(&mut reconnectable, local_domain_id, &mut receiver) {
                    if !reconnectable.reconnectable() {
                        log::debug!("client thread ended: {}", e);
                        break;
                    }

                    log::error!("client disconnected {}; will reconnect in {:?}", e, backoff);

                    loop {
                        std::thread::sleep(backoff);
                        match reconnectable.connect(false) {
                            Ok(_) => {
                                backoff = BASE_INTERVAL;
                                log::error!("Reconnected!");
                                break;
                            }
                            Err(err) => {
                                backoff = (backoff + backoff).min(MAX_INTERVAL);
                                log::error!(
                                    "problem reconnecting: {}; will reconnect in {:?}",
                                    err,
                                    backoff
                                );
                            }
                        }
                    }
                }
            }
            Future::with_executor(executor(), move || {
                let mux = Mux::get().unwrap();
                let client_domain = mux
                    .get_domain(local_domain_id)
                    .ok_or_else(|| format_err!("no such domain {}", local_domain_id))?;
                let client_domain =
                    client_domain.downcast_ref::<ClientDomain>().ok_or_else(|| {
                        format_err!("domain {} is not a ClientDomain instance", local_domain_id)
                    })?;
                client_domain.perform_detach();
                Ok(())
            });
        });

        Self { sender, local_domain_id }
    }

    #[allow(dead_code)]
    pub fn local_domain_id(&self) -> DomainId {
        self.local_domain_id
    }

    pub fn new_default_unix_domain(config: &Arc<Config>, initial: bool) -> Fallible<Self> {
        let unix_dom = config
            .unix_domains
            .first()
            .ok_or_else(|| err_msg("no default unix domain is configured"))?;
        Self::new_unix_domain(alloc_domain_id(), config, unix_dom, initial)
    }

    pub fn new_unix_domain(
        local_domain_id: DomainId,
        _config: &Arc<Config>,
        unix_dom: &UnixDomain,
        initial: bool,
    ) -> Fallible<Self> {
        let mut reconnectable =
            Reconnectable::new(ClientDomainConfig::Unix(unix_dom.clone()), None);
        reconnectable.connect(initial)?;
        Ok(Self::new(local_domain_id, reconnectable))
    }

    pub fn send_pdu(&self, pdu: Pdu) -> Future<Pdu> {
        let mut promise = Promise::new();
        let future = promise.get_future().expect("future already taken!?");
        match self.sender.send(ReaderMessage::SendPdu { pdu, promise }) {
            Ok(_) => future,
            Err(err) => Future::err(format_err!("{}", err)),
        }
    }

    rpc!(ping, Ping = (), Pong);
    rpc!(list_tabs, ListTabs = (), ListTabsResponse);
    rpc!(spawn, Spawn, SpawnResponse);
    rpc!(write_to_tab, WriteToTab, UnitResponse);
    rpc!(send_paste, SendPaste, UnitResponse);
    rpc!(key_down, SendKeyDown, UnitResponse);
    rpc!(mouse_event, SendMouseEvent, SendMouseEventResponse);
    rpc!(resize, Resize, UnitResponse);
    rpc!(get_tab_render_changes, GetTabRenderChanges, UnitResponse);
}
