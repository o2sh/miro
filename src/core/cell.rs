use super::color::ColorAttribute;
pub use super::escape::osc::Hyperlink;
use serde_derive::*;
use smallvec::SmallVec;
use std;
use std::mem;
use std::sync::Arc;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CellAttributes {
    attributes: u16,
    pub foreground: ColorAttribute,
    pub background: ColorAttribute,
    pub hyperlink: Option<Arc<Hyperlink>>,
}

macro_rules! bitfield {
    ($getter:ident, $setter:ident, $bitnum:expr) => {
        #[inline]
        pub fn $getter(&self) -> bool {
            (self.attributes & (1 << $bitnum)) == (1 << $bitnum)
        }

        #[inline]
        pub fn $setter(&mut self, value: bool) -> &mut Self {
            let attr_value = if value { 1 << $bitnum } else { 0 };
            self.attributes = (self.attributes & !(1 << $bitnum)) | attr_value;
            self
        }
    };

    ($getter:ident, $setter:ident, $bitmask:expr, $bitshift:expr) => {
        #[inline]
        pub fn $getter(&self) -> u16 {
            (self.attributes >> $bitshift) & $bitmask
        }

        #[inline]
        pub fn $setter(&mut self, value: u16) -> &mut Self {
            let clear = !($bitmask << $bitshift);
            let attr_value = (value & $bitmask) << $bitshift;
            self.attributes = (self.attributes & clear) | attr_value;
            self
        }
    };

    ($getter:ident, $setter:ident, $enum:ident, $bitmask:expr, $bitshift:expr) => {
        #[inline]
        pub fn $getter(&self) -> $enum {
            unsafe { mem::transmute(((self.attributes >> $bitshift) & $bitmask) as u16) }
        }

        #[inline]
        pub fn $setter(&mut self, value: $enum) -> &mut Self {
            let value = value as u16;
            let clear = !($bitmask << $bitshift);
            let attr_value = (value & $bitmask) << $bitshift;
            self.attributes = (self.attributes & clear) | attr_value;
            self
        }
    };
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u16)]
pub enum Intensity {
    Normal = 0,
    Bold = 1,
    Half = 2,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u16)]
pub enum Underline {
    None = 0,
    Single = 1,
    Double = 2,
}

impl Into<bool> for Underline {
    fn into(self) -> bool {
        self != Underline::None
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u16)]
pub enum Blink {
    None = 0,
    Slow = 1,
    Rapid = 2,
}

impl Into<bool> for Blink {
    fn into(self) -> bool {
        self != Blink::None
    }
}

impl CellAttributes {
    bitfield!(intensity, set_intensity, Intensity, 0b11, 0);
    bitfield!(underline, set_underline, Underline, 0b11, 2);
    bitfield!(blink, set_blink, Blink, 0b11, 4);
    bitfield!(italic, set_italic, 6);
    bitfield!(reverse, set_reverse, 7);
    bitfield!(strikethrough, set_strikethrough, 8);
    bitfield!(invisible, set_invisible, 9);
    bitfield!(wrapped, set_wrapped, 10);

    pub fn set_foreground<C: Into<ColorAttribute>>(&mut self, foreground: C) -> &mut Self {
        self.foreground = foreground.into();
        self
    }

    pub fn set_background<C: Into<ColorAttribute>>(&mut self, background: C) -> &mut Self {
        self.background = background.into();
        self
    }

    pub fn set_hyperlink(&mut self, link: Option<Arc<Hyperlink>>) -> &mut Self {
        self.hyperlink = link;
        self
    }

    pub fn clone_sgr_only(&self) -> Self {
        Self {
            attributes: self.attributes,
            foreground: self.foreground,
            background: self.background,
            hyperlink: None,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Cell {
    text: SmallVec<[u8; 4]>,
    attrs: CellAttributes,
}

impl Default for Cell {
    fn default() -> Self {
        Cell::new(' ', CellAttributes::default())
    }
}

impl Cell {
    fn nerf_control_char(text: &mut SmallVec<[u8; 4]>) {
        if text.is_empty() {
            text.push(b' ');
            return;
        }

        if text.as_slice() == [b'\r', b'\n'] {
            text.remove(1);
            text[0] = b' ';
            return;
        }

        if text.len() != 1 {
            return;
        }

        if text[0] < 0x20 || text[0] == 0x7f {
            text[0] = b' ';
        }
    }

    pub fn new(text: char, attrs: CellAttributes) -> Self {
        let len = text.len_utf8();
        let mut storage = SmallVec::with_capacity(len);
        unsafe {
            storage.set_len(len);
        }
        text.encode_utf8(&mut storage);
        Self::nerf_control_char(&mut storage);

        Self { text: storage, attrs }
    }

    pub fn new_grapheme(text: &str, attrs: CellAttributes) -> Self {
        let mut storage = SmallVec::from_slice(text.as_bytes());
        Self::nerf_control_char(&mut storage);

        Self { text: storage, attrs }
    }

    pub fn str(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(&self.text) }
    }

    pub fn width(&self) -> usize {
        grapheme_column_width(self.str())
    }

    pub fn attrs(&self) -> &CellAttributes {
        &self.attrs
    }
}

pub fn unicode_column_width(s: &str) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    s.graphemes(true).map(grapheme_column_width).sum()
}

pub fn grapheme_column_width(s: &str) -> usize {
    use xi_unicode::EmojiExt;
    for c in s.chars() {
        if c.is_emoji_modifier_base() || c.is_emoji_modifier() {
            return 2;
        }
    }
    UnicodeWidthStr::width(s)
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum AttributeChange {
    Intensity(Intensity),
    Underline(Underline),
    Italic(bool),
    Blink(Blink),
    Reverse(bool),
    StrikeThrough(bool),
    Invisible(bool),
    Foreground(ColorAttribute),
    Background(ColorAttribute),
    Hyperlink(Option<Arc<Hyperlink>>),
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn nerf_special() {
        for c in " \n\r\t".chars() {
            let cell = Cell::new(c, CellAttributes::default());
            assert_eq!(cell.str(), " ");
        }

        for g in &["", " ", "\n", "\r", "\t", "\r\n"] {
            let cell = Cell::new_grapheme(g, CellAttributes::default());
            assert_eq!(cell.str(), " ");
        }
    }

    #[test]
    fn test_width() {
        let foot = "\u{1f9b6}";
        eprintln!("foot chars");
        for c in foot.chars() {
            eprintln!("char: {:?}", c);
            use xi_unicode::EmojiExt;
            eprintln!("xi emoji: {}", c.is_emoji());
            eprintln!("xi emoji_mod: {}", c.is_emoji_modifier());
            eprintln!("xi emoji_mod_base: {}", c.is_emoji_modifier_base());
        }
        assert_eq!(unicode_column_width(foot), 2, "{} should be 2", foot);

        let women_holding_hands_dark_skin_tone_medium_light_skin_tone =
            "\u{1F469}\u{1F3FF}\u{200D}\u{1F91D}\u{200D}\u{1F469}\u{1F3FC}";

        let cell = Cell::new_grapheme(
            women_holding_hands_dark_skin_tone_medium_light_skin_tone,
            CellAttributes::default(),
        );
        assert_eq!(cell.str(), women_holding_hands_dark_skin_tone_medium_light_skin_tone);
        assert_eq!(
            cell.width(),
            2,
            "width of {} should be 2",
            women_holding_hands_dark_skin_tone_medium_light_skin_tone
        );
    }
}
