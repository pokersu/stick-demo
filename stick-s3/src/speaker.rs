//! 扬声器驱动（ES8311 音频编解码 + I2S0 TX）
//!
//! 硬件：I2S0, MCK=18, BCK=17, WS=15, DATA=14
//! ES8311 + NS4168 初始化由外部完成。
//!
//! ## 坑点
//! - **I2S0 与 I2S1 (Mic) 共享 BCK=17, WS=15, MCK=18**，
//!   同时激活时采样率必须一致，否则时钟冲突
//! - **NS4168 功放初始化**由外部通过 I2C 完成（不是所有硬件版本都有）

use esp_idf_hal::{
    gpio::{InputPin, OutputPin},
    i2s::{
        config::{DataBitWidth, SlotMode, StdClkConfig, StdConfig, StdGpioConfig, StdSlotConfig},
        I2sDriver, I2sTx,
    },
};

const SAMPLE_RATE: u32 = 44100;

pub struct Speaker<'d> {
    tx: I2sDriver<'d, I2sTx>,
}

impl<'d> Speaker<'d> {
    pub fn new(
        i2s: impl esp_idf_hal::i2s::I2s + 'd,
        mclk: impl OutputPin + InputPin + 'd,
        bclk: impl OutputPin + InputPin + 'd,
        ws: impl OutputPin + InputPin + 'd,
        data: impl OutputPin + 'd,
    ) -> Result<Self, &'static str> {
        let slot_cfg = StdSlotConfig::philips_slot_default(
            DataBitWidth::Bits16, SlotMode::Stereo,
        );
        let clk_cfg = StdClkConfig::from_sample_rate_hz(SAMPLE_RATE);
        let gpio_cfg = StdGpioConfig::default();
        let std_cfg = StdConfig::new(
            esp_idf_hal::i2s::config::Config::default(),
            clk_cfg, slot_cfg, gpio_cfg,
        );

        let tx = I2sDriver::new_std_tx(i2s, &std_cfg, bclk, data, Some(mclk), ws)
            .map_err(|_| "I2S TX")?;

        Ok(Self { tx })
    }

    /// 播放滴声（阻塞，50ms，1kHz）
    pub fn beep(&mut self) {
        self.tx.tx_enable().ok();
        let total = (SAMPLE_RATE * 50 / 1000) as usize;
        let cycle = (SAMPLE_RATE / 1000) as usize;
        let mut buf = [0u8; 256];
        let mut w = 0;
        while w < total * 4 {
            let n = (total * 4 - w).min(256);
            for i in (0..n).step_by(4) {
                let v: i16 = if (w / 4 + i / 4) % cycle < cycle / 2 { 12000 } else { -12000 };
                let l = v.to_le_bytes();
                buf[i] = l[0]; buf[i+1] = l[1];
                buf[i+2] = l[0]; buf[i+3] = l[1];
            }
            let _ = self.tx.write(&buf[..n], esp_idf_hal::delay::BLOCK);
            w += n;
        }
        self.tx.tx_disable().ok();
    }
}

impl Drop for Speaker<'_> {
    fn drop(&mut self) {
        let _ = self.tx.tx_disable();
    }
}
