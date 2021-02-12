//! Configuration for the gui portion of the terminal
use regex::Regex;
use serde_json::Value;
use std;
use std::collections::HashMap;

use term;
use term::color::RgbColor;

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
}

fn default_font_size() -> f64 {
    10.0
}

fn default_dpi() -> f64 {
    96.0
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font_size: default_font_size(),
            dpi: default_dpi(),
            font: TextStyle::default(),
            font_rules: Vec::new(),
            colors: None,
            scrollback_lines: None,
        }
    }
}

/// Represents textual styling.
/// TODO: I want to add some rules so that a user can specify the font
/// and colors to use in some situations.  For example, xterm has
/// a bold color option; I'd like to be able to express something
/// like "when text is bold, use this font pattern and set the text
/// color to X".  There are some interesting possibilities here;
/// instead of just setting the color to a specific value we could
/// apply a transform to the color attribute and make it X% brighter.
#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct TextStyle {
    /// A font config pattern to parse to locate the font.
    /// Note that the dpi and current font_size for the terminal
    /// will be set on the parsed result.
    pub fontconfig_pattern: String,

    /// If set, when rendering text that is set to the default
    /// foreground color, use this color instead.  This is most
    /// useful in a `[[font_rules]]` section to implement changing
    /// the text color for eg: bold text.
    pub foreground: Option<RgbColor>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self { fontconfig_pattern: "monospace".into(), foreground: None }
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
