//! M5StickS3 Plus 演示程序
//!
//! 显示 IMU 数据 + 水平仪 + 麦克风音量条 + WiFi (BtnA 触发)。
//! 展示了 stick-s3 驱动库的标准用法。

use core::fmt::Write;
use embedded_graphics::{
    mono_font::{ascii::FONT_7X13_BOLD, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle, Rectangle},
    text::Text,
};
use embedded_hal::{delay::DelayNs, i2c::I2c as _};
use esp_idf_hal::{delay::Ets, gpio::PinDriver, peripherals::Peripherals};
use stick_s3::{
    buttons::Buttons, display::Display, es8311, framebuffer::Fb,
    i2c_bus::I2cBus, imu::Imu, mic::Mic, pmic, sleep, speaker::Speaker, HEIGHT, WIDTH,
};

const SSID: &str = "your_ssid";
const PASS: &str = "your_password";

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::set_max_level(log::LevelFilter::Info);

    // 配置深睡唤醒源（GPIO12/BtnB 低电平唤醒），必须在最前面
    sleep::config_wakeup();

    let p = Peripherals::take().unwrap();
    let pins = p.pins;
    let mut delay = Ets;

    // ── PMIC + 电源管理 ──
    let i2c = pmic::init_pmic().expect("PMIC init failed");
    let i2c_bus = I2cBus::new(i2c);
    let mut bl = PinDriver::output(pins.gpio38).unwrap();
    bl.set_low().unwrap(); // 初始化完成前先关背光，防唤醒瞬间花屏

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

    // ── 音频电源 + ES8311 编解码器（Mic + Speaker 共享） ──
    {
        let mut i2c = i2c_bus.acquire();
        let _ = i2c.write(pmic::PMIC_ADDR, &[0x16, 0x00]);
        let _ = i2c.write(pmic::PMIC_ADDR, &[0x10, 0x0C]); // GPIO2+GPIO3=输出
        let _ = i2c.write(pmic::PMIC_ADDR, &[0x13, 0x00]); // 推挽
        let _ = i2c.write(pmic::PMIC_ADDR, &[0x11, 0x0C]); // GPIO2=高(背光) GPIO3=高(音频)
        std::thread::sleep(std::time::Duration::from_millis(20));

        if es8311::init_es8311(&mut i2c).is_ok() {
            log::info!("Audio power + ES8311 OK");
        } else {
            log::warn!("ES8311 init failed");
        }
        // NS4168 功放（可选，某些硬件版本没有）
        let _ = i2c.write(0x58, &[0x00, 0x00]).and_then(|_| {
            i2c.write(0x58, &[0x01, 0x1F]).and_then(|_| {
                i2c.write(0x58, &[0x02, 0x01])
            })
        });
    }

    // ── 麦克风 (I2S1 RX) ──
    let mut mic = {
        let mclk: esp_idf_hal::gpio::Gpio18 = unsafe { core::mem::transmute_copy(&pins.gpio18) };
        let bclk: esp_idf_hal::gpio::Gpio17 = unsafe { core::mem::transmute_copy(&pins.gpio17) };
        let ws:   esp_idf_hal::gpio::Gpio15 = unsafe { core::mem::transmute_copy(&pins.gpio15) };
        match Mic::new(p.i2s1, mclk, bclk, ws, pins.gpio16) {
            Ok(m) => { log::info!("Mic OK"); Some(m) }
            Err(e) => { log::warn!("Mic init failed: {}", e); None }
        }
    };
    let mut mic_level: u8 = 0;

    // ── 扬声器 (I2S0 TX) — 暂时禁用（与 Mic 时钟冲突） ──
    let _speaker: Option<Speaker> = None;

    // ── 按键 ──
    let mut btns = Buttons::new(pins.gpio11, pins.gpio12);

    // ── WiFi ──
    let mut wifi_ip = String::from("wifi ready");
    let mut wifi = stick_s3::wifi::Wifi::new(p.modem).ok();
    let mut wifi_connecting = false;
    let mut wifi_connected = false;

    // ── Framebuffer ──
    let mut buf = vec![0u16; (WIDTH * HEIGHT) as usize];
    let mut s = String::with_capacity(128);
    let white = MonoTextStyle::new(&FONT_7X13_BOLD, Rgb565::WHITE);

    // ── 主循环 ──
    loop {
        // 读取传感器
        if let Some(ref mut imu) = imu {
            if let Ok(d) = imu.read_all() {
                imu_data = d;
            }
        }
        if let Some(ref mut mic) = mic {
            let v = mic.read_volume();
            if v > mic_level { mic_level = v; }
            else { mic_level = mic_level.saturating_sub(2); }
        }

        // ── 清屏 + 绘制边框 ──
        buf.fill(0x0000);
        {
            let mut fb = Fb { buf: &mut buf[..] };
            let right = (WIDTH as i32) - 1;
            let bottom = (HEIGHT as i32) - 1;
            let edge = PrimitiveStyle::with_stroke(Rgb565::WHITE, 1);
            let _ = Line::new(Point::new(0, 0), Point::new(right, 0)).into_styled(edge).draw(&mut fb);
            let _ = Line::new(Point::new(0, 0), Point::new(0, bottom)).into_styled(edge).draw(&mut fb);
            let _ = Line::new(Point::new(right, 0), Point::new(right, bottom)).into_styled(edge).draw(&mut fb);
            let _ = Line::new(Point::new(0, bottom), Point::new(right, bottom)).into_styled(edge).draw(&mut fb);

            let mut y = 16i32;

            // ── IMU 数据 ──
            if let Some(ref _imu) = imu {
                s.clear(); let _ = write!(s, "A {:5.3} {:5.3} {:5.3}", imu_data.acc_x, imu_data.acc_y, imu_data.acc_z);
                let _ = Text::new(&s, Point::new(4, y), white).draw(&mut fb); y += 17;
                s.clear(); let _ = write!(s, "G {:5.3} {:5.3} {:5.3}", imu_data.gyr_x, imu_data.gyr_y, imu_data.gyr_z);
                let _ = Text::new(&s, Point::new(4, y), white).draw(&mut fb); y += 17;
                s.clear(); let _ = write!(s, "T {:4.1} C", imu_data.temp);
                let _ = Text::new(&s, Point::new(4, y), white).draw(&mut fb); y += 17;

                // 电池（通过 M5PM1 I2C 读取）
                {
                    let mut i2c = i2c_bus.acquire();
                    let (mv, charging) = stick_s3::battery::Battery::read_all(&mut i2c);
                    let pct = stick_s3::battery::Battery::pct(mv);
                    s.clear();
                    if mv > 0 {
                        let ch = if charging { '+' } else { ' ' };
                        let _ = write!(s, "BAT {:>3}% {:4.1}V{}", pct, mv as f32 / 1000.0, ch);
                    } else {
                        let _ = write!(s, "BAT ---");
                    }
                    let _ = Text::new(&s, Point::new(4, y), white).draw(&mut fb); y += 17;
                }
            }
            y += 4;

            // ── WiFi 状态 ──
            let _ = Text::new(&wifi_ip, Point::new(4, y), white).draw(&mut fb); y += 17;

            // ── 麦克风音量条 ──
            y += 2;
            s.clear(); let _ = write!(s, "MIC {:>3}% ", mic_level);
            let _ = Text::new(&s, Point::new(4, y), white).draw(&mut fb);
            let bar_x = 100; let bar_w = 128u32; let bar_h = 14u32;
            let _ = Rectangle::new(Point::new(bar_x, y - 12), Size::new(bar_w, bar_h))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(64, 64, 64), 1))
                .draw(&mut fb);
            if mic_level > 0 {
                let fill_w = (bar_w as u32) * (mic_level as u32) / 100;
                let color = if mic_level < 50 { Rgb565::GREEN } else if mic_level < 80 { Rgb565::YELLOW } else { Rgb565::RED };
                let _ = Rectangle::new(Point::new(bar_x + 1, y - 11), Size::new(fill_w.saturating_sub(2), bar_h.saturating_sub(2)))
                    .into_styled(PrimitiveStyle::with_fill(color))
                    .draw(&mut fb);
            }

            // ── 右侧水平仪 ──
            let (cx, cy, r) = (200i32, 38i32, 28i32);
            let _ = Circle::new(Point::new(cx - r, cy - r), (r * 2) as u32)
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(0, 180, 255), 1))
                .draw(&mut fb);
            let _ = Line::new(Point::new(cx - r / 2, cy), Point::new(cx + r / 2, cy))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(64, 64, 64), 1)).draw(&mut fb);
            let _ = Line::new(Point::new(cx, cy - r / 2), Point::new(cx, cy + r / 2))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(64, 64, 64), 1)).draw(&mut fb);

            // 水平仪小球
            let pitch = imu_data.acc_x.atan2(imu_data.acc_z.abs());
            let roll  = imu_data.acc_y.atan2(imu_data.acc_z.abs());
            let max_d = (r - 6) as f32;
            let lx = cx + (pitch / std::f32::consts::FRAC_PI_4 * max_d) as i32;
            let ly = cy + (roll  / std::f32::consts::FRAC_PI_4 * max_d) as i32;
            let _ = Circle::new(Point::new(lx - 4, ly - 4), 8)
                .into_styled(PrimitiveStyle::with_fill(Rgb565::YELLOW))
                .draw(&mut fb);
        }

        display.flush(&buf);

        // ── 按键处理 ──
        if btns.btn_a_was_pressed() {
            if let Some(ref mut w) = wifi {
                if wifi_connected || wifi_connecting {
                    wifi_ip = String::from("wifi ready");
                    wifi_connecting = false;
                    wifi_connected = false;
                } else {
                    log::info!("BtnA: connecting WiFi");
                    wifi_ip = String::from("connecting...");
                    wifi_connecting = true;
                    w.start_connect(SSID, PASS);
                }
            }
        }
        if btns.btn_b_was_pressed() {
            // 进入深睡前释放可能阻止休眠的外设
            let mic = mic.take();
            let wifi = wifi.take();
            sleep::enter(
                &mut bl, &mut display,
                || btns.btn_b_is_pressed(),
                move || { drop(mic); drop(wifi); },
            );
            // 不会执行到这里（深睡不返回）
        }

        // ── WiFi 后台轮询 ──
        if wifi_connecting && !wifi_connected {
            if let Some(ref mut w) = wifi {
                if w.ip().map_or(false, |ip| !ip.is_unspecified()) {
                    wifi_connected = true;
                    wifi_connecting = false;
                    wifi_ip = w.ip().map(|i| i.to_string()).unwrap_or_default();
                    log::info!("WiFi OK: {}", wifi_ip);
                }
            }
        }

        delay.delay_ms(100);
    }
}
