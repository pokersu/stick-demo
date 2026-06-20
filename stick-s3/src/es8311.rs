//! ES8311 音频编解码器初始化（M5Unified 移植）
//!
//! I2C 地址 0x18，SDA=47, SCL=48。
//! 合并了 Speaker (DAC) 和 Mic (ADC) 的配置。
//!
//! ## 坑点
//! - **RESET(0x00=0x80) 只能做一次**，后一次会冲掉前一次配置
//! - **寄存器 0x01** Speaker=0xB5, Mic=0xBA — 当前使用 Mic 时钟 0xBA
//! - **Speaker 和 Mic 同时使用时**，I2S0 和 I2S1 共享 BCK/WS 引脚，
//!   不能用不同采样率同时激活（当前方案：只激活 Mic, Speaker 注释掉）

const ES8311_ADDR: u8 = 0x18;

/// ES8311 初始化序列
///
/// ⚠ RESET 只做一次！如果在 Speaker 和 Mic 各做一次，后一次冲掉前一次。
/// ⚠ 0x01: Speaker=0xB5, Mic=0xBA，差异见 M5Unified。
///   合并配置优先用 Mic 时钟 (0xBA)，因为 Mic 是当前主要功能。
const ES8311_INIT: &[(u8, u8)] = &[
    (0x00, 0x80), // RESET / CSM POWER ON
    (0x01, 0xBA), // CLOCK_MANAGER / MCLK=BCLK
    (0x02, 0x18), // CLOCK_MANAGER / MULT_PRE=3
    (0x0D, 0x01), // SYSTEM / Power up analog circuitry
    // DAC 路径 (Speaker)
    (0x12, 0x00), // SYSTEM / power-up DAC
    (0x13, 0x10), // SYSTEM / Enable output to HP drive
    (0x32, 0xBF), // DAC / DAC volume (±0 dB)
    (0x37, 0x08), // DAC / Bypass DAC equalizer
    // ADC 路径 (Mic)
    (0x0E, 0x02), // SYSTEM / Enable analog PGA, enable ADC modulator
    (0x14, 0x10), // select Mic1p-Mic1n / PGA GAIN (minimum)
    (0x17, 0xFF), // ADC_VOLUME (MAXGAIN)
    (0x1C, 0x6A), // ADC Equalizer bypass, cancel DC offset
];

pub fn init_es8311<I2C: embedded_hal::i2c::I2c>(i2c: &mut I2C) -> Result<(), &'static str> {
    for &(reg, val) in ES8311_INIT {
        i2c.write(ES8311_ADDR, &[reg, val]).map_err(|_| "ES8311 I2C")?;
    }
    log::info!("ES8311 initialized");
    Ok(())
}
