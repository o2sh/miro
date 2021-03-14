use crate::core::escape::{Action, DeviceControlMode, Esc, OperatingSystemCommand, CSI};
use log::error;
use num;
use vtparse::{VTActor, VTParser};

pub struct Parser {
    state_machine: VTParser,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    pub fn new() -> Self {
        Self { state_machine: VTParser::new() }
    }

    pub fn parse<F: FnMut(Action)>(&mut self, bytes: &[u8], mut callback: F) {
        let mut perform = Performer { callback: &mut callback };
        self.state_machine.parse(bytes, &mut perform);
    }
}

struct Performer<'a, F: FnMut(Action) + 'a> {
    callback: &'a mut F,
}

impl<'a, F: FnMut(Action)> VTActor for Performer<'a, F> {
    fn print(&mut self, c: char) {
        (self.callback)(Action::Print(c));
    }

    fn execute_c0_or_c1(&mut self, byte: u8) {
        match num::FromPrimitive::from_u8(byte) {
            Some(code) => (self.callback)(Action::Control(code)),
            None => error!("impossible C0/C1 control code {:?} was dropped", byte),
        }
    }

    fn dcs_hook(
        &mut self,
        params: &[i64],
        intermediates: &[u8],
        ignored_extra_intermediates: bool,
    ) {
        (self.callback)(Action::DeviceControl(Box::new(DeviceControlMode::Enter {
            params: params.to_vec(),
            intermediates: intermediates.to_vec(),
            ignored_extra_intermediates,
        })));
    }

    fn dcs_put(&mut self, data: u8) {
        (self.callback)(Action::DeviceControl(Box::new(DeviceControlMode::Data(data))));
    }

    fn dcs_unhook(&mut self) {
        (self.callback)(Action::DeviceControl(Box::new(DeviceControlMode::Exit)));
    }

    fn osc_dispatch(&mut self, osc: &[&[u8]]) {
        let osc = OperatingSystemCommand::parse(osc);
        (self.callback)(Action::OperatingSystemCommand(Box::new(osc)));
    }

    fn csi_dispatch(
        &mut self,
        params: &[i64],
        intermediates: &[u8],
        ignored_extra_intermediates: bool,
        control: u8,
    ) {
        for action in
            CSI::parse(params, intermediates, ignored_extra_intermediates, control as char)
        {
            (self.callback)(Action::CSI(action));
        }
    }

    fn esc_dispatch(
        &mut self,
        _params: &[i64],
        intermediates: &[u8],
        _ignored_extra_intermediates: bool,
        control: u8,
    ) {
        (self.callback)(Action::Esc(Esc::parse(
            if intermediates.len() == 1 { Some(intermediates[0]) } else { None },
            control,
        )));
    }
}
