//! 内置 LED 驱动（M5StickS3 GPIO10）
//!
//! M5StickS3 内置一个红色 LED，通过 GPIO10 控制（高电平点亮）。

use esp_idf_hal::gpio::{Output, PinDriver};

pub struct Led<'d> {
    pin: PinDriver<'d, Output>,
}

impl<'d> Led<'d> {
    pub fn new(pin: impl esp_idf_hal::gpio::OutputPin + 'd) -> Self {
        Self { pin: PinDriver::output(pin).unwrap() }
    }

    pub fn on(&mut self)  { self.pin.set_high().unwrap(); }
    pub fn off(&mut self) { self.pin.set_low().unwrap(); }
    pub fn toggle(&mut self) { if self.pin.is_set_high() { self.off(); } else { self.on(); } }
}
