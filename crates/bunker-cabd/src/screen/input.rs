//! From an SDL event to what the conductor understands. Keyboard for the
//! desktop, the first joystick for the cabinet. Debug builds add function keys
//! that jump to a scenario.

use cabd_core::view::{DevCommand, Input, Scenario};
use sdl2::JoystickSubsystem;
use sdl2::event::Event as SdlEvent;
use sdl2::joystick::{HatState, Joystick};
use sdl2::keyboard::Keycode;
use tracing::{debug, info, warn};

/// An axis past this value counts as a direction. The range is -32768..32767.
const AXIS_THRESHOLD: i16 = 16000;
const BUTTON_CONFIRM: u8 = 0;
const BUTTON_BACK: u8 = 1;

/// What one SDL event means, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mapped {
    Input(Input),
    Command(DevCommand),
    Quit,
}

pub(crate) struct InputMap {
    subsystem: JoystickSubsystem,
    joystick: Option<Joystick>,
    axes: AxisState,
}

impl InputMap {
    pub(crate) fn new(subsystem: JoystickSubsystem) -> Self {
        Self {
            subsystem,
            joystick: None,
            axes: AxisState::default(),
        }
    }

    /// Opens the first joystick. RetroArch takes the device while a game runs,
    /// so the screen closes it before a launch and opens it again after.
    pub(crate) fn open_joystick(&mut self) {
        if self.joystick.is_some() {
            return;
        }
        match self.subsystem.num_joysticks() {
            Ok(0) => info!("no joystick, keyboard only"),
            Ok(_) => match self.subsystem.open(0) {
                Ok(js) => {
                    info!(name = %js.name(), buttons = js.num_buttons(), axes = js.num_axes(), hats = js.num_hats(), "joystick opened");
                    self.joystick = Some(js);
                }
                Err(e) => warn!(error = %e, "cannot open the joystick"),
            },
            Err(e) => warn!(error = %e, "cannot count joysticks"),
        }
    }

    pub(crate) fn close_joystick(&mut self) {
        if self.joystick.take().is_some() {
            info!("joystick closed");
        }
        self.axes = AxisState::default();
    }

    pub(crate) fn map(&mut self, event: SdlEvent) -> Option<Mapped> {
        let mapped = match event {
            SdlEvent::Quit { .. } => Some(Mapped::Quit),
            SdlEvent::KeyDown {
                keycode: Some(key),
                repeat: false,
                ..
            } => map_key(key),
            SdlEvent::JoyButtonDown { button_idx, .. } => match button_idx {
                BUTTON_CONFIRM => Some(Mapped::Input(Input::Confirm)),
                BUTTON_BACK => Some(Mapped::Input(Input::Back)),
                _ => None,
            },
            SdlEvent::JoyHatMotion { state, .. } => match state {
                HatState::Up => Some(Mapped::Input(Input::Up)),
                HatState::Down => Some(Mapped::Input(Input::Down)),
                HatState::Left => Some(Mapped::Input(Input::Left)),
                HatState::Right => Some(Mapped::Input(Input::Right)),
                _ => None,
            },
            SdlEvent::JoyAxisMotion {
                axis_idx, value, ..
            } => self.axes.update(axis_idx, value).map(Mapped::Input),
            _ => None,
        };
        if let Some(mapped) = &mapped {
            debug!(?mapped, "input");
        }
        mapped
    }
}

/// The last direction seen on the x and y axes, so a held stick sends one
/// event and not one per motion report.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct AxisState {
    x: i8,
    y: i8,
}

impl AxisState {
    fn update(&mut self, axis_idx: u8, value: i16) -> Option<Input> {
        let direction: i8 = if value <= -AXIS_THRESHOLD {
            -1
        } else if value >= AXIS_THRESHOLD {
            1
        } else {
            0
        };
        let slot = match axis_idx {
            0 => &mut self.x,
            1 => &mut self.y,
            _ => return None,
        };
        let previous = std::mem::replace(slot, direction);
        if direction == 0 || direction == previous {
            return None;
        }
        Some(match (axis_idx, direction) {
            (0, -1) => Input::Left,
            (0, _) => Input::Right,
            (_, -1) => Input::Up,
            (_, _) => Input::Down,
        })
    }
}

fn map_key(key: Keycode) -> Option<Mapped> {
    let input = match key {
        Keycode::Up => Input::Up,
        Keycode::Down => Input::Down,
        Keycode::Left => Input::Left,
        Keycode::Right => Input::Right,
        Keycode::Return | Keycode::Space | Keycode::Z => Input::Confirm,
        Keycode::Escape | Keycode::X => Input::Back,
        Keycode::Q => return Some(Mapped::Quit),
        Keycode::F1 if cfg!(debug_assertions) => {
            return Some(Mapped::Command(DevCommand::Scenario(Scenario::Attract)));
        }
        Keycode::F2 if cfg!(debug_assertions) => {
            return Some(Mapped::Command(DevCommand::Scenario(Scenario::Select)));
        }
        Keycode::F3 if cfg!(debug_assertions) => {
            return Some(Mapped::Command(DevCommand::Scenario(Scenario::Postgame)));
        }
        _ => return None,
    };
    Some(Mapped::Input(input))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_stick_sends_one_direction_and_the_return_to_center_sends_nothing() {
        let mut axes = AxisState::default();
        assert_eq!(axes.update(1, -20000), Some(Input::Up));
        assert_eq!(axes.update(1, -30000), None, "still held up");
        assert_eq!(axes.update(1, 0), None, "back to center");
        assert_eq!(axes.update(1, 20000), Some(Input::Down));
        assert_eq!(axes.update(0, 32767), Some(Input::Right));
        assert_eq!(
            axes.update(0, -32768),
            Some(Input::Left),
            "a flip needs no center pass"
        );
    }

    #[test]
    fn a_small_deflection_and_an_unknown_axis_send_nothing() {
        let mut axes = AxisState::default();
        assert_eq!(axes.update(0, 15999), None);
        assert_eq!(axes.update(0, -15999), None);
        assert_eq!(axes.update(2, 32767), None);
    }
}
