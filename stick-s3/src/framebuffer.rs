//! embedded-graphics 兼容的 RGB565 framebuffer
//!
//! 尺寸由 lib.rs 中的 WIDTH/HEIGHT 决定（当前为 240×135 横屏）。
//!
//! ## 坑点
//! - **Fb 只是对 &mut [u16] 的包装**，不持有内存，由调用方管理生命周期
//! - 坐标范围由 embedded-graphics 的 `Size::new(WIDTH, HEIGHT)` 决定
//! - **像素必须 swap_bytes()** — ST7789 要求大端字节序，ESP32 是小端
//! - **必须配合 MADCTL 的 MAD_BGR 位**（在 display.rs 中设置），否则 R/B 通道交换
//!   swap_bytes 修正字节序，MAD_BGR 修正 BGR 顺序，两者缺一不可

use crate::{HEIGHT, WIDTH};
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::Rgb565,
    prelude::{IntoStorage, OriginDimensions},
    Pixel,
};

/// embedded-graphics RGB565 framebuffer
///
/// 对 `&mut [u16]` 的简单包装，不持有内存。
pub struct Fb<'a> {
    pub buf: &'a mut [u16],
}

impl OriginDimensions for Fb<'_> {
    fn size(&self) -> embedded_graphics::geometry::Size {
        embedded_graphics::geometry::Size::new(WIDTH, HEIGHT)
    }
}

impl DrawTarget for Fb<'_> {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let w = WIDTH as usize;
        let len = self.buf.len();
        for Pixel(coord, color) in pixels {
            if coord.x < 0 || coord.y < 0 { continue; }
            let idx = coord.x as usize + coord.y as usize * w;
            if idx < len {
                // 字节交换 + BGR 模式适配 ST7789（M5GFX 标准做法）
                self.buf[idx] = color.into_storage().swap_bytes();
            }
        }
        Ok(())
    }
}
