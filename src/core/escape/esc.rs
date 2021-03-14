use num::{self, ToPrimitive};
use num_derive::*;
use std::fmt::{Display, Error as FmtError, Formatter, Write as FmtWrite};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Esc {
    Unspecified { intermediate: Option<u8>, control: u8 },
    Code(EscCode),
}

macro_rules! esc {
    ($low:expr) => {
        ($low as isize)
    };
    ($high:expr, $low:expr) => {
        ((($high as isize) << 8) | ($low as isize))
    };
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, ToPrimitive, Copy)]
pub enum EscCode {
    FullReset = esc!('c'),

    Index = esc!('D'),

    NextLine = esc!('E'),

    CursorPositionLowerLeft = esc!('F'),

    HorizontalTabSet = esc!('H'),

    ReverseIndex = esc!('M'),

    SingleShiftG2 = esc!('N'),

    SingleShiftG3 = esc!('O'),

    StartOfGuardedArea = esc!('V'),

    EndOfGuardedArea = esc!('W'),

    StartOfString = esc!('X'),

    ReturnTerminalId = esc!('Z'),

    StringTerminator = esc!('\\'),

    PrivacyMessage = esc!('^'),

    ApplicationProgramCommand = esc!('_'),

    DecSaveCursorPosition = esc!('7'),

    DecRestoreCursorPosition = esc!('8'),

    DecApplicationKeyPad = esc!('='),

    DecNormalKeyPad = esc!('>'),

    DecLineDrawing = esc!('(', '0'),

    AsciiCharacterSet = esc!('(', 'B'),

    ApplicationModeArrowUpPress = esc!('O', 'A'),
    ApplicationModeArrowDownPress = esc!('O', 'B'),
    ApplicationModeArrowRightPress = esc!('O', 'C'),
    ApplicationModeArrowLeftPress = esc!('O', 'D'),
    ApplicationModeHomePress = esc!('O', 'H'),
    ApplicationModeEndPress = esc!('O', 'F'),
    F1Press = esc!('O', 'P'),
    F2Press = esc!('O', 'Q'),
    F3Press = esc!('O', 'R'),
    F4Press = esc!('O', 'S'),
}

impl Esc {
    pub fn parse(intermediate: Option<u8>, control: u8) -> Self {
        Self::internal_parse(intermediate, control)
            .unwrap_or_else(|_| Esc::Unspecified { intermediate, control })
    }

    fn internal_parse(intermediate: Option<u8>, control: u8) -> Result<Self, ()> {
        let packed = match intermediate {
            Some(high) => ((u16::from(high)) << 8) | u16::from(control),
            None => u16::from(control),
        };

        let code = num::FromPrimitive::from_u16(packed).ok_or(())?;

        Ok(Esc::Code(code))
    }
}

impl Display for Esc {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        f.write_char(0x1b as char)?;
        use self::Esc::*;
        match self {
            Code(code) => {
                let packed = code.to_u16().expect("num-derive failed to implement ToPrimitive");
                if packed > u16::from(u8::max_value()) {
                    write!(f, "{}{}", (packed >> 8) as u8 as char, (packed & 0xff) as u8 as char)?;
                } else {
                    f.write_char((packed & 0xff) as u8 as char)?;
                }
            }
            Unspecified { intermediate, control } => {
                if let Some(i) = intermediate {
                    write!(f, "{}{}", *i as char, *control as char)?;
                } else {
                    f.write_char(*control as char)?;
                }
            }
        };
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn encode(osc: &Esc) -> String {
        format!("{}", osc)
    }

    fn parse(esc: &str) -> Esc {
        let result = if esc.len() == 1 {
            Esc::parse(None, esc.as_bytes()[0])
        } else {
            Esc::parse(Some(esc.as_bytes()[0]), esc.as_bytes()[1])
        };

        assert_eq!(encode(&result), format!("\x1b{}", esc));

        result
    }

    #[test]
    fn test() {
        assert_eq!(parse("(0"), Esc::Code(EscCode::DecLineDrawing));
        assert_eq!(parse("(B"), Esc::Code(EscCode::AsciiCharacterSet));
    }
}
