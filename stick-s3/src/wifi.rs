//! WiFi STA 模式驱动 — 自动扫描 + 凭据匹配
//!
//! ## 坑点
//! - **BlockingWifi::connect() 会阻塞**几秒等待事件响应，不可在主循环直接调用
//! - **改用 raw esp_wifi_connect()** 只触发连接，不等待
//! - **ip() 是瞬时快照**，连接成功后需要时间才能获取到 IP
//! - **NVS 键名最大 15 字节**，SSID 超过此长度的无法直接作为键名
//!   （可用 provision.json 配置短别名代替）

use crate::nvs::Nvs;
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
/// wifi.auto_connect(); // 扫描并按 NVS 凭据自动连接
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

    /// 扫描 → 匹配 NVS "wifi" 命名空间中的凭据 → 自动连接
    ///
    /// 遍历扫描到的 AP，如果其 SSID 在 NVS "wifi" 命名空间中有对应密码，
    /// 则立即连接（非阻塞）并返回该 SSID。
    /// 若无可匹配网络返回 `None`。
    pub fn auto_connect(&mut self) -> Option<String> {
        // ⚠ 如果已经配置过连接，先断开重置
        self.disconnect();

        let aps = self.wifi.scan().ok()?;
        log::info!("WiFi scan: found {} APs", aps.len());

        for ap in &aps {
            let ssid = ap.ssid.as_str();
            if ssid.is_empty() { continue; }
            log::info!("WiFi scan: AP '{}' rssi={}", ssid, ap.signal_strength);

            // 在 NVS "wifi" 命名空间中查找该 SSID 的密码
            match Nvs::new("wifi") {
                Ok(nvs) => {
                    if let Some(pass) = nvs.get_str(ssid) {
                        log::info!("WiFi: matched {} -> connecting", ssid);
                        self.connect_raw(ssid, &pass);
                        return Some(ssid.to_string());
                    } else {
                        log::info!("WiFi: '{}' not in NVS wifi namespace", ssid);
                    }
                }
                Err(e) => log::warn!("WiFi: Nvs::new(wifi) failed: {}", e),
            }
        }
        log::info!("WiFi scan: no known networks");
        None
    }

    /// ⚠ 设置配置并发起连接（非阻塞）— 原始 API
    ///
    /// 使用底层的 esp_wifi_connect() 只触发连接命令，
    /// 不等待事件，避免阻塞主循环。
    fn connect_raw(&mut self, ssid: &str, password: &str) {
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
