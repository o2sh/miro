use super::header::Header;
use super::quad::*;
use super::renderstate::RenderState;
use super::utilsprites::RenderMetrics;
use crate::core::color::RgbColor;
use crate::core::promise;
use crate::core::surface::CursorShape;
use crate::font::FontConfiguration;
use crate::gui::selection::Selection;
use crate::gui::RenderableDimensions;
use crate::mux::tab::Tab;
use crate::mux::Mux;
use crate::pty::PtySize;
use crate::term;
use crate::term::cell::Hyperlink;
use crate::term::clipboard::{Clipboard, SystemClipboard};
use crate::term::color::ColorPalette;
use crate::term::input::LastMouseClick;
use crate::term::input::MouseButton as TMB;
use crate::term::input::MouseEventKind as TMEK;
use crate::term::keyassignment::{KeyAssignment, KeyMap};
use crate::term::StableRowIndex;
use crate::term::Terminal;
use crate::term::{CursorPosition, Line};
use crate::window;
use crate::window::bitmaps::atlas::OutOfTextureSpace;
use crate::window::bitmaps::atlas::SpriteSlice;
use crate::window::bitmaps::Texture2d;
use crate::window::MouseButtons as WMB;
use crate::window::MouseEventKind as WMEK;
use crate::window::*;
use glium::{uniform, Surface};
use std::any::Any;
use std::cell::{Ref, RefCell, RefMut};
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

#[derive(Clone)]
struct PrevCursorPos {
    pos: CursorPosition,
    when: Instant,
}

#[derive(Default, Clone)]
pub struct TabState {
    viewport: Option<StableRowIndex>,
    selection: Selection,
}

pub struct TermWindow {
    pub window: Option<Window>,
    focused: Option<Instant>,
    fonts: Rc<FontConfiguration>,
    dimensions: Dimensions,
    terminal_size: PtySize,
    render_metrics: RenderMetrics,
    render_state: Option<RenderState>,
    keys: KeyMap,
    show_tab_bar: bool,
    last_mouse_coords: (usize, i64),
    frame_count: u32,
    clipboard: Arc<dyn Clipboard>,
    header: Header,
    tab_state: RefCell<TabState>,
    current_mouse_button: Option<MousePress>,
    last_mouse_click: Option<LastMouseClick>,
    current_highlight: Option<Arc<Hyperlink>>,
    last_blink_paint: Instant,
}

fn mouse_press_to_tmb(press: &MousePress) -> TMB {
    match press {
        MousePress::Left => TMB::Left,
        MousePress::Right => TMB::Right,
        MousePress::Middle => TMB::Middle,
    }
}

#[derive(Debug)]
enum Key {
    Code(crate::core::input::KeyCode),
    Composed(String),
    None,
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

    fn can_close(&self) -> bool {
        let mux = Mux::get().unwrap();
        mux.close();
        mux.can_close()
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn focus_change(&mut self, focused: bool) {
        self.focused = if focused { Some(Instant::now()) } else { None };

        if self.focused.is_none() {
            self.last_mouse_click = None;
            self.current_mouse_button = None;
        }

        // force cursor to be repainted
        self.window.as_ref().unwrap().invalidate();
    }

    fn mouse_event(&mut self, event: &MouseEvent, context: &dyn WindowOps) {
        let mux = Mux::get().unwrap();
        let tab = mux.get_tab();

        let x = (event.x as isize / self.render_metrics.cell_size.width) as usize;
        let y = (event.y as isize / self.render_metrics.cell_size.height) as i64;

        let first_line_offset = if self.show_tab_bar { 1 } else { 0 };
        self.last_mouse_coords = (x, y);

        // y position relative to top of viewport (not including tab bar)
        let term_y = y.saturating_sub(first_line_offset);

        match event.kind {
            WMEK::Release(_) => {
                self.current_mouse_button = None;
            }

            WMEK::Press(ref press) => {
                if let Some(focused) = self.focused.as_ref() {
                    if focused.elapsed() <= Duration::from_millis(200) {
                        return;
                    }
                }

                // Perform click counting
                let button = mouse_press_to_tmb(press);

                let click = match self.last_mouse_click.take() {
                    None => LastMouseClick::new(button),
                    Some(click) => click.add(button),
                };
                self.last_mouse_click = Some(click);
                self.current_mouse_button = Some(press.clone());
            }

            WMEK::VertWheel(amount) => {
                // adjust viewport
                let dims = tab.renderer().get_dimensions();
                let position =
                    self.get_viewport().unwrap_or(dims.physical_top).saturating_sub(amount.into());
                self.set_viewport(Some(position), dims);
                context.invalidate();
                return;
            }

            WMEK::Move => {}
            _ => {}
        }

        self.mouse_event_terminal(tab, x, term_y, event, context);
    }

    fn resize(&mut self, dimensions: Dimensions) {
        if dimensions.pixel_width == 0 || dimensions.pixel_height == 0 {
            // on windows, this can happen when minimizing the window.
            // NOP!
            return;
        }
        self.scaling_changed(dimensions, self.fonts.get_font_scale());
    }

    fn key_event(&mut self, key: &KeyEvent, _context: &dyn WindowOps) -> bool {
        if !key.key_is_down {
            return false;
        }
        let mux = Mux::get().unwrap();
        let tab = mux.get_tab();
        let modifiers = window_mods_to_termwiz_mods(key.modifiers);

        if let Some(key) = &key.raw_key {
            if let Key::Code(key) = self.win_key_code_to_termwiz_key_code(&key) {
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

        let key = self.win_key_code_to_termwiz_key_code(&key.key);
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

        if let Err(err) = self.paint_tab(&tab, frame) {
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

        let window = Window::new_window(
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
                keys: KeyMap::new(),
                show_tab_bar: false,
                header,
                frame_count: 0,
                clipboard: Arc::new(SystemClipboard::new()),
                terminal_size,
                tab_state: RefCell::new(TabState::default()),
                current_mouse_button: None,
                last_mouse_click: None,
                current_highlight: None,
                last_blink_paint: Instant::now(),
                last_mouse_coords: (0, -1),
            }),
        )?;

        Self::start_periodic_maintenance(window.clone());

        Ok(())
    }

    fn start_periodic_maintenance(window: Window) {
        Connection::get().unwrap().schedule_timer(
            std::time::Duration::from_millis(35),
            move || {
                window.apply(move |myself, window| {
                    if let Some(myself) = myself.downcast_mut::<Self>() {
                        myself.periodic_window_maintenance(window).unwrap();
                    }
                });
            },
        );
    }

    fn periodic_window_maintenance(&mut self, _window: &dyn WindowOps) -> anyhow::Result<()> {
        let mux = Mux::get().unwrap();
        let tab = mux.get_tab();

        let mut needs_invalidate = false;

        let render = tab.renderer();

        // If the model is dirty, arrange to re-paint
        let dims = render.get_dimensions();
        let viewport = self.get_viewport().unwrap_or(dims.physical_top);
        let visible_range = viewport..viewport + dims.viewport_rows as StableRowIndex;
        let dirty = render.get_dirty_lines(visible_range);

        if !dirty.is_empty() {
            needs_invalidate = true;
        }

        if needs_invalidate {
            self.window.as_ref().unwrap().invalidate();
        }

        Ok(())
    }

    fn win_key_code_to_termwiz_key_code(&self, key: &window::KeyCode) -> Key {
        use crate::core::input::KeyCode as KC;
        use window::KeyCode as WK;

        let code = match key {
            // TODO: consider eliminating these codes from termwiz::input::KeyCode
            WK::Char('\r') => KC::Enter,
            WK::Char('\t') => KC::Tab,
            WK::Char('\u{08}') => KC::Backspace,
            WK::Char('\u{7f}') => KC::Delete,
            WK::Char('\u{1b}') => KC::Escape,

            WK::Char(c) => KC::Char(*c),
            WK::Composed(ref s) => {
                let mut chars = s.chars();
                if let Some(first_char) = chars.next() {
                    if chars.next().is_none() {
                        // Was just a single char after all
                        return self.win_key_code_to_termwiz_key_code(&WK::Char(first_char));
                    }
                }
                return Key::Composed(s.to_owned());
            }
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
            WK::Cancel => KC::Cancel,
            WK::Clear => KC::Clear,
            WK::Shift => KC::Shift,
            WK::Control => KC::Control,
            WK::Alt => KC::Alt,
            WK::Pause => KC::Pause,
            WK::CapsLock => KC::CapsLock,
            WK::Print => KC::Print,
            WK::PrintScreen => KC::PrintScreen,
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
        };
        Key::Code(code)
    }

    fn recreate_texture_atlas(&mut self, size: Option<usize>) -> anyhow::Result<()> {
        self.render_state.as_mut().unwrap().recreate_texture_atlas(
            &self.fonts,
            &self.render_metrics,
            size,
        )
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

    fn apply_scale_change(&mut self, dimensions: &Dimensions, font_scale: f64) {
        self.fonts.change_scaling(font_scale, dimensions.dpi as f64 / 96.);
        self.render_metrics = RenderMetrics::new(&self.fonts);

        self.recreate_texture_atlas(None).expect("failed to recreate atlas");
    }

    fn apply_dimensions(
        &mut self,
        dimensions: &Dimensions,
        scale_changed_cells: Option<RowsAndCols>,
    ) {
        self.dimensions = *dimensions;

        // Technically speaking, we should compute the rows and cols
        // from the new dimensions and apply those to the tabs, and
        // then for the scaling changed case, try to re-apply the
        // original rows and cols, but if we do that we end up
        // double resizing the tabs, so we speculatively apply the
        // final size, which in that case should result in a NOP
        // change to the tab size.

        let (size, dims) = if let Some(cell_dims) = scale_changed_cells {
            // Scaling preserves existing terminal dimensions, yielding a new
            // overall set of window dimensions
            let size = PtySize {
                rows: cell_dims.rows as u16,
                cols: cell_dims.cols as u16,
                pixel_height: cell_dims.rows as u16 * self.render_metrics.cell_size.height as u16,
                pixel_width: cell_dims.cols as u16 * self.render_metrics.cell_size.width as u16,
            };

            let rows = size.rows + if self.show_tab_bar { 1 } else { 0 };
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
            // Resize of the window dimensions may result in changed terminal dimensions
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
        let gl_state = self.render_state.as_mut().unwrap();
        gl_state
            .advise_of_window_size_change(
                &self.render_metrics,
                dimensions.pixel_width,
                dimensions.pixel_height,
            )
            .expect("failed to advise of resize");

        self.terminal_size = size;

        let mux = Mux::get().unwrap();
        let tab = mux.get_tab();

        tab.resize(size).ok();
        self.update_title();

        // Queue up a speculative resize in order to preserve the number of rows+cols
        if let Some(_) = scale_changed_cells {
            if let Some(window) = self.window.as_ref() {
                window.set_inner_size(dims.pixel_width, dims.pixel_height);
            }
        }
    }

    fn current_cell_dimensions(&self) -> RowsAndCols {
        RowsAndCols {
            rows: self.terminal_size.rows as usize,
            cols: self.terminal_size.cols as usize,
        }
    }

    #[allow(clippy::float_cmp)]
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

    fn decrease_font_size(&mut self) {
        self.scaling_changed(self.dimensions, self.fonts.get_font_scale() * 0.9);
    }
    fn increase_font_size(&mut self) {
        self.scaling_changed(self.dimensions, self.fonts.get_font_scale() * 1.1);
    }
    fn reset_font_size(&mut self) {
        self.scaling_changed(self.dimensions, 1.);
    }

    fn paint_tab(&mut self, tab: &Ref<Tab>, frame: &mut glium::Frame) -> anyhow::Result<()> {
        let palette = tab.palette();

        let background_color = palette.resolve_bg(term::color::ColorAttribute::Default);
        let (r, g, b, a) = background_color.to_tuple_rgba();
        frame.clear_color_srgb(r, g, b, a);

        let first_line_offset = if self.show_tab_bar { 1 } else { 0 };

        let mut term = tab.renderer();
        let cursor = term.get_cursor_position();

        let current_viewport = self.get_viewport();
        let (stable_top, lines);
        let dims = term.get_dimensions();

        {
            let stable_range = match current_viewport {
                Some(top) => top..top + dims.viewport_rows as StableRowIndex,
                None => dims.physical_top..dims.physical_top + dims.viewport_rows as StableRowIndex,
            };

            let (top, vp_lines) = term.get_lines(stable_range);
            stable_top = top;
            lines = vp_lines;
        }

        let gl_state = self.render_state.as_ref().unwrap();
        let mut vb = gl_state.glyph_vertex_buffer.borrow_mut();
        let mut quads = gl_state.quads.map(&mut vb);

        let empty_line = Line::from("");
        if self.show_tab_bar {
            self.render_screen_line(0, &empty_line, 0..0, &cursor, &*term, &palette, &mut quads)?;
        }

        let selrange = self.selection().range.clone();

        for (line_idx, line) in lines.iter().enumerate() {
            let stable_row = stable_top + line_idx as StableRowIndex;
            let selrange = selrange.map(|sel| sel.cols_for_row(stable_row)).unwrap_or(0..0);

            self.render_screen_line(
                line_idx + first_line_offset,
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

    pub fn tab_state(&self) -> RefMut<TabState> {
        self.tab_state.borrow_mut()
    }
    pub fn selection(&self) -> RefMut<Selection> {
        RefMut::map(self.tab_state(), |state| &mut state.selection)
    }

    pub fn get_viewport(&self) -> Option<StableRowIndex> {
        self.tab_state().viewport
    }

    pub fn set_viewport(&mut self, position: Option<StableRowIndex>, dims: RenderableDimensions) {
        let pos = match position {
            Some(pos) => {
                // Drop out of scrolling mode if we're off the bottom
                if pos >= dims.physical_top {
                    None
                } else {
                    Some(pos.max(dims.scrollback_top))
                }
            }
            None => None,
        };

        let mut state = self.tab_state();
        state.viewport = pos;
    }

    fn mouse_event_terminal(
        &mut self,
        tab: Ref<Tab>,
        x: usize,
        y: i64,
        event: &MouseEvent,
        context: &dyn WindowOps,
    ) {
        let dims = tab.renderer().get_dimensions();
        let stable_row = self.get_viewport().unwrap_or(dims.physical_top) + y as StableRowIndex;

        let (top, mut lines) = tab.renderer().get_lines(stable_row..stable_row + 1);
        let new_highlight = if top == stable_row {
            if let Some(line) = lines.get_mut(0) {
                if let Some(cell) = line.cells().get(x) {
                    cell.attrs().hyperlink.as_ref().cloned()
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        match (self.current_highlight.as_ref(), new_highlight) {
            (Some(old_link), Some(new_link)) if Arc::ptr_eq(&old_link, &new_link) => {
                // Unchanged
            }
            (_, rhs) => {
                // We're hovering over a different URL, so invalidate and repaint
                // so that we render the underline correctly
                self.current_highlight = rhs;
                context.invalidate();
            }
        };

        context.set_cursor(Some(if self.current_highlight.is_some() {
            // When hovering over a hyperlink, show an appropriate
            // mouse cursor to give the cue that it is clickable
            MouseCursor::Hand
        } else {
            MouseCursor::Text
        }));

        let mouse_event = crate::term::MouseEvent {
            kind: match event.kind {
                WMEK::Move => TMEK::Move,
                WMEK::VertWheel(_) | WMEK::HorzWheel(_) | WMEK::Press(_) => TMEK::Press,
                WMEK::Release(_) => TMEK::Release,
            },
            button: match event.kind {
                WMEK::Release(ref press) | WMEK::Press(ref press) => mouse_press_to_tmb(press),
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
            y,
            modifiers: window_mods_to_termwiz_mods(event.modifiers),
        };

        tab.mouse_event(
            mouse_event,
            &mut Host { writer: &mut *tab.writer(), context, clipboard: &self.clipboard },
        )
        .ok();

        match event.kind {
            WMEK::Move => {}
            _ => {
                context.invalidate();
            }
        }
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
