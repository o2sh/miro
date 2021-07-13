use super::*;
use crate::window::connection::ConnectionOps;
use crate::window::{
    Dimensions, KeyEvent, MouseButtons, MouseCursor, MouseEvent, MouseEventKind, MousePress, Point,
    Rect, ScreenPoint, Size, WindowCallbacks, WindowOps, WindowOpsMut,
};
use anyhow::anyhow;
use std::any::Any;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::convert::TryInto;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use xcb::ffi::xcb_cursor_t;

struct XcbCursor {
    id: xcb_cursor_t,
    conn: Rc<Connection>,
}

impl Drop for XcbCursor {
    fn drop(&mut self) {
        xcb::free_cursor(&self.conn, self.id);
    }
}

pub(crate) struct WindowInner {
    window_id: xcb::xproto::Window,
    conn: Rc<Connection>,
    callbacks: Box<dyn WindowCallbacks>,
    width: u16,
    height: u16,
    expose: VecDeque<Rect>,
    paint_all: bool,
    cursor: Option<MouseCursor>,
    cursors: HashMap<Option<MouseCursor>, XcbCursor>,
    gl_state: Option<Rc<glium::backend::Context>>,
}

fn enclosing_boundary_with(a: &Rect, b: &Rect) -> Rect {
    let left = a.min_x().min(b.min_x());
    let right = a.max_x().max(b.max_x());

    let top = a.min_y().min(b.min_y());
    let bottom = a.max_y().max(b.max_y());

    Rect::new(Point::new(left, top), Size::new(right - left, bottom - top))
}

impl Drop for WindowInner {
    fn drop(&mut self) {
        xcb::destroy_window(self.conn.conn(), self.window_id);
    }
}

impl WindowInner {
    fn enable_opengl(&mut self) -> anyhow::Result<()> {
        let gl_state = crate::window::egl::GlState::create(
            Some(self.conn.display as *const _),
            self.window_id as *mut _,
        );

        let gl_state = gl_state.map(Rc::new).and_then(|state| unsafe {
            Ok(glium::backend::Context::new(
                Rc::clone(&state),
                true,
                if cfg!(debug_assertions) {
                    glium::debug::DebugCallbackBehavior::DebugMessageOnError
                } else {
                    glium::debug::DebugCallbackBehavior::Ignore
                },
            )?)
        })?;

        self.gl_state.replace(gl_state.clone());
        let window_handle = Window::from_id(self.window_id);
        self.callbacks.created(&window_handle, gl_state)
    }

    pub fn paint(&mut self) -> anyhow::Result<()> {
        let window_dimensions =
            Rect::from_size(Size::new(self.width as isize, self.height as isize));

        if self.paint_all {
            self.paint_all = false;
            self.expose.clear();
            self.expose.push_back(window_dimensions);
        } else if self.expose.is_empty() {
            return Ok(());
        }

        if let Some(gl_context) = self.gl_state.as_ref() {
            self.expose.clear();
            let mut frame = glium::Frame::new(
                Rc::clone(&gl_context),
                (u32::from(self.width), u32::from(self.height)),
            );

            self.callbacks.paint(&mut frame);
            frame.finish()?;
            return Ok(());
        }

        Ok(())
    }

    fn expose(&mut self, x: u16, y: u16, width: u16, height: u16) {
        let expose = Rect::new(
            Point::new(x as isize, y as isize),
            Size::new(width as isize, height as isize),
        );
        if let Some(prior) = self.expose.back_mut() {
            if prior.intersects(&expose) {
                *prior = enclosing_boundary_with(&prior, &expose);
                return;
            }
        }
        self.expose.push_back(expose);
    }

    fn do_mouse_event(&mut self, event: &MouseEvent) -> anyhow::Result<()> {
        self.callbacks.mouse_event(&event, &Window::from_id(self.window_id));
        Ok(())
    }

    fn set_cursor(&mut self, cursor: Option<MouseCursor>) -> anyhow::Result<()> {
        if cursor == self.cursor {
            return Ok(());
        }

        let cursor_id = match self.cursors.get(&cursor) {
            Some(cursor) => cursor.id,
            None => {
                let id_no = match cursor.unwrap_or(MouseCursor::Arrow) {
                    // `/usr/include/X11/cursorfont.h`
                    MouseCursor::Arrow => 132,
                    MouseCursor::Hand => 58,
                    MouseCursor::Text => 152,
                };

                let cursor_id: xcb::ffi::xcb_cursor_t = self.conn.generate_id();
                xcb::create_glyph_cursor(
                    &self.conn,
                    cursor_id,
                    self.conn.cursor_font_id,
                    self.conn.cursor_font_id,
                    id_no,
                    id_no + 1,
                    0xffff,
                    0xffff,
                    0xffff,
                    0,
                    0,
                    0,
                );

                self.cursors
                    .insert(cursor, XcbCursor { id: cursor_id, conn: Rc::clone(&self.conn) });

                cursor_id
            }
        };

        xcb::change_window_attributes(
            &self.conn,
            self.window_id,
            &[(xcb::ffi::XCB_CW_CURSOR, cursor_id)],
        );

        self.cursor = cursor;

        Ok(())
    }

    pub fn dispatch_event(&mut self, event: &xcb::GenericEvent) -> anyhow::Result<()> {
        let r = event.response_type() & 0x7f;
        match r {
            xcb::EXPOSE => {
                let expose: &xcb::ExposeEvent = unsafe { xcb::cast_event(event) };
                self.expose(expose.x(), expose.y(), expose.width(), expose.height());
            }
            xcb::CONFIGURE_NOTIFY => {
                let cfg: &xcb::ConfigureNotifyEvent = unsafe { xcb::cast_event(event) };
                self.width = cfg.width();
                self.height = cfg.height();
                self.callbacks.resize(Dimensions {
                    pixel_width: self.width as usize,
                    pixel_height: self.height as usize,
                    dpi: 96,
                })
            }
            xcb::KEY_PRESS | xcb::KEY_RELEASE => {
                let key_press: &xcb::KeyPressEvent = unsafe { xcb::cast_event(event) };
                if let Some((code, mods)) = self.conn.keyboard.process_key_event(key_press) {
                    let key = KeyEvent {
                        key: code,
                        raw_key: None,
                        modifiers: mods,
                        repeat_count: 1,
                        key_is_down: r == xcb::KEY_PRESS,
                    };
                    self.callbacks.key_event(&key, &Window::from_id(self.window_id));
                }
            }

            xcb::MOTION_NOTIFY => {
                let motion: &xcb::MotionNotifyEvent = unsafe { xcb::cast_event(event) };

                let event = MouseEvent {
                    kind: MouseEventKind::Move,
                    coords: Point::new(
                        motion.event_x().try_into().unwrap(),
                        motion.event_y().try_into().unwrap(),
                    ),
                    screen_coords: ScreenPoint::new(
                        motion.root_x().try_into().unwrap(),
                        motion.root_y().try_into().unwrap(),
                    ),
                    modifiers: xkeysyms::modifiers_from_state(motion.state()),
                    mouse_buttons: MouseButtons::default(),
                };
                self.do_mouse_event(&event)?;
            }
            xcb::BUTTON_PRESS | xcb::BUTTON_RELEASE => {
                let button_press: &xcb::ButtonPressEvent = unsafe { xcb::cast_event(event) };

                let kind = match button_press.detail() {
                    b @ 1..=3 => {
                        let button = match b {
                            1 => MousePress::Left,
                            2 => MousePress::Middle,
                            3 => MousePress::Right,
                            _ => unreachable!(),
                        };
                        if r == xcb::BUTTON_PRESS {
                            MouseEventKind::Press(button)
                        } else {
                            MouseEventKind::Release(button)
                        }
                    }
                    b @ 4..=5 => {
                        if r == xcb::BUTTON_RELEASE {
                            return Ok(());
                        }

                        // Ideally this would be configurable, but it's currently a bit
                        // awkward to configure this layer, so let's just improve the
                        // default for now!
                        const LINES_PER_TICK: i16 = 5;

                        MouseEventKind::VertWheel(if b == 4 {
                            LINES_PER_TICK
                        } else {
                            -LINES_PER_TICK
                        })
                    }
                    _ => {
                        eprintln!("button {} is not implemented", button_press.detail());
                        return Ok(());
                    }
                };

                let event = MouseEvent {
                    kind,
                    coords: Point::new(
                        button_press.event_x().try_into().unwrap(),
                        button_press.event_y().try_into().unwrap(),
                    ),
                    screen_coords: ScreenPoint::new(
                        button_press.root_x().try_into().unwrap(),
                        button_press.root_y().try_into().unwrap(),
                    ),
                    modifiers: xkeysyms::modifiers_from_state(button_press.state()),
                    mouse_buttons: MouseButtons::default(),
                };
                self.do_mouse_event(&event)?;
            }
            xcb::CLIENT_MESSAGE => {
                let msg: &xcb::ClientMessageEvent = unsafe { xcb::cast_event(event) };
                if msg.data().data32()[0] == self.conn.atom_delete() && self.callbacks.can_close() {
                    xcb::destroy_window(self.conn.conn(), self.window_id);
                }
            }
            xcb::DESTROY_NOTIFY => {
                self.callbacks.destroy();
                self.conn.windows.borrow_mut().remove(&self.window_id);
            }
            xcb::SELECTION_CLEAR => {
                self.selection_clear()?;
            }
            xcb::SELECTION_REQUEST => {
                self.selection_request(unsafe { xcb::cast_event(event) })?;
            }
            xcb::SELECTION_NOTIFY => {
                self.selection_notify(unsafe { xcb::cast_event(event) })?;
            }
            xcb::PROPERTY_NOTIFY => {
                let msg: &xcb::PropertyNotifyEvent = unsafe { xcb::cast_event(event) };
            }
            xcb::FOCUS_IN => {
                self.callbacks.focus_change(true);
            }
            xcb::FOCUS_OUT => {
                self.callbacks.focus_change(false);
            }
            _ => {
                eprintln!("unhandled: {:x}", r);
            }
        }

        Ok(())
    }

    /// If we own the selection, make sure that the X server reflects
    /// that and vice versa.
    fn update_selection_owner(&mut self) {
        self.conn.flush();
    }

    fn selection_clear(&mut self) -> anyhow::Result<()> {
        self.update_selection_owner();
        Ok(())
    }

    /// A selection request is made to us after we've announced that we own the selection
    /// and when another client wants to copy it.
    fn selection_request(&mut self, request: &xcb::SelectionRequestEvent) -> anyhow::Result<()> {
        Ok(())
    }

    fn selection_notify(&mut self, selection: &xcb::SelectionNotifyEvent) -> anyhow::Result<()> {
        Ok(())
    }

    #[allow(dead_code, clippy::identity_op)]
    fn disable_decorations(&mut self) -> anyhow::Result<()> {
        // Set the motif hints to disable decorations.
        // See https://stackoverflow.com/a/1909708
        #[repr(C)]
        struct MwmHints {
            flags: u32,
            functions: u32,
            decorations: u32,
            input_mode: i32,
            status: u32,
        }

        const HINTS_FUNCTIONS: u32 = 1 << 0;
        const HINTS_DECORATIONS: u32 = 1 << 1;
        const FUNC_ALL: u32 = 1 << 0;
        const FUNC_RESIZE: u32 = 1 << 1;
        const FUNC_MOVE: u32 = 1 << 2;
        const FUNC_MINIMIZE: u32 = 1 << 3;
        const FUNC_MAXIMIZE: u32 = 1 << 4;
        const FUNC_CLOSE: u32 = 1 << 5;

        let hints = MwmHints {
            flags: HINTS_DECORATIONS,
            functions: 0,
            decorations: 0, // off
            input_mode: 0,
            status: 0,
        };

        let hints_slice =
            unsafe { std::slice::from_raw_parts(&hints as *const _ as *const u32, 5) };

        let atom = xcb::intern_atom(self.conn.conn(), false, "_MOTIF_WM_HINTS").get_reply()?.atom();
        xcb::change_property(
            self.conn.conn(),
            xcb::PROP_MODE_REPLACE as u8,
            self.window_id,
            atom,
            atom,
            32,
            hints_slice,
        );
        Ok(())
    }
}

/// A Window!
#[derive(Debug, Clone)]
pub struct Window(xcb::xproto::Window);

impl Window {
    pub(crate) fn from_id(id: xcb::xproto::Window) -> Self {
        Self(id)
    }

    /// Create a new window on the specified screen with the specified
    /// dimensions
    pub fn new_window(
        class_name: &str,
        name: &str,
        width: usize,
        height: usize,
        callbacks: Box<dyn WindowCallbacks>,
    ) -> anyhow::Result<Window> {
        let conn = Connection::get().ok_or_else(|| {
            anyhow!(
                "new_window must be called on the gui thread after Connection::init has succeeded",
            )
        })?;

        let window_id;
        let window = {
            let setup = conn.conn().get_setup();
            let screen = setup
                .roots()
                .nth(conn.screen_num() as usize)
                .ok_or_else(|| anyhow!("no screen?"))?;

            window_id = conn.conn().generate_id();

            xcb::create_window_checked(
                conn.conn(),
                xcb::COPY_FROM_PARENT as u8,
                window_id,
                screen.root(),
                // x, y
                0,
                0,
                // width, height
                width.try_into()?,
                height.try_into()?,
                // border width
                0,
                xcb::WINDOW_CLASS_INPUT_OUTPUT as u16,
                conn.visual.visual_id(), // screen.root_visual(),
                &[(
                    xcb::CW_EVENT_MASK,
                    xcb::EVENT_MASK_EXPOSURE
                        | xcb::EVENT_MASK_FOCUS_CHANGE
                        | xcb::EVENT_MASK_KEY_PRESS
                        | xcb::EVENT_MASK_BUTTON_PRESS
                        | xcb::EVENT_MASK_BUTTON_RELEASE
                        | xcb::EVENT_MASK_POINTER_MOTION
                        | xcb::EVENT_MASK_BUTTON_MOTION
                        | xcb::EVENT_MASK_KEY_RELEASE
                        | xcb::EVENT_MASK_PROPERTY_CHANGE
                        | xcb::EVENT_MASK_STRUCTURE_NOTIFY,
                )],
            )
            .request_check()?;

            Arc::new(Mutex::new(WindowInner {
                window_id,
                conn: Rc::clone(&conn),
                callbacks,
                width: width.try_into()?,
                height: height.try_into()?,
                expose: VecDeque::new(),
                paint_all: true,
                cursor: None,
                cursors: HashMap::new(),
                gl_state: None,
            }))
        };

        xcb_util::icccm::set_wm_class(&*conn, window_id, class_name, class_name);

        xcb::change_property(
            &*conn,
            xcb::PROP_MODE_REPLACE as u8,
            window_id,
            conn.atom_protocols,
            4,
            32,
            &[conn.atom_delete],
        );

        // window.lock().unwrap().disable_decorations()?;

        let window_handle = Window::from_id(window_id);

        window.lock().unwrap().enable_opengl()?;

        conn.windows.borrow_mut().insert(window_id, window);

        window_handle.set_title(name);
        window_handle.show();

        Ok(window_handle)
    }
}

impl WindowOpsMut for WindowInner {
    fn close(&mut self) {
        xcb::destroy_window(self.conn.conn(), self.window_id);
    }
    fn hide(&mut self) {}
    fn show(&mut self) {
        xcb::map_window(self.conn.conn(), self.window_id);
    }
    fn set_cursor(&mut self, cursor: Option<MouseCursor>) {
        WindowInner::set_cursor(self, cursor).unwrap();
    }
    fn invalidate(&mut self) {
        self.paint_all = true;
    }

    fn set_inner_size(&self, width: usize, height: usize) {
        xcb::configure_window(
            self.conn.conn(),
            self.window_id,
            &[
                (xcb::CONFIG_WINDOW_WIDTH as u16, width as u32),
                (xcb::CONFIG_WINDOW_HEIGHT as u16, height as u32),
            ],
        );
    }

    /// Change the title for the window manager
    fn set_title(&mut self, title: &str) {
        xcb_util::icccm::set_wm_name(self.conn.conn(), self.window_id, title);
    }
}

impl WindowOps for Window {
    fn close(&self) {
        Connection::with_window_inner(self.0, |inner| {
            inner.close();
        })
    }

    fn hide(&self) {
        Connection::with_window_inner(self.0, |inner| {
            inner.hide();
        })
    }

    fn show(&self) {
        Connection::with_window_inner(self.0, |inner| {
            inner.show();
        })
    }

    fn set_cursor(&self, cursor: Option<MouseCursor>) {
        Connection::with_window_inner(self.0, move |inner| {
            let _ = inner.set_cursor(cursor);
        })
    }

    fn invalidate(&self) {
        Connection::with_window_inner(self.0, |inner| {
            inner.invalidate();
        })
    }

    fn set_title(&self, title: &str) {
        let title = title.to_owned();
        Connection::with_window_inner(self.0, move |inner| {
            inner.set_title(&title);
        })
    }

    fn set_inner_size(&self, width: usize, height: usize) {
        Connection::with_window_inner(self.0, move |inner| {
            inner.set_inner_size(width, height);
        })
    }

    fn apply<F: Send + 'static + Fn(&mut dyn Any, &dyn WindowOps)>(&self, func: F)
    where
        Self: Sized,
    {
        Connection::with_window_inner(self.0, move |inner| {
            let window = Window(inner.window_id);
            func(inner.callbacks.as_any(), &window);
        });
    }

    #[cfg(feature = "opengl")]
    fn enable_opengl(&self) -> promise::Future<()> {
        XConnection::with_window_inner(self.0, move |inner| inner.enable_opengl())
    }
}
