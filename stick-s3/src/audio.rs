//! NS4168 I2S 扬声器 + PDM 麦克风驱动
//!
//! ## 硬件
//!
//! - **扬声器**: NS4168 I2S Class-D 功放（I2C 地址 0x58，可选配置）
//!   - BCLK=2, LRCK=14, DATA=39, EN=46
//! - **麦克风**: PDM 数字麦克风
//!   - CLK=35, DATA=34
//!
//! ## 用法
//!
//! ```ignore
//! let mut audio = Audio::new(i2s_tx, i2s_rx, pins.gpio46);
//! audio.play_tone(440, 200);
//! ```

use esp_idf_hal::{
    gpio::{Output, OutputPin, PinDriver},
    i2s::{I2sDriver, I2sRx, I2sTx},
};

/// 扬声器采样率
pub const SAMPLE_RATE: u32 = 24_000;

/// NS4168 I2C 地址
const NS4168_ADDR: u8 = 0x58;

/// 音频驱动（扬声器 + 麦克风）
pub struct Audio<'d> {
    i2s_tx: I2sDriver<'d, I2sTx>,
    i2s_rx: I2sDriver<'d, I2sRx>,
    spk_en: PinDriver<'d, Output>,
}

impl<'d> Audio<'d> {
    /// 创建音频驱动
    ///
    /// 接收预先构建好的 I2S TX（扬声器）和 I2S RX（PDM 麦克风）驱动，
    /// 以及扬声器使能引脚。
    pub fn new(
        i2s_tx: I2sDriver<'d, I2sTx>,
        i2s_rx: I2sDriver<'d, I2sRx>,
        spk_en: impl OutputPin + 'd,
    ) -> Self {
        let mut spk_en = PinDriver::output(spk_en).unwrap();
        spk_en.set_high().unwrap(); // 使能扬声器
        Self { i2s_tx, i2s_rx, spk_en }
    }

    /// 通过 I2C 配置 NS4168（音量等）
    pub fn init_ns4168<I2C: embedded_hal::i2c::I2c>(
        &self,
        i2c: &mut I2C,
        volume: u8,
    ) -> Result<(), I2C::Error> {
        let vol = volume.min(0x1F);
        i2c.write(NS4168_ADDR, &[0x00, 0x00])?;
        i2c.write(NS4168_ADDR, &[0x01, vol])?;
        i2c.write(NS4168_ADDR, &[0x02, 0x01])?;
        i2c.write(NS4168_ADDR, &[0x03, 0x00])?;
        Ok(())
    }

    /// 播放单音（阻塞）
    pub fn play_tone(&mut self, freq: u32, dur_ms: u32) {
        self.i2s_tx.tx_enable().ok();
        let total = (SAMPLE_RATE * dur_ms / 1000) as usize;
        let cycle = (SAMPLE_RATE / freq) as usize;
        let mut buf = [0u8; 256];
        let mut w = 0;
        while w < total * 4 {
            let n = (total * 4 - w).min(256);
            for i in (0..n).step_by(4) {
                let v: i16 = if (w / 4 + i / 4) % cycle < cycle / 2 {
                    8000
                } else {
                    -8000
                };
                let l = v.to_le_bytes();
                buf[i] = l[0]; buf[i + 1] = l[1];
                buf[i + 2] = l[0]; buf[i + 3] = l[1];
            }
            self.i2s_tx.write(&buf[..n], esp_idf_hal::delay::BLOCK).ok();
            w += n;
        }
        self.i2s_tx.tx_disable().ok();
    }

    /// 读取麦克风峰值电平 (0-100%)
    ///
    /// 从 PDM RX 读取一小段样本，计算最大幅度并映射到百分比。
    pub fn read_mic_peak(&mut self) -> u8 {
        self.i2s_rx.rx_enable().ok();
        let mut buf = [0u8; 64];
        let n = self.i2s_rx.read(&mut buf, 0u32).unwrap_or(0);
        if n < 4 {
            return 0;
        }
        // 从 16-bit 样本中找最大绝对值
        let mut max_abs: u16 = 0;
        for chunk in buf[..n].chunks_exact(2) {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            let abs = sample.unsigned_abs();
            if abs > max_abs {
                max_abs = abs;
            }
        }
        // i16 最大值为 32768，映射到 0-100
        (max_abs as u32 * 100 / 32768) as u8
    }

    /// 关闭扬声器
    pub fn disable_speaker(&mut self) {
        self.spk_en.set_low().ok();
        self.i2s_tx.tx_disable().ok();
    }

    /// 开启扬声器
    pub fn enable_speaker(&mut self) {
        self.spk_en.set_high().ok();
    }
}

impl Drop for Audio<'_> {
    fn drop(&mut self) {
        let _ = self.spk_en.set_low();
    }
}
