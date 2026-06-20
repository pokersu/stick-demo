//! NVS 持久存储 (raw ESP-IDF API)

use core::ptr;
use esp_idf_svc::sys::*;

const NVS_RW: i32 = 0x01; // NVS_READWRITE

pub struct Nvs {
    handle: nvs_handle_t,
}

impl Nvs {
    fn init() {
        unsafe {
            let mut r = nvs_flash_init();
            if r == 0x1100 || r == 0x1106 { // NO_FREE_PAGES | NEW_VERSION_FOUND
                nvs_flash_erase();
                r = nvs_flash_init();
            }
            if r != 0 { /* warn: nvs_flash_init failed */ }
        }
    }

    pub fn new(namespace: &str) -> Result<Self, EspError> {
        Self::init();
        let ns = std::ffi::CString::new(namespace).unwrap();
        let mut handle: nvs_handle_t = 0;
        esp!(unsafe { nvs_open(ns.as_ptr(), NVS_RW as _, &mut handle) })?;
        Ok(Self { handle })
    }

    pub fn get_i32(&self, key: &str) -> Option<i32> {
        let ck = std::ffi::CString::new(key).ok()?;
        let mut val: i32 = 0;
        (unsafe { nvs_get_i32(self.handle, ck.as_ptr(), &mut val) } == 0).then_some(val)
    }

    pub fn set_i32(&mut self, key: &str, val: i32) -> Result<(), EspError> {
        let ck = std::ffi::CString::new(key).unwrap();
        esp!(unsafe { nvs_set_i32(self.handle, ck.as_ptr(), val) })?;
        esp!(unsafe { nvs_commit(self.handle) })?;
        Ok(())
    }

    pub fn get_str(&self, key: &str) -> Option<String> {
        let ck = std::ffi::CString::new(key).ok()?;
        let mut len: usize = 0;
        if unsafe { nvs_get_str(self.handle, ck.as_ptr(), ptr::null_mut(), &mut len) } != 0 || len == 0 { return None; }
        let mut buf = vec![0u8; len];
        if unsafe { nvs_get_str(self.handle, ck.as_ptr(), buf.as_mut_ptr() as *mut _, &mut len) } != 0 { return None; }
        buf.truncate(len - 1);
        String::from_utf8(buf).ok()
    }

    pub fn set_str(&mut self, key: &str, val: &str) -> Result<(), EspError> {
        let ck = std::ffi::CString::new(key).unwrap();
        let cv = std::ffi::CString::new(val).unwrap();
        esp!(unsafe { nvs_set_str(self.handle, ck.as_ptr(), cv.as_ptr()) })?;
        esp!(unsafe { nvs_commit(self.handle) })?;
        Ok(())
    }

    pub fn get_blob(&self, key: &str) -> Option<Vec<u8>> {
        let ck = std::ffi::CString::new(key).ok()?;
        let mut len: usize = 0;
        if unsafe { nvs_get_blob(self.handle, ck.as_ptr(), ptr::null_mut(), &mut len) } != 0 || len == 0 { return None; }
        let mut buf = vec![0u8; len];
        if unsafe { nvs_get_blob(self.handle, ck.as_ptr(), buf.as_mut_ptr() as *mut _, &mut len) } != 0 { return None; }
        Some(buf)
    }

    pub fn set_blob(&mut self, key: &str, val: &[u8]) -> Result<(), EspError> {
        let ck = std::ffi::CString::new(key).unwrap();
        esp!(unsafe { nvs_set_blob(self.handle, ck.as_ptr(), val.as_ptr() as *const _, val.len()) })?;
        esp!(unsafe { nvs_commit(self.handle) })?;
        Ok(())
    }

    pub fn remove(&mut self, key: &str) {
        if let Ok(ck) = std::ffi::CString::new(key) {
            unsafe { nvs_erase_key(self.handle, ck.as_ptr()); nvs_commit(self.handle); }
        }
    }
}

impl Drop for Nvs {
    fn drop(&mut self) { unsafe { nvs_close(self.handle) } }
}
