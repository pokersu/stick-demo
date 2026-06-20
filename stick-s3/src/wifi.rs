//! WiFi STA 模式驱动 — 完全非阻塞
//!
//! ## 坑点
//! - **BlockingWifi::connect() 会阻塞**几秒等待事件响应，不可在主循环直接调用
//! - **改用 raw esp_wifi_connect()** 只触发连接，不等待
//! - **ip() 是瞬时快照**，连接成功后需要时间才能获取到 IP

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    nvs::EspDefaultNvsPartition,
    sys::{self, EspError},
    wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};

/// WiFi STA 模式驱动 — 完全非阻塞
///
/// ## 用法
/// ```ignore
/// let mut wifi = Wifi::new(modem)?;
/// wifi.start_connect(ssid, pass);
/// // 主循环中通过 ip() 检查是否连上
/// ```
pub struct Wifi {
    wifi: BlockingWifi<EspWifi<'static>>,
    configured: bool,
}

impl Wifi {
    pub fn new(modem: esp_idf_hal::modem::Modem) -> Result<Self, EspError> {
        let nvs = EspDefaultNvsPartition::take()?;
        let sl = EspSystemEventLoop::take()?;
        let w = EspWifi::new(modem, sl.clone(), Some(nvs))?;
        let mut w = BlockingWifi::wrap(w, sl)?;
        w.start()?;
        let wifi: BlockingWifi<EspWifi<'static>> = unsafe { core::mem::transmute(w) };
        Ok(Self { wifi, configured: false })
    }

    /// 获取 STA 的 IPv4 地址（`None` 表示未连上）
    pub fn ip(&self) -> Option<embedded_svc::ipv4::Ipv4Addr> {
        self.wifi.wifi().sta_netif().get_ip_info().ok().map(|i| i.ip)
    }

    /// ⚠ 设置配置并发起连接（非阻塞）
    ///
    /// 使用底层的 esp_wifi_connect() 只触发连接命令，
    /// 不等待事件，避免阻塞主循环。
    pub fn start_connect(&mut self, ssid: &str, password: &str) {
        if self.configured { return; }
        let _ = self.wifi.set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: ssid.try_into().unwrap(), password: password.try_into().unwrap(), ..Default::default()
        }));
        // ⚠ 用原始 API 避免 BlockingWifi::connect() 的阻塞等待
        unsafe { let _ = sys::esp_wifi_connect(); }
        self.configured = true;
    }

    /// 断开连接
    pub fn disconnect(&mut self) {
        let _ = self.wifi.disconnect();
        self.configured = false;
    }
}
