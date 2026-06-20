//! I2C 共享总线
//!
//! `I2cBus` 用 `RefCell` 包装 `I2cDriver`，
//! 通过 `acquire()` 创建 `I2cProxy` 实现多设备安全共享。
//!
//! ## 坑点
//! - **RefCell 运行时检查借用规则**，同一时间只能有一个 `I2cProxy` 活跃
//! - **acquire() 返回的 I2cProxy 必须在同一行用完释放**（或显式 drop），
//!   否则下一个 acquire() 会 panic（RefCell 双重借用）

use std::cell::RefCell;
use esp_idf_hal::i2c::{I2cDriver, I2cError};

const TMO: u32 = 2000;

/// I2C 共享总线（基于 RefCell 实现多设备安全共享）
///
/// ⚠ `acquire()` 返回的 `I2cProxy` 必须在同一作用域用完释放，
/// 否则下一个 `acquire()` 会 panic（RefCell 双重借用）。
pub struct I2cBus<'a> {
    inner: RefCell<I2cDriver<'a>>,
}

impl<'a> I2cBus<'a> {
    pub fn new(i2c: I2cDriver<'a>) -> Self { Self { inner: RefCell::new(i2c) } }

    pub fn acquire(&self) -> I2cProxy<'_, 'a> { I2cProxy { bus: self } }

    pub fn with_mut<R>(&self, f: impl FnOnce(&mut I2cDriver<'a>) -> R) -> R {
        f(&mut self.inner.borrow_mut())
    }
}

/// 临时 I2C 代理 — drop 时释放对总线的借用
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
