use failure::Error;

pub mod ftwrap;
pub mod hbwrap;
use self::ftwrap::Library;
pub use FTEngine as Engine;

pub struct FTEngine {
    lib: Library,
}

impl FontEngine for FTEngine {
    fn new() -> Result<FTEngine, Error> {
        Ok(FTEngine {
            lib: Library::new()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontDescription {
    name: String,
}

/// A user provided font description that can be used
/// to lookup a font
impl FontDescription {
    pub fn new<S>(name: S) -> FontDescription
    where
        S: Into<String>,
    {
        FontDescription { name: name.into() }
    }
}

trait FontEngine {
    fn new() -> Result<Self, Error>
    where
        Self: Sized;
}
