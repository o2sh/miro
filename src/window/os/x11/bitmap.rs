/// The X protocol allows referencing a number of drawable
/// objects.  This trait marks those objects here in code.
pub trait Drawable {
    fn as_drawable(&self) -> xcb::xproto::Drawable;
}
