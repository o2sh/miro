use crate::core::color::RgbColor;
pub use crate::core::hyperlink::Hyperlink;
use anyhow::bail;
use base64;
use bitflags::bitflags;
use num;
use num_derive::*;
use std::fmt::{Display, Error as FmtError, Formatter};
use std::str;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorOrQuery {
    Color(RgbColor),
    Query,
}

impl Display for ColorOrQuery {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            ColorOrQuery::Query => write!(f, "?"),
            ColorOrQuery::Color(c) => write!(f, "{}", c.to_rgb_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatingSystemCommand {
    SetIconNameAndWindowTitle(String),
    SetWindowTitle(String),
    SetIconName(String),
    SetHyperlink(Option<Hyperlink>),
    ClearSelection(Selection),
    QuerySelection(Selection),
    SetSelection(Selection, String),
    SystemNotification(String),
    ChangeColorNumber(Vec<ChangeColorPair>),
    ChangeDynamicColors(DynamicColorNumber, Vec<ColorOrQuery>),
    Unspecified(Vec<Vec<u8>>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive)]
#[repr(u8)]
pub enum DynamicColorNumber {
    TextForegroundColor = 10,
    TextBackgroundColor = 11,
    TextCursorColor = 12,
    MouseForegroundColor = 13,
    MouseBackgroundColor = 14,
    TektronixForegroundColor = 15,
    TektronixBackgroundColor = 16,
    HighlightBackgroundColor = 17,
    TektronixCursorColor = 18,
    HighlightForegroundColor = 19,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeColorPair {
    pub palette_index: u8,
    pub color: ColorOrQuery,
}

bitflags! {
pub struct Selection :u16{
    const NONE = 0;
    const CLIPBOARD = 1<<1;
    const PRIMARY=1<<2;
    const SELECT=1<<3;
    const CUT0=1<<4;
    const CUT1=1<<5;
    const CUT2=1<<6;
    const CUT3=1<<7;
    const CUT4=1<<8;
    const CUT5=1<<9;
    const CUT6=1<<10;
    const CUT7=1<<11;
    const CUT8=1<<12;
    const CUT9=1<<13;
}
}

impl Selection {
    fn try_parse(buf: &[u8]) -> anyhow::Result<Selection> {
        if buf == b"" {
            Ok(Selection::SELECT | Selection::CUT0)
        } else {
            let mut s = Selection::NONE;
            for c in buf {
                s |= match c {
                    b'c' => Selection::CLIPBOARD,
                    b'p' => Selection::PRIMARY,
                    b's' => Selection::SELECT,
                    b'0' => Selection::CUT0,
                    b'1' => Selection::CUT1,
                    b'2' => Selection::CUT2,
                    b'3' => Selection::CUT3,
                    b'4' => Selection::CUT4,
                    b'5' => Selection::CUT5,
                    b'6' => Selection::CUT6,
                    b'7' => Selection::CUT7,
                    b'8' => Selection::CUT8,
                    b'9' => Selection::CUT9,
                    _ => bail!("invalid selection {:?}", buf),
                }
            }
            Ok(s)
        }
    }
}

impl Display for Selection {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        macro_rules! item {
            ($variant:ident, $s:expr) => {
                if (*self & Selection::$variant) != Selection::NONE {
                    write!(f, $s)?;
                }
            };
        }

        item!(CLIPBOARD, "c");
        item!(PRIMARY, "p");
        item!(SELECT, "s");
        item!(CUT0, "0");
        item!(CUT1, "1");
        item!(CUT2, "2");
        item!(CUT3, "3");
        item!(CUT4, "4");
        item!(CUT5, "5");
        item!(CUT6, "6");
        item!(CUT7, "7");
        item!(CUT8, "8");
        item!(CUT9, "9");
        Ok(())
    }
}

impl OperatingSystemCommand {
    pub fn parse(osc: &[&[u8]]) -> Self {
        Self::internal_parse(osc).unwrap_or_else(|_| {
            let mut vec = Vec::new();
            for slice in osc {
                vec.push(slice.to_vec());
            }
            OperatingSystemCommand::Unspecified(vec)
        })
    }

    fn parse_selection(osc: &[&[u8]]) -> anyhow::Result<Self> {
        if osc.len() == 2 {
            Selection::try_parse(osc[1]).map(OperatingSystemCommand::ClearSelection)
        } else if osc.len() == 3 && osc[2] == b"?" {
            Selection::try_parse(osc[1]).map(OperatingSystemCommand::QuerySelection)
        } else if osc.len() == 3 {
            let sel = Selection::try_parse(osc[1])?;
            let bytes = base64::decode(osc[2])?;
            let s = String::from_utf8(bytes)?;
            Ok(OperatingSystemCommand::SetSelection(sel, s))
        } else {
            bail!("unhandled OSC 52: {:?}", osc);
        }
    }

    fn parse_change_color_number(osc: &[&[u8]]) -> anyhow::Result<Self> {
        let mut pairs = vec![];
        let mut iter = osc.iter();
        iter.next();

        while let (Some(index), Some(spec)) = (iter.next(), iter.next()) {
            let index: u8 = str::from_utf8(index)?.parse()?;
            let spec = str::from_utf8(spec)?;
            let spec = if spec == "?" {
                ColorOrQuery::Query
            } else {
                ColorOrQuery::Color(
                    RgbColor::from_named_or_rgb_string(spec)
                        .ok_or_else(|| anyhow::anyhow!("invalid color spec"))?,
                )
            };

            pairs.push(ChangeColorPair { palette_index: index, color: spec });
        }

        Ok(OperatingSystemCommand::ChangeColorNumber(pairs))
    }

    fn parse_change_dynamic_color_number(idx: u8, osc: &[&[u8]]) -> anyhow::Result<Self> {
        let which_color: DynamicColorNumber = num::FromPrimitive::from_u8(idx)
            .ok_or_else(|| anyhow::anyhow!("osc code is not a valid DynamicColorNumber!?"))?;
        let mut colors = vec![];
        for spec in osc.iter().skip(1) {
            if spec == b"?" {
                colors.push(ColorOrQuery::Query);
            } else {
                let spec = str::from_utf8(spec)?;
                colors.push(ColorOrQuery::Color(
                    RgbColor::from_named_or_rgb_string(spec)
                        .ok_or_else(|| anyhow::anyhow!("invalid color spec"))?,
                ));
            }
        }

        Ok(OperatingSystemCommand::ChangeDynamicColors(which_color, colors))
    }

    fn internal_parse(osc: &[&[u8]]) -> anyhow::Result<Self> {
        anyhow::ensure!(!osc.is_empty(), "no params");
        let p1str = String::from_utf8_lossy(osc[0]);
        let code: i64 = p1str.parse()?;
        let osc_code: OperatingSystemCommandCode =
            num::FromPrimitive::from_i64(code).ok_or_else(|| anyhow::anyhow!("unknown code"))?;

        macro_rules! single_string {
            ($variant:ident) => {{
                if osc.len() != 2 {
                    bail!("wrong param count");
                }
                let s = String::from_utf8(osc[1].to_vec())?;

                Ok(OperatingSystemCommand::$variant(s))
            }};
        }

        use self::OperatingSystemCommandCode::*;
        match osc_code {
            SetIconNameAndWindowTitle => single_string!(SetIconNameAndWindowTitle),
            SetWindowTitle => single_string!(SetWindowTitle),
            SetIconName => single_string!(SetIconName),
            SetHyperlink => Ok(OperatingSystemCommand::SetHyperlink(Hyperlink::parse(osc)?)),
            ManipulateSelectionData => Self::parse_selection(osc),
            SystemNotification => single_string!(SystemNotification),
            ChangeColorNumber => Self::parse_change_color_number(osc),
            SetTextForegroundColor
            | SetTextBackgroundColor
            | SetTextCursorColor
            | SetMouseForegroundColor
            | SetMouseBackgroundColor
            | SetTektronixForegroundColor
            | SetTektronixBackgroundColor
            | SetHighlightBackgroundColor
            | SetTektronixCursorColor
            | SetHighlightForegroundColor => {
                Self::parse_change_dynamic_color_number(osc_code as u8, osc)
            }

            _ => bail!("not impl"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive)]
pub enum OperatingSystemCommandCode {
    SetIconNameAndWindowTitle = 0,
    SetIconName = 1,
    SetWindowTitle = 2,
    SetXWindowProperty = 3,
    ChangeColorNumber = 4,

    ChangeTitleTabColor = 6,
    SetCurrentWorkingDirectory = 7,

    SetHyperlink = 8,

    SystemNotification = 9,
    SetTextForegroundColor = 10,
    SetTextBackgroundColor = 11,
    SetTextCursorColor = 12,
    SetMouseForegroundColor = 13,
    SetMouseBackgroundColor = 14,
    SetTektronixForegroundColor = 15,
    SetTektronixBackgroundColor = 16,
    SetHighlightBackgroundColor = 17,
    SetTektronixCursorColor = 18,
    SetHighlightForegroundColor = 19,
    SetLogFileName = 46,
    SetFont = 50,
    EmacsShell = 51,
    ManipulateSelectionData = 52,
    RxvtProprietary = 777,
}

impl Display for OperatingSystemCommand {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        write!(f, "\x1b]")?;

        macro_rules! single_string {
            ($variant:ident, $s:expr) => {
                write!(f, "{};{}", OperatingSystemCommandCode::$variant as u8, $s)?
            };
        }

        use self::OperatingSystemCommand::*;
        match self {
            SetIconNameAndWindowTitle(title) => single_string!(SetIconNameAndWindowTitle, title),
            SetWindowTitle(title) => single_string!(SetWindowTitle, title),
            SetIconName(title) => single_string!(SetIconName, title),
            SetHyperlink(Some(link)) => link.fmt(f)?,
            SetHyperlink(None) => write!(f, "8;;")?,
            Unspecified(v) => {
                for (idx, item) in v.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ";")?;
                    }
                    f.write_str(&String::from_utf8_lossy(item))?;
                }
            }
            ClearSelection(s) => write!(f, "52;{}", s)?,
            QuerySelection(s) => write!(f, "52;{};?", s)?,
            SetSelection(s, val) => write!(f, "52;{};{}", s, base64::encode(val))?,
            SystemNotification(s) => write!(f, "9;{}", s)?,
            ChangeColorNumber(specs) => {
                write!(f, "4;")?;
                for pair in specs {
                    write!(f, "{};{}", pair.palette_index, pair.color)?
                }
            }
            ChangeDynamicColors(first_color, colors) => {
                write!(f, "{}", *first_color as u8)?;
                for color in colors {
                    write!(f, ";{}", color)?
                }
            }
        };
        write!(f, "\x07")?;
        Ok(())
    }
}
