use crate::core::cell::{AttributeChange, Cell, CellAttributes};
use crate::core::color::ColorAttribute;
use serde_derive::*;
use std::borrow::Cow;
use std::cmp::min;
use unicode_segmentation::UnicodeSegmentation;

pub mod change;
pub mod line;

pub use self::change::Change;
pub use self::line::Line;

/// Position holds 0-based positioning information, where
/// Absolute(0) is the start of the line or column,
/// Relative(0) is the current position in the line or
/// column and EndRelative(0) is the end position in the
/// line or column.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Position {
    NoChange,
    /// Negative values move up, positive values down
    Relative(isize),
    /// Relative to the start of the line or top of the screen
    Absolute(usize),
    /// Relative to the end of line or bottom of screen
    EndRelative(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CursorShape {
    Hidden,
    Default,
    BlinkingBlock,
    SteadyBlock,
    BlinkingUnderline,
    SteadyUnderline,
    BlinkingBar,
    SteadyBar,
}

impl Default for CursorShape {
    fn default() -> CursorShape {
        CursorShape::Default
    }
}

/// SequenceNo indicates a logical position within a stream of changes.
/// The sequence is only meaningful within a given `Surface` instance.
pub type SequenceNo = usize;

/// The `Surface` type represents the contents of a terminal screen.
/// It is not directly connected to a terminal device.
/// It consists of a buffer and a log of changes.  You can accumulate
/// updates to the screen by adding instances of the `Change` enum
/// that describe the updates.
///
/// When ready to render the `Surface` to a `Terminal`, you can use
/// the `get_changes` method to return an optimized stream of `Change`s
/// since the last render and then pass it to an instance of `Renderer`.
///
/// `Surface`s can also be composited together; this is useful when
/// building up a UI with layers or widgets: each widget can be its
/// own `Surface` instance and have its content maintained independently
/// from the other widgets on the screen and can then be copied into
/// the target `Surface` buffer for rendering.
///
/// To support more efficient updates in the composite use case, a
/// `draw_from_screen` method is available; the intent is to have one
/// `Surface` be hold the data that was last rendered, and a second `Surface`
/// of the same size that is repeatedly redrawn from the composite
/// of the widgets.  `draw_from_screen` is used to extract the smallest
/// difference between the updated screen and apply those changes to
/// the render target, and then use `get_changes` to render those without
/// repainting the world on each update.
#[derive(Default)]
pub struct Surface {
    width: usize,
    height: usize,
    lines: Vec<Line>,
    attributes: CellAttributes,
    xpos: usize,
    ypos: usize,
    seqno: SequenceNo,
    changes: Vec<Change>,
    cursor_shape: CursorShape,
    cursor_color: ColorAttribute,
    title: String,
}

impl Surface {
    /// Create a new Surface with the specified width and height.
    pub fn new(width: usize, height: usize) -> Self {
        let mut scr = Surface { width, height, ..Default::default() };
        scr.resize(width, height);
        scr
    }

    /// Returns the (width, height) of the surface
    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn cursor_position(&self) -> (usize, usize) {
        (self.xpos, self.ypos)
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Resize the Surface to the specified width and height.
    /// If the width and/or height are smaller than previously, the rows and/or
    /// columns are truncated.  If the width and/or height are larger than
    /// previously then an appropriate number of cells are added to the
    /// buffer and filled with default attributes.
    /// The resize event invalidates the change stream, discarding it and
    /// causing a subsequent `get_changes` call to yield a full repaint.
    /// If the cursor position would be outside the bounds of the newly resized
    /// screen, it will be moved to be within the new bounds.
    pub fn resize(&mut self, width: usize, height: usize) {
        self.lines.resize(height, Line::with_width(width));
        for line in &mut self.lines {
            line.resize(width);
        }
        self.width = width;
        self.height = height;

        // We need to invalidate the change stream prior to this
        // event, so we nominally generate an entry for the resize
        // here.  Since rendering a resize doesn't make sense, we
        // don't record a Change entry.  Instead what we do is
        // increment the sequence number and then flush the whole
        // stream.  The next call to get_changes() will perform a
        // full repaint, and that is what we want.
        // We only do this if we have any changes buffered.
        if !self.changes.is_empty() {
            self.seqno += 1;
            self.changes.clear();
        }

        // Ensure that the cursor position is well-defined
        self.xpos = compute_position_change(self.xpos, &Position::NoChange, self.width);
        self.ypos = compute_position_change(self.ypos, &Position::NoChange, self.height);
    }

    /// Efficiently apply a series of changes
    /// Returns the sequence number at the end of the change.
    pub fn add_changes(&mut self, mut changes: Vec<Change>) -> SequenceNo {
        let seq = self.seqno.saturating_sub(1) + changes.len();

        for change in &changes {
            self.apply_change(&change);
        }

        self.seqno += changes.len();
        self.changes.append(&mut changes);

        seq
    }

    fn apply_change(&mut self, change: &Change) {
        match change {
            Change::AllAttributes(attr) => self.attributes = attr.clone(),
            Change::Text(text) => self.print_text(text),
            Change::Attribute(change) => self.change_attribute(change),
            Change::CursorPosition { x, y } => self.set_cursor_pos(x, y),
            Change::ClearScreen(color) => self.clear_screen(*color),
            Change::ClearToEndOfLine(color) => self.clear_eol(*color),
            Change::ClearToEndOfScreen(color) => self.clear_eos(*color),
            Change::CursorColor(color) => self.cursor_color = *color,
            Change::CursorShape(shape) => self.cursor_shape = *shape,
            Change::Title(text) => self.title = text.to_owned(),
            Change::ScrollRegionUp { first_row, region_size, scroll_count } => {
                self.scroll_region_up(*first_row, *region_size, *scroll_count)
            }
            Change::ScrollRegionDown { first_row, region_size, scroll_count } => {
                self.scroll_region_down(*first_row, *region_size, *scroll_count)
            }
        }
    }

    fn clear_screen(&mut self, color: ColorAttribute) {
        self.attributes = CellAttributes::default().set_background(color).clone();
        let cleared = Cell::new(' ', self.attributes.clone());
        for line in &mut self.lines {
            line.fill_range(0.., &cleared);
        }
        self.xpos = 0;
        self.ypos = 0;
    }

    fn clear_eos(&mut self, color: ColorAttribute) {
        self.attributes = CellAttributes::default().set_background(color).clone();
        let cleared = Cell::new(' ', self.attributes.clone());
        self.lines[self.ypos].fill_range(self.xpos.., &cleared);
        for line in &mut self.lines.iter_mut().skip(self.ypos + 1) {
            line.fill_range(0.., &cleared);
        }
    }

    fn clear_eol(&mut self, color: ColorAttribute) {
        self.attributes = CellAttributes::default().set_background(color).clone();
        let cleared = Cell::new(' ', self.attributes.clone());
        self.lines[self.ypos].fill_range(self.xpos.., &cleared);
    }

    fn scroll_screen_up(&mut self) {
        self.lines.remove(0);
        self.lines.push(Line::with_width(self.width));
    }

    fn scroll_region_up(&mut self, start: usize, size: usize, count: usize) {
        // Replace the first lines with empty lines
        for index in start..start + min(count, size) {
            self.lines[index] = Line::with_width(self.width);
        }
        // Rotate the remaining lines up the surface.
        if 0 < count && count < size {
            self.lines[start..start + size].rotate_left(count);
        }
    }

    fn scroll_region_down(&mut self, start: usize, size: usize, count: usize) {
        // Replace the last lines with empty lines
        for index in start + size - min(count, size)..start + size {
            self.lines[index] = Line::with_width(self.width);
        }
        // Rotate the remaining lines down the surface.
        if 0 < count && count < size {
            self.lines[start..start + size].rotate_right(count);
        }
    }

    fn print_text(&mut self, text: &str) {
        for g in UnicodeSegmentation::graphemes(text, true) {
            if g == "\r\n" {
                self.xpos = 0;
                let new_y = self.ypos + 1;
                if new_y >= self.height {
                    self.scroll_screen_up();
                } else {
                    self.ypos = new_y;
                }
                continue;
            }

            if g == "\r" {
                self.xpos = 0;
                continue;
            }

            if g == "\n" {
                let new_y = self.ypos + 1;
                if new_y >= self.height {
                    self.scroll_screen_up();
                } else {
                    self.ypos = new_y;
                }
                continue;
            }

            if self.xpos >= self.width {
                let new_y = self.ypos + 1;
                if new_y >= self.height {
                    self.scroll_screen_up();
                } else {
                    self.ypos = new_y;
                }
                self.xpos = 0;
            }

            let cell = Cell::new_grapheme(g, self.attributes.clone());
            // the max(1) here is to ensure that we advance to the next cell
            // position for zero-width graphemes.  We want to make sure that
            // they occupy a cell so that we can re-emit them when we output them.
            // If we didn't do this, then we'd effectively filter them out from
            // the model, which seems like a lossy design choice.
            let width = cell.width().max(1);

            self.lines[self.ypos].set_cell(self.xpos, cell);

            // Increment the position now; we'll defer processing
            // wrapping until the next printed character, otherwise
            // we'll eagerly scroll when we reach the right margin.
            self.xpos += width;
        }
    }

    fn change_attribute(&mut self, change: &AttributeChange) {
        use crate::core::cell::AttributeChange::*;
        match change {
            Intensity(value) => {
                self.attributes.set_intensity(*value);
            }
            Underline(value) => {
                self.attributes.set_underline(*value);
            }
            Italic(value) => {
                self.attributes.set_italic(*value);
            }
            Blink(value) => {
                self.attributes.set_blink(*value);
            }
            Reverse(value) => {
                self.attributes.set_reverse(*value);
            }
            StrikeThrough(value) => {
                self.attributes.set_strikethrough(*value);
            }
            Invisible(value) => {
                self.attributes.set_invisible(*value);
            }
            Foreground(value) => self.attributes.foreground = *value,
            Background(value) => self.attributes.background = *value,
            Hyperlink(value) => self.attributes.hyperlink = value.clone(),
        }
    }

    fn set_cursor_pos(&mut self, x: &Position, y: &Position) {
        self.xpos = compute_position_change(self.xpos, x, self.width);
        self.ypos = compute_position_change(self.ypos, y, self.height);
    }

    pub fn screen_lines(&self) -> Vec<Cow<Line>> {
        self.lines.iter().map(|line| Cow::Borrowed(line)).collect()
    }

    pub fn has_changes(&self, seq: SequenceNo) -> bool {
        self.seqno != seq
    }

    pub fn current_seqno(&self) -> SequenceNo {
        self.seqno
    }

    /// After having called `get_changes` and processed the resultant
    /// change stream, the caller can then pass the returned `SequenceNo`
    /// value to this call to prune the list of changes and free up
    /// resources from the change log.
    pub fn flush_changes_older_than(&mut self, seq: SequenceNo) {
        let first = self.seqno.saturating_sub(self.changes.len());
        let idx = seq.saturating_sub(first);
        if idx > self.changes.len() {
            return;
        }
        self.changes = self.changes.split_off(idx);
    }
}

/// Applies a Position update to either the x or y position.
/// The value is clamped to be in the range: 0..limit
fn compute_position_change(current: usize, pos: &Position, limit: usize) -> usize {
    use self::Position::*;
    match pos {
        NoChange => min(current, limit.saturating_sub(1)),
        Relative(delta) => {
            if *delta > 0 {
                min(current.saturating_add(*delta as usize), limit - 1)
            } else {
                current.saturating_sub((*delta).abs() as usize)
            }
        }
        Absolute(abs) => min(*abs, limit - 1),
        EndRelative(delta) => limit.saturating_sub(*delta),
    }
}
