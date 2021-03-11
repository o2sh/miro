use crate::config::{Config, UnixDomain};
use crate::core::promise::Future;
use crate::core::surface::{Change, Position, SequenceNo, Surface};
use crate::create_user_owned_dirs;
use crate::frontend::executor;
use crate::mux::tab::{Tab, TabId};
use crate::mux::{Mux, MuxNotification, MuxSubscriber};
use crate::pty::PtySize;
use crate::ratelim::RateLimiter;
use crate::server::codec::*;
use crate::server::pollable::*;
use crate::server::UnixListener;
use crate::term::terminal::Clipboard;
use crossbeam_channel::TryRecvError;
use failure::{bail, format_err, Error, Fallible};
use libc::{mode_t, umask};
use log::{debug, error};
use std::collections::{HashMap, HashSet};
use std::fs::remove_file;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

struct LocalListener {
    listener: UnixListener,
}

impl LocalListener {
    pub fn new(listener: UnixListener) -> Self {
        Self { listener }
    }

    fn run(&mut self) {
        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    Future::with_executor(executor(), move || {
                        let mut session = ClientSession::new(stream);
                        thread::spawn(move || session.run());
                        Ok(())
                    });
                }
                Err(err) => {
                    error!("accept failed: {}", err);
                    return;
                }
            }
        }
    }
}

pub struct ClientSession<S: ReadAndWrite> {
    stream: S,
    surfaces_by_tab: Arc<Mutex<HashMap<TabId, ClientSurfaceState>>>,
    to_write_rx: PollableReceiver<DecodedPdu>,
    to_write_tx: PollableSender<DecodedPdu>,
    mux_rx: MuxSubscriber,
}

fn maybe_push_tab_changes(
    surfaces: &Arc<Mutex<HashMap<TabId, ClientSurfaceState>>>,
    tab: &Rc<dyn Tab>,
    sender: PollableSender<DecodedPdu>,
) -> Fallible<()> {
    let tab_id = tab.tab_id();
    let mut surfaces = surfaces.lock().unwrap();
    let (rows, cols) = tab.renderer().physical_dimensions();
    let surface = surfaces.entry(tab_id).or_insert_with(|| ClientSurfaceState::new(cols, rows));
    surface.update_surface_from_screen(&tab);

    let (new_seq, changes) = surface.get_and_flush_changes(surface.last_seq);
    if !changes.is_empty() {
        sender.send(DecodedPdu {
            pdu: Pdu::GetTabRenderChangesResponse(GetTabRenderChangesResponse {
                tab_id,
                sequence_no: surface.last_seq,
                changes,
            }),
            serial: 0,
        })?;
        surface.last_seq = new_seq;
    }
    Ok(())
}

struct ClientSurfaceState {
    surface: Surface,
    last_seq: SequenceNo,
    push_limiter: RateLimiter,
    update_limiter: RateLimiter,
}

impl ClientSurfaceState {
    fn new(cols: usize, rows: usize) -> Self {
        let mux = Mux::get().expect("to be running on gui thread");
        let push_limiter =
            RateLimiter::new(mux.config().ratelimit_mux_output_pushes_per_second.unwrap_or(10));
        let update_limiter =
            RateLimiter::new(mux.config().ratelimit_mux_output_scans_per_second.unwrap_or(100));
        let surface = Surface::new(cols, rows);
        Self { surface, last_seq: 0, push_limiter, update_limiter }
    }

    fn update_surface_from_screen(&mut self, tab: &Rc<dyn Tab>) {
        if !self.update_limiter.non_blocking_admittance_check(1) {
            return;
        }

        {
            let mut renderable = tab.renderer();
            let (rows, cols) = renderable.physical_dimensions();
            let (surface_width, surface_height) = self.surface.dimensions();

            if (rows != surface_height) || (cols != surface_width) {
                self.surface.resize(cols, rows);
                renderable.make_all_lines_dirty();
            }

            let (x, y) = self.surface.cursor_position();
            let cursor = renderable.get_cursor_position();
            if (x != cursor.x) || (y as i64 != cursor.y) {
                // Update the cursor, but if we're scrolled back
                // and it is our of range, skip the update.
                if cursor.y < rows as i64 {
                    self.surface.add_change(Change::CursorPosition {
                        x: Position::Absolute(cursor.x),
                        y: Position::Absolute(cursor.y as usize),
                    });
                }
            }

            let mut changes = vec![];

            for (line_idx, line, _selrange) in renderable.get_dirty_lines() {
                changes.append(&mut self.surface.diff_against_numbered_line(line_idx, &line));
            }

            self.surface.add_changes(changes);
        }

        let title = tab.get_title();
        if title != self.surface.title() {
            self.surface.add_change(Change::Title(title));
        }
    }

    fn get_and_flush_changes(&mut self, seq: SequenceNo) -> (SequenceNo, Vec<Change>) {
        let (new_seq, changes) = self.surface.get_changes(seq);

        if !changes.is_empty() && !self.push_limiter.non_blocking_admittance_check(1) {
            // Pretend that there are no changes
            return (seq, vec![]);
        }

        let changes = changes.into_owned();
        let (rows, cols) = self.surface.dimensions();

        // Keep the change log in the surface bounded;
        // we don't completely blow away the log each time
        // so that multiple clients have an opportunity to
        // resync from a smaller delta
        self.surface.flush_changes_older_than(new_seq.saturating_sub(rows * cols * 2));
        (new_seq, changes)
    }
}

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

impl<S: ReadAndWrite> ClientSession<S> {
    fn new(stream: S) -> Self {
        let (to_write_tx, to_write_rx) =
            pollable_channel().expect("failed to create pollable_channel");
        let mux = Mux::get().expect("to be running on gui thread");
        let mux_rx = mux.subscribe().expect("Mux::subscribe to succeed");
        Self {
            stream,
            surfaces_by_tab: Arc::new(Mutex::new(HashMap::new())),
            to_write_rx,
            to_write_tx,
            mux_rx,
        }
    }

    fn run(&mut self) {
        if let Err(e) = self.process() {
            error!("While processing session loop: {}", e);
        }
    }

    fn process(&mut self) -> Result<(), Error> {
        let mut read_buffer = Vec::with_capacity(1024);
        let mut tabs_to_output = HashSet::new();

        loop {
            loop {
                match self.to_write_rx.try_recv() {
                    Ok(decoded) => {
                        log::trace!("writing pdu with serial {}", decoded.serial);
                        decoded.pdu.encode(&mut self.stream, decoded.serial)?;
                        self.stream.flush()?;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => bail!("ClientSession was destroyed"),
                };
            }
            loop {
                match self.mux_rx.try_recv() {
                    Ok(notif) => match notif {
                        // Coalesce multiple TabOutputs for the same tab
                        MuxNotification::TabOutput(tab_id) => tabs_to_output.insert(tab_id),
                    },
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => bail!("ClientSession was destroyed"),
                };

                for tab_id in tabs_to_output.drain() {
                    let surfaces = Arc::clone(&self.surfaces_by_tab);
                    let sender = self.to_write_tx.clone();
                    Future::with_executor(executor(), move || {
                        let mux = Mux::get().unwrap();
                        let tab = mux
                            .get_tab(tab_id)
                            .ok_or_else(|| format_err!("no such tab {}", tab_id))?;
                        maybe_push_tab_changes(&surfaces, &tab, sender)?;
                        Ok(())
                    });
                }
            }

            let mut poll_array =
                [self.to_write_rx.as_poll_fd(), self.stream.as_poll_fd(), self.mux_rx.as_poll_fd()];
            poll_for_read(&mut poll_array);

            if poll_array[1].revents != 0 || self.stream.has_read_buffered() {
                loop {
                    self.stream.set_non_blocking(true)?;
                    let res = Pdu::try_read_and_decode(&mut self.stream, &mut read_buffer);
                    self.stream.set_non_blocking(false)?;
                    if let Some(decoded) = res? {
                        self.process_one(decoded)?;
                    } else {
                        break;
                    }
                }
            }
        }
    }

    fn process_one(&mut self, decoded: DecodedPdu) -> Fallible<()> {
        let start = Instant::now();
        let sender = self.to_write_tx.clone();
        let serial = decoded.serial;
        self.process_pdu(decoded.pdu).then(move |result| {
            let pdu = match result {
                Ok(pdu) => pdu,
                Err(err) => Pdu::ErrorResponse(ErrorResponse { reason: format!("Error: {}", err) }),
            };
            log::trace!("{} processing time {:?}", serial, start.elapsed());
            sender.send(DecodedPdu { pdu, serial })
        });
        Ok(())
    }

    fn process_pdu(&mut self, pdu: Pdu) -> Future<Pdu> {
        match pdu {
            Pdu::Ping(Ping {}) => Future::ok(Pdu::Pong(Pong {})),
            Pdu::ListTabs(ListTabs {}) => Future::with_executor(executor(), move || {
                let mux = Mux::get().unwrap();
                let mut tabs = vec![];
                for window_id in mux.iter_windows().into_iter() {
                    let window = mux.get_window(window_id).unwrap();
                    for tab in window.iter() {
                        let (rows, cols) = tab.renderer().physical_dimensions();
                        tabs.push(WindowAndTabEntry {
                            window_id,
                            tab_id: tab.tab_id(),
                            title: tab.get_title(),
                            size: PtySize {
                                cols: cols as u16,
                                rows: rows as u16,
                                pixel_height: 0,
                                pixel_width: 0,
                            },
                        });
                    }
                }
                log::error!("ListTabs {:#?}", tabs);
                Ok(Pdu::ListTabsResponse(ListTabsResponse { tabs }))
            }),

            Pdu::WriteToTab(WriteToTab { tab_id, data }) => {
                let surfaces = Arc::clone(&self.surfaces_by_tab);
                let sender = self.to_write_tx.clone();
                Future::with_executor(executor(), move || {
                    let mux = Mux::get().unwrap();
                    let tab =
                        mux.get_tab(tab_id).ok_or_else(|| format_err!("no such tab {}", tab_id))?;
                    tab.writer().write_all(&data)?;
                    maybe_push_tab_changes(&surfaces, &tab, sender)?;
                    Ok(Pdu::UnitResponse(UnitResponse {}))
                })
            }
            Pdu::SendPaste(SendPaste { tab_id, data }) => {
                let surfaces = Arc::clone(&self.surfaces_by_tab);
                let sender = self.to_write_tx.clone();
                Future::with_executor(executor(), move || {
                    let mux = Mux::get().unwrap();
                    let tab =
                        mux.get_tab(tab_id).ok_or_else(|| format_err!("no such tab {}", tab_id))?;
                    tab.send_paste(&data)?;
                    maybe_push_tab_changes(&surfaces, &tab, sender)?;
                    Ok(Pdu::UnitResponse(UnitResponse {}))
                })
            }

            Pdu::Resize(Resize { tab_id, size }) => Future::with_executor(executor(), move || {
                let mux = Mux::get().unwrap();
                let tab =
                    mux.get_tab(tab_id).ok_or_else(|| format_err!("no such tab {}", tab_id))?;
                tab.resize(size)?;
                Ok(Pdu::UnitResponse(UnitResponse {}))
            }),

            Pdu::SendKeyDown(SendKeyDown { tab_id, event }) => {
                let surfaces = Arc::clone(&self.surfaces_by_tab);
                let sender = self.to_write_tx.clone();
                Future::with_executor(executor(), move || {
                    let mux = Mux::get().unwrap();
                    let tab =
                        mux.get_tab(tab_id).ok_or_else(|| format_err!("no such tab {}", tab_id))?;
                    tab.key_down(event.key, event.modifiers)?;
                    maybe_push_tab_changes(&surfaces, &tab, sender)?;
                    Ok(Pdu::UnitResponse(UnitResponse {}))
                })
            }
            Pdu::SendMouseEvent(SendMouseEvent { tab_id, event }) => {
                let surfaces = Arc::clone(&self.surfaces_by_tab);
                let sender = self.to_write_tx.clone();
                Future::with_executor(executor(), move || {
                    let mux = Mux::get().unwrap();
                    let tab =
                        mux.get_tab(tab_id).ok_or_else(|| format_err!("no such tab {}", tab_id))?;
                    let mut host = BufferedTerminalHost {
                        tab_id,
                        write: tab.writer(),
                        title: None,
                        sender: sender.clone(),
                    };
                    tab.mouse_event(event, &mut host)?;
                    maybe_push_tab_changes(&surfaces, &tab, sender)?;

                    let highlight = tab.renderer().current_highlight().as_ref().cloned();

                    Ok(Pdu::SendMouseEventResponse(SendMouseEventResponse {
                        selection_range: tab.selection_range(),
                        highlight,
                    }))
                })
            }

            Pdu::Spawn(spawn) => Future::with_executor(executor(), move || {
                let mux = Mux::get().unwrap();
                let domain = mux.get_domain(spawn.domain_id).ok_or_else(|| {
                    format_err!("domain {} not found on this server", spawn.domain_id)
                })?;

                let window_id = if let Some(window_id) = spawn.window_id {
                    mux.get_window_mut(window_id).ok_or_else(|| {
                        format_err!("window_id {} not found on this server", window_id)
                    })?;
                    window_id
                } else {
                    mux.new_empty_window()
                };

                let tab = domain.spawn(spawn.size, spawn.command, window_id)?;
                Ok(Pdu::SpawnResponse(SpawnResponse { tab_id: tab.tab_id(), window_id }))
            }),

            Pdu::GetTabRenderChanges(GetTabRenderChanges { tab_id, .. }) => {
                let surfaces = Arc::clone(&self.surfaces_by_tab);
                let sender = self.to_write_tx.clone();
                Future::with_executor(executor(), move || {
                    let mux = Mux::get().unwrap();
                    let tab =
                        mux.get_tab(tab_id).ok_or_else(|| format_err!("no such tab {}", tab_id))?;
                    maybe_push_tab_changes(&surfaces, &tab, sender)?;
                    Ok(Pdu::UnitResponse(UnitResponse {}))
                })
            }

            Pdu::Invalid { .. } => Future::err(format_err!("invalid PDU {:?}", pdu)),
            Pdu::Pong { .. }
            | Pdu::ListTabsResponse { .. }
            | Pdu::SendMouseEventResponse { .. }
            | Pdu::SetClipboard { .. }
            | Pdu::OpenURL { .. }
            | Pdu::SpawnResponse { .. }
            | Pdu::GetTabRenderChangesResponse { .. }
            | Pdu::UnitResponse { .. }
            | Pdu::ErrorResponse { .. } => {
                Future::err(format_err!("expected a request, got {:?}", pdu))
            }
        }
    }
}

/// Unfortunately, novice unix users can sometimes be running
/// with an overly permissive umask so we take care to install
/// a more restrictive mask while we might be creating things
/// in the filesystem.
/// This struct locks down the umask for its lifetime, restoring
/// the prior umask when it is dropped.
struct UmaskSaver {
    #[cfg(unix)]
    mask: mode_t,
}

impl UmaskSaver {
    fn new() -> Self {
        Self {
            #[cfg(unix)]
            mask: unsafe { umask(0o077) },
        }
    }
}

impl Drop for UmaskSaver {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            umask(self.mask);
        }
    }
}

/// Take care when setting up the listener socket;
/// we need to be sure that the directory that we create it in
/// is owned by the user and has appropriate file permissions
/// that prevent other users from manipulating its contents.
fn safely_create_sock_path(unix_dom: &UnixDomain) -> Result<UnixListener, Error> {
    let sock_path = &unix_dom.socket_path();
    debug!("setting up {}", sock_path.display());

    let _saver = UmaskSaver::new();

    let sock_dir = sock_path
        .parent()
        .ok_or_else(|| format_err!("sock_path {} has no parent dir", sock_path.display()))?;

    create_user_owned_dirs(sock_dir)?;

    if sock_path.exists() {
        remove_file(sock_path)?;
    }

    UnixListener::bind(sock_path)
        .map_err(|e| format_err!("Failed to bind to {}: {}", sock_path.display(), e))
}

pub fn spawn_listener(config: &Arc<Config>) -> Fallible<()> {
    for unix_dom in &config.unix_domains {
        let mut listener = LocalListener::new(safely_create_sock_path(unix_dom)?);
        thread::spawn(move || {
            listener.run();
        });
    }
    Ok(())
}
