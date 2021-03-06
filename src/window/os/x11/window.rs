use super::*;
use crate::window::connection::ConnectionOps;
use crate::window::{
    Dimensions, KeyEvent, MouseButtons, MouseCursor, MouseEvent, MouseEventKind, MousePress,
    WindowCallbacks, WindowOps, WindowOpsMut,
};
use anyhow::anyhow;
use std::any::Any;
use std::convert::TryInto;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

pub(crate) struct WindowInner {
    window_id: xcb::xproto::Window,
    conn: Rc<Connection>,
    callbacks: Box<dyn WindowCallbacks>,
    width: u16,
    height: u16,
    cursor: Option<MouseCursor>,
    gl_state: Option<Rc<glium::backend::Context>>,
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
        if let Some(gl_context) = self.gl_state.as_ref() {
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

    fn do_mouse_event(&mut self, event: &MouseEvent) -> anyhow::Result<()> {
        self.callbacks.mouse_event(&event, &Window::from_id(self.window_id));
        Ok(())
    }

    fn set_cursor(&mut self, cursor: Option<MouseCursor>) -> anyhow::Result<()> {
        if cursor == self.cursor {
            return Ok(());
        }

        let id_no = match cursor.unwrap_or(MouseCursor::Arrow) {
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

        xcb::change_window_attributes(
            &self.conn,
            self.window_id,
            &[(xcb::ffi::XCB_CW_CURSOR, cursor_id)],
        );

        xcb::free_cursor(&self.conn, cursor_id);

        Ok(())
    }

    pub fn dispatch_event(&mut self, event: &xcb::GenericEvent) -> anyhow::Result<()> {
        let r = event.response_type() & 0x7f;
        match r {
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
                    x: motion.event_x().max(0) as u16,
                    y: motion.event_y().max(0) as u16,
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
                        MouseEventKind::VertWheel(if b == 4 { 1 } else { -1 })
                    }
                    _ => {
                        eprintln!("button {} is not implemented", button_press.detail());
                        return Ok(());
                    }
                };

                let event = MouseEvent {
                    kind,
                    x: button_press.event_x().max(0) as u16,
                    y: button_press.event_y().max(0) as u16,
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
}

#[derive(Debug, Clone)]
pub struct Window(xcb::xproto::Window);

impl Window {
    pub(crate) fn from_id(id: xcb::xproto::Window) -> Self {
        Self(id)
    }

    pub fn new_window(
        _class_name: &str,
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
                0,
                0,
                width.try_into()?,
                height.try_into()?,
                0,
                xcb::WINDOW_CLASS_INPUT_OUTPUT as u16,
                conn.visual.visual_id(),
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
                cursor: None,
                gl_state: None,
            }))
        };

        xcb::change_property(
            &*conn,
            xcb::PROP_MODE_REPLACE as u8,
            window_id,
            conn.atom_protocols,
            4,
            32,
            &[conn.atom_delete],
        );

        let window_handle = Window::from_id(window_id);
        window.lock().unwrap().enable_opengl()?;
        conn.windows.borrow_mut().insert(window_id, window.clone());
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

    fn set_title(&mut self, title: &str) {
        xcb_util::icccm::set_wm_name(self.conn.conn(), self.window_id, title);
    }
}

impl WindowOps for Window {
    fn close(&self) {
        Connection::with_window_inner(self.0, |inner| inner.close());
    }
    fn hide(&self) {
        Connection::with_window_inner(self.0, |inner| inner.hide());
    }
    fn show(&self) {
        Connection::with_window_inner(self.0, |inner| inner.show());
    }
    fn set_cursor(&self, cursor: Option<MouseCursor>) {
        Connection::with_window_inner(self.0, move |inner| {
            let _ = inner.set_cursor(cursor);
        });
    }
    fn set_title(&self, title: &str) {
        let title = title.to_owned();
        Connection::with_window_inner(self.0, move |inner| inner.set_title(&title));
    }

    fn set_inner_size(&self, width: usize, height: usize) {
        Connection::with_window_inner(self.0, move |inner| inner.set_inner_size(width, height));
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
}
