use super::quad::*;
use super::renderstate::*;
use super::utilsprites::RenderMetrics;
use crate::config::{Config, TextStyle};
use crate::core::color::RgbColor;
use crate::core::promise;
use crate::font::FontConfiguration;
use crate::gui::{executor, front_end};
use crate::mux::renderable::Renderable;
use crate::mux::tab::{Tab, TabId};
use crate::mux::window::WindowId as MuxWindowId;
use crate::mux::Mux;
use crate::term;
use crate::term::clipboard::{Clipboard, SystemClipboard};
use crate::term::color::ColorPalette;
use crate::term::keyassignment::{KeyAssignment, KeyMap, SpawnTabDomain};
use crate::term::{CursorPosition, Line};
use crate::window;
use crate::window::bitmaps::atlas::SpriteSlice;
use crate::window::bitmaps::Texture2d;
use crate::window::*;
use chrono::{DateTime, Utc};
use failure::Fallible;
use glium::{uniform, Surface};
use std::any::Any;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use sysinfo::{ProcessorExt, System, SystemExt};

pub struct TermWindow {
    window: Option<Window>,
    fonts: Rc<FontConfiguration>,
    config: Arc<Config>,
    dimensions: Dimensions,
    mux_window_id: MuxWindowId,
    render_metrics: RenderMetrics,
    render_state: RenderState,
    clipboard: Arc<dyn Clipboard>,
    keys: KeyMap,
    frame_count: u32,
    sys: System,
}

struct Host<'a> {
    writer: &'a mut dyn std::io::Write,
    context: &'a dyn WindowOps,
    clipboard: &'a Arc<dyn Clipboard>,
}

impl<'a> term::TerminalHost for Host<'a> {
    fn writer(&mut self) -> &mut dyn std::io::Write {
        self.writer
    }

    fn get_clipboard(&mut self) -> Fallible<Arc<dyn Clipboard>> {
        Ok(Arc::clone(self.clipboard))
    }

    fn set_title(&mut self, title: &str) {
        self.context.set_title(title);
    }

    fn click_link(&mut self, link: &Arc<term::cell::Hyperlink>) {
        let link = link.clone();
        promise::Future::with_executor(executor(), move || {
            log::error!("clicking {}", link.uri());
            if let Err(err) = open::that(link.uri()) {
                log::error!("failed to open {}: {:?}", link.uri(), err);
            }
            Ok(())
        });
    }
}

impl WindowCallbacks for TermWindow {
    fn created(&mut self, window: &Window) {
        self.window.replace(window.clone());
    }

    fn can_close(&mut self) -> bool {
        let mux = Mux::get().unwrap();
        let tab = match mux.get_active_tab_for_window(self.mux_window_id) {
            Some(tab) => tab,
            None => return true,
        };
        mux.remove_tab(tab.tab_id());
        if let Some(mut win) = mux.get_window_mut(self.mux_window_id) {
            win.remove_by_id(tab.tab_id());
            return win.is_empty();
        };
        true
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn mouse_event(&mut self, event: &MouseEvent, context: &dyn WindowOps) {
        let mux = Mux::get().unwrap();
        let tab = match mux.get_active_tab_for_window(self.mux_window_id) {
            Some(tab) => tab,
            None => return,
        };

        use term::input::MouseButton as TMB;
        use term::input::MouseEventKind as TMEK;
        use window::MouseButtons as WMB;
        use window::MouseEventKind as WMEK;
        tab.mouse_event(
            term::MouseEvent {
                kind: match event.kind {
                    WMEK::Move => TMEK::Move,
                    WMEK::VertWheel(_) | WMEK::HorzWheel(_) | WMEK::Press(_) => TMEK::Press,
                    WMEK::Release(_) => TMEK::Release,
                },
                button: match event.kind {
                    WMEK::Release(ref press) | WMEK::Press(ref press) => match press {
                        MousePress::Left => TMB::Left,
                        MousePress::Middle => TMB::Middle,
                        MousePress::Right => TMB::Right,
                    },
                    WMEK::Move => {
                        if event.mouse_buttons == WMB::LEFT {
                            TMB::Left
                        } else if event.mouse_buttons == WMB::RIGHT {
                            TMB::Right
                        } else if event.mouse_buttons == WMB::MIDDLE {
                            TMB::Middle
                        } else {
                            TMB::None
                        }
                    }
                    WMEK::VertWheel(amount) => {
                        if amount > 0 {
                            TMB::WheelUp(amount as usize)
                        } else {
                            TMB::WheelDown((-amount) as usize)
                        }
                    }
                    WMEK::HorzWheel(_) => TMB::None,
                },
                x: (event.x as isize / self.render_metrics.cell_size.width) as usize,
                y: (event.y as isize / self.render_metrics.cell_size.height) as i64,
                modifiers: window_mods_to_termwiz_mods(event.modifiers),
            },
            &mut Host { writer: &mut *tab.writer(), context, clipboard: &self.clipboard },
        )
        .ok();

        match event.kind {
            WMEK::Move => {}
            _ => context.invalidate(),
        }

        context.set_cursor(Some(if tab.renderer().current_highlight().is_some() {
            MouseCursor::Hand
        } else {
            MouseCursor::Text
        }));
    }

    fn resize(&mut self, dimensions: Dimensions) {
        if dimensions.pixel_width == 0 || dimensions.pixel_height == 0 {
            return;
        }
        self.scaling_changed(dimensions, self.fonts.get_font_scale());
    }

    fn key_event(&mut self, key: &KeyEvent, _context: &dyn WindowOps) -> bool {
        if !key.key_is_down {
            return false;
        }

        enum Key {
            Code(crate::core::input::KeyCode),
            Composed(String),
            None,
        }

        fn win_key_code_to_termwiz_key_code(key: &window::KeyCode) -> Key {
            use crate::core::input::KeyCode as KC;
            use window::KeyCode as WK;

            let code = match key {
                WK::Char('\r') => KC::Enter,
                WK::Char('\t') => KC::Tab,
                WK::Char('\u{08}') => KC::Backspace,
                WK::Char('\u{1b}') => KC::Escape,
                WK::Char('\u{7f}') => KC::Delete,
                WK::Char(c) => KC::Char(*c),
                WK::Function(f) => KC::Function(*f),
                WK::LeftArrow => KC::LeftArrow,
                WK::RightArrow => KC::RightArrow,
                WK::UpArrow => KC::UpArrow,
                WK::DownArrow => KC::DownArrow,
                WK::Home => KC::Home,
                WK::End => KC::End,
                WK::PageUp => KC::PageUp,
                WK::PageDown => KC::PageDown,
                WK::Insert => KC::Insert,
                WK::Super => KC::Super,
                WK::Clear => KC::Clear,
                WK::Shift => KC::Shift,
                WK::Control => KC::Control,
                WK::Alt => KC::Alt,
                WK::Pause => KC::Pause,
                WK::CapsLock => KC::CapsLock,
                WK::Print => KC::Print,
                WK::Help => KC::Help,
                WK::Multiply => KC::Multiply,
                WK::Applications => KC::Applications,
                WK::Add => KC::Add,
                WK::Numpad(0) => KC::Numpad0,
                WK::Numpad(1) => KC::Numpad1,
                WK::Numpad(2) => KC::Numpad2,
                WK::Numpad(3) => KC::Numpad3,
                WK::Numpad(4) => KC::Numpad4,
                WK::Numpad(5) => KC::Numpad5,
                WK::Numpad(6) => KC::Numpad6,
                WK::Numpad(7) => KC::Numpad7,
                WK::Numpad(8) => KC::Numpad8,
                WK::Numpad(9) => KC::Numpad9,
                WK::Numpad(_) => return Key::None,
                WK::Separator => KC::Separator,
                WK::Subtract => KC::Subtract,
                WK::Decimal => KC::Decimal,
                WK::Divide => KC::Divide,
                WK::NumLock => KC::NumLock,
                WK::ScrollLock => KC::ScrollLock,
                WK::BrowserBack => KC::BrowserBack,
                WK::BrowserForward => KC::BrowserForward,
                WK::BrowserRefresh => KC::BrowserRefresh,
                WK::BrowserStop => KC::BrowserStop,
                WK::BrowserFavorites => KC::BrowserFavorites,
                WK::BrowserHome => KC::BrowserHome,
                WK::VolumeMute => KC::VolumeMute,
                WK::VolumeDown => KC::VolumeDown,
                WK::VolumeUp => KC::VolumeUp,
                WK::Cancel => KC::Cancel,
                WK::Composed(ref s) => {
                    return Key::Composed(s.to_owned());
                }
                WK::PrintScreen => KC::PrintScreen,
            };
            Key::Code(code)
        }

        let mux = Mux::get().unwrap();
        if let Some(tab) = mux.get_active_tab_for_window(self.mux_window_id) {
            let modifiers = window_mods_to_termwiz_mods(key.modifiers);

            if let Some(key) = &key.raw_key {
                if let Key::Code(key) = win_key_code_to_termwiz_key_code(&key) {
                    if let Some(assignment) = self.keys.lookup(key, modifiers) {
                        self.perform_key_assignment(&tab, &assignment).ok();
                        return true;
                    }

                    if !self.config.send_composed_key_when_alt_is_pressed
                        && modifiers.contains(crate::core::input::Modifiers::ALT)
                    {
                        if tab.key_down(key, modifiers).is_ok() {
                            return true;
                        }
                    }
                }
            }

            let key = win_key_code_to_termwiz_key_code(&key.key);
            match key {
                Key::Code(key) => {
                    if let Some(assignment) = self.keys.lookup(key, modifiers) {
                        self.perform_key_assignment(&tab, &assignment).ok();
                        return true;
                    } else if tab.key_down(key, modifiers).is_ok() {
                        return true;
                    }
                }
                Key::Composed(s) => {
                    tab.writer().write_all(s.as_bytes()).ok();
                    return true;
                }
                Key::None => {}
            }
        }

        false
    }

    fn paint_opengl(&mut self, frame: &mut glium::Frame) {
        let mux = Mux::get().unwrap();
        let tab = match mux.get_active_tab_for_window(self.mux_window_id) {
            Some(tab) => tab,
            None => {
                frame.clear_color(0., 0., 0., 1.);
                return;
            }
        };

        self.update_text_cursor(&tab);
        self.clear(&tab, frame);
        self.paint_tab_opengl(&tab, frame).expect("error while painting tab");
        self.paint_header_opengl(&tab, frame).expect("error while painting sprite");
        self.update_title();
    }
}

impl TermWindow {
    pub fn new_window(
        config: &Arc<Config>,
        fontconfig: &Rc<FontConfiguration>,
        tab: &Rc<dyn Tab>,
        mux_window_id: MuxWindowId,
    ) -> Fallible<()> {
        log::error!("TermWindow::new_window called with mux_window_id {}", mux_window_id);
        let (physical_rows, physical_cols) = tab.renderer().physical_dimensions();

        let render_metrics = RenderMetrics::new(fontconfig);

        let width = render_metrics.cell_size.width as usize * physical_cols;
        let height = render_metrics.cell_size.height as usize * physical_rows;

        const ATLAS_SIZE: usize = 4096;
        let render_state = RenderState::Software(SoftwareRenderState::new(
            fontconfig,
            &render_metrics,
            ATLAS_SIZE,
        )?);

        let sys = System::new();

        let window = Window::new_window(
            "miro",
            "miro",
            width,
            height,
            Box::new(Self {
                window: None,
                mux_window_id,
                config: Arc::clone(config),
                fonts: Rc::clone(fontconfig),
                render_metrics,
                dimensions: Dimensions { pixel_width: width, pixel_height: height, dpi: 96 },
                render_state,
                clipboard: Arc::new(SystemClipboard::new()),
                keys: KeyMap::new(),
                frame_count: 0,
                sys,
            }),
        )?;

        let cloned_window = window.clone();

        Connection::get().unwrap().schedule_timer(
            std::time::Duration::from_millis(35),
            move || {
                let mux = Mux::get().unwrap();
                if let Some(tab) = mux.get_active_tab_for_window(mux_window_id) {
                    if tab.renderer().has_dirty_lines() {
                        cloned_window.invalidate();
                    }
                } else {
                    cloned_window.close();
                }
            },
        );

        if super::is_opengl_enabled() {
            window.enable_opengl(|any, window, maybe_ctx| {
                let mut termwindow = any.downcast_mut::<TermWindow>().expect("to be TermWindow");
                match maybe_ctx {
                    Ok(ctx) => {
                        match OpenGLRenderState::new(
                            ctx,
                            &termwindow.fonts,
                            &termwindow.render_metrics,
                            ATLAS_SIZE,
                            termwindow.dimensions.pixel_width,
                            termwindow.dimensions.pixel_height,
                            &termwindow.config.theme,
                        ) {
                            Ok(gl) => {
                                log::error!(
                                    "OpenGL initialized! {} {}",
                                    gl.context.get_opengl_renderer_string(),
                                    gl.context.get_opengl_version_string()
                                );
                                termwindow.render_state = RenderState::GL(gl);
                            }
                            Err(err) => {
                                log::error!("OpenGL init failed: {}", err);
                            }
                        }
                    }
                    Err(err) => log::error!("OpenGL init failed: {}", err),
                };

                window.show();
            });
        } else {
            window.show();
        }

        Ok(())
    }

    fn recreate_texture_atlas(&mut self, size: Option<usize>) -> Fallible<()> {
        self.render_state.recreate_texture_atlas(&self.fonts, &self.render_metrics, size)
    }

    fn update_title(&mut self) {
        let mux = Mux::get().unwrap();
        let window = match mux.get_window(self.mux_window_id) {
            Some(window) => window,
            _ => return,
        };
        let num_tabs = window.len();

        if num_tabs == 0 {
            return;
        }
        let tab_no = window.get_active_idx();

        let title = match window.get_active() {
            Some(tab) => tab.get_title(),
            None => return,
        };

        drop(window);

        if let Some(window) = self.window.as_ref() {
            if num_tabs == 1 {
                window.set_title(&title);
            } else {
                window.set_title(&format!("[{}/{}] {}", tab_no + 1, num_tabs, title));
            }
        }
    }

    fn update_text_cursor(&mut self, tab: &Rc<dyn Tab>) {
        let term = tab.renderer();
        let cursor = term.get_cursor_position();
        if let Some(win) = self.window.as_ref() {
            let r = Rect::new(
                Point::new(
                    cursor.x.max(0) as isize * self.render_metrics.cell_size.width,
                    cursor.y.max(0) as isize * self.render_metrics.cell_size.height,
                ),
                self.render_metrics.cell_size,
            );
            win.set_text_cursor_position(r);
        }
    }

    fn activate_tab(&mut self, tab_idx: usize) -> Fallible<()> {
        let mux = Mux::get().unwrap();
        let mut window = mux
            .get_window_mut(self.mux_window_id)
            .ok_or_else(|| failure::format_err!("no such window"))?;

        let max = window.len();
        if tab_idx < max {
            window.set_active(tab_idx);

            drop(window);
            self.update_title();
        }
        Ok(())
    }

    fn activate_tab_relative(&mut self, delta: isize) -> Fallible<()> {
        let mux = Mux::get().unwrap();
        let window = mux
            .get_window(self.mux_window_id)
            .ok_or_else(|| failure::format_err!("no such window"))?;

        let max = window.len();
        failure::ensure!(max > 0, "no more tabs");

        let active = window.get_active_idx() as isize;
        let tab = active + delta;
        let tab = if tab < 0 { max as isize + tab } else { tab };
        drop(window);
        self.activate_tab(tab as usize % max)
    }

    fn spawn_tab(&mut self, domain: &SpawnTabDomain) -> Fallible<TabId> {
        let rows = (self.dimensions.pixel_height as usize + 1)
            / self.render_metrics.cell_size.height as usize;
        let cols = (self.dimensions.pixel_width as usize + 1)
            / self.render_metrics.cell_size.width as usize;

        let size = crate::pty::PtySize {
            rows: rows as u16,
            cols: cols as u16,
            pixel_width: self.dimensions.pixel_width as u16,
            pixel_height: self.dimensions.pixel_height as u16,
        };

        let mux = Mux::get().unwrap();

        let domain = match domain {
            SpawnTabDomain::DefaultDomain => mux.default_domain().clone(),
            SpawnTabDomain::CurrentTabDomain => {
                let tab = match mux.get_active_tab_for_window(self.mux_window_id) {
                    Some(tab) => tab,
                    None => failure::bail!("window has no tabs?"),
                };
                mux.get_domain(tab.domain_id()).ok_or_else(|| {
                    failure::format_err!("current tab has unresolvable domain id!?")
                })?
            }
        };
        let tab = domain.spawn(size, self.mux_window_id)?;
        let tab_id = tab.tab_id();

        let len = {
            let window = mux
                .get_window(self.mux_window_id)
                .ok_or_else(|| failure::format_err!("no such window!?"))?;
            window.len()
        };
        self.activate_tab(len - 1)?;
        Ok(tab_id)
    }

    #[allow(dead_code)]
    fn perform_key_assignment(
        &mut self,
        tab: &Rc<dyn Tab>,
        assignment: &KeyAssignment,
    ) -> Fallible<()> {
        use KeyAssignment::*;
        match assignment {
            SpawnTab(spawn_where) => {
                self.spawn_tab(spawn_where)?;
            }
            SpawnWindow => {
                self.spawn_new_window();
            }
            ToggleFullScreen => {}
            Copy => {}
            Paste => {
                tab.trickle_paste(self.clipboard.get_contents()?)?;
            }
            ActivateTabRelative(n) => {
                self.activate_tab_relative(*n)?;
            }
            DecreaseFontSize => self.decrease_font_size(),
            IncreaseFontSize => self.increase_font_size(),
            ResetFontSize => self.reset_font_size(),
            ActivateTab(n) => {
                self.activate_tab(*n)?;
            }
            Hide => {
                if let Some(w) = self.window.as_ref() {
                    w.hide();
                }
            }
        };
        Ok(())
    }

    pub fn spawn_new_window(&mut self) {
        promise::Future::with_executor(executor(), move || {
            let mux = Mux::get().unwrap();
            let fonts = Rc::new(FontConfiguration::new(Arc::clone(mux.config())));
            let window_id = mux.new_empty_window();
            let tab = mux.default_domain().spawn(crate::pty::PtySize::default(), window_id)?;
            let front_end = front_end().expect("to be called on gui thread");
            front_end.spawn_new_window(mux.config(), &fonts, &tab, window_id)?;
            Ok(())
        });
    }

    #[allow(clippy::float_cmp)]
    fn scaling_changed(&mut self, dimensions: Dimensions, font_scale: f64) {
        let mux = Mux::get().unwrap();
        if let Some(window) = mux.get_window(self.mux_window_id) {
            let cols = self.dimensions.pixel_width / self.render_metrics.cell_size.width as usize;
            let rows = self.dimensions.pixel_height / self.render_metrics.cell_size.height as usize;

            let scale_changed =
                dimensions.dpi != self.dimensions.dpi || font_scale != self.fonts.get_font_scale();

            if scale_changed {
                let new_dpi = dimensions.dpi as f64 / 96.;
                self.fonts.change_scaling(font_scale, new_dpi);
                self.render_metrics = RenderMetrics::new(&self.fonts);
                self.recreate_texture_atlas(None).expect("failed to recreate atlas");
            }

            self.dimensions = dimensions;

            self.render_state
                .advise_of_window_size_change(
                    &self.render_metrics,
                    dimensions.pixel_width,
                    dimensions.pixel_height,
                )
                .expect("failed to advise of resize");

            let size = crate::pty::PtySize {
                rows: dimensions.pixel_height as u16 / self.render_metrics.cell_size.height as u16,
                cols: dimensions.pixel_width as u16 / self.render_metrics.cell_size.width as u16,
                pixel_height: dimensions.pixel_height as u16,
                pixel_width: dimensions.pixel_width as u16,
            };
            for tab in window.iter() {
                tab.resize(size).ok();
            }

            if scale_changed {
                if let Some(window) = self.window.as_ref() {
                    window.set_inner_size(
                        cols * self.render_metrics.cell_size.width as usize,
                        rows * self.render_metrics.cell_size.height as usize,
                    );
                }
            }
        };
    }

    fn decrease_font_size(&mut self) {
        self.scaling_changed(self.dimensions, self.fonts.get_font_scale() * 0.9);
    }
    fn increase_font_size(&mut self) {
        self.scaling_changed(self.dimensions, self.fonts.get_font_scale() * 1.1);
    }
    fn reset_font_size(&mut self) {
        self.scaling_changed(self.dimensions, 1.);
    }

    #[allow(unused_variables)]
    fn paint_header_opengl(&mut self, tab: &Rc<dyn Tab>, frame: &mut glium::Frame) -> Fallible<()> {
        self.frame_count += 1;

        if self.frame_count % 5 == 0 {
            self.sys.refresh_system();
        }

        let palette = tab.palette();

        let dpi = self.render_state.opengl().dpi;
        if self.dimensions.dpi as f32 != dpi {
            self.render_state.change_header_scaling(
                self.dimensions.dpi as f32,
                &self.render_metrics,
                self.dimensions.pixel_width,
                self.dimensions.pixel_height,
            )?;
        }
        let gl_state = self.render_state.opengl();

        let projection = euclid::Transform3D::<f32, f32, f32>::ortho(
            -(self.dimensions.pixel_width as f32) / 2.0,
            self.dimensions.pixel_width as f32 / 2.0,
            self.dimensions.pixel_height as f32 / 2.0,
            -(self.dimensions.pixel_height as f32) / 2.0,
            -1.0,
            1.0,
        )
        .to_arrays();

        let draw_params =
            glium::DrawParameters { blend: glium::Blend::alpha_blending(), ..Default::default() };

        frame.draw(
            &*gl_state.header_rect_vertex_buffer.borrow(),
            &gl_state.header_rect_index_buffer,
            &gl_state.header_program,
            &uniform! {
                projection: projection,
            },
            &draw_params,
        )?;

        self.render_header_line_opengl(&palette)?;

        let tex = gl_state.glyph_cache.borrow().atlas.texture();

        frame.draw(
            &*gl_state.header_glyph_vertex_buffer.borrow(),
            &gl_state.header_glyph_index_buffer,
            &gl_state.glyph_program,
            &uniform! {
                projection: projection,
                glyph_tex: &*tex,
                bg_and_line_layer: false,
            },
            &draw_params,
        )?;

        let number_of_sprites = gl_state.spritesheet.sprites.len();
        let sprite =
            &gl_state.spritesheet.sprites[(self.frame_count % number_of_sprites as u32) as usize];
        let w = self.dimensions.pixel_width as f32 as f32 / 2.0;
        frame.draw(
            &*gl_state.sprite_vertex_buffer.borrow(),
            &gl_state.sprite_index_buffer,
            &gl_state.sprite_program,
            &uniform! {
                projection: projection,
                tex: &gl_state.player_texture.tex,
                source_dimensions: sprite.size,
                source_position: sprite.position,
                source_texture_dimensions: [gl_state.player_texture.width, gl_state.player_texture.height]
            },
            &draw_params,
        )?;

        gl_state.slide_sprite(w);
        Ok(())
    }

    fn paint_tab_opengl(&mut self, tab: &Rc<dyn Tab>, frame: &mut glium::Frame) -> Fallible<()> {
        let palette = tab.palette();
        let mut term = tab.renderer();
        let cursor = term.get_cursor_position();
        let dirty_lines = term.get_dirty_lines();
        for (line_idx, line, selrange) in dirty_lines {
            self.render_screen_line_opengl(line_idx, &line, selrange, &cursor, &*term, &palette)?;
        }
        let gl_state = self.render_state.opengl();
        let tex = gl_state.glyph_cache.borrow().atlas.texture();
        let projection = euclid::Transform3D::<f32, f32, f32>::ortho(
            -(self.dimensions.pixel_width as f32) / 2.0,
            self.dimensions.pixel_width as f32 / 2.0,
            self.dimensions.pixel_height as f32 / 2.0,
            -(self.dimensions.pixel_height as f32) / 2.0,
            -1.0,
            1.0,
        )
        .to_arrays();

        let draw_params =
            glium::DrawParameters { blend: glium::Blend::alpha_blending(), ..Default::default() };

        frame.draw(
            &*gl_state.glyph_vertex_buffer.borrow(),
            &gl_state.glyph_index_buffer,
            &gl_state.glyph_program,
            &uniform! {
                projection: projection,
                glyph_tex: &*tex,
                bg_and_line_layer: true,
            },
            &draw_params,
        )?;

        frame.draw(
            &*gl_state.glyph_vertex_buffer.borrow(),
            &gl_state.glyph_index_buffer,
            &gl_state.glyph_program,
            &uniform! {
                projection: projection,
                glyph_tex: &*tex,
                bg_and_line_layer: false,
            },
            &draw_params,
        )?;

        term.clean_dirty_lines();

        Ok(())
    }

    fn render_header_line_opengl(&self, palette: &ColorPalette) -> Fallible<()> {
        let gl_state = self.render_state.opengl();
        let now: DateTime<Utc> = Utc::now();
        let current_time = now.format("%H:%M:%S").to_string();
        let cpu_load = format!("{}", self.sys.get_global_processor_info().get_cpu_usage().round());
        let mut vb = gl_state.header_glyph_vertex_buffer.borrow_mut();
        let mut vertices = vb
            .slice_mut(..)
            .ok_or_else(|| format_err!("we're confused about the screen size"))?
            .map();

        let style = TextStyle::default();

        let indent = 3 - cpu_load.len();

        let glyph_info = {
            let font = self.fonts.cached_font(&style)?;
            let mut font = font.borrow_mut();
            font.shape(&format!(
                "CPU:{}%{:indent$}{}",
                cpu_load,
                "",
                current_time,
                indent = indent
            ))?
        };

        let glyph_color = palette.resolve_fg(term::color::ColorAttribute::PaletteIndex(0xff));
        let bg_color = palette.resolve_bg(term::color::ColorAttribute::Default);

        for (glyph_idx, info) in glyph_info.iter().enumerate() {
            let glyph = gl_state.glyph_cache.borrow_mut().cached_glyph(info, &style)?;

            let left = (glyph.x_offset + glyph.bearing_x) as f32;
            let top = ((self.render_metrics.cell_size.height as f64
                + self.render_metrics.descender)
                - (glyph.y_offset + glyph.bearing_y)) as f32;

            let texture = glyph.texture.as_ref().unwrap_or(&gl_state.util_sprites.white_space);

            let slice = SpriteSlice {
                cell_idx: glyph_idx,
                num_cells: info.num_cells as usize,
                cell_width: self.render_metrics.cell_size.width as usize,
                scale: glyph.scale as f32,
                left_offset: left,
            };

            let pixel_rect = slice.pixel_rect(texture);
            let texture_rect = texture.texture.to_texture_coords(pixel_rect);

            let bottom = (pixel_rect.size.height as f32 * glyph.scale as f32) + top
                - self.render_metrics.cell_size.height as f32;
            let right =
                pixel_rect.size.width as f32 + left - self.render_metrics.cell_size.width as f32;

            let mut quad = Quad::for_cell(glyph_idx, &mut vertices);

            quad.set_fg_color(rgbcolor_to_window_color(glyph_color));
            quad.set_bg_color(rgbcolor_to_window_color(bg_color));
            quad.set_texture(texture_rect);
            quad.set_texture_adjust(left, top, right, bottom);
            quad.set_has_color(glyph.has_color);
        }

        Ok(())
    }

    fn render_screen_line_opengl(
        &self,
        line_idx: usize,
        line: &Line,
        selection: Range<usize>,
        cursor: &CursorPosition,
        terminal: &dyn Renderable,
        palette: &ColorPalette,
    ) -> Fallible<()> {
        let gl_state = self.render_state.opengl();

        let (_num_rows, num_cols) = terminal.physical_dimensions();
        let mut vb = gl_state.glyph_vertex_buffer.borrow_mut();
        let mut vertices = {
            let per_line = num_cols * VERTICES_PER_CELL;
            let start_pos = line_idx * per_line;
            vb.slice_mut(start_pos..start_pos + per_line)
                .ok_or_else(|| failure::err_msg("we're confused about the screen size"))?
                .map()
        };

        let current_highlight = terminal.current_highlight();

        let cell_clusters = line.cluster();
        let mut last_cell_idx = 0;
        for cluster in cell_clusters {
            let attrs = &cluster.attrs;
            let is_highlited_hyperlink = match (&attrs.hyperlink, &current_highlight) {
                (&Some(ref this), &Some(ref highlight)) => this == highlight,
                _ => false,
            };
            let style = self.fonts.match_style(attrs);

            let bg_color = palette.resolve_bg(attrs.background);
            let fg_color = match attrs.foreground {
                term::color::ColorAttribute::Default => {
                    if let Some(fg) = style.foreground {
                        fg
                    } else {
                        palette.resolve_fg(attrs.foreground)
                    }
                }
                term::color::ColorAttribute::PaletteIndex(idx) if idx < 8 => {
                    let idx =
                        if attrs.intensity() == term::Intensity::Bold { idx + 8 } else { idx };
                    palette.resolve_fg(term::color::ColorAttribute::PaletteIndex(idx))
                }
                _ => palette.resolve_fg(attrs.foreground),
            };

            let (fg_color, bg_color) = {
                let mut fg = fg_color;
                let mut bg = bg_color;

                if attrs.reverse() {
                    std::mem::swap(&mut fg, &mut bg);
                }

                (fg, bg)
            };

            let glyph_color = rgbcolor_to_window_color(fg_color);
            let bg_color = rgbcolor_to_window_color(bg_color);

            let glyph_info = {
                let font = self.fonts.cached_font(style)?;
                let mut font = font.borrow_mut();
                font.shape(&cluster.text)?
            };

            for info in &glyph_info {
                let cell_idx = cluster.byte_to_cell_idx[info.cluster as usize];
                let glyph = gl_state.glyph_cache.borrow_mut().cached_glyph(info, style)?;

                let left = (glyph.x_offset + glyph.bearing_x) as f32;
                let top = ((self.render_metrics.cell_size.height as f64
                    + self.render_metrics.descender)
                    - (glyph.y_offset + glyph.bearing_y)) as f32;

                let underline_tex_rect = gl_state
                    .util_sprites
                    .select_sprite(is_highlited_hyperlink, attrs.strikethrough(), attrs.underline())
                    .texture_coords();

                for glyph_idx in 0..info.num_cells as usize {
                    let cell_idx = cell_idx + glyph_idx;

                    if cell_idx >= num_cols {
                        break;
                    }
                    last_cell_idx = cell_idx;

                    let (glyph_color, bg_color) = self.compute_cell_fg_bg(
                        line_idx,
                        cell_idx,
                        cursor,
                        &selection,
                        glyph_color,
                        bg_color,
                        palette,
                    );

                    let texture =
                        glyph.texture.as_ref().unwrap_or(&gl_state.util_sprites.white_space);

                    let slice = SpriteSlice {
                        cell_idx: glyph_idx,
                        num_cells: info.num_cells as usize,
                        cell_width: self.render_metrics.cell_size.width as usize,
                        scale: glyph.scale as f32,
                        left_offset: left,
                    };

                    let pixel_rect = slice.pixel_rect(texture);
                    let texture_rect = texture.texture.to_texture_coords(pixel_rect);

                    let left = if glyph_idx == 0 { left } else { 0.0 };
                    let bottom = (pixel_rect.size.height as f32 * glyph.scale as f32) + top
                        - self.render_metrics.cell_size.height as f32;
                    let right = pixel_rect.size.width as f32 + left
                        - self.render_metrics.cell_size.width as f32;

                    let mut quad = Quad::for_cell(cell_idx, &mut vertices);

                    quad.set_fg_color(glyph_color);
                    quad.set_bg_color(bg_color);
                    quad.set_texture(texture_rect);
                    quad.set_texture_adjust(left, top, right, bottom);
                    quad.set_underline(underline_tex_rect);
                    quad.set_has_color(glyph.has_color);
                }
            }
        }

        let white_space = gl_state.util_sprites.white_space.texture_coords();

        for cell_idx in last_cell_idx + 1..num_cols {
            let (glyph_color, bg_color) = self.compute_cell_fg_bg(
                line_idx,
                cell_idx,
                cursor,
                &selection,
                rgbcolor_to_window_color(palette.foreground),
                rgbcolor_to_window_color(palette.background),
                palette,
            );

            let mut quad = Quad::for_cell(cell_idx, &mut vertices);

            quad.set_bg_color(bg_color);
            quad.set_fg_color(glyph_color);
            quad.set_texture(white_space);
            quad.set_texture_adjust(0., 0., 0., 0.);
            quad.set_underline(white_space);
            quad.set_has_color(false);
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn compute_cell_fg_bg(
        &self,
        line_idx: usize,
        cell_idx: usize,
        cursor: &CursorPosition,
        selection: &Range<usize>,
        fg_color: Color,
        bg_color: Color,
        palette: &ColorPalette,
    ) -> (Color, Color) {
        let selected = selection.contains(&cell_idx);
        let is_cursor = line_idx as i64 == cursor.y && cursor.x == cell_idx;

        let (fg_color, bg_color) = match (selected, is_cursor) {
            (false, false) => (fg_color, bg_color),

            (_, true) => (
                rgbcolor_to_window_color(palette.cursor_fg),
                rgbcolor_to_window_color(palette.cursor_bg),
            ),

            (true, false) => (
                rgbcolor_to_window_color(palette.selection_fg),
                rgbcolor_to_window_color(palette.selection_bg),
            ),
        };

        (fg_color, bg_color)
    }

    fn clear(&mut self, tab: &Rc<dyn Tab>, frame: &mut glium::Frame) {
        let palette = tab.palette();
        let background_color = palette.resolve_bg(term::color::ColorAttribute::Default);
        let (r, g, b, a) = background_color.to_tuple_rgba();
        frame.clear_color(r, g, b, a);
    }
}

fn rgbcolor_to_window_color(color: RgbColor) -> Color {
    Color::rgba(color.red, color.green, color.blue, 0xff)
}

fn window_mods_to_termwiz_mods(modifiers: window::Modifiers) -> crate::core::input::Modifiers {
    let mut result = crate::core::input::Modifiers::NONE;
    if modifiers.contains(window::Modifiers::SHIFT) {
        result.insert(crate::core::input::Modifiers::SHIFT);
    }
    if modifiers.contains(window::Modifiers::ALT) {
        result.insert(crate::core::input::Modifiers::ALT);
    }
    if modifiers.contains(window::Modifiers::CTRL) {
        result.insert(crate::core::input::Modifiers::CTRL);
    }
    if modifiers.contains(window::Modifiers::SUPER) {
        result.insert(crate::core::input::Modifiers::SUPER);
    }
    result
}
