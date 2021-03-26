use crate::core::hyperlink;
use crate::term;
use crate::term::color::RgbColor;
use regex::Regex;
use serde_derive::*;
use serde_json::Value;
use std;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
pub struct Theme {
    pub spritesheet_path: String,
    pub color: RgbColor,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            spritesheet_path: String::from("assets/gfx/mario.json"),
            color: RgbColor { red: 99, green: 137, blue: 250 },
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    #[serde(default = "default_dpi")]
    pub dpi: f64,
    #[serde(default)]
    pub font: TextStyle,
    #[serde(default = "default_hyperlink_rules")]
    pub hyperlink_rules: Vec<hyperlink::Rule>,
    pub ratelimit_output_bytes_per_second: Option<u32>,
    #[serde(default)]
    pub font_rules: Vec<StyleRule>,
    pub colors: Option<Palette>,
    pub scrollback_lines: Option<usize>,
    #[serde(default)]
    pub send_composed_key_when_alt_is_pressed: bool,
    pub theme: Theme,
}

fn default_hyperlink_rules() -> Vec<hyperlink::Rule> {
    vec![
        // URL with a protocol
        hyperlink::Rule::new(r"\b\w+://(?:[\w.-]+)\.[a-z]{2,15}\S*\b", "$0").unwrap(),
        // implicit mailto link
        hyperlink::Rule::new(r"\b\w+@[\w-]+(\.[\w-]+)+\b", "mailto:$0").unwrap(),
    ]
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
            ratelimit_output_bytes_per_second: None,
            font_rules: Vec::new(),
            colors: None,
            hyperlink_rules: default_hyperlink_rules(),
            scrollback_lines: None,
            send_composed_key_when_alt_is_pressed: false,
            theme: Theme::default(),
        }
    }
}

#[cfg(target_os = "macos")]
const FONT_FAMILY: &str = "Menlo";

#[cfg(not(target_os = "macos"))]
const FONT_FAMILY: &str = "monospace";

#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct FontAttributes {
    pub family: String,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
}

impl Default for FontAttributes {
    fn default() -> Self {
        Self { family: FONT_FAMILY.into(), bold: None, italic: None }
    }
}

fn empty_font_attributes() -> Vec<FontAttributes> {
    Vec::new()
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct TextStyle {
    #[serde(default = "empty_font_attributes")]
    pub font: Vec<FontAttributes>,
    pub foreground: Option<RgbColor>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self { foreground: None, font: vec![FontAttributes::default()] }
    }
}

impl TextStyle {
    fn make_bold(&self) -> Self {
        Self {
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

    fn make_italic(&self) -> Self {
        Self {
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

    #[cfg_attr(feature = "cargo-clippy", allow(clippy::let_and_return))]
    pub fn font_with_fallback(&self) -> Vec<FontAttributes> {
        #[allow(unused_mut)]
        let mut font = self.font.clone();

        if font.is_empty() {
            font.push(FontAttributes::default());
        }

        #[cfg(target_os = "macos")]
        font.push(FontAttributes { family: "Apple Color Emoji".into(), bold: None, italic: None });
        #[cfg(target_os = "macos")]
        font.push(FontAttributes { family: "Apple Symbols".into(), bold: None, italic: None });
        #[cfg(target_os = "macos")]
        font.push(FontAttributes { family: "Zapf Dingbats".into(), bold: None, italic: None });
        #[cfg(target_os = "macos")]
        font.push(FontAttributes { family: "Apple LiGothic".into(), bold: None, italic: None });
        #[cfg(not(target_os = "macos"))]
        font.push(FontAttributes { family: "Noto Color Emoji".into(), bold: None, italic: None });

        font
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct StyleRule {
    pub intensity: Option<term::Intensity>,
    pub underline: Option<term::Underline>,
    pub italic: Option<bool>,
    pub blink: Option<term::Blink>,
    pub reverse: Option<bool>,
    pub strikethrough: Option<bool>,
    pub invisible: Option<bool>,
    pub font: TextStyle,
}

impl Config {
    pub fn default_config(theme: Option<Theme>) -> Self {
        Self::default().compute_extra_defaults(theme)
    }

    fn compute_extra_defaults(&self, theme: Option<Theme>) -> Self {
        let mut cfg = self.clone();
        if theme.is_some() {
            cfg.theme = theme.unwrap();
        }
        if cfg.font_rules.is_empty() {
            // Expand out some reasonable default font rules
            let bold = self.font.make_bold();
            let italic = self.font.make_italic();
            let bold_italic = bold.make_italic();

            cfg.font_rules.push(StyleRule {
                italic: Some(true),
                font: italic,
                ..Default::default()
            });

            cfg.font_rules.push(StyleRule {
                intensity: Some(term::Intensity::Bold),
                font: bold,
                ..Default::default()
            });

            cfg.font_rules.push(StyleRule {
                italic: Some(true),
                intensity: Some(term::Intensity::Bold),
                font: bold_italic,
                ..Default::default()
            });
        }

        cfg
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Palette {
    pub foreground: Option<RgbColor>,
    pub background: Option<RgbColor>,
    pub cursor_fg: Option<RgbColor>,
    pub cursor_bg: Option<RgbColor>,
    pub selection_fg: Option<RgbColor>,
    pub selection_bg: Option<RgbColor>,
    pub ansi: Option<[RgbColor; 8]>,
    pub brights: Option<[RgbColor; 8]>,
}

impl From<Palette> for term::color::ColorPalette {
    fn from(cfg: Palette) -> term::color::ColorPalette {
        let mut p = term::color::ColorPalette::default();
        macro_rules! apply_color {
            ($name:ident) => {
                if let Some($name) = cfg.$name {
                    p.$name = $name;
                }
            };
        }
        apply_color!(foreground);
        apply_color!(background);
        apply_color!(cursor_fg);
        apply_color!(cursor_bg);
        apply_color!(selection_fg);
        apply_color!(selection_bg);

        if let Some(ansi) = cfg.ansi {
            for (idx, col) in ansi.iter().enumerate() {
                p.colors.0[idx] = *col;
            }
        }
        if let Some(brights) = cfg.brights {
            for (idx, col) in brights.iter().enumerate() {
                p.colors.0[idx + 8] = *col;
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
