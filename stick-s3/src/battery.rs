//! 电池电压监测 — 通过 M5PM1 读取
//!
//! M5StickS3 Plus 的电池连接到 M5PM1 电源管理芯片（I2C 0x6E），
//! 而非 ESP32 的 ADC。通过读取 PMIC 寄存器获取电压和充电状态。
//!
//! 参考 M5Unified Power_Class。
//!
//! ## M5PM1 寄存器
//! - 0x04 PWR_SRC: bit0=5VIN, bit1=5VINOUT, bit2=BAT
//! - 0x06 PWR_CFG: bit0=CHG_EN
//! - 0x22/0x23 VBAT: 电池电压 (小端, mV)
//! - 0x24/0x25 VIN: 5VIN 电压 (小端, mV)

use embedded_hal::i2c::I2c;

const PMIC_ADDR: u8 = 0x6E;

pub struct Battery;

impl Battery {
    /// 读取电池电压 (mV)
    pub fn read_mv<I2C: I2c>(i2c: &mut I2C) -> u32 {
        let mut buf = [0u8; 2];
        if i2c.write_read(PMIC_ADDR, &[0x22], &mut buf).is_ok() {
            let mv = (buf[1] as u32) << 8 | buf[0] as u32;
            if mv > 0 && mv < 5000 { return mv; }
        }
        0
    }

    /// 检测是否正在充电（USB 电源已接入）
    ///
    /// 通过 PWR_SRC(0x04) bit0 判断 5VIN 是否存在。
    /// M5StickS3 Plus 的 GPIO0 未连接充电状态信号，
    /// 改用电源来源寄存器检测更可靠。
    pub fn is_charging<I2C: I2c>(i2c: &mut I2C) -> bool {
        let mut reg = [0u8];
        i2c.write_read(PMIC_ADDR, &[0x04], &mut reg).ok()
            .map(|_| reg[0] & 0x01 != 0) // bit0=5VIN
            .unwrap_or(false)
    }

    /// 估算电池百分比（基于 3.3V~4.15V 范围）
    pub fn pct(mv: u32) -> u32 {
        ((mv as i32 - 3300) * 100 / (4150 - 3300)).clamp(0, 100) as u32
    }
}
