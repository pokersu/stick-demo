//! 内置 LED 驱动（M5StickS3 GPIO10）
//!
//! M5StickS3 内置一个红色 LED，通过 GPIO10 控制（高电平点亮）。

use esp_idf_hal::gpio::{Output, PinDriver};

/// 内置红色 LED（GPIO10，高电平点亮）
pub struct Led<'d> {
    pin: PinDriver<'d, Output>,
}

impl<'d> Led<'d> {
    /// 创建 LED 驱动
    pub fn new(pin: impl esp_idf_hal::gpio::OutputPin + 'd) -> Self {
        Self { pin: PinDriver::output(pin).unwrap() }
    }

    /// 点亮 LED
    pub fn on(&mut self)  { self.pin.set_high().unwrap(); }
    /// 关闭 LED
    pub fn off(&mut self) { self.pin.set_low().unwrap(); }
    /// 切换 LED 状态
    pub fn toggle(&mut self) { if self.pin.is_set_high() { self.off(); } else { self.on(); } }
}
