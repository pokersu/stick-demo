//! 双按键驱动（M5StickS3 物理按键）
//!
//! M5StickS3 Plus 仅有 2 个物理按键:
//! - Button A: GPIO11 (侧面按键，低电平有效，内部上拉)
//! - Button B: GPIO12 (侧面按键，低电平有效，内部上拉)
//!
//! 用法:
//! ```ignore
//! let mut btns = Buttons::new(pins.gpio11, pins.gpio12);
//! if btns.btn_a_is_pressed() { /* A 键按下 */ }
//! if btns.btn_b_was_pressed() { /* B 键本次 tick 被按下 */ }
//! ```

use esp_idf_hal::gpio::{Input, PinDriver, Pull};

/// 两个物理按键状态
pub struct Buttons<'d> {
    btn_a: PinDriver<'d, Input>,
    btn_b: PinDriver<'d, Input>,
    a_prev: bool,  // 前次采样状态 (true=释放)
    b_prev: bool,
}

impl<'d> Buttons<'d> {
    /// 创建按键驱动（内部上拉，低电平按下）
    pub fn new(
        pin_a: impl esp_idf_hal::gpio::InputPin + 'd,
        pin_b: impl esp_idf_hal::gpio::InputPin + 'd,
    ) -> Self {
        let btn_a = PinDriver::input(pin_a, Pull::Up).unwrap();
        let btn_b = PinDriver::input(pin_b, Pull::Up).unwrap();
        let a_prev = btn_a.is_high();
        let b_prev = btn_b.is_high();
        Self { btn_a, btn_b, a_prev, b_prev }
    }

    /// Button A 当前是否按下
    pub fn btn_a_is_pressed(&self) -> bool { self.btn_a.is_low() }

    /// Button B 当前是否按下
    pub fn btn_b_is_pressed(&self) -> bool { self.btn_b.is_low() }

    /// Button A 是否从上次 tick 至今被按下（边沿检测）
    pub fn btn_a_was_pressed(&mut self) -> bool {
        let now = self.btn_a.is_high();
        let pressed = !now && self.a_prev;
        self.a_prev = now;
        pressed
    }

    /// Button B 是否从上次 tick 至今被按下（边沿检测）
    pub fn btn_b_was_pressed(&mut self) -> bool {
        let now = self.btn_b.is_high();
        let pressed = !now && self.b_prev;
        self.b_prev = now;
        pressed
    }

    /// 任意按键被按下（边沿）
    pub fn any_pressed(&mut self) -> bool {
        self.btn_a_was_pressed() || self.btn_b_was_pressed()
    }

    /// 更新状态（在循环中调用以跟踪边沿）
    pub fn update(&mut self) {
        self.a_prev = self.btn_a.is_high();
        self.b_prev = self.btn_b.is_high();
    }

    /// 释放借用
    pub fn free(self) -> (PinDriver<'d, Input>, PinDriver<'d, Input>) {
        (self.btn_a, self.btn_b)
    }
}

/// 按键标识
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    A,
    B,
}
