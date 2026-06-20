//! 电池电压监测 — 通过 M5PM1 读取
//!
//! M5StickS3 Plus 的电池连接到 M5PM1 电源管理芯片（I2C 0x6E），
//! 而非 ESP32 的 ADC。通过读取 PMIC 寄存器获取电压和充电状态。
//!
//! ## 坑点
//! - **电池不是接 ESP32 ADC**，是通过 M5PM1 读取 (0x22/0x23)
//! - **GPIO0 没有连接充电状态信号**（M5StickS3 Plus 硬件差异），改用 PWR_SRC 判断
//! - **充电状态**通过 0x04 bit0 (5VIN) 判断有 USB 即充电
//!
//! 参考 M5Unified Power_Class。

use embedded_hal::i2c::I2c;

const PMIC_ADDR: u8 = 0x6E;

pub struct Battery;

impl Battery {
    /// 一次 I2C 事务读取电压(mV) + 充电状态
    pub fn read_all<I2C: I2c>(i2c: &mut I2C) -> (u32, bool) {
        let mut pwr = [0u8];
        let mut vbat = [0u8; 2];
        if i2c.write_read(PMIC_ADDR, &[0x04], &mut pwr).is_err()
            || i2c.write_read(PMIC_ADDR, &[0x22], &mut vbat).is_err()
        {
            return (0, false);
        }
        // ⚠ M5PM1 寄存器是小端格式: (VBAT_H << 8) | VBAT_L
        let mv = (vbat[1] as u32) << 8 | vbat[0] as u32;
        // ⚠ 过滤异常值（I2C 错误时可能读到 0xFFFF）
        let mv = if mv > 0 && mv < 5000 { mv } else { 0 };
        // ⚠ 充电判断：PWR_SRC(0x04) bit0=5VIN
        // M5Unified 用 GPIO0 引脚判断，但 Plus 硬件上该引脚未连接
        (mv, (pwr[0] & 0x01) != 0)
    }

    /// 读取电池电压 (mV)
    pub fn read_mv<I2C: I2c>(i2c: &mut I2C) -> u32 { Self::read_all(i2c).0 }
    /// 检测是否正在充电（5VIN 有电）
    pub fn is_charging<I2C: I2c>(i2c: &mut I2C) -> bool { Self::read_all(i2c).1 }

    /// 估算电池百分比（基于 3.3V~4.15V 范围）
    pub fn pct(mv: u32) -> u32 {
        ((mv as i32 - 3300) * 100 / (4150 - 3300)).clamp(0, 100) as u32
    }
}
