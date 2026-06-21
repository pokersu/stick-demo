//! # M5StickS3 硬件抽象层
//!
//! 提供 M5Stack StickS3 / StickS3 Plus 开发板的各外设驱动。
//! 所有驱动封装了 ESP-IDF HAL，对外提供简洁的 API。
//!
//! ## 驱动列表
//!
//! | 模块 | 功能 | 依赖 |
//! |------|------|------|
//! | [`display`] | ST7789 屏幕驱动 (SPI) | `display-interface`, `embedded-hal` |
//! | [`framebuffer`] | embedded-graphics DrawTarget 适配 | `embedded-graphics` |
//! | [`buttons`] | GPIO 按键 (边沿检测) | — |
//! | [`imu`] | BMI270 6 轴 IMU (I2C) | `embedded-hal` |
//! | [`pmic`] | M5PM1 电源管理 (I2C) | — |
//! | [`i2c_bus`] | I2C 共享总线 (RefCell) | `embedded-hal` |
//! | [`es8311`] | ES8311 音频编解码器 (I2C) | `embedded-hal` |
//! | [`mic`] | I2S1 PDM 麦克风 | — |
//! | [`speaker`] | I2S0 扬声器 + beep | — |
//! | [`wifi`] | WiFi STA (feature=wifi) | `esp-idf-svc` |
//! | [`led`] | GPIO LED | — |
//! | [`battery`] | ADC 电池电压 | — |
//!
//! ## 坑点记录 (Pitfalls)
//!
//! ### ST7789 显示
//! - **SLPOUT 延时必须 ≥130ms**，M5GFX 标准，过短会导致初始化失败
//! - **MADCTL=0x64** 横屏时必须交换 CASET/RASET 范围，否则花屏
//! - **OFFSET_X=52, OFFSET_Y=40** 是 ST7789 240×320 面板映射 135×240 的偏移，不可改
//! - **每次 flush 必须重设 CASET/RASET**，屏幕芯片内部寄存器是易失的
//!
//! ### M5PM1 电源管理
//! - **PMIC_ADDR=0x6E**，写寄存器时不能全字节覆写（会关掉背光 GPIO2）
//! - **0x09 必须写 0x00** 禁用 I2C 空闲休眠，否则 PMIC 半小时后断连
//! - PMIC ID=0xff 是正常的（M5STACK 定制芯片）
//!
//! ### BMI270 IMU
//! - **GYR_CONF 不能设 0x04**，该值是 reserved 无效值，陀螺仪不输出数据
//! - **配置加载需用 INIT_ADDR(0x5B/0x5C) + INIT_DATA(0x5E)**，不能用 0x50（预留给其他用途）
//! - 必须等 INTERNAL_STATUS(0x21)==0x01 确认 init 完成才能操作数据寄存器
//! - 温度传感器在 gyr_en=1 时自动使能，初始值 0x8000 无效，需等第一次更新
//!
//! ### ES8311 音频编解码
//! - Speaker 时钟 0xB5、Mic 时钟 0xBA，**RESET 只能做一次**，否则后一次初始化冲掉前一次
//! - I2S0 TX 和 I2S1 RX 共享 BCK/WS 引脚，**不能同时以不同采样率激活**
//!
//! ## 引脚定义
//!
//! 参考 M5Unified / sticks3-toolkit / M5GFX 确认。
//! ```text
//!       Display (SPI2)   |  Audio (I2S0 - Speaker)
//!       MOSI=39, SCLK=40 |  MCK=18, BCK=17, WS=15, DATA=14
//!       CS=41, DC=45     |
//!       RST=21, BL=38    |  Audio (I2S1 - Mic)
//!                        |  MCK=18, BCK=17, WS=15, DATA=16
//!       I2C (PMIC+ES8311)|
//!       SDA=47, SCL=48   |  Others: Buttons=11/12, LED=10, BAT=8
//! ```

// ── 废弃模块 ──
// pub mod audio; // audio.rs 已拆分为 mic/speaker/es8311

// ── 显示 ──
pub mod display;
pub mod framebuffer;

// ── 输入 ──
pub mod buttons;

// ── IMU ──
pub mod imu;

// ── 电源管理 ──
pub mod pmic;
pub mod i2c_bus;
pub mod sleep;

// ── 音频 ──
pub mod es8311;
pub mod mic;
pub mod speaker;

// ── 外设 ──
pub mod led;
pub mod battery;

// ── WiFi (feature-gated) ──
#[cfg(feature = "wifi")] pub mod nvs;
#[cfg(feature = "wifi")] pub mod provision;
#[cfg(feature = "wifi")] pub mod wifi;

/// 屏幕宽度（横屏 240）
pub const WIDTH: u32 = 240;
/// 屏幕高度（横屏 135）
pub const HEIGHT: u32 = 135;
/// ST7789 列偏移 — 面板在 240×320 控制器中的物理列起始位置
pub const OFFSET_X: u16 = 52;
/// ST7789 行偏移 — 面板在 240×320 控制器中的物理行起始位置
pub const OFFSET_Y: u16 = 40;

// ═══════════════════════════════════════════════
//  引脚定义（M5StickS3 / M5StickS3 Plus）
//  来源: M5Unified, M5GFX, sticks3-toolkit 实测
// ═══════════════════════════════════════════════

// ── 显示 — ST7789V (SPI2) ──
pub const PIN_LCD_MOSI: u8 = 39;
pub const PIN_LCD_SCLK: u8 = 40;
pub const PIN_LCD_CS:   u8 = 41;
pub const PIN_LCD_DC:   u8 = 45;
pub const PIN_LCD_RST:  u8 = 21;
pub const PIN_LCD_BL:   u8 = 38;

// ── 按键 ──
pub const PIN_BTN_A: u8 = 11;
pub const PIN_BTN_B: u8 = 12;

// ── I2C 总线（PMIC + 音频编解码器） ──
pub const PIN_I2C_SDA: u8 = 47;
pub const PIN_I2C_SCL: u8 = 48;

// ── I2S0 扬声器 TX ──
pub const PIN_SPK_MCK:  u8 = 18;
pub const PIN_SPK_BCLK: u8 = 17;
pub const PIN_SPK_WS:   u8 = 15;
pub const PIN_SPK_DATA: u8 = 14;

// ── I2S1 麦克风 RX（共享 MCK/BCLK/WS） ──
pub const PIN_MIC_DATA: u8 = 16;   // DIN (与扬声器共享 MCK=18, BCLK=17, WS=15)

// ── 内置 LED ──
pub const PIN_LED: u8 = 10;

// ── 电池 ADC (ADC1_CH7 = GPIO8) ──
pub const PIN_BAT_ADC: u8 = 8;
