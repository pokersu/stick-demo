//! I2C 共享总线
//!
//! `I2cBus` 用 `RefCell` 包装 `I2cDriver`，
//! 通过 `acquire()` 创建 `I2cProxy` 实现多设备安全共享。

use std::cell::RefCell;
use esp_idf_hal::i2c::{I2cDriver, I2cError};

const TMO: u32 = 2000;

pub struct I2cBus<'a> {
    inner: RefCell<I2cDriver<'a>>,
}

impl<'a> I2cBus<'a> {
    pub fn new(i2c: I2cDriver<'a>) -> Self { Self { inner: RefCell::new(i2c) } }

    pub fn acquire(&self) -> I2cProxy<'_, 'a> { I2cProxy { bus: self } }

    /// 临时借用 I2cDriver（用于需要 `&mut I2cDriver` 的初始化）
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut I2cDriver<'a>) -> R) -> R {
        f(&mut self.inner.borrow_mut())
    }
}

pub struct I2cProxy<'a, 'b> { bus: &'a I2cBus<'b> }

impl embedded_hal::i2c::ErrorType for I2cProxy<'_, '_> {
    type Error = I2cError;
}

impl embedded_hal::i2c::I2c for I2cProxy<'_, '_> {
    fn read(&mut self, a: u8, b: &mut [u8]) -> Result<(), I2cError> {
        self.bus.inner.borrow_mut().read(a, b, TMO).map_err(I2cError::from)
    }
    fn write(&mut self, a: u8, b: &[u8]) -> Result<(), I2cError> {
        self.bus.inner.borrow_mut().write(a, b, TMO).map_err(I2cError::from)
    }
    fn write_read(&mut self, a: u8, w: &[u8], r: &mut [u8]) -> Result<(), I2cError> {
        self.bus.inner.borrow_mut().write_read(a, w, r, TMO).map_err(I2cError::from)
    }
    fn transaction(&mut self, a: u8, ops: &mut [embedded_hal::i2c::Operation<'_>]) -> Result<(), I2cError> {
        self.bus.inner.borrow_mut().transaction(a, ops, TMO).map_err(I2cError::from)
    }
}
