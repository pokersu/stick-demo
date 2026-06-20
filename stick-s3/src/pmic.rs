//! PMIC (M5PM1) 电源管理 — I2C 地址 0x6E
//!
//! M5StickS3 Plus 的背光电源由 M5PM1 控制，PMIC GPIO2 必须输出高电平。
//!
//! ## 坑点
//! - **0x11(GPIO_OUT) 必须读-改-写**，全字节覆写会关掉背光 GPIO2
//! - **0x09(I2C_CFG) 必须写 0x00** 禁用空闲休眠，否则 PMIC 半小时后不通
//! - **ID=0xff 正常**（M5STACK 定制芯片固件），不是故障
//! - **GPIO 默认输出为开漏**，必须先设 0x13=0x00 为推挽才能正常输出电平

use esp_idf_hal::{
    gpio::{Gpio47, Gpio48},
    i2c::{I2cConfig, I2cDriver, I2C0, I2C1},
};

const ADDR: u8 = 0x6E;
const TMO: u32 = 10;
pub const PMIC_ADDR: u8 = ADDR;

/// 初始化 M5PM1：使能背光电源 + 设置电源保持
pub fn init_pmic() -> Option<I2cDriver<'static>> {
    probe_i2c0().or_else(|| probe_i2c1())
}

macro_rules! make_probe {
    ($name:ident, $i2c:ty) => {
        fn $name() -> Option<I2cDriver<'static>> {
            let mut i2c = unsafe {
                I2cDriver::new(
                    <$i2c>::steal(), Gpio47::steal(), Gpio48::steal(),
                    &I2cConfig::new().baudrate(100_000_u32.into()),
                )
            }.ok()?;
            let mut id = [0u8];
            i2c.write_read(ADDR, &[0x00], &mut id, TMO).ok()?;
            // ⚠ PMIC ID=0xff 是正常的（M5STACK 定制固件），不要误判为错误
            log::info!("PMIC found (ID=0x{:02x})", id[0]);

            for (reg, val) in &[
                (0x07, 0x05), // HOLD_CFG: 保持 GPIO0 + GPIO2（NeoPixel + 背光）
                // ⚠ 0x06=0x17: 保留全部默认（充电 + DCDC + LDO + LED），不改 CHG_EN
                (0x06, 0x17),
                // ⚠ 0x09=0x00: 禁用 I2C 空闲休眠！否则 PMIC 半小时后断连
                (0x09, 0x00),
                (0x16, 0x00), // GPIO0/1/2/3 = GPIO 功能
                // ⚠ 0x10=0x04: 全字节覆写，保留 GPIO0=输入(bit0=0), GPIO1=输入(bit1=0)
                // GPIO2=输出(bit2=1), GPIO3=输出(bit3=1)
                (0x10, 0x04),
                // ⚠ GPIO 默认输出类型为开漏！必须设 0x13=0x00 为推挽才能输出高电平
                (0x13, 0x00),
                // ⚠ 0x11=0x04: 全字节覆写会关 GPIO2！0x04 确保 GPIO2=高(背光亮), GPIO3=低
                // 如果改成 0x0C 则 GPIO2 和 GPIO3 同时为高
                (0x11, 0x04),
                // 屏蔽所有 IRQ
                (0x43, 0x1F),
                (0x45, 0x07),
            ] {
                let _ = i2c.write(ADDR, &[*reg, *val], TMO);
            }

            log::info!("PMIC initialized (backlight + hold)");
            Some(i2c)
        }
    };
}

make_probe!(probe_i2c0, I2C0);
make_probe!(probe_i2c1, I2C1);
