use crate::config::Config;
use crate::font::FontConfiguration;
use crate::opengl::renderer::Renderer;
use crate::pty::MasterPty;
use crate::term::{
    color::ColorPalette, hyperlink::Hyperlink, KeyCode, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind, Terminal, TerminalHost,
};
use crate::wakeup::WakeupMsg;
use clipboard::{ClipboardContext, ClipboardProvider};
use failure::Error;
use glium::glutin::event::ElementState;
use glium::{self, glutin};
use open::that;
use std::io;
use std::io::{Read, Write};
use std::process::Child;
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use sysinfo::{System, SystemExt};

struct Host {
    display: glium::Display,
    pty: MasterPty,
    clipboard: Clipboard,
}

pub struct TerminalWindow {
    host: Host,
    renderer: Renderer,
    width: u16,
    height: u16,
    cell_height: usize,
    cell_width: usize,
    terminal: Terminal,
    process: Child,
    last_mouse_coords: (f64, f64),
    last_modifiers: KeyModifiers,
    wakeup_receiver: Receiver<WakeupMsg>,
}

impl TerminalWindow {
    pub fn new(
        event_loop: &glutin::event_loop::EventLoop<WakeupMsg>,
        wakeup_receiver: Receiver<WakeupMsg>,
        terminal: Terminal,
        pty: MasterPty,
        process: Child,
        fonts: &Rc<FontConfiguration>,
        config: &Rc<Config>,
        sys: System,
    ) -> Result<TerminalWindow, Error> {
        let palette =
            config.colors.as_ref().map(|p| p.clone().into()).unwrap_or_else(ColorPalette::default);
        let (cell_height, cell_width) = {
            // Urgh, this is a bit repeaty, but we need to satisfy the borrow checker
            let font = fonts.default_font()?;
            let metrics = font.borrow_mut().get_fallback(0)?.metrics();
            (metrics.cell_height, metrics.cell_width)
        };

        let size = pty.get_size()?;
        let width = size.ws_xpixel;
        let height = size.ws_ypixel;
        let logical_size = glutin::dpi::LogicalSize::new(width as i32, height as i32);
        eprintln!("make window with {}x{}", width, height);

        let display = {
            let pref_context = glutin::ContextBuilder::new()
                .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGlEs, (2, 0)))
                .with_vsync(true)
                .with_pixel_format(24, 8)
                .with_srgb(true);
            let window = glutin::window::WindowBuilder::new()
                .with_inner_size(logical_size)
                .with_title("miro");

            glium::Display::new(window, pref_context, &event_loop)
                .map_err(|e| format_err!("{:?}", e))?
        };

        let host = Host { display, pty, clipboard: Clipboard::default() };

        let renderer =
            Renderer::new(&host.display, width, height, fonts, palette, &config.theme, sys)?;
        let cell_height = cell_height.ceil() as usize;
        let cell_width = cell_width.ceil() as usize;

        Ok(TerminalWindow {
            host,
            renderer,
            width,
            height,
            cell_height,
            cell_width,
            terminal,
            process,
            last_mouse_coords: (0.0, 0.0),
            last_modifiers: Default::default(),
            wakeup_receiver,
        })
    }

    pub fn paint(&mut self, with_sprite: bool) -> Result<(), Error> {
        self.renderer.frame_count += 1;
        if self.renderer.frame_count % 5 == 0 {
            self.renderer.sys.refresh_system();
        }
        let mut target = self.host.display.draw();
        let res = self.renderer.paint(&mut target, &mut self.terminal);
        if with_sprite {
            self.renderer.paint_sprite(&mut target)?;
        }
        target.finish().unwrap();
        res?;
        Ok(())
    }

    pub fn try_read_pty(&mut self) -> Result<(), Error> {
        const BUFSIZE: usize = 8192;
        let mut buf = [0; BUFSIZE];

        match self.host.pty.read(&mut buf) {
            Ok(size) => self.terminal.advance_bytes(&buf[0..size], &mut self.host),
            Err(err) => {
                if err.kind() != io::ErrorKind::WouldBlock {
                    eprintln!("error reading from pty: {:?}", err)
                }
            }
        }
        Ok(())
    }

    fn resize_surfaces(&mut self, width: u16, height: u16) -> Result<bool, Error> {
        if width != self.width || height != self.height {
            debug!("resize {},{}", width, height);

            self.width = width;
            self.height = height;
            self.renderer.resize(&self.host.display, width, height)?;

            // The +1 in here is to handle an irritating case.
            // When we get N rows with a gap of cell_height - 1 left at
            // the bottom, we can usually squeeze that extra row in there,
            // so optimistically pretend that we have that extra pixel!
            let rows = ((height as usize + 1) / self.cell_height) as u16;
            let cols = ((width as usize + 1) / self.cell_width) as u16;
            self.host.pty.resize(rows, cols, width, height)?;
            self.terminal.resize(rows as usize, cols as usize);
            self.paint_if_needed(false)?;

            Ok(true)
        } else {
            debug!("ignoring extra resize");
            Ok(false)
        }
    }

    fn decode_modifiers(state: glium::glutin::event::ModifiersState) -> KeyModifiers {
        let mut mods = Default::default();
        if state.shift() {
            mods |= KeyModifiers::SHIFT;
        }
        if state.ctrl() {
            mods |= KeyModifiers::CTRL;
        }
        if state.alt() {
            mods |= KeyModifiers::ALT;
        }
        if state.logo() {
            mods |= KeyModifiers::SUPER;
        }
        mods
    }

    fn mouse_move(&mut self, x: f64, y: f64) -> Result<(), Error> {
        self.last_mouse_coords = (x, y);
        self.terminal.mouse_event(
            MouseEvent {
                kind: MouseEventKind::Move,
                button: MouseButton::None,
                x: (x as usize / self.cell_width) as usize,
                y: (y as usize / self.cell_height) as i64,
                modifiers: self.last_modifiers,
            },
            &mut self.host,
        )?;
        // Deliberately not forcing a paint on mouse move as it
        // makes selection feel sluggish
        // self.paint_if_needed()?;

        Ok(())
    }

    fn mouse_click(
        &mut self,
        state: ElementState,
        button: glutin::event::MouseButton,
    ) -> Result<(), Error> {
        self.terminal.mouse_event(
            MouseEvent {
                kind: match state {
                    ElementState::Pressed => MouseEventKind::Press,
                    ElementState::Released => MouseEventKind::Release,
                },
                button: match button {
                    glutin::event::MouseButton::Left => MouseButton::Left,
                    glutin::event::MouseButton::Right => MouseButton::Right,
                    glutin::event::MouseButton::Middle => MouseButton::Middle,
                    glutin::event::MouseButton::Other(_) => return Ok(()),
                },
                x: (self.last_mouse_coords.0 as usize / self.cell_width) as usize,
                y: (self.last_mouse_coords.1 as usize / self.cell_height) as i64,
                modifiers: self.last_modifiers,
            },
            &mut self.host,
        )?;
        self.paint_if_needed(false)?;

        Ok(())
    }

    fn mouse_wheel(&mut self, delta: glutin::event::MouseScrollDelta) -> Result<(), Error> {
        let button = match delta {
            glutin::event::MouseScrollDelta::LineDelta(_, lines) if lines > 0.0 => {
                MouseButton::WheelUp
            }
            glutin::event::MouseScrollDelta::LineDelta(_, lines) if lines < 0.0 => {
                MouseButton::WheelDown
            }
            glutin::event::MouseScrollDelta::PixelDelta(position) => {
                let lines = position.y as f32 / self.cell_height as f32;
                if lines > 0.0 {
                    MouseButton::WheelUp
                } else if lines < 0.0 {
                    MouseButton::WheelDown
                } else {
                    return Ok(());
                }
            }
            _ => return Ok(()),
        };
        self.terminal.mouse_event(
            MouseEvent {
                kind: MouseEventKind::Press,
                button,
                x: (self.last_mouse_coords.0 as usize / self.cell_width) as usize,
                y: (self.last_mouse_coords.1 as usize / self.cell_height) as i64,
                modifiers: self.last_modifiers,
            },
            &mut self.host,
        )?;
        self.paint_if_needed(false)?;

        Ok(())
    }

    fn key_event(&mut self, event: glium::glutin::event::KeyboardInput) -> Result<(), Error> {
        let mods = self.last_modifiers;
        if let Some(code) = event.virtual_keycode {
            use glium::glutin::event::VirtualKeyCode as V;
            let key = match code {
                V::Key1
                | V::Key2
                | V::Key3
                | V::Key4
                | V::Key5
                | V::Key6
                | V::Key7
                | V::Key8
                | V::Key9
                | V::Key0
                | V::A
                | V::B
                | V::C
                | V::D
                | V::E
                | V::F
                | V::G
                | V::H
                | V::I
                | V::J
                | V::K
                | V::L
                | V::M
                | V::N
                | V::O
                | V::P
                | V::Q
                | V::R
                | V::S
                | V::T
                | V::U
                | V::V
                | V::W
                | V::X
                | V::Y
                | V::Z
                | V::Return
                | V::Back
                | V::Escape
                | V::Delete
                | V::Colon
                | V::Space
                | V::Equals
                | V::Plus
                | V::Apostrophe
                | V::Backslash
                | V::Grave
                | V::LBracket
                | V::Minus
                | V::Period
                | V::RBracket
                | V::Semicolon
                | V::Slash
                | V::Comma
                | V::At
                | V::Tab => {
                    // These are all handled by ReceivedCharacter
                    return Ok(());
                }
                V::Insert => KeyCode::Insert,
                V::Home => KeyCode::Home,
                V::End => KeyCode::End,
                V::PageDown => KeyCode::PageDown,
                V::PageUp => KeyCode::PageUp,
                V::Left => KeyCode::Left,
                V::Up => KeyCode::Up,
                V::Right => KeyCode::Right,
                V::Down => KeyCode::Down,
                V::LAlt | V::RAlt => KeyCode::Alt,
                V::LControl | V::RControl => KeyCode::Control,
                V::LShift | V::RShift => KeyCode::Shift,
                V::LWin | V::RWin => KeyCode::Super,
                _ => {
                    eprintln!("unhandled key: {:?}", event);
                    return Ok(());
                }
            };

            match event.state {
                ElementState::Pressed => self.terminal.key_down(key, mods, &mut self.host)?,
                ElementState::Released => self.terminal.key_up(key, mods, &mut self.host)?,
            }
        }
        self.paint_if_needed(false)?;
        Ok(())
    }

    pub fn paint_if_needed(&mut self, with_sprite: bool) -> Result<(), Error> {
        if self.terminal.has_dirty_lines() {
            self.paint(with_sprite)?;
        }
        Ok(())
    }

    pub fn test_for_child_exit(&mut self) -> Result<(), Error> {
        match self.process.try_wait() {
            Ok(Some(status)) => {
                bail!("child exited: {}", status);
            }
            Ok(None) => Ok(()),
            Err(e) => {
                bail!("failed to wait for child: {}", e);
            }
        }
    }

    pub fn dispatch_event(&mut self, event: glutin::event::Event<WakeupMsg>) -> Result<(), Error> {
        use glium::glutin::event::{Event, WindowEvent};
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                bail!("window close requested!");
            }
            Event::WindowEvent { event: WindowEvent::Resized(size), .. } => {
                self.resize_surfaces(size.width as u16, size.height as u16)?;
            }
            Event::WindowEvent { event: WindowEvent::ReceivedCharacter(c), .. } => {
                self.terminal.key_down(KeyCode::Char(c), self.last_modifiers, &mut self.host)?;
                self.paint_if_needed(false)?;
            }
            Event::WindowEvent { event: WindowEvent::KeyboardInput { input, .. }, .. } => {
                self.key_event(input)?;
            }
            Event::WindowEvent { event: WindowEvent::CursorMoved { position, .. }, .. } => {
                self.mouse_move(position.x, position.y)?;
            }
            Event::WindowEvent { event: WindowEvent::MouseInput { state, button, .. }, .. } => {
                self.mouse_click(state, button)?;
            }
            Event::WindowEvent { event: WindowEvent::MouseWheel { delta, .. }, .. } => {
                self.mouse_wheel(delta)?;
            }
            Event::WindowEvent { event: WindowEvent::ModifiersChanged(modifiers), .. } => {
                self.last_modifiers = Self::decode_modifiers(modifiers);
            }

            Event::UserEvent(_) => loop {
                match self.wakeup_receiver.try_recv() {
                    Ok(WakeupMsg::PtyReadable) => self.try_read_pty()?,
                    Ok(WakeupMsg::SigChld) => self.test_for_child_exit()?,
                    Ok(WakeupMsg::Paint) => self.paint(true)?,
                    Ok(WakeupMsg::Paste) => {}
                    Err(_) => break,
                }
            },
            _ => {}
        }
        Ok(())
    }
}

/// macOS gets unhappy if we set up the clipboard too early,
/// so we use this to defer it until we use it
#[derive(Default)]
struct Clipboard {
    clipboard: Option<ClipboardContext>,
}

impl Clipboard {
    fn clipboard(&mut self) -> Result<&mut ClipboardContext, Error> {
        if self.clipboard.is_none() {
            self.clipboard = Some(ClipboardContext::new().map_err(|e| format_err!("{}", e))?);
        }
        Ok(self.clipboard.as_mut().unwrap())
    }

    pub fn get_clipboard(&mut self) -> Result<String, Error> {
        self.clipboard()?.get_contents().map_err(|e| format_err!("{}", e))
    }

    pub fn set_clipboard(&mut self, clip: Option<String>) -> Result<(), Error> {
        self.clipboard()?
            .set_contents(clip.unwrap_or_else(|| "".into()))
            .map_err(|e| format_err!("{}", e))?;
        self.get_clipboard().map(|_| ())
    }
}

impl TerminalHost for Host {
    fn writer(&mut self) -> &mut dyn Write {
        &mut self.pty
    }
    fn click_link(&mut self, link: &Rc<Hyperlink>) {
        match that(link.uri()) {
            Ok(_) => {}
            Err(err) => eprintln!("failed to open {}: {:?}", link.uri(), err),
        }
    }

    fn get_clipboard(&mut self) -> Result<String, Error> {
        self.clipboard.get_clipboard()
    }

    fn set_clipboard(&mut self, clip: Option<String>) -> Result<(), Error> {
        self.clipboard.set_clipboard(clip)
    }

    fn set_title(&mut self, title: &str) {
        self.display.gl_window().window().set_title(title);
    }
}
