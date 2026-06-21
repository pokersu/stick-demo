//! LVGL UI 探索 — M5StickS3
//!
//! LVGL + ST7789 渲染集成

use esp_idf_hal::{delay::Ets, peripherals::Peripherals};
use stick_s3::display::Display;

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let p = Peripherals::take().unwrap();
    let pins = p.pins;
    let mut delay = Ets;

    // 初始化显示
    let mut display = Display::new(
        p.spi2, pins.gpio40, pins.gpio39,
        pins.gpio41, pins.gpio45, pins.gpio21,
    );
    display.init(&mut delay);

    log::info!("LVGL UI started on M5StickS3");
}
