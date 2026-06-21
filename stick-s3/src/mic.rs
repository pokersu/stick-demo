//! 麦克风驱动（M5StickS3 内置，ES8311 + I2S1 RX）
//!
//! 硬件：I2S1, MCK=18, BCK=17, WS=15, DATA=16
//! ES8311 初始化由外部完成，本模块仅配置 I2S RX。
//!
//! ## 坑点
//! - **I2S1 与 I2S0 (Speaker) 共享 BCK=17, WS=15, MCK=18**，
//!   同时激活时采样率必须一致，否则时钟冲突
//! - **ES8311 的 ADC 需要先初始化**（由外部调用 es8311::init_es8311）

use esp_idf_hal::{
    gpio::{InputPin, OutputPin},
    i2s::{
        config::{Config, DataBitWidth, SlotMode, StdClkConfig, StdConfig, StdGpioConfig, StdSlotConfig},
        I2sDriver, I2sRx,
    },
};

/// 麦克风音量 0-100 — 每帧调用 tick() 更新缓存
pub struct Mic<'d> {
    rx: I2sDriver<'d, I2sRx>,
    level: u8,
}

impl<'d> Mic<'d> {
    /// 创建麦克风驱动（需要先外部初始化 ES8311）
    pub fn new(
        i2s: impl esp_idf_hal::i2s::I2s + 'd,
        mclk: impl OutputPin + InputPin + 'd,
        bclk: impl OutputPin + InputPin + 'd,
        ws: impl OutputPin + InputPin + 'd,
        din: impl InputPin + 'd,
    ) -> Result<Self, &'static str> {
        let slot_cfg = StdSlotConfig::philips_slot_default(
            DataBitWidth::Bits16, SlotMode::Mono,
        );
        let clk_cfg = StdClkConfig::from_sample_rate_hz(16000);
        let gpio_cfg = StdGpioConfig::default();
        let std_cfg = StdConfig::new(Config::default(), clk_cfg, slot_cfg, gpio_cfg);

        let mut rx = I2sDriver::new_std_rx(i2s, &std_cfg, bclk, din, Some(mclk), ws)
            .map_err(|_| "I2S RX")?;
        rx.rx_enable().ok();
        Ok(Self { rx, level: 0 })
    }

    /// 每帧调用 — 读取音量并缓存最新值（带自然衰减）
    pub fn tick(&mut self) {
        let v = self.read_volume();
        if v > self.level {
            self.level = v;
        } else {
            self.level = self.level.saturating_sub(2);
        }
    }

    /// 获取缓存的最新音量 (0-100)
    pub fn volume(&self) -> u8 { self.level }

    /// 读取麦克风音量 (0-100)，基于峰值
    pub fn read_volume(&mut self) -> u8 {
        let mut buf = [0u8; 128];
        let n = self.rx.read(&mut buf, 0u32).unwrap_or(0);
        if n < 4 { return 0; }
        let mut max_abs: u16 = 0;
        for chunk in buf[..n].chunks_exact(2) {
            let s = i16::from_le_bytes([chunk[0], chunk[1]]);
            let a = s.unsigned_abs();
            if a > max_abs { max_abs = a; }
        }
        (max_abs as u32 * 100 / 16000).min(100) as u8  // 灵敏度: 满量程的 ~50% 即 100%
    }
}
