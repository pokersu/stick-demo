//! ST7789 显示驱动
//!
//! 参考 M5GFX (LovyanGFX) 架构:
//!   - Panel_ST7789 初始化命令序列
//!   - Panel_LCD::setRotation() 行列偏移算法
//!   - Panel_LCD::setWindow() CASET/RASET 写入

use crate::sleep::DisplaySleep;

use crate::{HEIGHT, OFFSET_X, OFFSET_Y, WIDTH};
use display_interface::{DataFormat, WriteOnlyDataCommand};
use display_interface_spi::SPIInterface;
use embedded_hal::delay::DelayNs;
use esp_idf_hal::{
    gpio::{Gpio0, Output, PinDriver},
    spi::{Dma, SpiDeviceDriver, SpiDriver, SpiDriverConfig, SpiAnyPins},
    units::FromValueType,
};

// ── 寄存器命令 ──
const SWRESET: u8 = 0x01;
const SLPOUT:  u8 = 0x11;
const INVOFF:  u8 = 0x20;
const INVON:   u8 = 0x21;
const DISPON:  u8 = 0x29;
const CASET:   u8 = 0x2A;
const RASET:   u8 = 0x2B;
const RAMWR:   u8 = 0x2C;
const MADCTL:  u8 = 0x36;
const COLMOD:  u8 = 0x3A;

// ST7789 扩展命令 (M5GFX Panel_ST7789)
const CMD_GCTRL:    u8 = 0xB7;
const CMD_VCOMS:    u8 = 0xBB;
const CMD_LCMCTRL:  u8 = 0xC0;
const CMD_VDVVRHEN: u8 = 0xC2;
const CMD_VRHS:     u8 = 0xC3;
const CMD_VDVSET:   u8 = 0xC4;
const CMD_PWCTRL1:  u8 = 0xD0;
const CMD_RAMCTRL:  u8 = 0xB0;
const CMD_PVGAMCTRL:u8 = 0xE0;
const CMD_NVGAMCTRL:u8 = 0xE1;

// MADCTL 位
const MAD_MY: u8 = 0x80;
const MAD_MX: u8 = 0x40;
const MAD_MV: u8 = 0x20;
const MAD_ML: u8 = 0x10;
const MAD_MH: u8 = 0x04;

// M5GFX getMadCtl 旋转映射表
const MADCTL_TABLE: [u8; 8] = [
    0,                                          // 0
    MAD_MV | MAD_MX | MAD_MH,                  // 1
    MAD_MX | MAD_MH | MAD_MY | MAD_ML,          // 2
    MAD_MV | MAD_MY | MAD_ML,                   // 3
    MAD_MY | MAD_ML,                             // 4
    MAD_MV,                                      // 5
    MAD_MX | MAD_MH,                             // 6
    MAD_MV | MAD_MX | MAD_MY | MAD_MH | MAD_ML, // 7
];

// M5GFX rowstart 条件掩码
const ROWSTART_MASK: u8 = 0b10010110;

// 面板物理参数 (M5StickS3)
const MEMORY_WIDTH:  u16 = 240;
const MEMORY_HEIGHT: u16 = 320;
const PANEL_WIDTH:   u16 = 135;
const PANEL_HEIGHT:  u16 = 240;

pub struct Display<'a> {
    di:   SPIInterface<SpiDeviceDriver<'a, SpiDriver<'a>>, PinDriver<'a, Output>>,
    _rst: PinDriver<'a, Output>,
    colstart: u16,
    rowstart: u16,
}

impl<'a> Display<'a> {
    pub fn new(
        spi:  impl SpiAnyPins  + 'a,
        sclk: impl esp_idf_hal::gpio::OutputPin + 'a,
        sdo:  impl esp_idf_hal::gpio::OutputPin + 'a,
        cs:   impl esp_idf_hal::gpio::OutputPin + 'a,
        dc:   impl esp_idf_hal::gpio::OutputPin + 'a,
        rst:  impl esp_idf_hal::gpio::OutputPin + 'a,
    ) -> Self {
        let config = esp_idf_hal::spi::config::Config::new()
            .baudrate(20_u32.MHz().into())
            .data_mode(esp_idf_hal::spi::config::MODE_0)
            .queue_size(1);

        let device = SpiDeviceDriver::new_single(
            spi, sclk, sdo,
            None::<Gpio0>,
            Some(cs),
            &SpiDriverConfig::new().dma(Dma::Auto(4096)),
            &config,
        ).unwrap();

        Self {
            di:   SPIInterface::new(device, PinDriver::output(dc).unwrap()),
            _rst: PinDriver::output(rst).unwrap(),
            colstart: 0,
            rowstart: 0,
        }
    }

    /// 初始化 ST7789 — 命令序列来自 M5GFX Panel_ST7789::getInitCommands
    pub fn init(&mut self, delay: &mut impl DelayNs) {
        rst_pulse(&mut self._rst, delay);

        self.cmd(SWRESET); delay.delay_us(150_000);
        self.cmd(SLPOUT);  delay.delay_ms(130);   // M5GFX: 130ms after SLPOUT
        self.cmd(INVOFF);

        // 面板调优
        self.cmd_data(CMD_GCTRL,    &[0x35]);
        self.cmd_data(CMD_VCOMS,   &[0x28]);
        self.cmd_data(CMD_LCMCTRL, &[0x0C]);
        self.cmd_data(CMD_VDVVRHEN,&[0x01, 0xFF]);
        self.cmd_data(CMD_VRHS,    &[0x10]);
        self.cmd_data(CMD_VDVSET,  &[0x20]);
        self.cmd_data(CMD_PWCTRL1, &[0xA4, 0xA1]);
        self.cmd_data(CMD_RAMCTRL, &[0x00, 0xC0]);

        // Gamma (ST7789V)
        self.cmd_data(CMD_PVGAMCTRL, &[0xD0, 0x00, 0x02, 0x07, 0x0A, 0x28, 0x32, 0x44,
                                           0x42, 0x06, 0x0E, 0x12, 0x14, 0x17]);
        self.cmd_data(CMD_NVGAMCTRL, &[0xD0, 0x00, 0x02, 0x07, 0x0A, 0x28, 0x31, 0x54,
                                           0x47, 0x0E, 0x1C, 0x17, 0x1B, 0x1E]);

        self.cmd_data(COLMOD, &[0x55]); // RGB565
        self.cmd(INVON);   delay.delay_us(10_000);
        self.cmd(DISPON);  delay.delay_us(10_000);

        // 默认横屏
        self.set_rotation(1);
    }

    // ── 旋转 (M5GFX setRotation + update_madctl) ──

    pub fn set_rotation(&mut self, r: u8) {
        let internal = (r & 3) | ((r & 4) ^ 0);  // offset_rotation=0

        let (mut ox, mut oy) = (OFFSET_X, OFFSET_Y);
        let (mut pw, mut ph) = (PANEL_WIDTH, PANEL_HEIGHT);
        let (mut mw, mut mh) = (MEMORY_WIDTH, MEMORY_HEIGHT);

        if internal & 1 != 0 {
            core::mem::swap(&mut ox, &mut oy);
            core::mem::swap(&mut pw, &mut ph);
            core::mem::swap(&mut mw, &mut mh);
        }

        self.colstart = if internal & 2 != 0 { mw - (pw + ox) } else { ox };
        self.rowstart = if ((1u8 << internal) & ROWSTART_MASK) != 0 { mh - (ph + oy) } else { oy };

        self.cmd_data(MADCTL, &[MADCTL_TABLE[internal as usize]]);
    }

    // ── 窗口 (M5GFX setWindow) ──

    fn set_window(&mut self, xs: u16, ys: u16, xe: u16, ye: u16) {
        self.cmd(CASET);
        self.di.send_data(DataFormat::U8(&(self.colstart + xs).to_be_bytes())).unwrap();
        self.di.send_data(DataFormat::U8(&(self.colstart + xe).to_be_bytes())).unwrap();
        self.cmd(RASET);
        self.di.send_data(DataFormat::U8(&(self.rowstart + ys).to_be_bytes())).unwrap();
        self.di.send_data(DataFormat::U8(&(self.rowstart + ye).to_be_bytes())).unwrap();
    }

    // ── 全屏刷新 ──

    pub fn flush(&mut self, buf: &[u16]) {
        let w = WIDTH as u16;
        let h = HEIGHT as u16;
        self.set_window(0, 0, w - 1, h - 1);
        self.cmd(RAMWR);
        self.di.send_data(DataFormat::U16(buf)).unwrap();
    }

    // ── 底层 ──

    fn cmd(&mut self, b: u8) {
        self.di.send_commands(DataFormat::U8(&[b])).unwrap();
    }

    fn cmd_data(&mut self, cmd: u8, data: &[u8]) {
        self.cmd(cmd);
        self.di.send_data(DataFormat::U8(data)).unwrap();
    }
}

impl DisplaySleep for Display<'_> {
    fn sleep(&mut self) {
        self.cmd(0x28); // DISPOFF
        self.cmd(0x10); // SLPIN
    }
}

fn rst_pulse(rst: &mut PinDriver<'_, Output>, delay: &mut impl DelayNs) {
    rst.set_high().unwrap();
    delay.delay_us(10);
    rst.set_low().unwrap();
    delay.delay_us(10);
    rst.set_high().unwrap();
    delay.delay_us(10);
}
