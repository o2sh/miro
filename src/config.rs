//! Configuration for the gui portion of the terminal
use failure::Error;
use regex::Regex;
use serde_json::Value;
use std;
use std::collections::HashMap;
use std::env;
use std::ffi::CStr;

use crate::term;
use crate::term::color::RgbColor;

#[derive(Debug, Deserialize, Clone)]
pub struct Theme {
    pub spritesheet_path: String,
    pub header_color: RgbColor,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// The font size, measured in points
    #[serde(default = "default_font_size")]
    pub font_size: f64,

    /// The DPI to assume
    #[serde(default = "default_dpi")]
    pub dpi: f64,

    /// The baseline font to use
    #[serde(default)]
    pub font: TextStyle,

    /// An optional set of style rules to select the font based
    /// on the cell attributes
    #[serde(default)]
    pub font_rules: Vec<StyleRule>,

    /// The color palette
    pub colors: Option<Palette>,

    /// How many lines of scrollback you want to retain
    pub scrollback_lines: Option<usize>,

    pub theme: Theme,
}

fn default_font_size() -> f64 {
    10.0
}

fn default_dpi() -> f64 {
    96.0
}

impl Config {
    pub fn new(theme: Theme) -> Self {
        Self {
            font_size: default_font_size(),
            dpi: default_dpi(),
            font: TextStyle::default(),
            font_rules: Vec::new(),
            colors: None,
            scrollback_lines: None,
            theme,
        }
    }
}

#[cfg(target_os = "macos")]
const FONT_FAMILY: &str = "Menlo";
#[cfg(not(target_os = "macos"))]
const FONT_FAMILY: &str = "monospace";

#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct FontAttributes {
    /// The font family name
    pub family: String,
    /// Whether the font should be a bold variant
    pub bold: Option<bool>,
    /// Whether the font should be an italic variant
    pub italic: Option<bool>,
}

impl Default for FontAttributes {
    fn default() -> Self {
        Self { family: FONT_FAMILY.into(), bold: None, italic: None }
    }
}

fn default_fontconfig_pattern() -> String {
    FONT_FAMILY.into()
}

fn empty_font_attributes() -> Vec<FontAttributes> {
    Vec::new()
}

/// Represents textual styling.
#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct TextStyle {
    /// A font config pattern to parse to locate the font.
    /// Note that the dpi and current font_size for the terminal
    /// will be set on the parsed result.
    #[serde(default = "default_fontconfig_pattern")]
    pub fontconfig_pattern: String,

    #[serde(default = "empty_font_attributes")]
    pub font: Vec<FontAttributes>,

    /// If set, when rendering text that is set to the default
    /// foreground color, use this color instead.  This is most
    /// useful in a `[[font_rules]]` section to implement changing
    /// the text color for eg: bold text.
    pub foreground: Option<RgbColor>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            fontconfig_pattern: FONT_FAMILY.into(),
            foreground: None,
            font: vec![FontAttributes::default()],
        }
    }
}

impl TextStyle {
    /// Make a version of this style with bold enabled.
    /// Semi-lame: we just append fontconfig style settings
    /// to the string blindly.  We could get more involved
    /// but it would mean adding in the fontsystem stuff here
    /// and this is probably good enough.
    fn make_bold(&self) -> Self {
        Self {
            fontconfig_pattern: format!("{}:weight=bold", self.fontconfig_pattern),
            foreground: self.foreground,
            font: self
                .font
                .iter()
                .map(|attr| {
                    let mut attr = attr.clone();
                    attr.bold = Some(true);
                    attr
                })
                .collect(),
        }
    }

    /// Make a version of this style with italic enabled.
    /// Semi-lame: we just append fontconfig style settings
    /// to the string blindly.  We could get more involved
    /// but it would mean adding in the fontsystem stuff here
    /// and this is probably good enough.
    fn make_italic(&self) -> Self {
        Self {
            fontconfig_pattern: format!("{}:style=Italic", self.fontconfig_pattern),
            foreground: self.foreground,
            font: self
                .font
                .iter()
                .map(|attr| {
                    let mut attr = attr.clone();
                    attr.italic = Some(true);
                    attr
                })
                .collect(),
        }
    }

    pub fn font_with_fallback(&self) -> Vec<FontAttributes> {
        #[allow(unused_mut)]
        let mut font = self.font.clone();
        #[cfg(target_os = "macos")]
        font.push(FontAttributes { family: "Apple Color Emoji".into(), bold: None, italic: None });
        #[cfg(target_os = "macos")]
        font.push(FontAttributes { family: "Apple Symbols".into(), bold: None, italic: None });
        #[cfg(target_os = "macos")]
        font.push(FontAttributes { family: "Zapf Dingbats".into(), bold: None, italic: None });
        #[cfg(windows)]
        font.push(FontAttributes { family: "Segoe UI".into(), bold: None, italic: None });
        font
    }
}

/// Defines a rule that can be used to select a TextStyle given
/// an input CellAttributes value.  The logic that applies the
/// matching can be found in src/font/mod.rs.  The concept is that
/// the user can specify something like this:
///
/// ```
/// [[font_rules]]
/// italic = true
/// font = { fontconfig_pattern = "Operator Mono SSm Lig:style=Italic" }
/// ```
///
/// The above is translated as: "if the CellAttributes have the italic bit
/// set, then use the italic style of font rather than the default", and
/// stop processing further font rules.
#[derive(Debug, Deserialize, Clone)]
pub struct StyleRule {
    /// If present, this rule matches when CellAttributes::intensity holds
    /// a value that matches this rule.  Valid values are "Bold", "Normal",
    /// "Half".
    pub intensity: Option<term::Intensity>,
    /// If present, this rule matches when CellAttributes::underline holds
    /// a value that matches this rule.  Valid values are "None", "Single",
    /// "Double".
    pub underline: Option<term::Underline>,
    /// If present, this rule matches when CellAttributes::italic holds
    /// a value that matches this rule.
    pub italic: Option<bool>,
    /// If present, this rule matches when CellAttributes::blink holds
    /// a value that matches this rule.
    pub blink: Option<bool>,
    /// If present, this rule matches when CellAttributes::reverse holds
    /// a value that matches this rule.
    pub reverse: Option<bool>,
    /// If present, this rule matches when CellAttributes::strikethrough holds
    /// a value that matches this rule.
    pub strikethrough: Option<bool>,
    /// If present, this rule matches when CellAttributes::invisible holds
    /// a value that matches this rule.
    pub invisible: Option<bool>,

    /// When this rule matches, `font` specifies the styling to be used.
    pub font: TextStyle,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Palette {
    /// The text color to use when the attributes are reset to default
    pub foreground: Option<RgbColor>,
    /// The background color to use when the attributes are reset to default
    pub background: Option<RgbColor>,
    /// The color of the cursor
    pub cursor: Option<RgbColor>,
    /// A list of 8 colors corresponding to the basic ANSI palette
    pub ansi: Option<[RgbColor; 8]>,
    /// A list of 8 colors corresponding to bright versions of the
    /// ANSI palette
    pub brights: Option<[RgbColor; 8]>,
}

impl From<Palette> for term::color::ColorPalette {
    fn from(cfg: Palette) -> term::color::ColorPalette {
        let mut p = term::color::ColorPalette::default();
        if let Some(foreground) = cfg.foreground {
            p.foreground = foreground;
        }
        if let Some(background) = cfg.background {
            p.background = background;
        }
        if let Some(cursor) = cfg.cursor {
            p.cursor = cursor;
        }
        if let Some(ansi) = cfg.ansi {
            for (idx, col) in ansi.iter().enumerate() {
                p.colors[idx] = *col;
            }
        }
        if let Some(brights) = cfg.brights {
            for (idx, col) in brights.iter().enumerate() {
                p.colors[idx + 8] = *col;
            }
        }
        p
    }
}

#[derive(Clone)]
pub struct SpriteSheetConfig {
    pub image_path: String,
    pub sheets: HashMap<String, SpriteConfig>,
}

#[derive(Clone)]
pub struct SpriteConfig {
    pub frame: Rect,
}

#[derive(Clone)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

impl SpriteSheetConfig {
    pub fn load(path: &str) -> Option<Self> {
        let text = std::fs::read_to_string(path).expect("load sprite sheet failed");
        let deserialized_opt = serde_json::from_str(&text);
        if let Err(_err) = deserialized_opt {
            return None;
        }
        let deserialized: Value = deserialized_opt.unwrap();

        let image_path = get_mainname(deserialized["meta"]["image"].as_str()?);

        let mut sheets = HashMap::new();
        for (key, frame) in deserialized["frames"].as_object()? {
            let sheet = convert_sheet(frame)?;
            sheets.insert(get_mainname(key), sheet);
        }
        Some(Self { image_path, sheets })
    }
}

fn convert_sheet(sheet: &Value) -> Option<SpriteConfig> {
    let frame = convert_rect(&sheet["frame"])?;
    Some(SpriteConfig { frame })
}

fn convert_rect(value: &Value) -> Option<Rect> {
    Some(Rect {
        x: value["x"].as_i64()? as i32,
        y: value["y"].as_i64()? as i32,
        w: value["w"].as_i64()? as u32,
        h: value["h"].as_i64()? as u32,
    })
}

fn get_mainname(filename: &str) -> String {
    let re = Regex::new(r"^(.*)").unwrap();
    re.captures(filename)
        .map_or_else(|| filename.to_string(), |caps| caps.get(1).unwrap().as_str().to_string())
}

/// Determine which shell to run.
/// We take the contents of the $SHELL env var first, then
/// fall back to looking it up from the password database.
pub fn get_shell() -> Result<String, Error> {
    env::var("SHELL").or_else(|_| {
        let ent = unsafe { libc::getpwuid(libc::getuid()) };

        if ent.is_null() {
            Ok("/bin/sh".into())
        } else {
            let shell = unsafe { CStr::from_ptr((*ent).pw_shell) };
            shell
                .to_str()
                .map(str::to_owned)
                .map_err(|e| format_err!("failed to resolve shell: {:?}", e))
        }
    })
}
