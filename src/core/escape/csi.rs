use super::OneBased;
use crate::core::cell::{Blink, Intensity, Underline};
use crate::core::color::{AnsiColor, ColorSpec, RgbColor};
use crate::core::input::{Modifiers, MouseButtons};
use num::{self, ToPrimitive};
use num_derive::*;
use std::fmt::{Display, Error as FmtError, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSI {
    Sgr(Sgr),

    Cursor(Cursor),

    Edit(Edit),

    Mode(Mode),

    Device(Box<Device>),

    Mouse(MouseReport),

    Window(Window),

    Unspecified(Box<Unspecified>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unspecified {
    params: Vec<i64>,

    intermediates: Vec<u8>,

    ignored_extra_intermediates: bool,

    control: char,
}

impl Display for Unspecified {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        for (idx, p) in self.params.iter().enumerate() {
            if idx > 0 {
                write!(f, ";{}", p)?;
            } else {
                write!(f, "{}", p)?;
            }
        }
        for i in &self.intermediates {
            write!(f, "{}", *i as char)?;
        }
        write!(f, "{}", self.control)
    }
}

impl Display for CSI {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        write!(f, "\x1b[")?;
        match self {
            CSI::Sgr(sgr) => sgr.fmt(f)?,
            CSI::Cursor(c) => c.fmt(f)?,
            CSI::Edit(e) => e.fmt(f)?,
            CSI::Mode(mode) => mode.fmt(f)?,
            CSI::Unspecified(unspec) => unspec.fmt(f)?,
            CSI::Mouse(mouse) => mouse.fmt(f)?,
            CSI::Device(dev) => dev.fmt(f)?,
            CSI::Window(window) => window.fmt(f)?,
        };
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive, ToPrimitive)]
pub enum CursorStyle {
    Default = 0,
    BlinkingBlock = 1,
    SteadyBlock = 2,
    BlinkingUnderline = 3,
    SteadyUnderline = 4,
    BlinkingBar = 5,
    SteadyBar = 6,
}

impl Default for CursorStyle {
    fn default() -> CursorStyle {
        CursorStyle::Default
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, ToPrimitive)]
pub enum DeviceAttributeCodes {
    Columns132 = 1,
    Printer = 2,
    RegisGraphics = 3,
    SixelGraphics = 4,
    SelectiveErase = 6,
    UserDefinedKeys = 8,
    NationalReplacementCharsets = 9,
    TechnicalCharacters = 15,
    UserWindows = 18,
    HorizontalScrolling = 21,
    AnsiColor = 22,
    AnsiTextLocator = 29,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceAttribute {
    Code(DeviceAttributeCodes),
    Unspecified(u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceAttributeFlags {
    pub attributes: Vec<DeviceAttribute>,
}

impl DeviceAttributeFlags {
    fn emit(&self, f: &mut Formatter, leader: &str) -> Result<(), FmtError> {
        write!(f, "{}", leader)?;
        for item in &self.attributes {
            match item {
                DeviceAttribute::Code(c) => write!(f, ";{}", c.to_u16().ok_or_else(|| FmtError)?)?,
                DeviceAttribute::Unspecified(c) => write!(f, ";{}", *c)?,
            }
        }
        write!(f, "c")?;
        Ok(())
    }

    fn from_params(params: &[i64]) -> Self {
        let mut attributes = Vec::new();
        for p in params {
            match num::FromPrimitive::from_i64(*p) {
                Some(c) => attributes.push(DeviceAttribute::Code(c)),
                None => attributes.push(DeviceAttribute::Unspecified(*p as u16)),
            }
        }
        Self { attributes }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceAttributes {
    Vt100WithAdvancedVideoOption,
    Vt101WithNoOptions,
    Vt102,
    Vt220(DeviceAttributeFlags),
    Vt320(DeviceAttributeFlags),
    Vt420(DeviceAttributeFlags),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Device {
    DeviceAttributes(DeviceAttributes),

    SoftReset,
    RequestPrimaryDeviceAttributes,
    RequestSecondaryDeviceAttributes,
    StatusReport,
}

impl Display for Device {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            Device::DeviceAttributes(DeviceAttributes::Vt100WithAdvancedVideoOption) => {
                write!(f, "?1;2c")?
            }
            Device::DeviceAttributes(DeviceAttributes::Vt101WithNoOptions) => write!(f, "?1;0c")?,
            Device::DeviceAttributes(DeviceAttributes::Vt102) => write!(f, "?6c")?,
            Device::DeviceAttributes(DeviceAttributes::Vt220(attr)) => attr.emit(f, "?62")?,
            Device::DeviceAttributes(DeviceAttributes::Vt320(attr)) => attr.emit(f, "?63")?,
            Device::DeviceAttributes(DeviceAttributes::Vt420(attr)) => attr.emit(f, "?64")?,
            Device::SoftReset => write!(f, "!p")?,
            Device::RequestPrimaryDeviceAttributes => write!(f, "c")?,
            Device::RequestSecondaryDeviceAttributes => write!(f, ">c")?,
            Device::StatusReport => write!(f, "5n")?,
        };
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MouseButton {
    Button1Press,
    Button2Press,
    Button3Press,
    Button4Press,
    Button5Press,
    Button1Release,
    Button2Release,
    Button3Release,
    Button4Release,
    Button5Release,
    Button1Drag,
    Button2Drag,
    Button3Drag,
    None,
}

impl From<MouseButton> for MouseButtons {
    fn from(button: MouseButton) -> MouseButtons {
        match button {
            MouseButton::Button1Press | MouseButton::Button1Drag => MouseButtons::LEFT,
            MouseButton::Button2Press | MouseButton::Button2Drag => MouseButtons::MIDDLE,
            MouseButton::Button3Press | MouseButton::Button3Drag => MouseButtons::RIGHT,
            MouseButton::Button4Press => MouseButtons::VERT_WHEEL | MouseButtons::WHEEL_POSITIVE,
            MouseButton::Button5Press => MouseButtons::VERT_WHEEL,
            _ => MouseButtons::NONE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Window {
    DeIconify,
    Iconify,
    MoveWindow {
        x: i64,
        y: i64,
    },
    ResizeWindowPixels {
        width: Option<i64>,
        height: Option<i64>,
    },
    RaiseWindow,
    LowerWindow,
    RefreshWindow,
    ResizeWindowCells {
        width: Option<i64>,
        height: Option<i64>,
    },
    RestoreMaximizedWindow,
    MaximizeWindow,
    MaximizeWindowVertically,
    MaximizeWindowHorizontally,
    UndoFullScreenMode,
    ChangeToFullScreenMode,
    ToggleFullScreen,
    ReportWindowState,
    ReportWindowPosition,
    ReportTextAreaPosition,
    ReportTextAreaSizePixels,
    ReportWindowSizePixels,
    ReportScreenSizePixels,
    ReportCellSizePixels,
    ReportTextAreaSizeCells,
    ReportScreenSizeCells,
    ReportIconLabel,
    ReportWindowTitle,
    PushIconAndWindowTitle,
    PushIconTitle,
    PushWindowTitle,
    PopIconAndWindowTitle,
    PopIconTitle,
    PopWindowTitle,

    ChecksumRectangularArea {
        request_id: i64,
        page_number: i64,
        top: OneBased,
        left: OneBased,
        bottom: OneBased,
        right: OneBased,
    },
}

fn numstr_or_empty(x: &Option<i64>) -> String {
    match x {
        Some(x) => format!("{}", x),
        None => "".to_owned(),
    }
}

impl Display for Window {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            Window::DeIconify => write!(f, "1t"),
            Window::Iconify => write!(f, "2t"),
            Window::MoveWindow { x, y } => write!(f, "3;{};{}t", x, y),
            Window::ResizeWindowPixels { width, height } => {
                write!(f, "4;{};{}t", numstr_or_empty(width), numstr_or_empty(height))
            }
            Window::RaiseWindow => write!(f, "5t"),
            Window::LowerWindow => write!(f, "6t"),
            Window::RefreshWindow => write!(f, "7t"),
            Window::ResizeWindowCells { width, height } => {
                write!(f, "8;{};{}t", numstr_or_empty(width), numstr_or_empty(height))
            }
            Window::RestoreMaximizedWindow => write!(f, "9;0t"),
            Window::MaximizeWindow => write!(f, "9;1t"),
            Window::MaximizeWindowVertically => write!(f, "9;2t"),
            Window::MaximizeWindowHorizontally => write!(f, "9;3t"),
            Window::UndoFullScreenMode => write!(f, "10;0t"),
            Window::ChangeToFullScreenMode => write!(f, "10;1t"),
            Window::ToggleFullScreen => write!(f, "10;2t"),
            Window::ReportWindowState => write!(f, "11t"),
            Window::ReportWindowPosition => write!(f, "13t"),
            Window::ReportTextAreaPosition => write!(f, "13;2t"),
            Window::ReportTextAreaSizePixels => write!(f, "14t"),
            Window::ReportWindowSizePixels => write!(f, "14;2t"),
            Window::ReportScreenSizePixels => write!(f, "15t"),
            Window::ReportCellSizePixels => write!(f, "16t"),
            Window::ReportTextAreaSizeCells => write!(f, "18t"),
            Window::ReportScreenSizeCells => write!(f, "19t"),
            Window::ReportIconLabel => write!(f, "20t"),
            Window::ReportWindowTitle => write!(f, "21t"),
            Window::PushIconAndWindowTitle => write!(f, "22;0t"),
            Window::PushIconTitle => write!(f, "22;1t"),
            Window::PushWindowTitle => write!(f, "22;2t"),
            Window::PopIconAndWindowTitle => write!(f, "23;0t"),
            Window::PopIconTitle => write!(f, "23;1t"),
            Window::PopWindowTitle => write!(f, "23;2t"),
            Window::ChecksumRectangularArea {
                request_id,
                page_number,
                top,
                left,
                bottom,
                right,
            } => {
                write!(f, "{};{};{};{};{};{}*y", request_id, page_number, top, left, bottom, right,)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MouseReport {
    SGR1006 { x: u16, y: u16, button: MouseButton, modifiers: Modifiers },
}

impl Display for MouseReport {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            MouseReport::SGR1006 { x, y, button, modifiers } => {
                let mut b = 0;
                if (*modifiers & Modifiers::SHIFT) != Modifiers::NONE {
                    b |= 4;
                }
                if (*modifiers & Modifiers::ALT) != Modifiers::NONE {
                    b |= 8;
                }
                if (*modifiers & Modifiers::CTRL) != Modifiers::NONE {
                    b |= 16;
                }
                b |= match button {
                    MouseButton::Button1Press | MouseButton::Button1Release => 0,
                    MouseButton::Button2Press | MouseButton::Button2Release => 1,
                    MouseButton::Button3Press | MouseButton::Button3Release => 2,
                    MouseButton::Button4Press | MouseButton::Button4Release => 64,
                    MouseButton::Button5Press | MouseButton::Button5Release => 65,
                    MouseButton::Button1Drag => 32,
                    MouseButton::Button2Drag => 33,
                    MouseButton::Button3Drag => 34,
                    MouseButton::None => 35,
                };
                let trailer = match button {
                    MouseButton::Button1Press
                    | MouseButton::Button2Press
                    | MouseButton::Button3Press
                    | MouseButton::Button4Press
                    | MouseButton::Button5Press
                    | MouseButton::Button1Drag
                    | MouseButton::Button2Drag
                    | MouseButton::Button3Drag
                    | MouseButton::None => 'M',
                    _ => 'm',
                };
                write!(f, "<{};{};{}{}", b, x, y, trailer)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    SetDecPrivateMode(DecPrivateMode),
    ResetDecPrivateMode(DecPrivateMode),
    SaveDecPrivateMode(DecPrivateMode),
    RestoreDecPrivateMode(DecPrivateMode),
    SetMode(TerminalMode),
    ResetMode(TerminalMode),
}

impl Display for Mode {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        macro_rules! emit {
            ($flag:expr, $mode:expr) => {{
                let value = match $mode {
                    DecPrivateMode::Code(mode) => mode.to_u16().ok_or_else(|| FmtError)?,
                    DecPrivateMode::Unspecified(mode) => *mode,
                };
                write!(f, "?{}{}", value, $flag)
            }};
        }
        macro_rules! emit_mode {
            ($flag:expr, $mode:expr) => {{
                let value = match $mode {
                    TerminalMode::Code(mode) => mode.to_u16().ok_or_else(|| FmtError)?,
                    TerminalMode::Unspecified(mode) => *mode,
                };
                write!(f, "?{}{}", value, $flag)
            }};
        }
        match self {
            Mode::SetDecPrivateMode(mode) => emit!("h", mode),
            Mode::ResetDecPrivateMode(mode) => emit!("l", mode),
            Mode::SaveDecPrivateMode(mode) => emit!("s", mode),
            Mode::RestoreDecPrivateMode(mode) => emit!("r", mode),
            Mode::SetMode(mode) => emit_mode!("h", mode),
            Mode::ResetMode(mode) => emit_mode!("l", mode),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecPrivateMode {
    Code(DecPrivateModeCode),
    Unspecified(u16),
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, ToPrimitive)]
pub enum DecPrivateModeCode {
    ApplicationCursorKeys = 1,
    StartBlinkingCursor = 12,
    ShowCursor = 25,

    MouseTracking = 1000,

    HighlightMouseTracking = 1001,

    ButtonEventMouse = 1002,

    AnyEventMouse = 1003,

    SGRMouse = 1006,
    ClearAndEnableAlternateScreen = 1049,
    EnableAlternateScreen = 47,
    BracketedPaste = 2004,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalMode {
    Code(TerminalModeCode),
    Unspecified(u16),
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, ToPrimitive)]
pub enum TerminalModeCode {
    KeyboardAction = 2,
    Insert = 4,
    SendReceive = 12,
    AutomaticNewline = 20,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cursor {
    BackwardTabulation(u32),

    TabulationClear(TabulationClear),

    CharacterAbsolute(OneBased),

    CharacterPositionAbsolute(OneBased),

    CharacterPositionBackward(u32),

    CharacterPositionForward(u32),

    CharacterAndLinePosition { line: OneBased, col: OneBased },

    LinePositionAbsolute(u32),

    LinePositionBackward(u32),

    LinePositionForward(u32),

    ForwardTabulation(u32),

    NextLine(u32),

    PrecedingLine(u32),

    ActivePositionReport { line: OneBased, col: OneBased },

    RequestActivePositionReport,

    SaveCursor,
    RestoreCursor,

    TabulationControl(CursorTabulationControl),

    Left(u32),

    Down(u32),

    Right(u32),

    Position { line: OneBased, col: OneBased },

    Up(u32),

    LineTabulation(u32),

    SetTopAndBottomMargins { top: OneBased, bottom: OneBased },

    CursorStyle(CursorStyle),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    DeleteCharacter(u32),

    DeleteLine(u32),

    EraseCharacter(u32),

    EraseInLine(EraseInLine),

    InsertCharacter(u32),

    InsertLine(u32),

    ScrollDown(u32),

    ScrollUp(u32),

    EraseInDisplay(EraseInDisplay),

    Repeat(u32),
}

trait EncodeCSIParam {
    fn write_csi(&self, f: &mut Formatter, control: &str) -> Result<(), FmtError>;
}

impl<T: ParamEnum + PartialEq + num::ToPrimitive> EncodeCSIParam for T {
    fn write_csi(&self, f: &mut Formatter, control: &str) -> Result<(), FmtError> {
        if *self == ParamEnum::default() {
            write!(f, "{}", control)
        } else {
            let value = self.to_i64().ok_or_else(|| FmtError)?;
            write!(f, "{}{}", value, control)
        }
    }
}

impl EncodeCSIParam for u32 {
    fn write_csi(&self, f: &mut Formatter, control: &str) -> Result<(), FmtError> {
        if *self == 1 {
            write!(f, "{}", control)
        } else {
            write!(f, "{}{}", *self, control)
        }
    }
}

impl EncodeCSIParam for OneBased {
    fn write_csi(&self, f: &mut Formatter, control: &str) -> Result<(), FmtError> {
        if self.as_one_based() == 1 {
            write!(f, "{}", control)
        } else {
            write!(f, "{}{}", *self, control)
        }
    }
}

impl Display for Edit {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            Edit::DeleteCharacter(n) => n.write_csi(f, "P")?,
            Edit::DeleteLine(n) => n.write_csi(f, "M")?,
            Edit::EraseCharacter(n) => n.write_csi(f, "X")?,
            Edit::EraseInLine(n) => n.write_csi(f, "K")?,
            Edit::InsertCharacter(n) => n.write_csi(f, "@")?,
            Edit::InsertLine(n) => n.write_csi(f, "L")?,
            Edit::ScrollDown(n) => n.write_csi(f, "T")?,
            Edit::ScrollUp(n) => n.write_csi(f, "S")?,
            Edit::EraseInDisplay(n) => n.write_csi(f, "J")?,
            Edit::Repeat(n) => n.write_csi(f, "b")?,
        }
        Ok(())
    }
}

impl Display for Cursor {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            Cursor::BackwardTabulation(n) => n.write_csi(f, "Z")?,
            Cursor::CharacterAbsolute(col) => col.write_csi(f, "G")?,
            Cursor::ForwardTabulation(n) => n.write_csi(f, "I")?,
            Cursor::NextLine(n) => n.write_csi(f, "E")?,
            Cursor::PrecedingLine(n) => n.write_csi(f, "F")?,
            Cursor::ActivePositionReport { line, col } => write!(f, "{};{}R", line, col)?,
            Cursor::Left(n) => n.write_csi(f, "D")?,
            Cursor::Down(n) => n.write_csi(f, "B")?,
            Cursor::Right(n) => n.write_csi(f, "C")?,
            Cursor::Up(n) => n.write_csi(f, "A")?,
            Cursor::Position { line, col } => write!(f, "{};{}H", line, col)?,
            Cursor::LineTabulation(n) => n.write_csi(f, "Y")?,
            Cursor::TabulationControl(n) => n.write_csi(f, "W")?,
            Cursor::TabulationClear(n) => n.write_csi(f, "g")?,
            Cursor::CharacterPositionAbsolute(n) => n.write_csi(f, "`")?,
            Cursor::CharacterPositionBackward(n) => n.write_csi(f, "j")?,
            Cursor::CharacterPositionForward(n) => n.write_csi(f, "a")?,
            Cursor::CharacterAndLinePosition { line, col } => write!(f, "{};{}f", line, col)?,
            Cursor::LinePositionAbsolute(n) => n.write_csi(f, "d")?,
            Cursor::LinePositionBackward(n) => n.write_csi(f, "k")?,
            Cursor::LinePositionForward(n) => n.write_csi(f, "e")?,
            Cursor::SetTopAndBottomMargins { top, bottom } => {
                if top.as_one_based() == 1 && bottom.as_one_based() == u32::max_value() {
                    write!(f, "r")?;
                } else {
                    write!(f, "{};{}r", top, bottom)?;
                }
            }
            Cursor::RequestActivePositionReport => write!(f, "6n")?,
            Cursor::SaveCursor => write!(f, "s")?,
            Cursor::RestoreCursor => write!(f, "u")?,
            Cursor::CursorStyle(style) => write!(f, "{} q", *style as u8)?,
        }
        Ok(())
    }
}

trait ParseParams: Sized {
    fn parse_params(params: &[i64]) -> Result<Self, ()>;
}

impl ParseParams for u32 {
    fn parse_params(params: &[i64]) -> Result<u32, ()> {
        if params.is_empty() {
            Ok(1)
        } else if params.len() == 1 {
            to_1b_u32(params[0])
        } else {
            Err(())
        }
    }
}

impl ParseParams for OneBased {
    fn parse_params(params: &[i64]) -> Result<OneBased, ()> {
        if params.is_empty() {
            Ok(OneBased::new(1))
        } else if params.len() == 1 {
            OneBased::from_esc_param(params[0])
        } else {
            Err(())
        }
    }
}

impl ParseParams for (OneBased, OneBased) {
    fn parse_params(params: &[i64]) -> Result<(OneBased, OneBased), ()> {
        if params.is_empty() {
            Ok((OneBased::new(1), OneBased::new(1)))
        } else if params.len() == 2 {
            Ok((OneBased::from_esc_param(params[0])?, OneBased::from_esc_param(params[1])?))
        } else {
            Err(())
        }
    }
}

trait ParamEnum: num::FromPrimitive {
    fn default() -> Self;
}

impl<T: ParamEnum> ParseParams for T {
    fn parse_params(params: &[i64]) -> Result<Self, ()> {
        if params.is_empty() {
            Ok(ParamEnum::default())
        } else if params.len() == 1 {
            num::FromPrimitive::from_i64(params[0]).ok_or(())
        } else {
            Err(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, Copy, ToPrimitive)]
pub enum CursorTabulationControl {
    SetCharacterTabStopAtActivePosition = 0,
    SetLineTabStopAtActiveLine = 1,
    ClearCharacterTabStopAtActivePosition = 2,
    ClearLineTabstopAtActiveLine = 3,
    ClearAllCharacterTabStopsAtActiveLine = 4,
    ClearAllCharacterTabStops = 5,
    ClearAllLineTabStops = 6,
}

impl ParamEnum for CursorTabulationControl {
    fn default() -> Self {
        CursorTabulationControl::SetCharacterTabStopAtActivePosition
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, Copy, ToPrimitive)]
pub enum TabulationClear {
    ClearCharacterTabStopAtActivePosition = 0,
    ClearLineTabStopAtActiveLine = 1,
    ClearCharacterTabStopsAtActiveLine = 2,
    ClearAllCharacterTabStops = 3,
    ClearAllLineTabStops = 4,
    ClearAllTabStops = 5,
}

impl ParamEnum for TabulationClear {
    fn default() -> Self {
        TabulationClear::ClearCharacterTabStopAtActivePosition
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, Copy, ToPrimitive)]
pub enum EraseInLine {
    EraseToEndOfLine = 0,
    EraseToStartOfLine = 1,
    EraseLine = 2,
}

impl ParamEnum for EraseInLine {
    fn default() -> Self {
        EraseInLine::EraseToEndOfLine
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, Copy, ToPrimitive)]
pub enum EraseInDisplay {
    EraseToEndOfDisplay = 0,

    EraseToStartOfDisplay = 1,

    EraseDisplay = 2,

    EraseScrollback = 3,
}

impl ParamEnum for EraseInDisplay {
    fn default() -> Self {
        EraseInDisplay::EraseToEndOfDisplay
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sgr {
    Reset,

    Intensity(Intensity),
    Underline(Underline),
    Blink(Blink),
    Italic(bool),
    Inverse(bool),
    Invisible(bool),
    StrikeThrough(bool),
    Font(Font),
    Foreground(ColorSpec),
    Background(ColorSpec),
}

impl Display for Sgr {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        macro_rules! code {
            ($t:ident) => {
                write!(f, "{}m", SgrCode::$t as i64)?
            };
        }

        macro_rules! ansi_color {
            ($idx:expr, $eightbit:ident, $( ($Ansi:ident, $code:ident) ),*) => {
                if let Some(ansi) = num::FromPrimitive::from_u8($idx) {
                    match ansi {
                        $(AnsiColor::$Ansi => code!($code) ,)*
                    }
                } else {
                    write!(f, "{};5;{}m", SgrCode::$eightbit as i64, $idx)?
                }
            }
        }

        match self {
            Sgr::Reset => code!(Reset),
            Sgr::Intensity(Intensity::Bold) => code!(IntensityBold),
            Sgr::Intensity(Intensity::Half) => code!(IntensityDim),
            Sgr::Intensity(Intensity::Normal) => code!(NormalIntensity),
            Sgr::Underline(Underline::Single) => code!(UnderlineOn),
            Sgr::Underline(Underline::Double) => code!(UnderlineDouble),
            Sgr::Underline(Underline::None) => code!(UnderlineOff),
            Sgr::Blink(Blink::Slow) => code!(BlinkOn),
            Sgr::Blink(Blink::Rapid) => code!(RapidBlinkOn),
            Sgr::Blink(Blink::None) => code!(BlinkOff),
            Sgr::Italic(true) => code!(ItalicOn),
            Sgr::Italic(false) => code!(ItalicOff),
            Sgr::Inverse(true) => code!(InverseOn),
            Sgr::Inverse(false) => code!(InverseOff),
            Sgr::Invisible(true) => code!(InvisibleOn),
            Sgr::Invisible(false) => code!(InvisibleOff),
            Sgr::StrikeThrough(true) => code!(StrikeThroughOn),
            Sgr::StrikeThrough(false) => code!(StrikeThroughOff),
            Sgr::Font(Font::Default) => code!(DefaultFont),
            Sgr::Font(Font::Alternate(1)) => code!(AltFont1),
            Sgr::Font(Font::Alternate(2)) => code!(AltFont2),
            Sgr::Font(Font::Alternate(3)) => code!(AltFont3),
            Sgr::Font(Font::Alternate(4)) => code!(AltFont4),
            Sgr::Font(Font::Alternate(5)) => code!(AltFont5),
            Sgr::Font(Font::Alternate(6)) => code!(AltFont6),
            Sgr::Font(Font::Alternate(7)) => code!(AltFont7),
            Sgr::Font(Font::Alternate(8)) => code!(AltFont8),
            Sgr::Font(Font::Alternate(9)) => code!(AltFont9),
            Sgr::Font(_) => { /* there are no other possible font values */ }
            Sgr::Foreground(ColorSpec::Default) => code!(ForegroundDefault),
            Sgr::Background(ColorSpec::Default) => code!(BackgroundDefault),
            Sgr::Foreground(ColorSpec::PaletteIndex(idx)) => ansi_color!(
                *idx,
                ForegroundColor,
                (Black, ForegroundBlack),
                (Maroon, ForegroundRed),
                (Green, ForegroundGreen),
                (Olive, ForegroundYellow),
                (Navy, ForegroundBlue),
                (Purple, ForegroundMagenta),
                (Teal, ForegroundCyan),
                (Silver, ForegroundWhite),
                (Grey, ForegroundBrightBlack),
                (Red, ForegroundBrightRed),
                (Lime, ForegroundBrightGreen),
                (Yellow, ForegroundBrightYellow),
                (Blue, ForegroundBrightBlue),
                (Fuschia, ForegroundBrightMagenta),
                (Aqua, ForegroundBrightCyan),
                (White, ForegroundBrightWhite)
            ),
            Sgr::Foreground(ColorSpec::TrueColor(c)) => write!(
                f,
                "{};2;{};{};{}m",
                SgrCode::ForegroundColor as i64,
                c.red,
                c.green,
                c.blue
            )?,
            Sgr::Background(ColorSpec::PaletteIndex(idx)) => ansi_color!(
                *idx,
                BackgroundColor,
                (Black, BackgroundBlack),
                (Maroon, BackgroundRed),
                (Green, BackgroundGreen),
                (Olive, BackgroundYellow),
                (Navy, BackgroundBlue),
                (Purple, BackgroundMagenta),
                (Teal, BackgroundCyan),
                (Silver, BackgroundWhite),
                (Grey, BackgroundBrightBlack),
                (Red, BackgroundBrightRed),
                (Lime, BackgroundBrightGreen),
                (Yellow, BackgroundBrightYellow),
                (Blue, BackgroundBrightBlue),
                (Fuschia, BackgroundBrightMagenta),
                (Aqua, BackgroundBrightCyan),
                (White, BackgroundBrightWhite)
            ),
            Sgr::Background(ColorSpec::TrueColor(c)) => write!(
                f,
                "{};2;{};{};{}m",
                SgrCode::BackgroundColor as i64,
                c.red,
                c.green,
                c.blue
            )?,
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Font {
    Default,
    Alternate(u8),
}

struct CSIParser<'a> {
    intermediates: &'a [u8],

    ignored_extra_intermediates: bool,
    control: char,

    params: Option<&'a [i64]>,
}

impl CSI {
    pub fn parse<'a>(
        params: &'a [i64],
        intermediates: &'a [u8],
        ignored_extra_intermediates: bool,
        control: char,
    ) -> impl Iterator<Item = CSI> + 'a {
        CSIParser { intermediates, ignored_extra_intermediates, control, params: Some(params) }
    }
}

fn to_u8(v: i64) -> Result<u8, ()> {
    if v <= i64::from(u8::max_value()) {
        Ok(v as u8)
    } else {
        Err(())
    }
}

fn to_1b_u32(v: i64) -> Result<u32, ()> {
    if v == 0 {
        Ok(1)
    } else if v > 0 && v <= i64::from(u32::max_value()) {
        Ok(v as u32)
    } else {
        Err(())
    }
}

macro_rules! noparams {
    ($ns:ident, $variant:ident, $params:expr) => {{
        if $params.len() != 0 {
            Err(())
        } else {
            Ok(CSI::$ns($ns::$variant))
        }
    }};
}

macro_rules! parse {
    ($ns:ident, $variant:ident, $params:expr) => {{
        let value = ParseParams::parse_params($params)?;
        Ok(CSI::$ns($ns::$variant(value)))
    }};

    ($ns:ident, $variant:ident, $first:ident, $second:ident, $params:expr) => {{
        let (p1, p2): (OneBased, OneBased) = ParseParams::parse_params($params)?;
        Ok(CSI::$ns($ns::$variant { $first: p1, $second: p2 }))
    }};
}

impl<'a> CSIParser<'a> {
    fn parse_next(&mut self, params: &'a [i64]) -> Result<CSI, ()> {
        match (self.control, self.intermediates) {
            ('@', &[]) => parse!(Edit, InsertCharacter, params),
            ('`', &[]) => parse!(Cursor, CharacterPositionAbsolute, params),
            ('A', &[]) => parse!(Cursor, Up, params),
            ('B', &[]) => parse!(Cursor, Down, params),
            ('C', &[]) => parse!(Cursor, Right, params),
            ('D', &[]) => parse!(Cursor, Left, params),
            ('E', &[]) => parse!(Cursor, NextLine, params),
            ('F', &[]) => parse!(Cursor, PrecedingLine, params),
            ('G', &[]) => parse!(Cursor, CharacterAbsolute, params),
            ('H', &[]) => parse!(Cursor, Position, line, col, params),
            ('I', &[]) => parse!(Cursor, ForwardTabulation, params),
            ('J', &[]) => parse!(Edit, EraseInDisplay, params),
            ('K', &[]) => parse!(Edit, EraseInLine, params),
            ('L', &[]) => parse!(Edit, InsertLine, params),
            ('M', &[]) => parse!(Edit, DeleteLine, params),
            ('P', &[]) => parse!(Edit, DeleteCharacter, params),
            ('R', &[]) => parse!(Cursor, ActivePositionReport, line, col, params),
            ('S', &[]) => parse!(Edit, ScrollUp, params),
            ('T', &[]) => parse!(Edit, ScrollDown, params),
            ('W', &[]) => parse!(Cursor, TabulationControl, params),
            ('X', &[]) => parse!(Edit, EraseCharacter, params),
            ('Y', &[]) => parse!(Cursor, LineTabulation, params),
            ('Z', &[]) => parse!(Cursor, BackwardTabulation, params),

            ('a', &[]) => parse!(Cursor, CharacterPositionForward, params),
            ('b', &[]) => parse!(Edit, Repeat, params),
            ('d', &[]) => parse!(Cursor, LinePositionAbsolute, params),
            ('e', &[]) => parse!(Cursor, LinePositionForward, params),
            ('f', &[]) => parse!(Cursor, CharacterAndLinePosition, line, col, params),
            ('g', &[]) => parse!(Cursor, TabulationClear, params),
            ('h', &[]) => self.terminal_mode(params).map(|mode| CSI::Mode(Mode::SetMode(mode))),
            ('j', &[]) => parse!(Cursor, CharacterPositionBackward, params),
            ('k', &[]) => parse!(Cursor, LinePositionBackward, params),
            ('l', &[]) => self.terminal_mode(params).map(|mode| CSI::Mode(Mode::ResetMode(mode))),

            ('m', &[]) => self.sgr(params).map(CSI::Sgr),
            ('n', &[]) => self.dsr(params),
            ('q', &[b' ']) => self.cursor_style(params),
            ('r', &[]) => self.decstbm(params),
            ('s', &[]) => noparams!(Cursor, SaveCursor, params),
            ('t', &[]) => self.window(params).map(CSI::Window),
            ('u', &[]) => noparams!(Cursor, RestoreCursor, params),
            ('y', &[b'*']) => {
                fn p(params: &[i64], idx: usize) -> Result<i64, ()> {
                    params.get(idx).cloned().ok_or(())
                }
                let request_id = p(params, 0)?;
                let page_number = p(params, 1)?;
                let top = OneBased::from_optional_esc_param(params.get(2))?;
                let left = OneBased::from_optional_esc_param(params.get(3))?;
                let bottom = OneBased::from_optional_esc_param(params.get(4))?;
                let right = OneBased::from_optional_esc_param(params.get(5))?;
                Ok(CSI::Window(Window::ChecksumRectangularArea {
                    request_id,
                    page_number,
                    top,
                    left,
                    bottom,
                    right,
                }))
            }

            ('p', &[b'!']) => Ok(CSI::Device(Box::new(Device::SoftReset))),

            ('h', &[b'?']) => self.dec(params).map(|mode| CSI::Mode(Mode::SetDecPrivateMode(mode))),
            ('l', &[b'?']) => {
                self.dec(params).map(|mode| CSI::Mode(Mode::ResetDecPrivateMode(mode)))
            }
            ('r', &[b'?']) => {
                self.dec(params).map(|mode| CSI::Mode(Mode::RestoreDecPrivateMode(mode)))
            }
            ('s', &[b'?']) => {
                self.dec(params).map(|mode| CSI::Mode(Mode::SaveDecPrivateMode(mode)))
            }

            ('m', &[b'<']) | ('M', &[b'<']) => self.mouse_sgr1006(params).map(CSI::Mouse),

            ('c', &[]) => {
                self.req_primary_device_attributes(params).map(|dev| CSI::Device(Box::new(dev)))
            }
            ('c', &[b'>']) => {
                self.req_secondary_device_attributes(params).map(|dev| CSI::Device(Box::new(dev)))
            }
            ('c', &[b'?']) => {
                self.secondary_device_attributes(params).map(|dev| CSI::Device(Box::new(dev)))
            }

            _ => Err(()),
        }
    }

    fn advance_by<T>(&mut self, n: usize, params: &'a [i64], result: T) -> T {
        let (_, next) = params.split_at(n);
        if !next.is_empty() {
            self.params = Some(next);
        }
        result
    }

    fn cursor_style(&mut self, params: &'a [i64]) -> Result<CSI, ()> {
        if params.len() != 1 {
            Err(())
        } else {
            match num::FromPrimitive::from_i64(params[0]) {
                None => Err(()),
                Some(style) => {
                    Ok(self.advance_by(1, params, CSI::Cursor(Cursor::CursorStyle(style))))
                }
            }
        }
    }

    fn dsr(&mut self, params: &'a [i64]) -> Result<CSI, ()> {
        if params == [5] {
            Ok(self.advance_by(1, params, CSI::Device(Box::new(Device::StatusReport))))
        } else if params == [6] {
            Ok(self.advance_by(1, params, CSI::Cursor(Cursor::RequestActivePositionReport)))
        } else {
            Err(())
        }
    }

    fn decstbm(&mut self, params: &'a [i64]) -> Result<CSI, ()> {
        if params.is_empty() {
            Ok(CSI::Cursor(Cursor::SetTopAndBottomMargins {
                top: OneBased::new(1),
                bottom: OneBased::new(u32::max_value()),
            }))
        } else if params.len() == 2 {
            Ok(self.advance_by(
                2,
                params,
                CSI::Cursor(Cursor::SetTopAndBottomMargins {
                    top: OneBased::from_esc_param(params[0])?,
                    bottom: OneBased::from_esc_param(params[1])?,
                }),
            ))
        } else {
            Err(())
        }
    }

    fn req_primary_device_attributes(&mut self, params: &'a [i64]) -> Result<Device, ()> {
        if params.is_empty() {
            Ok(Device::RequestPrimaryDeviceAttributes)
        } else if params == [0] {
            Ok(self.advance_by(1, params, Device::RequestPrimaryDeviceAttributes))
        } else {
            Err(())
        }
    }

    fn req_secondary_device_attributes(&mut self, params: &'a [i64]) -> Result<Device, ()> {
        if params.is_empty() {
            Ok(Device::RequestSecondaryDeviceAttributes)
        } else if params == [0] {
            Ok(self.advance_by(1, params, Device::RequestSecondaryDeviceAttributes))
        } else {
            Err(())
        }
    }

    fn secondary_device_attributes(&mut self, params: &'a [i64]) -> Result<Device, ()> {
        if params == [1, 0] {
            Ok(self.advance_by(
                2,
                params,
                Device::DeviceAttributes(DeviceAttributes::Vt101WithNoOptions),
            ))
        } else if params == [6] {
            Ok(self.advance_by(1, params, Device::DeviceAttributes(DeviceAttributes::Vt102)))
        } else if params == [1, 2] {
            Ok(self.advance_by(
                2,
                params,
                Device::DeviceAttributes(DeviceAttributes::Vt100WithAdvancedVideoOption),
            ))
        } else if !params.is_empty() && params[0] == 62 {
            Ok(self.advance_by(
                params.len(),
                params,
                Device::DeviceAttributes(DeviceAttributes::Vt220(
                    DeviceAttributeFlags::from_params(&params[1..]),
                )),
            ))
        } else if !params.is_empty() && params[0] == 63 {
            Ok(self.advance_by(
                params.len(),
                params,
                Device::DeviceAttributes(DeviceAttributes::Vt320(
                    DeviceAttributeFlags::from_params(&params[1..]),
                )),
            ))
        } else if !params.is_empty() && params[0] == 64 {
            Ok(self.advance_by(
                params.len(),
                params,
                Device::DeviceAttributes(DeviceAttributes::Vt420(
                    DeviceAttributeFlags::from_params(&params[1..]),
                )),
            ))
        } else {
            Err(())
        }
    }

    fn mouse_sgr1006(&mut self, params: &'a [i64]) -> Result<MouseReport, ()> {
        if params.len() != 3 {
            return Err(());
        }

        let button = match (self.control, params[0] & 0b110_0011) {
            ('M', 0) => MouseButton::Button1Press,
            ('m', 0) => MouseButton::Button1Release,
            ('M', 1) => MouseButton::Button2Press,
            ('m', 1) => MouseButton::Button2Release,
            ('M', 2) => MouseButton::Button3Press,
            ('m', 2) => MouseButton::Button3Release,
            ('M', 64) => MouseButton::Button4Press,
            ('m', 64) => MouseButton::Button4Release,
            ('M', 65) => MouseButton::Button5Press,
            ('m', 65) => MouseButton::Button5Release,
            ('M', 32) => MouseButton::Button1Drag,
            ('M', 33) => MouseButton::Button2Drag,
            ('M', 34) => MouseButton::Button3Drag,

            ('M', 35) => MouseButton::None,
            ('M', 3) => MouseButton::None,
            ('m', 3) => MouseButton::None,
            _ => {
                return Err(());
            }
        };

        let mut modifiers = Modifiers::NONE;
        if params[0] & 4 != 0 {
            modifiers |= Modifiers::SHIFT;
        }
        if params[0] & 8 != 0 {
            modifiers |= Modifiers::ALT;
        }
        if params[0] & 16 != 0 {
            modifiers |= Modifiers::CTRL;
        }

        Ok(self.advance_by(
            3,
            params,
            MouseReport::SGR1006 { x: params[1] as u16, y: params[2] as u16, button, modifiers },
        ))
    }

    fn dec(&mut self, params: &'a [i64]) -> Result<DecPrivateMode, ()> {
        match num::FromPrimitive::from_i64(params[0]) {
            None => Ok(DecPrivateMode::Unspecified(params[0].to_u16().ok_or(())?)),
            Some(mode) => Ok(self.advance_by(1, params, DecPrivateMode::Code(mode))),
        }
    }

    fn terminal_mode(&mut self, params: &'a [i64]) -> Result<TerminalMode, ()> {
        match num::FromPrimitive::from_i64(params[0]) {
            None => Ok(TerminalMode::Unspecified(params[0].to_u16().ok_or(())?)),
            Some(mode) => Ok(self.advance_by(1, params, TerminalMode::Code(mode))),
        }
    }

    fn parse_sgr_color(&mut self, params: &'a [i64]) -> Result<ColorSpec, ()> {
        if params.len() >= 5 && params[1] == 2 {
            let red = to_u8(params[2])?;
            let green = to_u8(params[3])?;
            let blue = to_u8(params[4])?;
            let res = RgbColor::new(red, green, blue).into();
            Ok(self.advance_by(5, params, res))
        } else if params.len() >= 3 && params[1] == 5 {
            let idx = to_u8(params[2])?;
            Ok(self.advance_by(3, params, ColorSpec::PaletteIndex(idx)))
        } else {
            Err(())
        }
    }

    fn window(&mut self, params: &'a [i64]) -> Result<Window, ()> {
        if params.is_empty() {
            Err(())
        } else {
            let arg1 = params.get(1).cloned();
            let arg2 = params.get(2).cloned();
            match params[0] {
                1 => Ok(Window::DeIconify),
                2 => Ok(Window::Iconify),
                3 => Ok(Window::MoveWindow { x: arg1.unwrap_or(0), y: arg2.unwrap_or(0) }),
                4 => Ok(Window::ResizeWindowPixels { width: arg1, height: arg2 }),
                5 => Ok(Window::RaiseWindow),
                6 => Ok(Window::LowerWindow),
                7 => Ok(Window::RefreshWindow),
                8 => Ok(Window::ResizeWindowCells { width: arg1, height: arg2 }),
                9 => match arg1 {
                    Some(0) => Ok(Window::RestoreMaximizedWindow),
                    Some(1) => Ok(Window::MaximizeWindow),
                    Some(2) => Ok(Window::MaximizeWindowVertically),
                    Some(3) => Ok(Window::MaximizeWindowHorizontally),
                    _ => Err(()),
                },
                10 => match arg1 {
                    Some(0) => Ok(Window::UndoFullScreenMode),
                    Some(1) => Ok(Window::ChangeToFullScreenMode),
                    Some(2) => Ok(Window::ToggleFullScreen),
                    _ => Err(()),
                },
                11 => Ok(Window::ReportWindowState),
                13 => match arg1 {
                    None => Ok(Window::ReportWindowPosition),
                    Some(2) => Ok(Window::ReportTextAreaPosition),
                    _ => Err(()),
                },
                14 => match arg1 {
                    None => Ok(Window::ReportTextAreaSizePixels),
                    Some(2) => Ok(Window::ReportWindowSizePixels),
                    _ => Err(()),
                },
                15 => Ok(Window::ReportScreenSizePixels),
                16 => Ok(Window::ReportCellSizePixels),
                18 => Ok(Window::ReportTextAreaSizeCells),
                19 => Ok(Window::ReportScreenSizeCells),
                20 => Ok(Window::ReportIconLabel),
                21 => Ok(Window::ReportWindowTitle),
                22 => match arg1 {
                    Some(0) => Ok(Window::PushIconAndWindowTitle),
                    Some(1) => Ok(Window::PushIconTitle),
                    Some(2) => Ok(Window::PushWindowTitle),
                    _ => Err(()),
                },
                23 => match arg1 {
                    Some(0) => Ok(Window::PopIconAndWindowTitle),
                    Some(1) => Ok(Window::PopIconTitle),
                    Some(2) => Ok(Window::PopWindowTitle),
                    _ => Err(()),
                },
                _ => Err(()),
            }
        }
    }

    fn sgr(&mut self, params: &'a [i64]) -> Result<Sgr, ()> {
        if params.is_empty() {
            Ok(Sgr::Reset)
        } else {
            macro_rules! one {
                ($t:expr) => {
                    Ok(self.advance_by(1, params, $t))
                };
            };

            match num::FromPrimitive::from_i64(params[0]) {
                None => Err(()),
                Some(sgr) => match sgr {
                    SgrCode::Reset => one!(Sgr::Reset),
                    SgrCode::IntensityBold => one!(Sgr::Intensity(Intensity::Bold)),
                    SgrCode::IntensityDim => one!(Sgr::Intensity(Intensity::Half)),
                    SgrCode::NormalIntensity => one!(Sgr::Intensity(Intensity::Normal)),
                    SgrCode::UnderlineOn => one!(Sgr::Underline(Underline::Single)),
                    SgrCode::UnderlineDouble => one!(Sgr::Underline(Underline::Double)),
                    SgrCode::UnderlineOff => one!(Sgr::Underline(Underline::None)),
                    SgrCode::BlinkOn => one!(Sgr::Blink(Blink::Slow)),
                    SgrCode::RapidBlinkOn => one!(Sgr::Blink(Blink::Rapid)),
                    SgrCode::BlinkOff => one!(Sgr::Blink(Blink::None)),
                    SgrCode::ItalicOn => one!(Sgr::Italic(true)),
                    SgrCode::ItalicOff => one!(Sgr::Italic(false)),
                    SgrCode::ForegroundColor => self.parse_sgr_color(params).map(Sgr::Foreground),
                    SgrCode::ForegroundBlack => one!(Sgr::Foreground(AnsiColor::Black.into())),
                    SgrCode::ForegroundRed => one!(Sgr::Foreground(AnsiColor::Maroon.into())),
                    SgrCode::ForegroundGreen => one!(Sgr::Foreground(AnsiColor::Green.into())),
                    SgrCode::ForegroundYellow => one!(Sgr::Foreground(AnsiColor::Olive.into())),
                    SgrCode::ForegroundBlue => one!(Sgr::Foreground(AnsiColor::Navy.into())),
                    SgrCode::ForegroundMagenta => one!(Sgr::Foreground(AnsiColor::Purple.into())),
                    SgrCode::ForegroundCyan => one!(Sgr::Foreground(AnsiColor::Teal.into())),
                    SgrCode::ForegroundWhite => one!(Sgr::Foreground(AnsiColor::Silver.into())),
                    SgrCode::ForegroundDefault => one!(Sgr::Foreground(ColorSpec::Default)),
                    SgrCode::ForegroundBrightBlack => one!(Sgr::Foreground(AnsiColor::Grey.into())),
                    SgrCode::ForegroundBrightRed => one!(Sgr::Foreground(AnsiColor::Red.into())),
                    SgrCode::ForegroundBrightGreen => one!(Sgr::Foreground(AnsiColor::Lime.into())),
                    SgrCode::ForegroundBrightYellow => {
                        one!(Sgr::Foreground(AnsiColor::Yellow.into()))
                    }
                    SgrCode::ForegroundBrightBlue => one!(Sgr::Foreground(AnsiColor::Blue.into())),
                    SgrCode::ForegroundBrightMagenta => {
                        one!(Sgr::Foreground(AnsiColor::Fuschia.into()))
                    }
                    SgrCode::ForegroundBrightCyan => one!(Sgr::Foreground(AnsiColor::Aqua.into())),
                    SgrCode::ForegroundBrightWhite => {
                        one!(Sgr::Foreground(AnsiColor::White.into()))
                    }

                    SgrCode::BackgroundColor => self.parse_sgr_color(params).map(Sgr::Background),
                    SgrCode::BackgroundBlack => one!(Sgr::Background(AnsiColor::Black.into())),
                    SgrCode::BackgroundRed => one!(Sgr::Background(AnsiColor::Maroon.into())),
                    SgrCode::BackgroundGreen => one!(Sgr::Background(AnsiColor::Green.into())),
                    SgrCode::BackgroundYellow => one!(Sgr::Background(AnsiColor::Olive.into())),
                    SgrCode::BackgroundBlue => one!(Sgr::Background(AnsiColor::Navy.into())),
                    SgrCode::BackgroundMagenta => one!(Sgr::Background(AnsiColor::Purple.into())),
                    SgrCode::BackgroundCyan => one!(Sgr::Background(AnsiColor::Teal.into())),
                    SgrCode::BackgroundWhite => one!(Sgr::Background(AnsiColor::Silver.into())),
                    SgrCode::BackgroundDefault => one!(Sgr::Background(ColorSpec::Default)),
                    SgrCode::BackgroundBrightBlack => one!(Sgr::Background(AnsiColor::Grey.into())),
                    SgrCode::BackgroundBrightRed => one!(Sgr::Background(AnsiColor::Red.into())),
                    SgrCode::BackgroundBrightGreen => one!(Sgr::Background(AnsiColor::Lime.into())),
                    SgrCode::BackgroundBrightYellow => {
                        one!(Sgr::Background(AnsiColor::Yellow.into()))
                    }
                    SgrCode::BackgroundBrightBlue => one!(Sgr::Background(AnsiColor::Blue.into())),
                    SgrCode::BackgroundBrightMagenta => {
                        one!(Sgr::Background(AnsiColor::Fuschia.into()))
                    }
                    SgrCode::BackgroundBrightCyan => one!(Sgr::Background(AnsiColor::Aqua.into())),
                    SgrCode::BackgroundBrightWhite => {
                        one!(Sgr::Background(AnsiColor::White.into()))
                    }

                    SgrCode::InverseOn => one!(Sgr::Inverse(true)),
                    SgrCode::InverseOff => one!(Sgr::Inverse(false)),
                    SgrCode::InvisibleOn => one!(Sgr::Invisible(true)),
                    SgrCode::InvisibleOff => one!(Sgr::Invisible(false)),
                    SgrCode::StrikeThroughOn => one!(Sgr::StrikeThrough(true)),
                    SgrCode::StrikeThroughOff => one!(Sgr::StrikeThrough(false)),
                    SgrCode::DefaultFont => one!(Sgr::Font(Font::Default)),
                    SgrCode::AltFont1 => one!(Sgr::Font(Font::Alternate(1))),
                    SgrCode::AltFont2 => one!(Sgr::Font(Font::Alternate(2))),
                    SgrCode::AltFont3 => one!(Sgr::Font(Font::Alternate(3))),
                    SgrCode::AltFont4 => one!(Sgr::Font(Font::Alternate(4))),
                    SgrCode::AltFont5 => one!(Sgr::Font(Font::Alternate(5))),
                    SgrCode::AltFont6 => one!(Sgr::Font(Font::Alternate(6))),
                    SgrCode::AltFont7 => one!(Sgr::Font(Font::Alternate(7))),
                    SgrCode::AltFont8 => one!(Sgr::Font(Font::Alternate(8))),
                    SgrCode::AltFont9 => one!(Sgr::Font(Font::Alternate(9))),
                },
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive)]
pub enum SgrCode {
    Reset = 0,
    IntensityBold = 1,
    IntensityDim = 2,
    ItalicOn = 3,
    UnderlineOn = 4,

    BlinkOn = 5,

    RapidBlinkOn = 6,
    InverseOn = 7,
    InvisibleOn = 8,
    StrikeThroughOn = 9,
    DefaultFont = 10,
    AltFont1 = 11,
    AltFont2 = 12,
    AltFont3 = 13,
    AltFont4 = 14,
    AltFont5 = 15,
    AltFont6 = 16,
    AltFont7 = 17,
    AltFont8 = 18,
    AltFont9 = 19,

    UnderlineDouble = 21,
    NormalIntensity = 22,
    ItalicOff = 23,
    UnderlineOff = 24,
    BlinkOff = 25,
    InverseOff = 27,
    InvisibleOff = 28,
    StrikeThroughOff = 29,
    ForegroundBlack = 30,
    ForegroundRed = 31,
    ForegroundGreen = 32,
    ForegroundYellow = 33,
    ForegroundBlue = 34,
    ForegroundMagenta = 35,
    ForegroundCyan = 36,
    ForegroundWhite = 37,
    ForegroundDefault = 39,
    BackgroundBlack = 40,
    BackgroundRed = 41,
    BackgroundGreen = 42,
    BackgroundYellow = 43,
    BackgroundBlue = 44,
    BackgroundMagenta = 45,
    BackgroundCyan = 46,
    BackgroundWhite = 47,
    BackgroundDefault = 49,

    ForegroundBrightBlack = 90,
    ForegroundBrightRed = 91,
    ForegroundBrightGreen = 92,
    ForegroundBrightYellow = 93,
    ForegroundBrightBlue = 94,
    ForegroundBrightMagenta = 95,
    ForegroundBrightCyan = 96,
    ForegroundBrightWhite = 97,

    BackgroundBrightBlack = 100,
    BackgroundBrightRed = 101,
    BackgroundBrightGreen = 102,
    BackgroundBrightYellow = 103,
    BackgroundBrightBlue = 104,
    BackgroundBrightMagenta = 105,
    BackgroundBrightCyan = 106,
    BackgroundBrightWhite = 107,

    ForegroundColor = 38,
    BackgroundColor = 48,
}

impl<'a> Iterator for CSIParser<'a> {
    type Item = CSI;

    fn next(&mut self) -> Option<CSI> {
        let params = match self.params.take() {
            None => return None,
            Some(params) => params,
        };

        match self.parse_next(&params) {
            Ok(csi) => Some(csi),
            Err(()) => Some(CSI::Unspecified(Box::new(Unspecified {
                params: params.to_vec(),
                intermediates: self.intermediates.to_vec(),
                ignored_extra_intermediates: self.ignored_extra_intermediates,
                control: self.control,
            }))),
        }
    }
}
