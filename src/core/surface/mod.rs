use serde_derive::*;
use std::borrow::Cow;
use std::cmp::min;

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
    xpos: usize,
    ypos: usize,
    seqno: SequenceNo,
    changes: Vec<Change>,
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
