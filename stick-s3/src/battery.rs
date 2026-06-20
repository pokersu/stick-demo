//! 电池电压监测 — 通过 M5PM1 读取
//!
//! M5StickS3 Plus 的电池连接到 M5PM1 电源管理芯片（I2C 0x6E），
//! 而非 ESP32 的 ADC。通过读取 PMIC 寄存器获取电压和充电状态。
//!
//! 参考 M5Unified Power_Class。
//!
//! ## M5PM1 寄存器
//! - 0x04 PWR_SRC: bit0=5VIN, bit1=5VINOUT, bit2=BAT
//! - 0x22/0x23 VBAT: 电池电压 (小端, mV)
//!
//! ## 用法
//! ```ignore
//! let (mv, chg) = Battery::read_all(&mut i2c);
//! let pct = Battery::pct(mv);
//! ```

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
        let mv = (vbat[1] as u32) << 8 | vbat[0] as u32;
        let mv = if mv > 0 && mv < 5000 { mv } else { 0 };
        (mv, (pwr[0] & 0x01) != 0) // bit0=5VIN
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
