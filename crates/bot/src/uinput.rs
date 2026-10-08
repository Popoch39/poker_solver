//! Clicks through uinput: a virtual absolute pointer, as the kernel sees a
//! VM's tablet mouse.
//!
//! The device has absolute X/Y axes spanning the [`ScreenArea`] and a left
//! button, and nothing that makes it a touchscreen or a tablet: udev tags
//! it a mouse, libinput gives its positions to the compositor as absolute
//! pointer motion, mapped over the whole layout (no monitor bound). An
//! axis value is then a layout coordinate less the area's corner.
//!
//! `/dev/uinput` is root's alone by default (`crw------- root`). See the
//! crate docs for the udev rule that lets the desktop's user open it.

use std::thread::sleep;
use std::time::Duration;

use evdev::uinput::VirtualDevice;
use evdev::{
    AbsInfo, AbsoluteAxisCode, AbsoluteAxisEvent, AttributeSet, InputEvent, KeyCode, KeyEvent,
    UinputAbsSetup,
};

use crate::inject::{InjectError, Injector, ScreenArea, ScreenPoint};
use crate::stop::EmergencyStop;

/// Time for the compositor to open a new input device.
const HOTPLUG: Duration = Duration::from_secs(1);
/// Between the move and the press, and the press and the release: a click
/// the client sees as such, at the place moved to.
const STEP: Duration = Duration::from_millis(40);

/// Left clicks through a virtual uinput pointer.
pub struct Uinput {
    device: VirtualDevice,
    area: ScreenArea,
    stop: EmergencyStop,
}

impl Uinput {
    /// A virtual pointer over `area`. A raised `stop` cancels a click
    /// before its press; a press is always released.
    pub fn new(area: ScreenArea, stop: EmergencyStop) -> Result<Self, InjectError> {
        let axis = |code, size: u32| {
            let max = i32::try_from(size).unwrap_or(i32::MAX) - 1;
            UinputAbsSetup::new(code, AbsInfo::new(0, 0, max, 0, 0, 1))
        };
        let device = VirtualDevice::builder()
            .and_then(|b| {
                b.name("nitro-bot pointer")
                    .with_keys(&AttributeSet::from_iter([KeyCode::BTN_LEFT]))?
                    .with_absolute_axis(&axis(AbsoluteAxisCode::ABS_X, area.width))?
                    .with_absolute_axis(&axis(AbsoluteAxisCode::ABS_Y, area.height))?
                    .build()
            })
            .map_err(|e| {
                InjectError(format!(
                    "cannot create the uinput pointer (is /dev/uinput writable? see the udev rule in the docs): {e}"
                ))
            })?;
        sleep(HOTPLUG);
        Ok(Self { device, area, stop })
    }

    fn emit(&mut self, events: &[InputEvent]) -> Result<(), InjectError> {
        self.device
            .emit(events)
            .map_err(|e| InjectError(format!("uinput: {e}")))
    }

    fn button(&mut self, pressed: bool) -> Result<(), InjectError> {
        self.emit(&[*KeyEvent::new(KeyCode::BTN_LEFT, i32::from(pressed))])
    }
}

impl Injector for Uinput {
    fn click(&mut self, at: ScreenPoint) -> Result<(), InjectError> {
        if !self.area.contains(at) {
            return Err(InjectError(format!("{at:?} is off the screen")));
        }
        let (x, y) = (at.x - self.area.x, at.y - self.area.y);
        self.emit(&[
            *AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_X, x),
            *AbsoluteAxisEvent::new(AbsoluteAxisCode::ABS_Y, y),
        ])?;
        sleep(STEP);
        if self.stop.is_triggered() {
            return Err(InjectError("emergency stop before the press".to_owned()));
        }
        let pressed = self.button(true);
        sleep(STEP);
        // Released even if the press failed: a button must never stay down.
        let released = self.button(false);
        pressed.and(released)
    }
}
