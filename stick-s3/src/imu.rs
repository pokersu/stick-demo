//! BMI270 IMU 驱动 (I2C)
//!
//! I2C 地址: 0x68, CHIP_ID: 0x24
//!
//! ## 坑点
//! - **GYR_CONF=0x44 → 陀螺仪无数据** — ODR=0x04 是 reserved 无效值
//! - **配置加载必须用 0x5B/0x5C+0x5E**，不是 0x50
//! - **必须等 INTERNAL_STATUS(0x21)==0x01** 才可操作数据寄存器
//! - **温度初始值 0x8000** 无效，需等第一次更新
//! - **ACC_RANGE=3** 时缩放因子为 2048 LSB/g，不是 16384

use embedded_hal::{delay::DelayNs, i2c::I2c};

mod cfg { include!("bmi270_config.rs"); }

/// IMU 六轴数据（加速度 + 陀螺仪 + 温度）
#[derive(Debug, Clone, Copy, Default)]
pub struct ImuData {
    /// X/Y/Z 轴加速度 (m/s²)
    pub acc_x: f32, pub acc_y: f32, pub acc_z: f32,
    /// X/Y/Z 轴角速度 (°/s)
    pub gyr_x: f32, pub gyr_y: f32, pub gyr_z: f32,
    /// 温度 (°C)
    pub temp: f32,
}

/// BMI270 IMU 驱动
pub struct Imu<I2C: I2c> { i2c: I2C, addr: u8 }

impl<I2C: I2c> Imu<I2C> {
    /// 初始化 BMI270：加载配置、等待 init_ok、使能传感器
    pub fn new(mut i2c: I2C, delay: &mut impl DelayNs) -> Result<Self, Error<I2C::Error>> {
        let addr = probe(&mut i2c).ok_or(Error::Custom("BMI270 not found"))?;

        // ⚠ 软复位后需要等待 100ms 让芯片稳定
        i2c.write(addr, &[0x7E, 0xB6]).map_err(Error::I2c)?;
        delay.delay_ms(100);

        // ⚠ 数据手册要求：加载配置前必须先关高级省电，否则配置被忽略
        i2c.write(addr, &[0x7C, 0x00]).map_err(Error::I2c)?;
        delay.delay_ms(5);

        // 进入配置加载模式
        i2c.write(addr, &[0x59, 0x00]).map_err(Error::I2c)?;
        delay.delay_ms(5);

        // ⚠ 分页加载配置。注意：INIT_ADDR 是 0x5B/0x5C，不是 0x50！
        // 0x50 是 FEATURES_PAGE 寄存器，用在读取而非写入 config
        for page in 0..256usize {
            let off = page * 32;
            i2c.write(addr, &[0x5B, 0x00]).map_err(Error::I2c)?;
            i2c.write(addr, &[0x5C, page as u8]).map_err(Error::I2c)?;
            let mut buf = [0u8; 33];
            buf[0] = 0x5E; // INIT_DATA
            buf[1..33].copy_from_slice(&cfg::BMI270_CONFIG[off..off + 32]);
            i2c.write(addr, &buf).map_err(Error::I2c)?;
        }
        log::info!("BMI270 config loaded (256 pages)");

        // 触发内部初始化
        i2c.write(addr, &[0x59, 0x01]).map_err(Error::I2c)?;

        // ⚠ 必须等待 INTERNAL_STATUS(0x21).msg==0x01 (init_ok)
        // 否则数据寄存器全是 0
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
            let mut st = [0u8];
            let _ = i2c.write_read(addr, &[0x21], &mut st);
            log::warn!("BMI270 init FAILED: INTERNAL_STATUS=0x{:02X}", st[0]);
            return Err(Error::Custom("BMI270 init failed"));
        }
        log::info!("BMI270 init OK");

        // 再次关省电
        i2c.write(addr, &[0x7C, 0x00]).map_err(Error::I2c)?;
        delay.delay_ms(5);

        // 使能 acc + gyr + temp（gyr_en 自动开启温度传感器）
        i2c.write(addr, &[0x7D, 0x0E]).map_err(Error::I2c)?;
        delay.delay_ms(50);

        // ⚠ ACC_CONF=0xA8: 100Hz, normal mode, performance filter
        // GYR_CONF=0xA9: 200Hz, normal mode, performance filter
        // 之前踩坑: GYR_CONF=0x44 的 ODR=0x04 是 reserved，陀螺仪无数据
        i2c.write(addr, &[0x40, 0xA8]).map_err(Error::I2c)?; // ACC_CONF
        i2c.write(addr, &[0x41, 0x03]).map_err(Error::I2c)?; // ACC_RANGE: ±16g
        i2c.write(addr, &[0x42, 0xA9]).map_err(Error::I2c)?; // GYR_CONF
        i2c.write(addr, &[0x43, 0x00]).map_err(Error::I2c)?; // GYR_RANGE: ±2000dps
        delay.delay_ms(50);

        // 等待数据就绪
        for _ in 0..10 {
            let mut st = [0u8];
            let _ = i2c.write_read(addr, &[0x03], &mut st);
            if st[0] & 0xC0 != 0 { break; } // drdy_acc | drdy_gyr
            delay.delay_ms(10);
        }

        // 试读
        let mut r = [0u8; 12];
        i2c.write_read(addr, &[0x0C], &mut r).map_err(Error::I2c)?;
        log::info!("BMI270 raw=[{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}]",
            r[0], r[1], r[2], r[3], r[4], r[5]);

        Ok(Self { i2c, addr })
    }

    /// 读取 IMU 全量数据（加速度 + 陀螺仪 + 温度）
    pub fn read_all(&mut self) -> Result<ImuData, Error<I2C::Error>> {
        // 先检查 STATUS(0x03) 的 drdy_acc(bit7)/drdy_gyr(bit6)
        let mut st = [0u8];
        self.i2c.write_read(self.addr, &[0x03], &mut st).map_err(Error::I2c)?;
        if st[0] & 0xC0 == 0 {
            return Err(Error::Custom("no data ready"));
        }

        // 从 DATA_8(0x0C) 起读 12 字节：acc_x/y/z + gyr_x/y/z
        let mut buf = [0u8; 12];
        self.i2c.write_read(self.addr, &[0x0C], &mut buf).map_err(Error::I2c)?;
        if buf.iter().all(|&b| b == 0) {
            return Err(Error::Custom("all zero"));
        }
        let ax = i16::from_le_bytes([buf[0], buf[1]]) as f32;
        let ay = i16::from_le_bytes([buf[2], buf[3]]) as f32;
        let az = i16::from_le_bytes([buf[4], buf[5]]) as f32;
        let gx = i16::from_le_bytes([buf[6], buf[7]]) as f32;
        let gy = i16::from_le_bytes([buf[8], buf[9]]) as f32;
        let gz = i16::from_le_bytes([buf[10], buf[11]]) as f32;

        // ⚠ 温度寄存器 0x22/0x23，分辨率 512 LSB/K，0x0000=23°C
        // 初始值 0x8000 无效（此时返回 0.0）
        let mut tbuf = [0u8; 2];
        let _ = self.i2c.write_read(self.addr, &[0x22], &mut tbuf);
        let temp_raw = i16::from_le_bytes(tbuf);
        let temp = if temp_raw == i16::MIN { 0.0 } else { 23.0 + temp_raw as f32 / 512.0 };

        Ok(ImuData {
            // ⚠ ±16g → 2048 LSB/g，不是 16384（那是 ±2g 的）
            acc_x: ax / 2048.0 * 9.80665,
            acc_y: ay / 2048.0 * 9.80665,
            acc_z: az / 2048.0 * 9.80665,
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
/// IMU 驱动错误类型
pub enum Error<E> { I2c(E), Custom(&'static str) }
