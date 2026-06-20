//! 深度睡眠管理
//!
//! 封装 ESP32-S3 深度睡眠的进入流程，支持传入清理回调。
//!
//! ## 坑点
//! - **EXT1 唤醒必须等按键释放后才可进深睡**，否则立即唤醒
//! - **I2C/SPI/I2S 外设必须 drop**，否则可能阻止深睡
//! - **背光由 PMIC 保持**，睡眠前手动关掉，唤醒后 PMIC 保持状态会重新亮

use esp_idf_hal::{
    gpio::{Output, PinDriver},
    sys::{esp_deep_sleep_start, esp_sleep_enable_ext1_wakeup, esp_sleep_ext1_wakeup_mode_t_ESP_EXT1_WAKEUP_ANY_LOW},
};

/// 配置 BtnB (GPIO12) 低电平唤醒
pub fn config_wakeup() {
    unsafe {
        esp_sleep_enable_ext1_wakeup(
            1u64 << 12,
            esp_sleep_ext1_wakeup_mode_t_ESP_EXT1_WAKEUP_ANY_LOW,
        );
    }
}

/// 进入深度睡眠
///
/// 1. 调用 `cleanup` 释放外设资源
/// 2. 关背光、关显示
/// 3. 等待按键释放（防 EXT1 立即唤醒）
/// 4. 执行 `esp_deep_sleep_start()`
pub fn enter(
    bl: &mut PinDriver<'_, Output>,
    display: &mut impl DisplaySleep,
    mut btn_b_is_pressed: impl FnMut() -> bool,
    cleanup: impl FnOnce(),
) {
    log::info!("Sleep: entering deep sleep...");

    cleanup();

    let _ = bl.set_low();
    display.sleep();

    // ⚠ 必须等按键释放再进深睡，否则 EXT1 立即唤醒
    while btn_b_is_pressed() {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    std::thread::sleep(std::time::Duration::from_millis(30));

    // ⚠ 此函数不返回（硬件复位唤醒）
    unsafe { esp_deep_sleep_start(); }
}

/// 显示驱动需实现的睡眠 trait
pub trait DisplaySleep {
    fn sleep(&mut self);
}
