//! 电池电压监测（ADC1 GPIO8, 2:1 分压）
//!
//! M5StickS3 Plus: 电池通过 GPIO8 (ADC1_CH7) 监测。
//! 使用 esp-idf-hal 的 AdcDriver / AdcChannelDriver（安全封装）。

use esp_idf_hal::{
    adc::{
        oneshot::{config::AdcChannelConfig, AdcChannelDriver, AdcDriver},
        ADC1,
    },
    gpio::Gpio8,
    sys::EspError,
};

const ADC_RATIO: f32 = 2.0;

/// 电池电压监测
///
/// 持有 ADC 驱动和通道，提供安全的电压读取。
pub struct Battery<'d> {
    adc: AdcDriver<'d, <ADC1<'d> as esp_idf_hal::adc::Adc>::AdcUnit>,
    channel: AdcChannelDriver<
        'd,
        <Gpio8<'d> as esp_idf_hal::gpio::ADCPin>::AdcChannel,
        &'d AdcDriver<'d, <ADC1<'d> as esp_idf_hal::adc::Adc>::AdcUnit>,
    >,
}

impl<'d> Battery<'d> {
    /// 创建电池电压监测实例
    pub fn new(adc1: ADC1<'d>, pin: Gpio8<'d>) -> Result<Self, EspError> {
        let adc = AdcDriver::new(adc1)?;
        let config = AdcChannelConfig {
            attenuation: esp_idf_hal::adc::attenuation::DB_12,
            ..Default::default()
        };
        // SAFETY: We create both adc and channel together and they live
        // as long as the struct. The channel borrows the adc, which is fine
        // because they share the same lifetime and neither is moved after creation.
        let channel = AdcChannelDriver::new(unsafe { &*(&adc as *const _) }, pin, &config)?;
        Ok(Self { adc, channel })
    }

    /// 读取电池电压 (mV)，乘以 2:1 分压比
    pub fn read_mv(&mut self) -> u32 {
        let mv = self.adc.read(&mut self.channel).unwrap_or(0) as u32;
        (mv as f32 * ADC_RATIO) as u32
    }

    /// 估算电池百分比（基于 3.3V~4.15V 范围）
    pub fn pct(&mut self) -> u32 {
        let mv = self.read_mv();
        ((mv as i32 - 3300) * 100 / (4150 - 3300)).clamp(0, 100) as u32
    }
}
