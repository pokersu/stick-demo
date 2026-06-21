//! M5StickS3 Plus 演示程序 — 真正的 async/await 版本
//!
//! 使用 futures::executor::LocalPool 驱动多个并发异步任务。
//! 每个外设由独立的 async task 管理，通过 shared state 通信。

use core::fmt::Write;
use embedded_graphics::{
    mono_font::{ascii::FONT_7X13_BOLD, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle, Rectangle},
    text::Text,
};
use embedded_hal::i2c::I2c as _;
use esp_idf_hal::{
    delay::Ets, gpio::{Output, PinDriver},
    peripherals::Peripherals,
};
use futures::{
    executor::{LocalPool, LocalSpawner},
    task::LocalSpawnExt,
};
use futures_timer::Delay;
use std::{cell::RefCell, rc::Rc, time::Duration};
use stick_s3::{
    battery::Battery, buttons::Buttons, display::Display, es8311, framebuffer::Fb,
    imu::Imu, i2c_bus::I2cBus, mic::Mic, nvs::Nvs, pmic, provision, sleep,
    wifi::{Wifi, WifiStatus},
    HEIGHT, WIDTH,
};

// ═══════════════════════════════════════════════
//  共享状态
// ═══════════════════════════════════════════════

struct SharedState {
    imu_data: stick_s3::imu::ImuData,
    mic_level: u8,
    wifi_status: String,
    btn_a_count: i32,
    btn_b_count: i32,
}

// ═══════════════════════════════════════════════
//  WiFi 命令通道
// ═══════════════════════════════════════════════

enum WifiCmd { Connect }

// ═══════════════════════════════════════════════
//  Async 任务
// ═══════════════════════════════════════════════

/// IMU 读取任务（每 100ms）
async fn task_imu(i2c_bus: Rc<I2cBus<'static>>, state: Rc<RefCell<SharedState>>) {
    let mut delay = Ets;
    let mut imu = Imu::new(i2c_bus.acquire(), &mut delay).ok();
    loop {
        if let Some(ref mut imu) = imu {
            imu.tick();
            state.borrow_mut().imu_data = *imu.data();
        }
        Delay::new(Duration::from_millis(100)).await;
    }
}

/// 麦克风读取任务（每 100ms）
async fn task_mic(mic: Option<Mic<'static>>, state: Rc<RefCell<SharedState>>) {
    let mut mic = mic;
    loop {
        if let Some(ref mut mic) = mic {
            mic.tick();
            state.borrow_mut().mic_level = mic.volume();
        }
        Delay::new(Duration::from_millis(100)).await;
    }
}

/// WiFi 状态机任务（每 50ms）
async fn task_wifi(
    wifi: Option<Wifi>,
    cmd_rx: std::sync::mpsc::Receiver<WifiCmd>,
    state: Rc<RefCell<SharedState>>,
) {
    let mut wifi = wifi;
    loop {
        // 处理命令
        if let Some(ref mut w) = wifi {
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    WifiCmd::Connect => {
                        match w.status() {
                            WifiStatus::Idle | WifiStatus::Failed(_) => w.start_auto_connect(),
                            _ => w.disconnect(),
                        }
                    }
                }
            }

            // 推进状态机
            w.tick();

            // 更新状态字符串
            let text = match w.status() {
                WifiStatus::Idle => "wifi ready".to_string(),
                WifiStatus::Scanning => "scanning...".to_string(),
                WifiStatus::Connecting(ssid) => format!("connecting {}", ssid),
                WifiStatus::Connected(ssid) => {
                    let ip = w.ip().map(|i| i.to_string()).unwrap_or_default();
                    format!("{} {}", ip, ssid)
                }
                WifiStatus::Failed(msg) => msg.to_string(),
            };
            state.borrow_mut().wifi_status = text;
        }
        Delay::new(Duration::from_millis(50)).await;
    }
}

/// 按键检测任务（每 50ms）
async fn task_buttons(
    mut btns: Buttons<'static>,
    cmd_tx: std::sync::mpsc::Sender<WifiCmd>,
    nvs: Option<Nvs>,
    state: Rc<RefCell<SharedState>>,
) {
    let mut nvs = nvs;
    loop {
        if btns.btn_a_was_pressed() {
            let mut s = state.borrow_mut();
            s.btn_a_count += 1;
            if let Some(ref mut n) = nvs { let _ = n.set_i32("a", s.btn_a_count); }
            let _ = cmd_tx.send(WifiCmd::Connect);
        }
        if btns.btn_b_was_pressed() {
            let mut s = state.borrow_mut();
            s.btn_b_count += 1;
            if let Some(ref mut n) = nvs { let _ = n.set_i32("b", s.btn_b_count); }
            log::info!("Sleep requested — press reset to wake");
        }
        Delay::new(Duration::from_millis(50)).await;
    }
}

/// 渲染任务（每 100ms）
async fn task_render(
    display: Option<Display<'static>>,
    i2c_bus: Rc<I2cBus<'static>>,
    _bl: PinDriver<'static, Output>,
    state: Rc<RefCell<SharedState>>,
) {
    let mut display = display;
    let mut buf = vec![0u16; (WIDTH * HEIGHT) as usize];
    let mut s = String::with_capacity(128);
    let white = MonoTextStyle::new(&FONT_7X13_BOLD, Rgb565::WHITE);

    loop {
        {
            let st = state.borrow();
            let d = &st.imu_data;
            let mic_level = st.mic_level;
            let btn_sum = st.btn_a_count + st.btn_b_count;

            // ── 清屏 + 边框 ──
            buf.fill(0x0000);
            let mut fb = Fb { buf: &mut buf[..] };
            let right = (WIDTH as i32) - 1;
            let bottom = (HEIGHT as i32) - 1;
            let edge = PrimitiveStyle::with_stroke(Rgb565::WHITE, 1);
            let _ = Line::new(Point::new(0, 0), Point::new(right, 0)).into_styled(edge).draw(&mut fb);
            let _ = Line::new(Point::new(0, 0), Point::new(0, bottom)).into_styled(edge).draw(&mut fb);
            let _ = Line::new(Point::new(right, 0), Point::new(right, bottom)).into_styled(edge).draw(&mut fb);
            let _ = Line::new(Point::new(0, bottom), Point::new(right, bottom)).into_styled(edge).draw(&mut fb);

            let mut y = 16i32;

            // ── IMU ──
            s.clear(); let _ = write!(s, "A {:5.3} {:5.3} {:5.3}", d.acc_x, d.acc_y, d.acc_z);
            let _ = Text::new(&s, Point::new(4, y), white).draw(&mut fb); y += 17;
            s.clear(); let _ = write!(s, "G {:5.3} {:5.3} {:5.3}", d.gyr_x, d.gyr_y, d.gyr_z);
            let _ = Text::new(&s, Point::new(4, y), white).draw(&mut fb); y += 17;
            s.clear(); let _ = write!(s, "T {:4.1} C", d.temp);
            let _ = Text::new(&s, Point::new(4, y), white).draw(&mut fb); y += 17;

            // ── 电池 ──
            {
                let mut i2c = i2c_bus.acquire();
                let (mv, charging) = Battery::read_all(&mut i2c);
                let pct = Battery::pct(mv);
                s.clear();
                if mv > 0 {
                    let ch = if charging { '+' } else { ' ' };
                    let _ = write!(s, "BAT {:>3}% {:4.1}V{}", pct, mv as f32 / 1000.0, ch);
                } else {
                    let _ = write!(s, "BAT ---");
                }
                let _ = Text::new(&s, Point::new(4, y), white).draw(&mut fb); y += 17;
            }
            y += 4;

            // ── WiFi ──
            let _ = Text::new(&st.wifi_status, Point::new(4, y), white).draw(&mut fb); y += 17;

            // ── 麦克风 ──
            y += 2;
            s.clear(); let _ = write!(s, "MIC {:>3}% ", mic_level);
            let _ = Text::new(&s, Point::new(4, y), white).draw(&mut fb);
            let bar_x = 100; let bar_w = 128u32; let bar_h = 14u32;
            let _ = Rectangle::new(Point::new(bar_x, y - 12), Size::new(bar_w, bar_h))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(64, 64, 64), 1)).draw(&mut fb);
            if mic_level > 0 {
                let fill_w = bar_w * (mic_level as u32) / 100;
                let color = if mic_level < 50 { Rgb565::new(0, 255, 0) } else if mic_level < 80 { Rgb565::new(255, 255, 0) } else { Rgb565::new(255, 0, 0) };
                let _ = Rectangle::new(Point::new(bar_x + 1, y - 11), Size::new(fill_w.saturating_sub(2), bar_h.saturating_sub(2)))
                    .into_styled(PrimitiveStyle::with_fill(color)).draw(&mut fb);
            }

            // ── NVS 按键计数 ──
            y += 17;
            s.clear(); let _ = write!(s, "NVS {:>3}", btn_sum);
            let _ = Text::new(&s, Point::new(4, y), white).draw(&mut fb);

            // ── 水平仪 ──
            let (cx, cy, r) = (200i32, 38i32, 28i32);
            let _ = Circle::new(Point::new(cx - r, cy - r), (r * 2) as u32)
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 1)).draw(&mut fb);
            let _ = Circle::new(Point::new(cx - 7, cy - 7), 14)
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(0, 255, 0), 1)).draw(&mut fb);
            let _ = Line::new(Point::new(cx - r / 2, cy), Point::new(cx + r / 2, cy))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(64, 64, 64), 1)).draw(&mut fb);
            let _ = Line::new(Point::new(cx, cy - r / 2), Point::new(cx, cy + r / 2))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(64, 64, 64), 1)).draw(&mut fb);

            let pitch = d.acc_x.atan2(d.acc_z.abs());
            let roll  = d.acc_y.atan2(d.acc_z.abs());
            let max_d = (r - 6) as f32;
            let lx = cx + (pitch / std::f32::consts::FRAC_PI_4 * max_d) as i32;
            let ly = cy + (roll  / std::f32::consts::FRAC_PI_4 * max_d) as i32;
            let ball_color = if (lx - cx).pow(2) + (ly - cy).pow(2) <= 49 { Rgb565::new(0, 255, 0) } else { Rgb565::YELLOW };
            let _ = Circle::new(Point::new(lx - 4, ly - 4), 8)
                .into_styled(PrimitiveStyle::with_fill(ball_color)).draw(&mut fb);
        }

        // flush 在 state 作用域外（不持有 borrow）
        if let Some(ref mut display) = display {
            display.flush(&mut buf);
        }

        Delay::new(Duration::from_millis(100)).await;
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
        let _ = i2c.write(0x58, &[0x00, 0x00]).and_then(|_| {
            i2c.write(0x58, &[0x01, 0x1F]).and_then(|_| i2c.write(0x58, &[0x02, 0x01]))
        });
    }

    // ── 麦克风 ──
    let mic = {
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
    let btn_a_count = nvs.as_ref().and_then(|n| n.get_i32("a")).unwrap_or(0);
    let btn_b_count = nvs.as_ref().and_then(|n| n.get_i32("b")).unwrap_or(0);
    log::info!("NVS: BtnA={} BtnB={}", btn_a_count, btn_b_count);

    // ── 按键 ──
    let btns = Buttons::new(pins.gpio11, pins.gpio12);

    // ── WiFi ──
    let wifi = stick_s3::wifi::Wifi::new(p.modem).ok();

    // ═══════════════════════════════════════════
    //  异步运行时
    // ═══════════════════════════════════════════

    let state = Rc::new(RefCell::new(SharedState {
        imu_data: stick_s3::imu::ImuData::default(),
        mic_level: 0,
        wifi_status: String::from("wifi ready"),
        btn_a_count,
        btn_b_count,
    }));

    let (wifi_cmd_tx, wifi_cmd_rx) = std::sync::mpsc::channel();

    let mut pool = LocalPool::new();
    let spawner: LocalSpawner = pool.spawner();

    // 异步任务不需要 Handle 返回值，run() 会一直运行
    let _ = spawner.spawn_local(task_imu(i2c_bus.clone(), state.clone()));
    let _ = spawner.spawn_local(task_mic(mic, state.clone()));
    let _ = spawner.spawn_local(task_wifi(wifi, wifi_cmd_rx, state.clone()));
    let _ = spawner.spawn_local(task_buttons(btns, wifi_cmd_tx, nvs, state.clone()));
    let _ = spawner.spawn_local(task_render(Some(display), i2c_bus, bl, state));

    log::info!("Async runtime started — {} tasks", 5);
    pool.run(); // 永远不返回
}
