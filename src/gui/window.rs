use super::header::Header;
use super::quad::*;
use super::renderstate::RenderState;
use super::utilsprites::RenderMetrics;
use crate::core::color::RgbColor;
use crate::core::promise;
use crate::core::surface::CursorShape;
use crate::font::FontConfiguration;
use crate::mux::tab::Tab;
use crate::mux::Mux;
use crate::pty::PtySize;
use crate::term;
use crate::term::clipboard::{Clipboard, SystemClipboard};
use crate::term::color::ColorPalette;
use crate::term::keyassignment::{KeyAssignment, KeyMap};
use crate::term::Terminal;
use crate::term::{CursorPosition, Line};
use crate::window;
use crate::window::bitmaps::atlas::OutOfTextureSpace;
use crate::window::bitmaps::atlas::SpriteSlice;
use crate::window::bitmaps::Texture2d;
use crate::window::*;
use glium::{uniform, Surface};
use std::any::Any;
use std::cell::Ref;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

const ATLAS_SIZE: usize = 4096;

#[derive(Debug, Clone, Copy)]
struct RowsAndCols {
    rows: usize,
    cols: usize,
}

pub struct TermWindow {
    window: Option<Window>,
    fonts: Rc<FontConfiguration>,
    dimensions: Dimensions,
    render_metrics: RenderMetrics,
    render_state: Option<RenderState>,
    clipboard: Arc<dyn Clipboard>,
    keys: KeyMap,
    frame_count: u32,
    terminal_size: PtySize,
    header: Header,
    focused: Option<Instant>,
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

    fn get_clipboard(&mut self) -> anyhow::Result<Arc<dyn Clipboard>> {
        Ok(Arc::clone(self.clipboard))
    }

    fn set_title(&mut self, title: &str) {
        self.context.set_title(title);
    }

    fn click_link(&mut self, link: &Arc<term::cell::Hyperlink>) {
        let link = link.clone();
        promise::spawn(async move { if let Err(_) = open::that(link.uri()) {} });
    }
}

impl WindowCallbacks for TermWindow {
    fn created(
        &mut self,
        window: &Window,
        ctx: std::rc::Rc<glium::backend::Context>,
    ) -> anyhow::Result<()> {
        self.window.replace(window.clone());
        let mux = Mux::get().unwrap();
        self.render_state = Some(RenderState::new(
            ctx,
            &self.fonts,
            &self.render_metrics,
            ATLAS_SIZE,
            self.dimensions.pixel_width,
            self.dimensions.pixel_height,
            &mux.config().theme,
        )?);

        window.show();

        if self.render_state.is_none() {
            panic!("No OpenGL");
        }

        Ok(())
    }

    fn focus_change(&mut self, focused: bool) {
        self.focused = if focused { Some(Instant::now()) } else { None };
        let mux = Mux::get().unwrap();
        let tab = mux.get_tab();
        tab.renderer().make_all_lines_dirty();
    }

    fn can_close(&self) -> bool {
        let mux = Mux::get().unwrap();
        mux.close();
        mux.can_close()
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn mouse_event(&mut self, event: &MouseEvent, context: &dyn WindowOps) {
        use term::input::MouseButton as TMB;
        use term::input::MouseEventKind as TMEK;
        use window::MouseButtons as WMB;
        use window::MouseEventKind as WMEK;

        let mux = Mux::get().unwrap();
        let tab = mux.get_tab();

        let x = (event.x as isize / self.render_metrics.cell_size.width) as usize;
        let y = (event.y as isize / self.render_metrics.cell_size.height) as i64;

        let adjusted_y = y.saturating_sub(self.header.offset as i64);

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
                x,
                y: adjusted_y,
                modifiers: window_mods_to_termwiz_mods(event.modifiers),
            },
            &mut Host { writer: &mut *tab.writer(), context, clipboard: &self.clipboard },
        )
        .ok();

        match event.kind {
            WMEK::Move => {}
            WMEK::Press(_) => {
                if let Some(focused) = self.focused.as_ref() {
                    let now = Instant::now();
                    if now - *focused <= Duration::from_millis(200) {
                        return;
                    }
                }
            }
            _ => {}
        }

        context.set_cursor(Some(if y < self.header.offset as i64 {
            MouseCursor::Arrow
        } else if tab.renderer().current_highlight().is_some() {
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
        let tab = mux.get_tab();
        let modifiers = window_mods_to_termwiz_mods(key.modifiers);

        if let Some(key) = &key.raw_key {
            if let Key::Code(key) = win_key_code_to_termwiz_key_code(&key) {
                if let Some(assignment) = self.keys.lookup(key, modifiers) {
                    self.perform_key_assignment(&tab, &assignment).ok();
                    return true;
                }

                if !mux.config().send_composed_key_when_alt_is_pressed
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

        false
    }

    fn paint(&mut self, frame: &mut glium::Frame) {
        let mux = Mux::get().unwrap();
        let tab = mux.get_tab();

        self.update_text_cursor(&tab);
        self.update_title();

        if let Err(err) = self.paint_screen(&tab, frame) {
            if let Some(&OutOfTextureSpace { size }) = err.downcast_ref::<OutOfTextureSpace>() {
                if let Err(_) = self.recreate_texture_atlas(Some(size)) {
                    self.recreate_texture_atlas(None)
                        .expect("OutOfTextureSpace and failed to recreate atlas");
                }
                tab.renderer().make_all_lines_dirty();
                return self.paint(frame);
            }
        }
    }
}

impl TermWindow {
    pub fn new_window(fontconfig: &Rc<FontConfiguration>) -> anyhow::Result<()> {
        let mux = Mux::get().unwrap();
        let tab = mux.get_tab();
        let (physical_rows, physical_cols) = tab.renderer().physical_dimensions();

        let render_metrics = RenderMetrics::new(fontconfig);

        let terminal_size = PtySize {
            rows: physical_rows as u16,
            cols: physical_cols as u16,
            pixel_width: (render_metrics.cell_size.width as usize * physical_cols) as u16,
            pixel_height: (render_metrics.cell_size.height as usize * physical_rows) as u16,
        };

        let header = Header::new();

        let dimensions = Dimensions {
            pixel_width: (terminal_size.cols * render_metrics.cell_size.width as u16) as usize,
            pixel_height: (header.offset + terminal_size.rows as usize)
                * render_metrics.cell_size.height as usize,
            dpi: 96,
        };

        Window::new_window(
            "miro",
            "miro",
            dimensions.pixel_width,
            dimensions.pixel_height,
            Box::new(Self {
                focused: None,
                window: None,
                fonts: Rc::clone(fontconfig),
                render_metrics,
                dimensions,
                render_state: None,
                clipboard: Arc::new(SystemClipboard::new()),
                keys: KeyMap::new(),
                header,
                frame_count: 0,
                terminal_size,
            }),
        )?;

        Ok(())
    }

    fn update_title(&mut self) {
        let mux = Mux::get().unwrap();
        let tab = mux.get_tab();
        let title = tab.get_title();

        if let Some(window) = self.window.as_ref() {
            window.set_title(&title);
        }
    }

    fn update_text_cursor(&mut self, tab: &Ref<Tab>) {
        let term = tab.renderer();
        let cursor = term.cursor_pos();
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

    fn perform_key_assignment(
        &mut self,
        tab: &Ref<Tab>,
        assignment: &KeyAssignment,
    ) -> anyhow::Result<()> {
        use KeyAssignment::*;
        match assignment {
            ToggleFullScreen => {}
            Copy => {}
            Paste => {
                tab.trickle_paste(self.clipboard.get_contents()?)?;
            }
            DecreaseFontSize => self.decrease_font_size(),
            IncreaseFontSize => self.increase_font_size(),
            ResetFontSize => self.reset_font_size(),
            Hide => {
                if let Some(w) = self.window.as_ref() {
                    w.hide();
                }
            }
        };
        Ok(())
    }

    fn scaling_changed(&mut self, dimensions: Dimensions, font_scale: f64) {
        let scale_changed =
            dimensions.dpi != self.dimensions.dpi || font_scale != self.fonts.get_font_scale();

        let scale_changed_cells = if scale_changed {
            let cell_dims = self.current_cell_dimensions();
            self.apply_scale_change(&dimensions, font_scale);
            Some(cell_dims)
        } else {
            None
        };

        self.apply_dimensions(&dimensions, scale_changed_cells);
    }

    fn current_cell_dimensions(&self) -> RowsAndCols {
        RowsAndCols {
            rows: self.terminal_size.rows as usize,
            cols: self.terminal_size.cols as usize,
        }
    }

    fn apply_scale_change(&mut self, dimensions: &Dimensions, font_scale: f64) {
        self.fonts.change_scaling(font_scale, dimensions.dpi as f64 / 96.);
        self.render_metrics = RenderMetrics::new(&self.fonts);
        let gl_state = self.render_state.as_mut().unwrap();
        gl_state
            .header
            .change_scaling(
                dimensions.dpi as f32 / 96.,
                self.dimensions.pixel_width,
                self.dimensions.pixel_height,
            )
            .expect("failed to rescale header");
        self.recreate_texture_atlas(None).expect("failed to recreate atlas");
    }

    fn recreate_texture_atlas(&mut self, size: Option<usize>) -> anyhow::Result<()> {
        self.render_state.as_mut().unwrap().recreate_texture_atlas(
            &self.fonts,
            &self.render_metrics,
            size,
        )
    }

    fn apply_dimensions(
        &mut self,
        dimensions: &Dimensions,
        scale_changed_cells: Option<RowsAndCols>,
    ) {
        self.dimensions = *dimensions;

        let (size, dims) = if let Some(cell_dims) = scale_changed_cells {
            let size = PtySize {
                rows: cell_dims.rows as u16,
                cols: cell_dims.cols as u16,
                pixel_height: cell_dims.rows as u16 * self.render_metrics.cell_size.height as u16,
                pixel_width: cell_dims.cols as u16 * self.render_metrics.cell_size.width as u16,
            };

            let rows = size.rows + self.header.offset as u16;
            let cols = size.cols;

            let pixel_height = rows * self.render_metrics.cell_size.height as u16;

            let pixel_width = cols * self.render_metrics.cell_size.width as u16;

            let dims = Dimensions {
                pixel_width: pixel_width as usize,
                pixel_height: pixel_height as usize,
                dpi: dimensions.dpi,
            };

            (size, dims)
        } else {
            let rows = (dimensions.pixel_height / self.render_metrics.cell_size.height as usize)
                .saturating_sub(self.header.offset);
            let cols = dimensions.pixel_width / self.render_metrics.cell_size.width as usize;

            let size = PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_height: dimensions.pixel_height as u16,
                pixel_width: dimensions.pixel_width as u16,
            };

            (size, *dimensions)
        };

        let mux = Mux::get().unwrap();
        let tab = mux.get_tab();
        let gl_state = self.render_state.as_mut().unwrap();

        gl_state
            .advise_of_window_size_change(
                &self.render_metrics,
                dimensions.pixel_width,
                dimensions.pixel_height,
            )
            .expect("failed to advise of resize");

        self.terminal_size = size;

        tab.resize(size).ok();
        self.update_title();

        if let Some(_) = scale_changed_cells {
            if let Some(window) = self.window.as_ref() {
                window.set_inner_size(dims.pixel_width, dims.pixel_height);
            }
        }
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

    fn paint_screen(&mut self, tab: &Ref<Tab>, frame: &mut glium::Frame) -> anyhow::Result<()> {
        self.frame_count += 1;
        let palette = tab.palette();
        let gl_state = self.render_state.as_ref().unwrap();
        self.clear(&palette, frame);
        self.paint_term(tab, &gl_state, &palette, frame)?;
        self.header.paint(
            &gl_state,
            &palette,
            &self.dimensions,
            self.frame_count,
            &self.render_metrics,
            self.fonts.as_ref(),
            frame,
        )?;

        Ok(())
    }

    fn paint_term(
        &self,
        tab: &Ref<Tab>,
        gl_state: &RenderState,
        palette: &ColorPalette,
        frame: &mut glium::Frame,
    ) -> anyhow::Result<()> {
        let mut term = tab.renderer();

        let mut vb = gl_state.glyph_vertex_buffer.borrow_mut();
        let mut quads = gl_state.quads.map(&mut vb);

        let cursor = {
            let cursor = term.cursor_pos();
            CursorPosition { x: cursor.x, y: cursor.y + self.header.offset as i64 }
        };

        let empty_line = Line::from("");
        for i in 0..=self.header.offset - 1 {
            self.render_screen_line(i, &empty_line, 0..0, &cursor, &*term, &palette, &mut quads)?;
        }

        let dirty_lines = term.get_dirty_lines();
        for (line_idx, line, selrange) in dirty_lines {
            self.render_screen_line(
                line_idx + self.header.offset,
                &line,
                selrange,
                &cursor,
                &*term,
                &palette,
                &mut quads,
            )?;
        }

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

        drop(quads);

        let draw_params =
            glium::DrawParameters { blend: glium::Blend::alpha_blending(), ..Default::default() };

        frame.draw(
            &*vb,
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
            &*vb,
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

    fn render_screen_line(
        &self,
        line_idx: usize,
        line: &Line,
        selection: Range<usize>,
        cursor: &CursorPosition,
        terminal: &Terminal,
        palette: &ColorPalette,
        quads: &mut MappedQuads,
    ) -> anyhow::Result<()> {
        let gl_state = self.render_state.as_ref().unwrap();
        let (_num_rows, num_cols) = terminal.physical_dimensions();

        let current_highlight = terminal.current_highlight();
        let cursor_border_color = rgbcolor_to_window_color(palette.cursor_border);

        let cell_clusters = line.cluster();
        let mut last_cell_idx = 0;
        for cluster in cell_clusters {
            let attrs = &cluster.attrs;
            let is_highlited_hyperlink = match (&attrs.hyperlink, &current_highlight) {
                (&Some(ref this), &Some(ref highlight)) => Arc::ptr_eq(this, highlight),
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
                let font = self.fonts.resolve_font(style)?;
                font.shape(&cluster.text)?
            };

            for info in &glyph_info {
                let cell_idx = cluster.byte_to_cell_idx[info.cluster as usize];
                let glyph = gl_state.glyph_cache.borrow_mut().cached_glyph(info, style)?;

                let left = (glyph.x_offset + glyph.bearing_x).get() as f32;
                let top = ((PixelLength::new(self.render_metrics.cell_size.height as f64)
                    + self.render_metrics.descender)
                    - (glyph.y_offset + glyph.bearing_y))
                    .get() as f32;

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

                    let (glyph_color, bg_color, cursor_shape) = self.compute_cell_fg_bg(
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

                    let mut quad = quads.cell(cell_idx, line_idx)?;

                    quad.set_fg_color(glyph_color);
                    quad.set_bg_color(bg_color);
                    quad.set_texture(texture_rect);
                    quad.set_texture_adjust(left, top, right, bottom);
                    quad.set_underline(underline_tex_rect);
                    quad.set_has_color(glyph.has_color);
                    quad.set_cursor(
                        gl_state.util_sprites.cursor_sprite(cursor_shape).texture_coords(),
                    );
                    quad.set_cursor_color(cursor_border_color);
                }
            }
        }

        let white_space = gl_state.util_sprites.white_space.texture_coords();

        for cell_idx in last_cell_idx + 1..num_cols {
            let (glyph_color, bg_color, cursor_shape) = self.compute_cell_fg_bg(
                line_idx,
                cell_idx,
                cursor,
                &selection,
                rgbcolor_to_window_color(palette.foreground),
                rgbcolor_to_window_color(palette.background),
                palette,
            );

            let mut quad = quads.cell(cell_idx, line_idx)?;

            quad.set_bg_color(bg_color);
            quad.set_fg_color(glyph_color);
            quad.set_texture(white_space);
            quad.set_texture_adjust(0., 0., 0., 0.);
            quad.set_underline(white_space);
            quad.set_has_color(false);
            quad.set_cursor(gl_state.util_sprites.cursor_sprite(cursor_shape).texture_coords());
            quad.set_cursor_color(cursor_border_color);
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
    ) -> (Color, Color, CursorShape) {
        let selected = selection.contains(&cell_idx);

        let is_cursor = line_idx as i64 == cursor.y && cursor.x == cell_idx;

        let cursor_shape = if is_cursor { CursorShape::SteadyBlock } else { CursorShape::Hidden };

        let (fg_color, bg_color) = match (selected, self.focused.is_some(), cursor_shape) {
            (true, _, CursorShape::Hidden) => (
                rgbcolor_to_window_color(palette.selection_fg),
                rgbcolor_to_window_color(palette.selection_bg),
            ),

            (_, true, CursorShape::BlinkingBlock) | (_, true, CursorShape::SteadyBlock) => (
                rgbcolor_to_window_color(palette.cursor_fg),
                rgbcolor_to_window_color(palette.cursor_bg),
            ),

            _ => (fg_color, bg_color),
        };

        (fg_color, bg_color, cursor_shape)
    }

    fn clear(&self, palette: &ColorPalette, frame: &mut glium::Frame) {
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
