//! 深度睡眠管理
//!
//! 封装 ESP32-S3 深度睡眠的进入流程，支持传入清理回调。
//!
//! ## 用法
//!
//! ```ignore
//! sleep::enter(&mut bl, &mut display, || {
//!     drop(mic);
//!     drop(wifi);
//! });
//! ```
//!
//! ## 唤醒
//!
//! 深度睡眠唤醒 = 硬件复位，`app_main()` 从头执行，无需特殊处理。
//!
//! ## 坑点
//!
//! - **EXT1 唤醒必须等按键释放后才可进深睡**，否则立即唤醒
//! - **GPIO 引脚在深睡期间保持最后状态**（PMIC GPIO2 保持高电平 → 背光亮着）
//! - **I2C 总线不停用可能导致深睡失败**（I2S DMA、SPI 等）

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
/// 1. 调用 `cleanup` 释放外设资源（I2S、SPI、WiFi 等）
/// 2. 关背光、关显示
/// 3. 等待按键释放（防 EXT1 立即唤醒）
/// 4. 执行 `esp_deep_sleep_start()`
///
/// # 参数
///
/// * `bl` - 背光引脚驱动
/// * `display` - 显示驱动（需实现 `sleep()` 方法）
/// * `btn_b_is_pressed` - 判断 BtnB 是否仍按下的闭包
/// * `cleanup` - 释放外设资源的闭包（drop 掉外设驱动）
pub fn enter(
    bl: &mut PinDriver<'_, Output>,
    display: &mut impl DisplaySleep,
    mut btn_b_is_pressed: impl FnMut() -> bool,
    cleanup: impl FnOnce(),
) {
    log::info!("Sleep: entering deep sleep...");

    // 清理外设
    cleanup();

    // 关背光 + 关显示
    let _ = bl.set_low();
    display.sleep();

    // 等待按键释放（否则 EXT1 立即唤醒）
    while btn_b_is_pressed() {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    std::thread::sleep(std::time::Duration::from_millis(30));

    // 进入深睡（此函数不返回）
    unsafe {
        esp_deep_sleep_start();
    }
    log::warn!("Sleep: deep sleep failed to start");
}

/// 显示驱动需实现的睡眠 trait
pub trait DisplaySleep {
    fn sleep(&mut self);
}
