#![cfg_attr(feature = "cargo-clippy", allow(clippy::useless_attribute))]

use num_derive::*;
use std::fmt::{Display, Error as FmtError, Formatter, Write as FmtWrite};

pub mod csi;
pub mod esc;
pub mod osc;
pub mod parser;

pub use self::csi::CSI;
pub use self::esc::Esc;
pub use self::esc::EscCode;
pub use self::osc::OperatingSystemCommand;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Print(char),

    Control(ControlCode),

    DeviceControl(Box<DeviceControlMode>),

    OperatingSystemCommand(Box<OperatingSystemCommand>),
    CSI(CSI),
    Esc(Esc),
}

impl Display for Action {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            Action::Print(c) => write!(f, "{}", c),
            Action::Control(c) => f.write_char(*c as u8 as char),
            Action::DeviceControl(_) => unimplemented!(),
            Action::OperatingSystemCommand(osc) => osc.fmt(f),
            Action::CSI(csi) => csi.fmt(f),
            Action::Esc(esc) => esc.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceControlMode {
    Enter { params: Vec<i64>, intermediates: Vec<u8>, ignored_extra_intermediates: bool },

    Exit,

    Data(u8),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, FromPrimitive)]
#[repr(u8)]
pub enum ControlCode {
    Null = 0,
    StartOfHeading = 1,
    StartOfText = 2,
    EndOfText = 3,
    EndOfTransmission = 4,
    Enquiry = 5,
    Acknowledge = 6,
    Bell = 7,
    Backspace = 8,
    HorizontalTab = b'\t',
    LineFeed = b'\n',
    VerticalTab = 0xb,
    FormFeed = 0xc,
    CarriageReturn = b'\r',
    ShiftOut = 0xe,
    ShiftIn = 0xf,
    DataLinkEscape = 0x10,
    DeviceControlOne = 0x11,
    DeviceControlTwo = 0x12,
    DeviceControlThree = 0x13,
    DeviceControlFour = 0x14,
    NegativeAcknowledge = 0x15,
    SynchronousIdle = 0x16,
    EndOfTransmissionBlock = 0x17,
    Cancel = 0x18,
    EndOfMedium = 0x19,
    Substitute = 0x1a,
    Escape = 0x1b,
    FileSeparator = 0x1c,
    GroupSeparator = 0x1d,
    RecordSeparator = 0x1e,
    UnitSeparator = 0x1f,

    BPH = 0x82,
    NBH = 0x83,
    NEL = 0x85,
    SSA = 0x86,
    ESA = 0x87,
    HTS = 0x88,
    HTJ = 0x89,
    VTS = 0x8a,
    PLD = 0x8b,
    PLU = 0x8c,
    RI = 0x8d,
    SS2 = 0x8e,
    SS3 = 0x8f,
    DCS = 0x90,
    PU1 = 0x91,
    PU2 = 0x92,
    STS = 0x93,
    CCH = 0x94,
    MW = 0x95,
    SPA = 0x96,
    EPA = 0x97,
    SOS = 0x98,
    SCI = 0x9a,
    CSI = 0x9b,
    ST = 0x9c,
    OSC = 0x9d,
    PM = 0x9e,
    APC = 0x9f,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OneBased {
    value: u32,
}

impl OneBased {
    pub fn new(value: u32) -> Self {
        debug_assert!(value != 0, "programmer error: deliberately assigning zero to a OneBased");
        Self { value }
    }

    pub fn from_zero_based(value: u32) -> Self {
        Self { value: value + 1 }
    }

    pub fn from_esc_param(v: i64) -> Result<Self, ()> {
        if v == 0 {
            Ok(Self { value: num::one() })
        } else if v > 0 && v <= i64::from(u32::max_value()) {
            Ok(Self { value: v as u32 })
        } else {
            Err(())
        }
    }

    pub fn from_optional_esc_param(o: Option<&i64>) -> Result<Self, ()> {
        Self::from_esc_param(o.cloned().unwrap_or(1))
    }

    pub fn as_zero_based(self) -> u32 {
        self.value.saturating_sub(1)
    }

    pub fn as_one_based(self) -> u32 {
        self.value
    }
}

impl Display for OneBased {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        self.value.fmt(f)
    }
}
