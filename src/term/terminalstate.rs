use super::*;
use crate::core::escape::csi::{
    Cursor, DecPrivateMode, DecPrivateModeCode, Device, Edit, EraseInDisplay, EraseInLine, Mode,
    Sgr, TerminalMode, TerminalModeCode, Window,
};
use crate::core::escape::osc::{ChangeColorPair, ColorOrQuery};
use crate::core::escape::{
    Action, ControlCode, Esc, EscCode, OneBased, OperatingSystemCommand, CSI,
};
use crate::gui::RenderableDimensions;
use crate::term::clipboard::Clipboard;
use crate::term::color::ColorPalette;
use anyhow::{bail, Result};
use std::fmt::Write;
use std::sync::Arc;

struct TabStop {
    tabs: Vec<bool>,
    tab_width: usize,
}

impl TabStop {
    fn new(screen_width: usize, tab_width: usize) -> Self {
        let mut tabs = Vec::with_capacity(screen_width);

        for i in 0..screen_width {
            tabs.push((i % tab_width) == 0);
        }
        Self { tabs, tab_width }
    }

    fn set_tab_stop(&mut self, col: usize) {
        self.tabs[col] = true;
    }

    fn find_next_tab_stop(&self, col: usize) -> Option<usize> {
        for i in col + 1..self.tabs.len() {
            if self.tabs[i] {
                return Some(i);
            }
        }
        None
    }

    /// Respond to the terminal resizing.
    /// If the screen got bigger, we need to expand the tab stops
    /// into the new columns with the appropriate width.
    fn resize(&mut self, screen_width: usize) {
        let current = self.tabs.len();
        if screen_width > current {
            for i in current..screen_width {
                self.tabs.push((i % self.tab_width) == 0);
            }
        }
    }
}

#[derive(Debug, Clone)]
struct SavedCursor {
    position: CursorPosition,
    wrap_next: bool,
    pen: CellAttributes,
    dec_origin_mode: bool,
    // TODO: selective_erase when supported
}

struct ScreenOrAlt {
    /// The primary screen + scrollback
    screen: Screen,
    /// The alternate screen; no scrollback
    alt_screen: Screen,
    /// Tells us which screen is active
    alt_screen_is_active: bool,
    saved_cursor: Option<SavedCursor>,
    alt_saved_cursor: Option<SavedCursor>,
}

impl Deref for ScreenOrAlt {
    type Target = Screen;

    fn deref(&self) -> &Screen {
        if self.alt_screen_is_active {
            &self.alt_screen
        } else {
            &self.screen
        }
    }
}

impl DerefMut for ScreenOrAlt {
    fn deref_mut(&mut self) -> &mut Screen {
        if self.alt_screen_is_active {
            &mut self.alt_screen
        } else {
            &mut self.screen
        }
    }
}

impl ScreenOrAlt {
    pub fn new(physical_rows: usize, physical_cols: usize, scrollback_size: usize) -> Self {
        let screen = Screen::new(physical_rows, physical_cols, scrollback_size);
        let alt_screen = Screen::new(physical_rows, physical_cols, 0);

        Self {
            screen,
            alt_screen,
            alt_screen_is_active: false,
            saved_cursor: None,
            alt_saved_cursor: None,
        }
    }

    pub fn resize(
        &mut self,
        physical_rows: usize,
        physical_cols: usize,
        cursor: CursorPosition,
    ) -> CursorPosition {
        let cursor_main = self.screen.resize(physical_rows, physical_cols, cursor);
        let cursor_alt = self.alt_screen.resize(physical_rows, physical_cols, cursor);
        if self.alt_screen_is_active {
            cursor_alt
        } else {
            cursor_main
        }
    }

    pub fn activate_alt_screen(&mut self) {
        self.alt_screen_is_active = true;
        self.dirty_top_phys_rows();
    }

    pub fn activate_primary_screen(&mut self) {
        self.alt_screen_is_active = false;
        self.dirty_top_phys_rows();
    }

    // When switching between alt and primary screen, we implicitly change
    // the content associated with StableRowIndex 0..num_rows.  The muxer
    // use case needs to know to invalidate its cache, so we mark those rows
    // as dirty.
    fn dirty_top_phys_rows(&mut self) {
        let num_rows = self.screen.physical_rows;
        for line_idx in 0..num_rows {
            self.screen.line_mut(line_idx).set_dirty();
        }
    }

    pub fn is_alt_screen_active(&self) -> bool {
        self.alt_screen_is_active
    }

    pub fn saved_cursor(&mut self) -> &mut Option<SavedCursor> {
        if self.alt_screen_is_active {
            &mut self.alt_saved_cursor
        } else {
            &mut self.saved_cursor
        }
    }
}

/// Manages the state for the terminal
pub struct TerminalState {
    screen: ScreenOrAlt,
    /// The current set of attributes in effect for the next
    /// attempt to print to the display
    pen: CellAttributes,
    /// The current cursor position, relative to the top left
    /// of the screen.  0-based index.
    cursor: CursorPosition,

    /// if true, implicitly move to the next line on the next
    /// printed character
    wrap_next: bool,
    clipboard: Option<Arc<dyn Clipboard>>,

    /// If true, writing a character inserts a new cell
    insert: bool,

    /// https://vt100.net/docs/vt510-rm/DECAWM.html
    dec_auto_wrap: bool,

    /// Reverse Wraparound Mode
    reverse_wraparound_mode: bool,

    /// https://vt100.net/docs/vt510-rm/DECOM.html
    /// When OriginMode is enabled, cursor is constrained to the
    /// scroll region and its position is relative to the scroll
    /// region.
    dec_origin_mode: bool,

    /// The scroll region
    scroll_region: Range<VisibleRowIndex>,

    /// When set, modifies the sequence of bytes sent for keys
    /// designated as cursor keys.  This includes various navigation
    /// keys.  The code in key_down() is responsible for interpreting this.
    application_cursor_keys: bool,

    dec_ansi_mode: bool,

    /// https://vt100.net/docs/vt3xx-gp/chapter14.html has a discussion
    /// on what sixel scrolling mode does
    sixel_scrolling: bool,
    use_private_color_registers_for_each_graphic: bool,

    /// When set, modifies the sequence of bytes sent for keys
    /// in the numeric keypad portion of the keyboard.
    application_keypad: bool,

    /// When set, pasting the clipboard should bracket the data with
    /// designated marker characters.
    bracketed_paste: bool,

    /// Movement events enabled
    any_event_mouse: bool,
    /// SGR style mouse tracking and reporting is enabled
    sgr_mouse: bool,
    mouse_tracking: bool,
    /// Button events enabled
    button_event_mouse: bool,
    current_mouse_button: MouseButton,
    cursor_visible: bool,
    dec_line_drawing_mode: bool,

    tabs: TabStop,

    /// The terminal title string
    title: String,
    palette: ColorPalette,

    pixel_width: usize,
    pixel_height: usize,

    writer: Box<dyn std::io::Write>,
}

fn encode_modifiers(mods: KeyModifiers) -> u8 {
    let mut number = 0;
    if mods.contains(KeyModifiers::SHIFT) {
        number |= 1;
    }
    if mods.contains(KeyModifiers::ALT) {
        number |= 2;
    }
    if mods.contains(KeyModifiers::CTRL) {
        number |= 4;
    }
    number
}

/// characters that when masked for CTRL could be an ascii control character
/// or could be a key that a user legitimately wants to process in their
/// terminal application
fn is_ambiguous_ascii_ctrl(c: char) -> bool {
    match c {
        'i' | 'I' | 'm' | 'M' | '[' | '{' | '@' => true,
        _ => false,
    }
}

impl TerminalState {
    /// Constructs the terminal state.
    /// You generally want the `Terminal` struct rather than this one;
    /// Terminal contains and dereferences to `TerminalState`.
    pub fn new(
        physical_rows: usize,
        physical_cols: usize,
        pixel_width: usize,
        pixel_height: usize,
        scrollback_size: usize,
        writer: Box<dyn std::io::Write>,
    ) -> TerminalState {
        let screen = ScreenOrAlt::new(physical_rows, physical_cols, scrollback_size);

        TerminalState {
            screen,
            pen: CellAttributes::default(),
            cursor: CursorPosition::default(),
            scroll_region: 0..physical_rows as VisibleRowIndex,
            wrap_next: false,
            // We default auto wrap to true even though the default for
            // a dec terminal is false, because it is more useful this way.
            dec_auto_wrap: true,
            reverse_wraparound_mode: false,
            dec_origin_mode: false,
            insert: false,
            application_cursor_keys: false,
            dec_ansi_mode: false,
            sixel_scrolling: true,
            use_private_color_registers_for_each_graphic: false,
            application_keypad: false,
            bracketed_paste: false,
            sgr_mouse: false,
            any_event_mouse: false,
            button_event_mouse: false,
            mouse_tracking: false,
            cursor_visible: true,
            dec_line_drawing_mode: false,
            current_mouse_button: MouseButton::None,
            tabs: TabStop::new(physical_cols, 8),
            title: "miro".to_string(),
            palette: ColorPalette::default(),
            pixel_height,
            pixel_width,
            clipboard: None,
            writer,
        }
    }

    pub fn set_clipboard(&mut self, clipboard: &Arc<dyn Clipboard>) {
        self.clipboard.replace(Arc::clone(clipboard));
    }

    /// Returns the title text associated with the terminal session.
    /// The title can be changed by the application using a number
    /// of escape sequences.
    pub fn get_title(&self) -> &str {
        &self.title
    }

    /// Returns a copy of the palette.
    /// By default we don't keep a copy in the terminal state,
    /// preferring to take the config values from the users
    /// config file and updating to changes live.
    /// However, if they have used dynamic color scheme escape
    /// sequences we'll fork a copy of the palette at that time
    /// so that we can start tracking those changes.
    pub fn palette(&self) -> &ColorPalette {
        &self.palette
    }

    /// Returns a reference to the active screen (either the primary or
    /// the alternate screen).
    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    /// Returns a mutable reference to the active screen (either the primary or
    /// the alternate screen).
    pub fn screen_mut(&mut self) -> &mut Screen {
        &mut self.screen
    }

    fn set_clipboard_contents(&self, text: Option<String>) -> anyhow::Result<()> {
        if let Some(clip) = self.clipboard.as_ref() {
            clip.set_contents(text)?;
        }
        Ok(())
    }

    fn legacy_mouse_coord(position: i64) -> char {
        let pos = if position < 0 || position > 255 - 32 { 0 as u8 } else { position as u8 };

        (pos + 1 + 32) as char
    }

    fn mouse_report_button_number(&self, event: &MouseEvent) -> i8 {
        let button = match event.button {
            MouseButton::None => self.current_mouse_button,
            b => b,
        };
        let mut code = match button {
            MouseButton::None => 3,
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
            MouseButton::WheelUp(_) => 64,
            MouseButton::WheelDown(_) => 65,
        };

        if event.modifiers.contains(KeyModifiers::SHIFT) {
            code += 4;
        }
        if event.modifiers.contains(KeyModifiers::ALT) {
            code += 8;
        }
        if event.modifiers.contains(KeyModifiers::CTRL) {
            code += 16;
        }

        code
    }

    fn mouse_wheel(&mut self, event: MouseEvent) -> Result<()> {
        let button = self.mouse_report_button_number(&event);

        if self.sgr_mouse
            && (self.mouse_tracking || self.button_event_mouse || self.any_event_mouse)
        {
            write!(self.writer, "\x1b[<{};{};{}M", button, event.x + 1, event.y + 1)?;
        } else if self.mouse_tracking || self.button_event_mouse || self.any_event_mouse {
            write!(
                self.writer,
                "\x1b[M{}{}{}",
                (32 + button) as u8 as char,
                Self::legacy_mouse_coord(event.x as i64),
                Self::legacy_mouse_coord(event.y),
            )?;
        } else if self.screen.is_alt_screen_active() {
            // Send cursor keys instead (equivalent to xterm's alternateScroll mode)
            self.key_down(
                match event.button {
                    MouseButton::WheelDown(_) => KeyCode::DownArrow,
                    MouseButton::WheelUp(_) => KeyCode::UpArrow,
                    _ => panic!(""),
                },
                KeyModifiers::default(),
            )?;
        }
        Ok(())
    }

    fn mouse_button_press(&mut self, event: MouseEvent) -> Result<()> {
        self.current_mouse_button = event.button;

        if !(self.mouse_tracking || self.button_event_mouse || self.any_event_mouse) {
            return Ok(());
        }

        let button = self.mouse_report_button_number(&event);
        if self.sgr_mouse {
            write!(self.writer, "\x1b[<{};{};{}M", button, event.x + 1, event.y + 1)?;
        } else {
            write!(
                self.writer,
                "\x1b[M{}{}{}",
                (32 + button) as u8 as char,
                Self::legacy_mouse_coord(event.x as i64),
                Self::legacy_mouse_coord(event.y),
            )?;
        }

        Ok(())
    }

    fn mouse_button_release(&mut self, event: MouseEvent) -> Result<()> {
        if self.current_mouse_button != MouseButton::None
            && (self.mouse_tracking || self.button_event_mouse || self.any_event_mouse)
        {
            if self.sgr_mouse {
                let release_button = self.mouse_report_button_number(&event);
                self.current_mouse_button = MouseButton::None;
                write!(self.writer, "\x1b[<{};{};{}m", release_button, event.x + 1, event.y + 1)?;
            } else {
                let release_button = 3;
                self.current_mouse_button = MouseButton::None;
                write!(
                    self.writer,
                    "\x1b[M{}{}{}",
                    (32 + release_button) as u8 as char,
                    Self::legacy_mouse_coord(event.x as i64),
                    Self::legacy_mouse_coord(event.y),
                )?;
            }
        }

        Ok(())
    }

    fn mouse_move(&mut self, event: MouseEvent) -> Result<()> {
        let reportable = self.any_event_mouse || self.current_mouse_button != MouseButton::None;
        // Note: self.mouse_tracking on its own is for clicks, not drags!
        if reportable && (self.button_event_mouse || self.any_event_mouse) {
            let button = 32 + self.mouse_report_button_number(&event);

            if self.sgr_mouse {
                write!(self.writer, "\x1b[<{};{};{}M", button, event.x + 1, event.y + 1)?;
            } else {
                write!(
                    self.writer,
                    "\x1b[M{}{}{}",
                    (32 + button) as u8 as char,
                    Self::legacy_mouse_coord(event.x as i64),
                    Self::legacy_mouse_coord(event.y),
                )?;
            }
        }
        Ok(())
    }

    /// Informs the terminal of a mouse event.
    /// If mouse reporting has been activated, the mouse event will be encoded
    /// appropriately and written to the associated writer.
    pub fn mouse_event(&mut self, mut event: MouseEvent) -> Result<()> {
        // Clamp the mouse coordinates to the size of the model.
        // This situation can trigger for example when the
        // window is resized and leaves a partial row at the bottom of the
        // terminal.  The mouse can move over that portion and the gui layer
        // can thus send us out-of-bounds row or column numbers.  We want to
        // make sure that we clamp this and handle it nicely at the model layer.
        event.y = event.y.min(self.screen().physical_rows as i64 - 1);
        event.x = event.x.min(self.screen().physical_cols - 1);

        match event {
            MouseEvent { kind: MouseEventKind::Press, button: MouseButton::WheelUp(_), .. }
            | MouseEvent {
                kind: MouseEventKind::Press, button: MouseButton::WheelDown(_), ..
            } => self.mouse_wheel(event),
            MouseEvent { kind: MouseEventKind::Press, .. } => self.mouse_button_press(event),
            MouseEvent { kind: MouseEventKind::Release, .. } => self.mouse_button_release(event),
            MouseEvent { kind: MouseEventKind::Move, .. } => self.mouse_move(event),
        }
    }

    /// Discards the scrollback, leaving only the data that is present
    /// in the viewport.
    pub fn erase_scrollback(&mut self) {
        self.screen_mut().erase_scrollback();
    }

    /// Returns true if the associated application has enabled any of the
    /// supported mouse reporting modes.
    /// This is useful for the hosting GUI application to decide how best
    /// to dispatch mouse events to the terminal.
    pub fn is_mouse_grabbed(&self) -> bool {
        self.mouse_tracking || self.button_event_mouse || self.any_event_mouse
    }

    /// Returns true if the associated application has enabled
    /// bracketed paste mode, which can be helpful to the hosting
    /// GUI application to decide about fragmenting a large paste.
    pub fn bracketed_paste_enabled(&self) -> bool {
        self.bracketed_paste
    }

    /// Send text to the terminal that is the result of pasting.
    /// If bracketed paste mode is enabled, the paste is enclosed
    /// in the bracketing, otherwise it is fed to the writer as-is.
    pub fn send_paste(&mut self, text: &str) -> Result<()> {
        if self.bracketed_paste {
            let buf = format!("\x1b[200~{}\x1b[201~", text);
            self.writer.write_all(buf.as_bytes())?;
        } else {
            self.writer.write_all(text.as_bytes())?;
        }
        Ok(())
    }

    fn csi_u_encode(&self, buf: &mut String, c: char, mods: KeyModifiers) -> Result<()> {
        let c = if mods.contains(KeyModifiers::CTRL) { ((c as u8) & 0x1f) as char } else { c };
        if mods.contains(KeyModifiers::ALT) {
            buf.push(0x1b as char);
        }
        write!(buf, "{}", c)?;
        Ok(())
    }

    /// Processes a key_down event generated by the gui/render layer
    /// that is embedding the Terminal.  This method translates the
    /// keycode into a sequence of bytes to send to the slave end
    /// of the pty via the `Write`-able object provided by the caller.
    #[allow(clippy::cognitive_complexity)]
    pub fn key_down(&mut self, key: KeyCode, mods: KeyModifiers) -> Result<()> {
        use crate::core::input::KeyCode::*;

        let mods = match key {
            Char(c)
                if (c.is_ascii_punctuation() || c.is_ascii_uppercase())
                    && mods.contains(KeyModifiers::SHIFT) =>
            {
                mods & !KeyModifiers::SHIFT
            }
            _ => mods,
        };

        // Normalize Backspace and Delete
        let key = match key {
            Char('\x7f') => Delete,
            Char('\x08') => Backspace,
            c => c,
        };

        let mut buf = String::new();

        // TODO: also respect self.application_keypad

        let to_send = match key {
            Char(c) if is_ambiguous_ascii_ctrl(c) && mods.contains(KeyModifiers::CTRL) => {
                self.csi_u_encode(&mut buf, c, mods)?;
                buf.as_str()
            }
            Char(c) if c.is_ascii_uppercase() && mods.contains(KeyModifiers::CTRL) => {
                self.csi_u_encode(&mut buf, c, mods)?;
                buf.as_str()
            }

            Char(c)
                if (c.is_ascii_alphanumeric() || c.is_ascii_punctuation() || c == ' ')
                    && mods.contains(KeyModifiers::CTRL) =>
            {
                let c = ((c as u8) & 0x1f) as char;
                if mods.contains(KeyModifiers::ALT) {
                    buf.push(0x1b as char);
                }
                buf.push(c);
                buf.as_str()
            }

            // When alt is pressed, send escape first to indicate to the peer that
            // ALT is pressed.  We do this only for ascii alnum characters because
            // eg: on macOS generates altgr style glyphs and keeps the ALT key
            // in the modifier set.  This confuses eg: zsh which then just displays
            // <fffffffff> as the input, so we want to avoid that.
            Char(c)
                if (c.is_ascii_alphanumeric() || c.is_ascii_punctuation())
                    && mods.contains(KeyModifiers::ALT) =>
            {
                buf.push(0x1b as char);
                buf.push(c);
                buf.as_str()
            }

            Enter | Escape | Backspace => {
                let c = match key {
                    Enter => '\r',
                    Escape => '\x1b',
                    // Backspace sends the default VERASE which is confusingly
                    // the DEL ascii codepoint
                    Backspace => '\x7f',
                    _ => unreachable!(),
                };
                if mods.contains(KeyModifiers::SHIFT) || mods.contains(KeyModifiers::CTRL) {
                    self.csi_u_encode(&mut buf, c, mods)?;
                } else {
                    if mods.contains(KeyModifiers::ALT) && key != Escape {
                        buf.push(0x1b as char);
                    }
                    buf.push(c);
                }
                buf.as_str()
            }

            Tab => {
                if mods.contains(KeyModifiers::ALT) {
                    buf.push(0x1b as char);
                }
                let mods = mods & !KeyModifiers::ALT;
                if mods == KeyModifiers::CTRL {
                    buf.push_str("\x1b[9;5u");
                } else if mods == KeyModifiers::CTRL | KeyModifiers::SHIFT {
                    buf.push_str("\x1b[1;5Z");
                } else if mods == KeyModifiers::SHIFT {
                    buf.push_str("\x1b[Z");
                } else {
                    buf.push('\t');
                }
                buf.as_str()
            }

            Char(c) => {
                if mods.is_empty() {
                    buf.push(c);
                } else {
                    self.csi_u_encode(&mut buf, c, mods)?;
                }
                buf.as_str()
            }

            Home
            | End
            | UpArrow
            | DownArrow
            | RightArrow
            | LeftArrow
            | ApplicationUpArrow
            | ApplicationDownArrow
            | ApplicationRightArrow
            | ApplicationLeftArrow => {
                let (force_app, c) = match key {
                    UpArrow => (false, 'A'),
                    DownArrow => (false, 'B'),
                    RightArrow => (false, 'C'),
                    LeftArrow => (false, 'D'),
                    Home => (false, 'H'),
                    End => (false, 'F'),
                    ApplicationUpArrow => (true, 'A'),
                    ApplicationDownArrow => (true, 'B'),
                    ApplicationRightArrow => (true, 'C'),
                    ApplicationLeftArrow => (true, 'D'),
                    _ => unreachable!(),
                };
                buf.as_str()
            }

            PageUp | PageDown | Insert | Delete => {
                let c = match key {
                    Insert => 2,
                    Delete => 3,
                    PageUp => 5,
                    PageDown => 6,
                    _ => unreachable!(),
                };

                if mods.contains(KeyModifiers::SHIFT) || mods.contains(KeyModifiers::CTRL) {
                    write!(buf, "\x1b[{};{}~", c, 1 + encode_modifiers(mods))?;
                } else {
                    if mods.contains(KeyModifiers::ALT) {
                        buf.push(0x1b as char);
                    }
                    write!(buf, "\x1b[{}~", c)?;
                }
                buf.as_str()
            }

            Function(n) => {
                if mods.is_empty() && n < 5 {
                    // F1-F4 are encoded using SS3 if there are no modifiers
                    match n {
                        1 => "\x1bOP",
                        2 => "\x1bOQ",
                        3 => "\x1bOR",
                        4 => "\x1bOS",
                        _ => unreachable!("wat?"),
                    }
                } else {
                    // Higher numbered F-keys plus modified F-keys are encoded
                    // using CSI instead of SS3.
                    let intro = match n {
                        1 => "\x1b[11",
                        2 => "\x1b[12",
                        3 => "\x1b[13",
                        4 => "\x1b[14",
                        5 => "\x1b[15",
                        6 => "\x1b[17",
                        7 => "\x1b[18",
                        8 => "\x1b[19",
                        9 => "\x1b[20",
                        10 => "\x1b[21",
                        11 => "\x1b[23",
                        12 => "\x1b[24",
                        _ => panic!(""),
                    };
                    write!(buf, "{};{}~", intro, 1 + encode_modifiers(mods))?;
                    buf.as_str()
                }
            }

            // TODO: emit numpad sequences
            Numpad0 | Numpad1 | Numpad2 | Numpad3 | Numpad4 | Numpad5 | Numpad6 | Numpad7
            | Numpad8 | Numpad9 | Multiply | Add | Separator | Subtract | Decimal | Divide => "",

            // Modifier keys pressed on their own don't expand to anything
            Control | LeftControl | RightControl | Alt | LeftAlt | RightAlt | Menu | LeftMenu
            | RightMenu | Super | Hyper | Shift | LeftShift | RightShift | Meta | LeftWindows
            | RightWindows | NumLock | ScrollLock => "",

            Cancel | Clear | Pause | CapsLock | Select | Print | PrintScreen | Execute | Help
            | Applications | Sleep | BrowserBack | BrowserForward | BrowserRefresh
            | BrowserStop | BrowserSearch | BrowserFavorites | BrowserHome | VolumeMute
            | VolumeDown | VolumeUp | MediaNextTrack | MediaPrevTrack | MediaStop
            | MediaPlayPause | InternalPasteStart | InternalPasteEnd => "",
        };

        // debug!("sending {:?}, {:?}", to_send, key);
        self.writer.write_all(to_send.as_bytes())?;

        Ok(())
    }

    /// Informs the terminal that the viewport of the window has resized to the
    /// specified dimensions.
    pub fn resize(
        &mut self,
        physical_rows: usize,
        physical_cols: usize,
        pixel_width: usize,
        pixel_height: usize,
    ) {
        let adjusted_cursor = self.screen.resize(physical_rows, physical_cols, self.cursor);
        self.scroll_region = 0..physical_rows as i64;
        self.pixel_height = pixel_height;
        self.pixel_width = pixel_width;
        self.tabs.resize(physical_cols);
        self.set_cursor_pos(
            &Position::Absolute(adjusted_cursor.x as i64),
            &Position::Absolute(adjusted_cursor.y),
        );
    }

    /// Clear the dirty flag for all dirty lines
    pub fn clean_dirty_lines(&mut self) {
        let screen = self.screen_mut();
        for line in &mut screen.lines {
            line.clear_dirty();
        }
    }

    /// When dealing with selection, mark a range of lines as dirty
    pub fn make_all_lines_dirty(&mut self) {
        let screen = self.screen_mut();
        for line in &mut screen.lines {
            line.set_dirty();
        }
    }
    pub fn get_cursor_position(&self) -> CursorPosition {
        let pos = self.cursor_pos();

        CursorPosition { x: pos.x, y: self.screen().visible_row_to_stable_row(pos.y) as i64 }
    }

    pub fn get_dirty_lines(&self, lines: Range<StableRowIndex>) -> RangeSet<StableRowIndex> {
        let screen = self.screen();
        let phys = screen.stable_range(&lines);
        let mut set = RangeSet::new();
        for (idx, line) in
            screen.lines.iter().enumerate().skip(phys.start).take(phys.end - phys.start)
        {
            if line.is_dirty() {
                set.add(screen.phys_to_stable_row_index(idx))
            }
        }
        set
    }

    pub fn get_lines(&mut self, lines: Range<StableRowIndex>) -> (StableRowIndex, Vec<Line>) {
        let screen = self.screen_mut();
        let phys_range = screen.stable_range(&lines);
        (
            screen.phys_to_stable_row_index(phys_range.start),
            screen
                .lines
                .iter_mut()
                .skip(phys_range.start)
                .take(phys_range.end - phys_range.start)
                .map(|line| {
                    let cloned = line.clone();
                    line.clear_dirty();
                    cloned
                })
                .collect(),
        )
    }

    pub fn get_dimensions(&self) -> RenderableDimensions {
        let screen = self.screen();
        RenderableDimensions {
            cols: screen.physical_cols,
            viewport_rows: screen.physical_rows,
            scrollback_rows: screen.lines.len(),
            physical_top: screen.visible_row_to_stable_row(0),
            scrollback_top: screen.phys_to_stable_row_index(0),
        }
    }

    pub fn physical_dimensions(&self) -> (usize, usize) {
        let screen = self.screen();
        (screen.physical_rows, screen.physical_cols)
    }

    /// Returns the 0-based cursor position relative to the top left of
    /// the visible screen
    pub fn cursor_pos(&self) -> CursorPosition {
        CursorPosition { x: self.cursor.x, y: self.cursor.y }
    }

    /// Sets the cursor position. x and y are 0-based and relative to the
    /// top left of the visible screen.
    fn set_cursor_pos(&mut self, x: &Position, y: &Position) {
        let x = match *x {
            Position::Relative(x) => (self.cursor.x as i64 + x).max(0),
            Position::Absolute(x) => x,
        };
        let y = match *y {
            Position::Relative(y) => (self.cursor.y + y).max(0),
            Position::Absolute(y) => y,
        };

        let rows = self.screen().physical_rows;
        let cols = self.screen().physical_cols;
        let old_y = self.cursor.y;

        self.cursor.x = x.min(cols as i64 - 1) as usize;

        if self.dec_origin_mode {
            self.cursor.y = (self.scroll_region.start + y).min(self.scroll_region.end - 1);
        } else {
            self.cursor.y = y.min(rows as i64 - 1);
        }
        self.wrap_next = false;

        let new_y = self.cursor.y;
        let screen = self.screen_mut();
        screen.dirty_line(old_y);
        screen.dirty_line(new_y);
    }

    fn scroll_up(&mut self, num_rows: usize) {
        let scroll_region = self.scroll_region.clone();
        self.screen_mut().scroll_up(&scroll_region, num_rows)
    }

    fn scroll_down(&mut self, num_rows: usize) {
        let scroll_region = self.scroll_region.clone();
        self.screen_mut().scroll_down(&scroll_region, num_rows)
    }

    fn new_line(&mut self, move_to_first_column: bool) {
        let x = if move_to_first_column { 0 } else { self.cursor.x };
        let y = self.cursor.y;
        let y = if y == self.scroll_region.end - 1 {
            self.scroll_up(1);
            y
        } else {
            y + 1
        };
        self.set_cursor_pos(&Position::Absolute(x as i64), &Position::Absolute(y as i64));
    }

    /// Moves the cursor down one line in the same column.
    /// If the cursor is at the bottom margin, the page scrolls up.
    fn c1_index(&mut self) {
        let y = self.cursor.y;
        let y = if y == self.scroll_region.end - 1 {
            self.scroll_up(1);
            y
        } else {
            y + 1
        };
        self.set_cursor_pos(&Position::Relative(0), &Position::Absolute(y as i64));
    }

    /// Moves the cursor to the first position on the next line.
    /// If the cursor is at the bottom margin, the page scrolls up.
    fn c1_nel(&mut self) {
        self.new_line(true);
    }

    /// Sets a horizontal tab stop at the column where the cursor is.
    fn c1_hts(&mut self) {
        self.tabs.set_tab_stop(self.cursor.x);
    }

    /// Moves the cursor to the next tab stop. If there are no more tab stops,
    /// the cursor moves to the right margin. HT does not cause text to auto
    /// wrap.
    fn c0_horizontal_tab(&mut self) {
        let x = match self.tabs.find_next_tab_stop(self.cursor.x) {
            Some(x) => x,
            None => self.screen().physical_cols - 1,
        };
        self.set_cursor_pos(&Position::Absolute(x as i64), &Position::Relative(0));
    }

    /// Move the cursor up 1 line.  If the position is at the top scroll margin,
    /// scroll the region down.
    fn c1_reverse_index(&mut self) {
        let y = self.cursor.y;
        let y = if y == self.scroll_region.start {
            self.scroll_down(1);
            y
        } else {
            y - 1
        };
        self.set_cursor_pos(&Position::Relative(0), &Position::Absolute(y as i64));
    }

    fn set_hyperlink(&mut self, link: Option<Hyperlink>) {
        self.pen.hyperlink = match link {
            Some(hyperlink) => Some(Arc::new(hyperlink)),
            None => None,
        }
    }

    fn perform_device(&mut self, dev: Device) {
        match dev {
            Device::DeviceAttributes(a) => {}
            Device::SoftReset => {
                // TODO: see https://vt100.net/docs/vt510-rm/DECSTR.html
                self.pen = CellAttributes::default();
                self.insert = false;
                self.dec_auto_wrap = false;
                self.application_cursor_keys = false;
                self.application_keypad = false;
                self.scroll_region = 0..self.screen().physical_rows as i64;
                self.screen.activate_alt_screen();
                self.screen.saved_cursor().take();
                self.screen.activate_primary_screen();
                self.screen.saved_cursor().take();

                self.reverse_wraparound_mode = false;
            }
            Device::RequestPrimaryDeviceAttributes => {
                let mut ident = "\x1b[?63".to_string(); // Vt320
                ident.push_str(";4"); // Sixel graphics
                ident.push_str(";6"); // Selective erase
                ident.push('c');

                self.writer.write(ident.as_bytes()).ok();
            }
            Device::RequestSecondaryDeviceAttributes => {
                self.writer.write(b"\x1b[>0;0;0c").ok();
            }
            Device::StatusReport => {
                self.writer.write(b"\x1b[0n").ok();
            }
        }
    }

    fn perform_csi_mode(&mut self, mode: Mode) {
        match mode {
            Mode::SetDecPrivateMode(DecPrivateMode::Code(
                DecPrivateModeCode::StartBlinkingCursor,
            ))
            | Mode::ResetDecPrivateMode(DecPrivateMode::Code(
                DecPrivateModeCode::StartBlinkingCursor,
            )) => {}

            Mode::SetMode(TerminalMode::Code(TerminalModeCode::Insert)) => {
                self.insert = true;
            }
            Mode::ResetMode(TerminalMode::Code(TerminalModeCode::Insert)) => {
                self.insert = false;
            }

            Mode::SetDecPrivateMode(DecPrivateMode::Code(DecPrivateModeCode::BracketedPaste)) => {
                self.bracketed_paste = true;
            }
            Mode::ResetDecPrivateMode(DecPrivateMode::Code(DecPrivateModeCode::BracketedPaste)) => {
                self.bracketed_paste = false;
            }

            Mode::SetDecPrivateMode(DecPrivateMode::Code(
                DecPrivateModeCode::EnableAlternateScreen,
            )) => {
                if !self.screen.is_alt_screen_active() {
                    self.screen.activate_alt_screen();
                    self.pen = CellAttributes::default();
                }
            }
            Mode::ResetDecPrivateMode(DecPrivateMode::Code(
                DecPrivateModeCode::EnableAlternateScreen,
            )) => {
                if self.screen.is_alt_screen_active() {
                    self.screen.activate_primary_screen();
                    self.pen = CellAttributes::default();
                }
            }

            Mode::SetDecPrivateMode(DecPrivateMode::Code(
                DecPrivateModeCode::ApplicationCursorKeys,
            )) => {
                self.application_cursor_keys = true;
            }
            Mode::ResetDecPrivateMode(DecPrivateMode::Code(
                DecPrivateModeCode::ApplicationCursorKeys,
            )) => {
                self.application_cursor_keys = false;
            }

            Mode::SetDecPrivateMode(DecPrivateMode::Code(DecPrivateModeCode::ShowCursor)) => {
                self.cursor_visible = true;
            }
            Mode::ResetDecPrivateMode(DecPrivateMode::Code(DecPrivateModeCode::ShowCursor)) => {
                self.cursor_visible = false;
            }

            Mode::SetDecPrivateMode(DecPrivateMode::Code(DecPrivateModeCode::MouseTracking)) => {
                self.mouse_tracking = true;
            }
            Mode::ResetDecPrivateMode(DecPrivateMode::Code(DecPrivateModeCode::MouseTracking)) => {
                self.mouse_tracking = false;
            }

            Mode::SetDecPrivateMode(DecPrivateMode::Code(
                DecPrivateModeCode::HighlightMouseTracking,
            ))
            | Mode::ResetDecPrivateMode(DecPrivateMode::Code(
                DecPrivateModeCode::HighlightMouseTracking,
            )) => {}

            Mode::SetDecPrivateMode(DecPrivateMode::Code(DecPrivateModeCode::ButtonEventMouse)) => {
                self.button_event_mouse = true;
            }
            Mode::ResetDecPrivateMode(DecPrivateMode::Code(
                DecPrivateModeCode::ButtonEventMouse,
            )) => {
                self.button_event_mouse = false;
            }

            Mode::SetDecPrivateMode(DecPrivateMode::Code(DecPrivateModeCode::AnyEventMouse)) => {
                self.any_event_mouse = true;
            }
            Mode::ResetDecPrivateMode(DecPrivateMode::Code(DecPrivateModeCode::AnyEventMouse)) => {
                self.any_event_mouse = false;
            }

            Mode::SetDecPrivateMode(DecPrivateMode::Code(DecPrivateModeCode::SGRMouse)) => {
                self.sgr_mouse = true;
            }
            Mode::ResetDecPrivateMode(DecPrivateMode::Code(DecPrivateModeCode::SGRMouse)) => {
                self.sgr_mouse = false;
            }

            Mode::SetDecPrivateMode(DecPrivateMode::Code(
                DecPrivateModeCode::ClearAndEnableAlternateScreen,
            )) => {
                if !self.screen.is_alt_screen_active() {
                    self.dec_save_cursor();
                    self.screen.activate_alt_screen();
                    self.set_cursor_pos(&Position::Absolute(0), &Position::Absolute(0));
                    self.pen = CellAttributes::default();
                    self.erase_in_display(EraseInDisplay::EraseDisplay);
                }
            }
            Mode::ResetDecPrivateMode(DecPrivateMode::Code(
                DecPrivateModeCode::ClearAndEnableAlternateScreen,
            )) => {
                if self.screen.is_alt_screen_active() {
                    self.screen.activate_primary_screen();
                    self.dec_restore_cursor();
                }
            }
            Mode::SaveDecPrivateMode(DecPrivateMode::Code(_))
            | Mode::RestoreDecPrivateMode(DecPrivateMode::Code(_)) => {}

            Mode::SetDecPrivateMode(DecPrivateMode::Unspecified(n))
            | Mode::ResetDecPrivateMode(DecPrivateMode::Unspecified(n))
            | Mode::SaveDecPrivateMode(DecPrivateMode::Unspecified(n))
            | Mode::RestoreDecPrivateMode(DecPrivateMode::Unspecified(n)) => {}

            Mode::SetMode(TerminalMode::Unspecified(n))
            | Mode::ResetMode(TerminalMode::Unspecified(n)) => {}

            Mode::SetMode(m) | Mode::ResetMode(m) => {}
        }
    }

    fn checksum_rectangle(&mut self, left: u32, top: u32, right: u32, bottom: u32) -> u16 {
        let screen = self.screen_mut();
        let mut checksum = 0;
        for y in top..=bottom {
            let line_idx = screen.phys_row(VisibleRowIndex::from(y));
            let line = screen.line_mut(line_idx);
            for (col, cell) in line.cells().iter().enumerate().skip(left as usize) {
                if col > right as usize {
                    break;
                }

                let ch = cell.str().chars().nth(0).unwrap() as u32;

                checksum += u16::from(ch as u8);
            }
        }
        checksum
    }

    fn perform_csi_window(&mut self, window: Window) {
        match window {
            Window::ReportTextAreaSizeCells => {
                let screen = self.screen();
                let height = Some(screen.physical_rows as i64);
                let width = Some(screen.physical_cols as i64);

                let response = Window::ResizeWindowCells { width, height };
                write!(self.writer, "{}", CSI::Window(response)).ok();
            }

            Window::ReportTextAreaSizePixels => {
                let response = Window::ResizeWindowPixels {
                    width: Some(self.pixel_width as i64),
                    height: Some(self.pixel_height as i64),
                };
                write!(self.writer, "{}", CSI::Window(response)).ok();
            }

            Window::ChecksumRectangularArea { request_id, top, left, bottom, right, .. } => {
                let checksum = self.checksum_rectangle(
                    left.as_zero_based(),
                    top.as_zero_based(),
                    right.as_zero_based(),
                    bottom.as_zero_based(),
                );
                write!(self.writer, "\x1bP{}!~{:04x}\x1b\\", request_id, checksum).ok();
            }
            Window::ResizeWindowCells { .. } => {
                // We don't allow the application to change the window size; that's
                // up to the user!
            }
            Window::Iconify | Window::DeIconify => {}
            Window::PopIconAndWindowTitle
            | Window::PopWindowTitle
            | Window::PopIconTitle
            | Window::PushIconAndWindowTitle
            | Window::PushIconTitle
            | Window::PushWindowTitle => {}

            _ => {}
        }
    }

    fn erase_in_display(&mut self, erase: EraseInDisplay) {
        let cy = self.cursor.y;
        let pen = self.pen.clone_sgr_only();
        let rows = self.screen().physical_rows as VisibleRowIndex;
        let col_range = 0..usize::max_value();
        let row_range = match erase {
            EraseInDisplay::EraseToEndOfDisplay => {
                self.perform_csi_edit(Edit::EraseInLine(EraseInLine::EraseToEndOfLine));
                cy + 1..rows
            }
            EraseInDisplay::EraseToStartOfDisplay => {
                self.perform_csi_edit(Edit::EraseInLine(EraseInLine::EraseToStartOfLine));
                0..cy
            }
            EraseInDisplay::EraseDisplay => 0..rows,
            EraseInDisplay::EraseScrollback => {
                self.screen_mut().erase_scrollback();
                return;
            }
        };

        {
            let screen = self.screen_mut();
            for y in row_range.clone() {
                screen.clear_line(y, col_range.clone(), &pen);
            }
        }
    }

    fn perform_csi_edit(&mut self, edit: Edit) {
        match edit {
            Edit::DeleteCharacter(n) => {
                let y = self.cursor.y;
                let x = self.cursor.x;
                let limit = (x + n as usize).min(self.screen().physical_cols);
                {
                    let screen = self.screen_mut();
                    for _ in x..limit as usize {
                        screen.erase_cell(x, y);
                    }
                }
            }
            Edit::DeleteLine(n) => {
                if self.scroll_region.contains(&self.cursor.y) {
                    let scroll_region = self.cursor.y..self.scroll_region.end;
                    self.screen_mut().scroll_up(&scroll_region, n as usize);
                }
            }
            Edit::EraseCharacter(n) => {
                let y = self.cursor.y;
                let x = self.cursor.x;
                let limit = (x + n as usize).min(self.screen().physical_cols);
                {
                    let blank = Cell::new(' ', self.pen.clone_sgr_only());
                    let screen = self.screen_mut();
                    for x in x..limit as usize {
                        screen.set_cell(x, y, &blank);
                    }
                }
            }

            Edit::EraseInLine(erase) => {
                let cx = self.cursor.x;
                let cy = self.cursor.y;
                let pen = self.pen.clone_sgr_only();
                let cols = self.screen().physical_cols;
                let range = match erase {
                    EraseInLine::EraseToEndOfLine => cx..cols,
                    EraseInLine::EraseToStartOfLine => 0..cx + 1,
                    EraseInLine::EraseLine => 0..cols,
                };

                self.screen_mut().clear_line(cy, range.clone(), &pen);
            }
            Edit::InsertCharacter(n) => {
                let y = self.cursor.y;
                let x = self.cursor.x;
                // TODO: this limiting behavior may not be correct.  There's also a
                // SEM sequence that impacts the scope of ICH and ECH to consider.
                let limit = (x + n as usize).min(self.screen().physical_cols);
                {
                    let screen = self.screen_mut();
                    for x in x..limit as usize {
                        screen.insert_cell(x, y);
                    }
                }
            }
            Edit::InsertLine(n) => {
                if self.scroll_region.contains(&self.cursor.y) {
                    let scroll_region = self.cursor.y..self.scroll_region.end;
                    self.screen_mut().scroll_down(&scroll_region, n as usize);
                }
            }
            Edit::ScrollDown(n) => self.scroll_down(n as usize),
            Edit::ScrollUp(n) => self.scroll_up(n as usize),
            Edit::EraseInDisplay(erase) => self.erase_in_display(erase),
            Edit::Repeat(n) => {
                let y = self.cursor.y;
                let x = self.cursor.x;
                let to_copy = x.saturating_sub(1);
                let screen = self.screen_mut();
                let line_idx = screen.phys_row(y);
                let line = screen.line_mut(line_idx);
                if let Some(cell) = line.cells().get(to_copy).cloned() {
                    line.fill_range(x..=x + n as usize, &cell);
                    self.set_cursor_pos(&Position::Relative(i64::from(n)), &Position::Relative(0))
                }
            }
        }
    }

    fn perform_csi_cursor(&mut self, cursor: Cursor) {
        match cursor {
            Cursor::SetTopAndBottomMargins { top, bottom } => {
                let rows = self.screen().physical_rows;
                let mut top = i64::from(top.as_zero_based()).min(rows as i64 - 1).max(0);
                let mut bottom = i64::from(bottom.as_zero_based()).min(rows as i64 - 1).max(0);
                if top > bottom {
                    std::mem::swap(&mut top, &mut bottom);
                }
                self.scroll_region = top..bottom + 1;
            }
            Cursor::ForwardTabulation(n) => {
                for _ in 0..n {
                    self.c0_horizontal_tab();
                }
            }
            Cursor::BackwardTabulation(_) => {}
            Cursor::TabulationClear(_) => {}
            Cursor::TabulationControl(_) => {}
            Cursor::LineTabulation(_) => {}

            Cursor::Left(n) => {
                self.set_cursor_pos(&Position::Relative(-(i64::from(n))), &Position::Relative(0))
            }
            Cursor::Right(n) => {
                self.set_cursor_pos(&Position::Relative(i64::from(n)), &Position::Relative(0))
            }
            Cursor::Up(n) => {
                self.set_cursor_pos(&Position::Relative(0), &Position::Relative(-(i64::from(n))))
            }
            Cursor::Down(n) => {
                self.set_cursor_pos(&Position::Relative(0), &Position::Relative(i64::from(n)))
            }
            Cursor::CharacterAndLinePosition { line, col } | Cursor::Position { line, col } => self
                .set_cursor_pos(
                    &Position::Absolute(i64::from(col.as_zero_based())),
                    &Position::Absolute(i64::from(line.as_zero_based())),
                ),
            Cursor::CharacterAbsolute(col) | Cursor::CharacterPositionAbsolute(col) => self
                .set_cursor_pos(
                    &Position::Absolute(i64::from(col.as_zero_based())),
                    &Position::Relative(0),
                ),
            Cursor::CharacterPositionBackward(col) => {
                self.set_cursor_pos(&Position::Relative(-(i64::from(col))), &Position::Relative(0))
            }
            Cursor::CharacterPositionForward(col) => {
                self.set_cursor_pos(&Position::Relative(i64::from(col)), &Position::Relative(0))
            }
            Cursor::LinePositionAbsolute(line) => self.set_cursor_pos(
                &Position::Relative(0),
                &Position::Absolute((i64::from(line)).saturating_sub(1)),
            ),
            Cursor::LinePositionBackward(line) => {
                self.set_cursor_pos(&Position::Relative(0), &Position::Relative(-(i64::from(line))))
            }
            Cursor::LinePositionForward(line) => {
                self.set_cursor_pos(&Position::Relative(0), &Position::Relative(i64::from(line)))
            }
            Cursor::NextLine(n) => {
                for _ in 0..n {
                    self.new_line(true);
                }
            }
            Cursor::PrecedingLine(n) => {
                self.set_cursor_pos(&Position::Absolute(0), &Position::Relative(-(i64::from(n))))
            }
            Cursor::ActivePositionReport { .. } => {
                // This is really a response from the terminal, and
                // we don't need to process it as a terminal command
            }
            Cursor::RequestActivePositionReport => {
                let line = OneBased::from_zero_based(self.cursor.y as u32);
                let col = OneBased::from_zero_based(self.cursor.x as u32);
                let report = CSI::Cursor(Cursor::ActivePositionReport { line, col });
                write!(self.writer, "{}", report).ok();
            }
            Cursor::SaveCursor => self.dec_save_cursor(),
            Cursor::RestoreCursor => self.dec_restore_cursor(),
            Cursor::CursorStyle(style) => {}
        }
    }

    /// https://vt100.net/docs/vt510-rm/DECSC.html
    fn dec_save_cursor(&mut self) {
        let saved = SavedCursor {
            position: self.cursor,
            wrap_next: self.wrap_next,
            pen: self.pen.clone(),
            dec_origin_mode: self.dec_origin_mode,
        };
        *self.screen.saved_cursor() = Some(saved);
    }

    /// https://vt100.net/docs/vt510-rm/DECRC.html
    fn dec_restore_cursor(&mut self) {
        let saved = self.screen.saved_cursor().clone().unwrap_or_else(|| SavedCursor {
            position: CursorPosition::default(),
            wrap_next: false,
            pen: Default::default(),
            dec_origin_mode: false,
        });
        let x = saved.position.x;
        let y = saved.position.y;
        self.set_cursor_pos(&Position::Absolute(x as i64), &Position::Absolute(y));
        self.wrap_next = saved.wrap_next;
        self.pen = saved.pen;
        self.dec_origin_mode = saved.dec_origin_mode;
    }

    fn perform_csi_sgr(&mut self, sgr: Sgr) {
        match sgr {
            Sgr::Reset => {
                let link = self.pen.hyperlink.take();
                self.pen = CellAttributes::default();
                self.pen.hyperlink = link;
            }
            Sgr::Intensity(intensity) => {
                self.pen.set_intensity(intensity);
            }
            Sgr::Underline(underline) => {
                self.pen.set_underline(underline);
            }
            Sgr::Blink(blink) => {
                self.pen.set_blink(blink);
            }
            Sgr::Italic(italic) => {
                self.pen.set_italic(italic);
            }
            Sgr::Inverse(inverse) => {
                self.pen.set_reverse(inverse);
            }
            Sgr::Invisible(invis) => {
                self.pen.set_invisible(invis);
            }
            Sgr::StrikeThrough(strike) => {
                self.pen.set_strikethrough(strike);
            }
            Sgr::Foreground(col) => {
                self.pen.set_foreground(col);
            }
            Sgr::Background(col) => {
                self.pen.set_background(col);
            }
            Sgr::Font(_) => {}
        }
    }
}

/// A helper struct for implementing `vtparse::VTActor` while compartmentalizing
/// the terminal state and the embedding/host terminal interface
pub(crate) struct Performer<'a> {
    pub state: &'a mut TerminalState,
    print: Option<String>,
}

impl<'a> Deref for Performer<'a> {
    type Target = TerminalState;

    fn deref(&self) -> &TerminalState {
        self.state
    }
}

impl<'a> DerefMut for Performer<'a> {
    fn deref_mut(&mut self) -> &mut TerminalState {
        &mut self.state
    }
}

impl<'a> Drop for Performer<'a> {
    fn drop(&mut self) {
        self.flush_print();
    }
}

impl<'a> Performer<'a> {
    pub fn new(state: &'a mut TerminalState) -> Self {
        Self { state, print: None }
    }

    fn flush_print(&mut self) {
        let p = match self.print.take() {
            Some(s) => s,
            None => return,
        };

        let mut x_offset = 0;

        for g in unicode_segmentation::UnicodeSegmentation::graphemes(p.as_str(), true) {
            let g = if self.dec_line_drawing_mode {
                match g {
                    "j" => "┘",
                    "k" => "┐",
                    "l" => "┌",
                    "m" => "└",
                    "n" => "┼",
                    "q" => "─",
                    "t" => "├",
                    "u" => "┤",
                    "v" => "┴",
                    "w" => "┬",
                    "x" => "│",
                    _ => g,
                }
            } else {
                g
            };

            if !self.insert && self.wrap_next {
                self.new_line(true);
            }

            let x = self.cursor.x;
            let y = self.cursor.y;
            let width = self.screen().physical_cols;

            let mut pen = self.pen.clone();
            // the max(1) here is to ensure that we advance to the next cell
            // position for zero-width graphemes.  We want to make sure that
            // they occupy a cell so that we can re-emit them when we output them.
            // If we didn't do this, then we'd effectively filter them out from
            // the model, which seems like a lossy design choice.
            let print_width = unicode_column_width(g).max(1);

            if !self.insert && x + print_width >= width {
                pen.set_wrapped(true);
            }

            let cell = Cell::new_grapheme(g, pen);

            if self.insert {
                let screen = self.screen_mut();
                for _ in x..x + print_width as usize {
                    screen.insert_cell(x + x_offset, y);
                }
            }

            // Assign the cell
            self.screen_mut().set_cell(x + x_offset, y, &cell);

            if self.insert {
                x_offset += print_width;
            } else if x + print_width < width {
                self.cursor.x += print_width;
                self.wrap_next = false;
            } else {
                self.wrap_next = self.dec_auto_wrap;
            }
        }
    }

    pub fn perform(&mut self, action: Action) {
        match action {
            Action::Print(c) => self.print(c),
            Action::Control(code) => self.control(code),
            Action::DeviceControl(ctrl) => {}
            Action::OperatingSystemCommand(osc) => self.osc_dispatch(*osc),
            Action::Esc(esc) => self.esc_dispatch(esc),
            Action::CSI(csi) => self.csi_dispatch(csi),
        }
    }

    /// Draw a character to the screen
    fn print(&mut self, c: char) {
        // We buffer up the chars to increase the chances of correctly grouping graphemes into cells
        self.print.get_or_insert_with(String::new).push(c);
    }

    fn control(&mut self, control: ControlCode) {
        self.flush_print();
        match control {
            ControlCode::LineFeed | ControlCode::VerticalTab | ControlCode::FormFeed => {
                self.new_line(false)
            }
            ControlCode::CarriageReturn => {
                self.set_cursor_pos(&Position::Absolute(0), &Position::Relative(0));
            }
            ControlCode::Backspace => {
                if self.reverse_wraparound_mode
                    && self.dec_auto_wrap
                    && self.cursor.x == 0
                    && self.cursor.y == self.scroll_region.start
                {
                    // Backspace off the top-left wraps around to the bottom right
                    let x_pos = Position::Absolute(self.screen().physical_cols as i64 - 1);
                    let y_pos = Position::Absolute(self.scroll_region.end - 1);
                    self.set_cursor_pos(&x_pos, &y_pos);
                } else if self.reverse_wraparound_mode && self.dec_auto_wrap && self.cursor.x == 0 {
                    // Backspace off the left wraps around to the prior line on the right
                    let x_pos = Position::Absolute(self.screen().physical_cols as i64 - 1);
                    let y_pos = Position::Relative(-1);
                    self.set_cursor_pos(&x_pos, &y_pos);
                } else {
                    self.set_cursor_pos(&Position::Relative(-1), &Position::Relative(0));
                }
            }
            ControlCode::HorizontalTab => self.c0_horizontal_tab(),
            ControlCode::Bell => {}
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, csi: CSI) {
        self.flush_print();
        match csi {
            CSI::Sgr(sgr) => self.state.perform_csi_sgr(sgr),
            CSI::Cursor(cursor) => self.state.perform_csi_cursor(cursor),
            CSI::Edit(edit) => self.state.perform_csi_edit(edit),
            CSI::Mode(mode) => self.state.perform_csi_mode(mode),
            CSI::Device(dev) => self.state.perform_device(*dev),
            CSI::Mouse(mouse) => {}
            CSI::Window(window) => self.state.perform_csi_window(window),
            CSI::Unspecified(unspec) => {}
        };
    }

    fn esc_dispatch(&mut self, esc: Esc) {
        self.flush_print();
        match esc {
            Esc::Code(EscCode::StringTerminator) => {
                // String Terminator (ST); explicitly has nothing to do here, as its purpose is
                // handled implicitly through a state transition in the vtparse state tables.
            }
            Esc::Code(EscCode::DecApplicationKeyPad) => {
                self.application_keypad = true;
            }
            Esc::Code(EscCode::DecNormalKeyPad) => {
                self.application_keypad = false;
            }
            Esc::Code(EscCode::ReverseIndex) => self.c1_reverse_index(),
            Esc::Code(EscCode::Index) => self.c1_index(),
            Esc::Code(EscCode::NextLine) => self.c1_nel(),
            Esc::Code(EscCode::HorizontalTabSet) => self.c1_hts(),
            Esc::Code(EscCode::DecLineDrawing) => {
                self.dec_line_drawing_mode = true;
            }
            Esc::Code(EscCode::AsciiCharacterSet) => {
                self.dec_line_drawing_mode = false;
            }
            Esc::Code(EscCode::DecSaveCursorPosition) => self.dec_save_cursor(),
            Esc::Code(EscCode::DecRestoreCursorPosition) => self.dec_restore_cursor(),

            // RIS resets a device to its initial state, i.e. the state it has after it is switched
            // on. This may imply, if applicable: remove tabulation stops, remove qualified areas,
            // reset graphic rendition, erase all positions, move active position to first
            // character position of first line.
            Esc::Code(EscCode::FullReset) => {
                self.pen = Default::default();
                self.cursor = Default::default();
                self.wrap_next = false;
                self.insert = false;
                self.dec_auto_wrap = true;
                self.reverse_wraparound_mode = false;
                self.dec_origin_mode = false;
                self.use_private_color_registers_for_each_graphic = false;
                self.application_cursor_keys = false;
                self.sixel_scrolling = true;
                self.dec_ansi_mode = false;
                self.application_keypad = false;
                self.bracketed_paste = false;
                self.sgr_mouse = false;
                self.any_event_mouse = false;
                self.button_event_mouse = false;
                self.current_mouse_button = MouseButton::None;
                self.cursor_visible = true;
                self.dec_line_drawing_mode = false;
                self.tabs = TabStop::new(self.screen().physical_cols, 8);
                self.scroll_region = 0..self.screen().physical_rows as VisibleRowIndex;
            }

            _ => {}
        }
    }

    fn osc_dispatch(&mut self, osc: OperatingSystemCommand) {
        self.flush_print();
        match osc {
            OperatingSystemCommand::SetIconNameAndWindowTitle(title)
            | OperatingSystemCommand::SetWindowTitle(title) => {
                self.title = title.clone();
            }
            OperatingSystemCommand::SetIconName(_) => {}
            OperatingSystemCommand::SetHyperlink(link) => {
                self.set_hyperlink(link);
            }
            OperatingSystemCommand::Unspecified(unspec) => {
                let mut output = String::new();
                write!(&mut output, "Unhandled OSC ").ok();
                for item in unspec {
                    write!(&mut output, " {}", String::from_utf8_lossy(&item)).ok();
                }
            }

            OperatingSystemCommand::ClearSelection(_) => {
                self.set_clipboard_contents(None).ok();
            }
            OperatingSystemCommand::QuerySelection(_) => {}
            OperatingSystemCommand::SetSelection(_, selection_data) => {
                match self.set_clipboard_contents(Some(selection_data)) {
                    Ok(_) => (),
                    Err(err) => panic!("failed to set clipboard in response to OSC 52: {:?}", err),
                }
            }
            OperatingSystemCommand::SystemNotification(message) => {}
            OperatingSystemCommand::ChangeColorNumber(specs) => {
                for pair in specs {
                    match pair.color {
                        ColorOrQuery::Query => {
                            let response =
                                OperatingSystemCommand::ChangeColorNumber(vec![ChangeColorPair {
                                    palette_index: pair.palette_index,
                                    color: ColorOrQuery::Color(
                                        self.palette().colors.0[pair.palette_index as usize],
                                    ),
                                }]);
                            write!(self.writer, "{}", response).ok();
                        }
                        ColorOrQuery::Color(c) => {
                            self.palette.colors.0[pair.palette_index as usize] = c;
                        }
                    }
                }
                self.make_all_lines_dirty();
            }

            OperatingSystemCommand::ChangeDynamicColors(first_color, colors) => {
                use crate::core::escape::osc::DynamicColorNumber;
                let mut idx: u8 = first_color as u8;
                for color in colors {
                    let which_color: Option<DynamicColorNumber> = num::FromPrimitive::from_u8(idx);
                    if let Some(which_color) = which_color {
                        macro_rules! set_or_query {
                            ($name:ident) => {
                                match color {
                                    ColorOrQuery::Query => {
                                        let response = OperatingSystemCommand::ChangeDynamicColors(
                                            which_color,
                                            vec![ColorOrQuery::Color(self.palette().$name)],
                                        );
                                        write!(self.writer, "{}", response).ok();
                                    }
                                    ColorOrQuery::Color(c) => self.palette.$name = c,
                                }
                            };
                        }
                        match which_color {
                            DynamicColorNumber::TextForegroundColor => set_or_query!(foreground),
                            DynamicColorNumber::TextBackgroundColor => set_or_query!(background),
                            DynamicColorNumber::TextCursorColor => set_or_query!(cursor_bg),
                            DynamicColorNumber::HighlightForegroundColor => {
                                set_or_query!(selection_fg)
                            }
                            DynamicColorNumber::HighlightBackgroundColor => {
                                set_or_query!(selection_bg)
                            }
                            DynamicColorNumber::MouseForegroundColor
                            | DynamicColorNumber::MouseBackgroundColor
                            | DynamicColorNumber::TektronixForegroundColor
                            | DynamicColorNumber::TektronixBackgroundColor
                            | DynamicColorNumber::TektronixCursorColor => {}
                        }
                    }
                    idx += 1;
                }
                self.make_all_lines_dirty();
            }
        }
    }
}
