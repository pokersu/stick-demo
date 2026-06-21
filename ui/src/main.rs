//! Slint UI 探索 — M5StickS3
//!
//! 使用 Slint 声明式 UI 框架 + SoftwareRenderer 渲染到 ST7789 屏幕。

#![allow(non_snake_case)]

use core::fmt::Write;
use embedded_hal::i2c::I2c as _;
use esp_idf_hal::{delay::Ets, gpio::PinDriver, peripherals::Peripherals};
use std::rc::Rc;
use stick_s3::{
    battery::Battery, buttons::Buttons, display::Display, es8311, framebuffer::Fb,
    i2c_bus::I2cBus, imu::Imu, mic::Mic, nvs::Nvs, pmic, provision, sleep,
    wifi::{Wifi, WifiStatus},
    HEIGHT, WIDTH,
};

// ── Slint — 编译时从 app.slint 生成 Rust 代码 ──
slint::slint! {
    include!("../app.slint");
}

/// 嵌入式 Slint 平台
struct EmbeddedPlatform {
    window: Rc<slint::platform::software_renderer::MinimalSoftwareWindow>,
}

impl slint::platform::Platform for EmbeddedPlatform {
    fn create_window_adapter(
        &self,
    ) -> Result<Rc<dyn slint::WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }
    fn duration_since_start(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
}

// ═══════════════════════════════════════════════
//  入口
// ═══════════════════════════════════════════════

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::set_max_level(log::LevelFilter::Info);
    sleep::config_wakeup();

    let p = Peripherals::take().unwrap();
    let pins = p.pins;
    let mut delay = Ets;

    // ── PMIC + I2C ──
    let i2c = pmic::init_pmic().expect("PMIC init failed");
    let i2c_bus = Rc::new(I2cBus::new(i2c));
    let mut bl = PinDriver::output(pins.gpio38).unwrap();
    bl.set_low().unwrap();

    // ── 显示 ──
    let mut display = Display::new(
        p.spi2, pins.gpio40, pins.gpio39,
        pins.gpio41, pins.gpio45, pins.gpio21,
    );
    display.init(&mut delay);
    bl.set_high().unwrap();

    // ── IMU ──
    let mut imu = Imu::new(i2c_bus.acquire(), &mut delay).ok();
    let mut imu_data = stick_s3::imu::ImuData::default();

    // ── 音频 ──
    {
        let mut i2c = i2c_bus.acquire();
        let _ = i2c.write(pmic::PMIC_ADDR, &[0x16, 0x00]);
        let _ = i2c.write(pmic::PMIC_ADDR, &[0x10, 0x0C]);
        let _ = i2c.write(pmic::PMIC_ADDR, &[0x13, 0x00]);
        let _ = i2c.write(pmic::PMIC_ADDR, &[0x11, 0x0C]);
        std::thread::sleep(std::time::Duration::from_millis(20));
        if es8311::init_es8311(&mut i2c).is_ok() {
            log::info!("Audio power + ES8311 OK");
        } else {
            log::warn!("ES8311 init failed");
        }
    }

    // ── 麦克风 ──
    let mut mic = {
        let mclk: esp_idf_hal::gpio::Gpio18 = unsafe { core::mem::transmute_copy(&pins.gpio18) };
        let bclk: esp_idf_hal::gpio::Gpio17 = unsafe { core::mem::transmute_copy(&pins.gpio17) };
        let ws:   esp_idf_hal::gpio::Gpio15 = unsafe { core::mem::transmute_copy(&pins.gpio15) };
        match Mic::new(p.i2s1, mclk, bclk, ws, pins.gpio16) {
            Ok(m) => { log::info!("Mic OK"); Some(m) }
            Err(e) => { log::warn!("Mic init failed: {}", e); None }
        }
    };

    // ── Provision + NVS ──
    provision::apply();
    let nvs = Nvs::new("buttons").ok();
    let mut btn_a_count: i32 = nvs.as_ref().and_then(|n| n.get_i32("a")).unwrap_or(0);
    let mut btn_b_count: i32 = nvs.as_ref().and_then(|n| n.get_i32("b")).unwrap_or(0);

    // ── 按键 ──
    let mut btns = Buttons::new(pins.gpio11, pins.gpio12);

    // ── WiFi ──
    let mut wifi = Wifi::new(p.modem).ok();

    // ═══════════════════════════════════════════
    //  Slint — 注册嵌入式平台 + 创建组件
    // ═══════════════════════════════════════════

    let window = Rc::new(
        slint::platform::software_renderer::MinimalSoftwareWindow::new(
            slint::platform::software_renderer::RenderingRotation::NoRotation,
        ),
    );

    slint::platform::set_platform(Box::new(EmbeddedPlatform {
        window: window.clone(),
    }))
    .expect("Slint platform already set");

    let app = App::new().unwrap();
    let app_handle = app.as_weak();

    // ═══════════════════════════════════════════
    //  Framebuffer
    // ═══════════════════════════════════════════

    let mut fb_buf = vec![0u16; (WIDTH * HEIGHT) as usize];

    // ═══════════════════════════════════════════
    //  主循环
    // ═══════════════════════════════════════════

    log::info!("Slint UI started (240x135)");

    loop {
        // ── 传感器 tick ──
        btns.tick();
        if let Some(ref mut imu) = imu {
            imu.tick();
            imu_data = *imu.data();
        }
        if let Some(ref mut mic) = mic {
            mic.tick();
        }
        if let Some(ref mut w) = wifi {
            w.tick();
        }

        // ── 按键事件 ──
        if btns.btn_a_was_pressed() {
            btn_a_count += 1;
            if let Some(ref mut n) = nvs {
                let _ = n.set_i32("a", btn_a_count);
            }
            if let Some(ref mut w) = wifi {
                match w.status() {
                    WifiStatus::Idle | WifiStatus::Failed(_) => w.start_auto_connect(),
                    _ => w.disconnect(),
                }
            }
        }
        if btns.btn_b_was_pressed() {
            btn_b_count += 1;
            if let Some(ref mut n) = nvs {
                let _ = n.set_i32("b", btn_b_count);
            }
        }

        // ── 更新 Slint 属性 ──
        if let Some(app) = app_handle.upgrade() {
            let mut s = String::with_capacity(48);
            let _ = write!(s, "{:.2} {:.2} {:.2}", imu_data.acc_x, imu_data.acc_y, imu_data.acc_z);
            app.set_acc(s.into());

            s.clear();
            let _ = write!(s, "{:.1} {:.1} {:.1}", imu_data.gyr_x, imu_data.gyr_y, imu_data.gyr_z);
            app.set_gyr(s.into());

            s.clear();
            let _ = write!(s, "{:.1} C", imu_data.temp);
            app.set_temp(s.into());

            // 电池
            {
                let mut i2c = i2c_bus.acquire();
                let (mv, charging) = Battery::read_all(&mut i2c);
                let pct = Battery::pct(mv);
                s.clear();
                if mv > 0 {
                    let ch = if charging { '+' } else { ' ' };
                    let _ = write!(s, "{:>3}% {:4.1}V{}", pct, mv as f32 / 1000.0, ch);
                } else {
                    let _ = write!(s, "BAT ---");
                }
                app.set_battery(s.into());
            }

            // WiFi
            let wifi_text = match wifi.as_ref().and_then(|w| Some(w.status())) {
                Some(WifiStatus::Idle) => "wifi ready",
                Some(WifiStatus::Scanning) => "scanning...",
                Some(WifiStatus::Connecting(ssid)) => {
                    app.set_wifi(format!("con {}", ssid).into());
                    ""
                }
                Some(WifiStatus::Connected(ssid)) => {
                    let ip = wifi.as_ref().and_then(|w| w.ip().map(|i| i.to_string())).unwrap_or_default();
                    app.set_wifi(format!("{} {}", ip, ssid).into());
                    ""
                }
                Some(WifiStatus::Failed(msg)) => msg,
                None => "wifi n/a",
            };
            if !wifi_text.is_empty() {
                app.set_wifi(wifi_text.into());
            }

            // 麦克风
            let mic_level = mic.as_ref().map(|m| m.volume()).unwrap_or(0);
            app.set_mic(format!("{}%", mic_level).into());

            // NVS
            app.set_btn_count(btn_a_count + btn_b_count);
        }

        // ── Slint 渲染到 framebuffer ──
        window.draw_if_needed(|renderer| {
            renderer.render_by_line(|line| {
                let y = line as usize;
                let start = y * WIDTH as usize;
                &mut fb_buf[start..start + WIDTH as usize]
            });
        });

        // ── 显示刷新 ──
        display.flush(&mut fb_buf);

        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
