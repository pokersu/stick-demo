//! BMI270 IMU 驱动 (I2C)
//!
//! I2C 地址: 0x68, CHIP_ID: 0x24
//!
//! ## 坑点 (Pitfalls)
//!
//! - **GYR_CONF=0x44 会导致陀螺仪无数据** — 其 ODR 字段 0x04 是 reserved 无效值
//! - **配置加载必须用 INIT_ADDR(0x5B/0x5C) + INIT_DATA(0x5E)** — 0x50 不是 INIT_ADDR
//! - **必须等待 INTERNAL_STATUS(0x21)==0x01** 确认初始化完成，否则数据寄存器无效
//! - **温度传感器**在 gyr_en=1 时自动使能，100Hz 更新，初始值 0x8000 无效
//!
//! 参考: BMI270 数据手册, Bosch 官方驱动, sticks3-toolkit

use embedded_hal::{delay::DelayNs, i2c::I2c};

mod cfg { include!("bmi270_config.rs"); }

#[derive(Debug, Clone, Copy, Default)]
pub struct ImuData {
    pub acc_x: f32, pub acc_y: f32, pub acc_z: f32,
    pub gyr_x: f32, pub gyr_y: f32, pub gyr_z: f32,
    pub temp: f32,
}

pub struct Imu<I2C: I2c> { i2c: I2C, addr: u8 }

impl<I2C: I2c> Imu<I2C> {
    pub fn new(mut i2c: I2C, delay: &mut impl DelayNs) -> Result<Self, Error<I2C::Error>> {
        let addr = probe(&mut i2c).ok_or(Error::Custom("BMI270 not found"))?;

        // 1. 软复位
        i2c.write(addr, &[0x7E, 0xB6]).map_err(Error::I2c)?;
        delay.delay_ms(100);

        // 2. 加载配置前先关省电（数据手册必做步骤）
        i2c.write(addr, &[0x7C, 0x00]).map_err(Error::I2c)?;
        delay.delay_ms(5);

        // 3. 进入配置加载模式
        i2c.write(addr, &[0x59, 0x00]).map_err(Error::I2c)?;
        delay.delay_ms(5);

        // 4. 分页加载配置（256页 × 32字节）
        // 每页前先通过 INIT_ADDR_0(0x5B)/INIT_ADDR_1(0x5C) 设置写入地址
        // 地址 = page * 16 words (32 bytes = 16 words)
        for page in 0..256usize {
            let off = page * 32;
            // 设置 INIT_ADDR: 32字节 = 16 words, 地址递增单位为 word
            i2c.write(addr, &[0x5B, 0x00]).map_err(Error::I2c)?; // bits 0-3 = 0
            i2c.write(addr, &[0x5C, page as u8]).map_err(Error::I2c)?; // bits 4-11 = page
            // 写入 32 字节到 INIT_DATA (0x5E)
            let mut buf = [0u8; 33];
            buf[0] = 0x5E;
            buf[1..33].copy_from_slice(&cfg::BMI270_CONFIG[off..off + 32]);
            i2c.write(addr, &buf).map_err(Error::I2c)?;
        }
        log::info!("BMI270 config loaded (256 pages)");

        // 5. 触发内部初始化
        i2c.write(addr, &[0x59, 0x01]).map_err(Error::I2c)?;

        // 6. 等待初始化完成（INTERNAL_STATUS.msg == 0x01 init_ok）
        let mut init_ok = false;
        for _ in 0..100 {
            delay.delay_ms(1);
            let mut st = [0u8];
            let _ = i2c.write_read(addr, &[0x21], &mut st);
            if st[0] & 0x0F == 0x01 {
                init_ok = true;
                break;
            }
        }
        if !init_ok {
            // 读出最终状态用于调试
            let mut st = [0u8];
            let _ = i2c.write_read(addr, &[0x21], &mut st);
            log::warn!("BMI270 init FAILED: INTERNAL_STATUS=0x{:02X}", st[0]);
            return Err(Error::Custom("BMI270 init failed"));
        }
        log::info!("BMI270 init OK");

        // 7. 再次关闭省电（init 后进入 configuration mode，但确保）
        i2c.write(addr, &[0x7C, 0x00]).map_err(Error::I2c)?;
        delay.delay_ms(5);

        // 8. 使能 acc + gyr + temp
        i2c.write(addr, &[0x7D, 0x0E]).map_err(Error::I2c)?;
        delay.delay_ms(50);

        // 9. 配置 acc: 100Hz, normal mode, performance filter, ±16g
        i2c.write(addr, &[0x40, 0xA8]).map_err(Error::I2c)?; // ACC_CONF
        i2c.write(addr, &[0x41, 0x03]).map_err(Error::I2c)?; // ACC_RANGE: ±16g
        // 10. 配置 gyr: 200Hz, normal mode, performance filter, ±2000°/s
        i2c.write(addr, &[0x42, 0xA9]).map_err(Error::I2c)?; // GYR_CONF
        i2c.write(addr, &[0x43, 0x00]).map_err(Error::I2c)?; // GYR_RANGE
        delay.delay_ms(50);

        // 11. 等待数据就绪（最多等 100ms）
        for _ in 0..10 {
            let mut st = [0u8];
            let _ = i2c.write_read(addr, &[0x03], &mut st);
            if st[0] & 0xC0 != 0 { break; } // drdy_acc | drdy_gyr
            delay.delay_ms(10);
        }

        // 12. 试读一次
        let mut r = [0u8; 12];
        i2c.write_read(addr, &[0x0C], &mut r).map_err(Error::I2c)?;
        log::info!("BMI270 raw=[{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}]",
            r[0], r[1], r[2], r[3], r[4], r[5]);

        Ok(Self { i2c, addr })
    }

    pub fn read_all(&mut self) -> Result<ImuData, Error<I2C::Error>> {
        // 先检查 STATUS 寄存器是否有数据就绪
        let mut st = [0u8];
        self.i2c.write_read(self.addr, &[0x03], &mut st).map_err(Error::I2c)?;
        if st[0] & 0xC0 == 0 { // drdy_acc(bit7) == 0 && drdy_gyr(bit6) == 0
            return Err(Error::Custom("no data ready"));
        }

        let mut buf = [0u8; 12];
        self.i2c.write_read(self.addr, &[0x0C], &mut buf).map_err(Error::I2c)?;
        // 读 STATUS 清除标志后，再确认数据非零
        if buf.iter().all(|&b| b == 0) {
            return Err(Error::Custom("all zero"));
        }
        let ax = i16::from_le_bytes([buf[0], buf[1]]) as f32;
        let ay = i16::from_le_bytes([buf[2], buf[3]]) as f32;
        let az = i16::from_le_bytes([buf[4], buf[5]]) as f32;
        let gx = i16::from_le_bytes([buf[6], buf[7]]) as f32;
        let gy = i16::from_le_bytes([buf[8], buf[9]]) as f32;
        let gz = i16::from_le_bytes([buf[10], buf[11]]) as f32;
        // 读温度（0x22 TEMPERATURE_0_LSB, 0x23 TEMPERATURE_1_MSB）
        let mut tbuf = [0u8; 2];
        let _ = self.i2c.write_read(self.addr, &[0x22], &mut tbuf);
        let temp_raw = i16::from_le_bytes(tbuf);
        let temp = if temp_raw == i16::MIN { 0.0 } else { 23.0 + temp_raw as f32 / 512.0 };

        Ok(ImuData {
            // ±16g → 2048 LSB/g
            acc_x: ax / 2048.0 * 9.80665,
            acc_y: ay / 2048.0 * 9.80665,
            acc_z: az / 2048.0 * 9.80665,
            // ±2000°/s → 16.4 LSB/°/s
            gyr_x: gx / 16.4, gyr_y: gy / 16.4, gyr_z: gz / 16.4,
            temp,
        })
    }
}

fn probe<I2C: I2c>(i2c: &mut I2C) -> Option<u8> {
    for addr in &[0x68, 0x69] {
        let mut id = [0u8];
        if i2c.write_read(*addr, &[0x00], &mut id).is_ok() && id[0] == 0x24 {
            return Some(*addr);
        }
    }
    None
}

#[derive(Debug)]
pub enum Error<E> { I2c(E), Custom(&'static str) }
