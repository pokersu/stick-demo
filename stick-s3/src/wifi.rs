//! WiFi STA 模式驱动 — 完全非阻塞状态机
//!
//! 所有 WiFi 操作（扫描、连接）拆分为多帧状态机，
//! 每帧调用 `tick()` 推进状态，不阻塞主循环。
//!
//! ## 用法
//! ```ignore
//! wifi.start_auto_connect();     // 触发扫描
//! loop {
//!     wifi.tick();               // 每帧推进
//!     update_sensors();          // 不卡顿
//! }
//! wifi.status()  // 查看状态
//! ```
//!
//! ## 坑点
//! - **NVS 键名最大 15 字节**，SSID 过长的无法直接作为键名
//! - **连接使用原始 esp_wifi_connect()** 只触发命令，不阻塞等待事件
//! - **ip() 是瞬时快照**，连接成功后需要时间才能获取到

use crate::nvs::Nvs;
use esp_idf_svc::{
    nvs::EspDefaultNvsPartition,
    sys::{self, EspError},
    wifi::{ClientConfiguration, Configuration, EspWifi},
};

/// WiFi 连接状态
#[derive(Debug, Clone, PartialEq)]
pub enum WifiStatus {
    Idle,
    Scanning,
    Connecting(String), // 正在连接某 SSID
    Connected(String),  // 已连上某 SSID
    Failed(&'static str),
}

/// WiFi STA 模式驱动 — 状态机
pub struct Wifi {
    wifi: EspWifi<'static>,
    status: WifiStatus,
}

impl Wifi {
    pub fn new(modem: esp_idf_hal::modem::Modem) -> Result<Self, EspError> {
        let nvs = EspDefaultNvsPartition::take()?;
        let sl = esp_idf_svc::eventloop::EspSystemEventLoop::take()?;
        let mut w = EspWifi::new(modem, sl, Some(nvs))?;
        w.start()?;
        let wifi: EspWifi<'static> = unsafe { core::mem::transmute(w) };
        log::info!("WiFi: started");
        Ok(Self { wifi, status: WifiStatus::Idle })
    }

    /// 获取 STA 的 IPv4 地址
    pub fn ip(&self) -> Option<embedded_svc::ipv4::Ipv4Addr> {
        self.wifi.sta_netif().get_ip_info().ok().map(|i| i.ip)
    }

    /// 当前状态
    pub fn status(&self) -> &WifiStatus { &self.status }

    /// 触发自动连接：扫描 → 匹配 NVS → 连接（非阻塞，返回后主循环需每帧调用 tick()）
    pub fn start_auto_connect(&mut self) {
        self.disconnect();
        match self.wifi.start_scan(&Default::default(), false) {
            Ok(_) => {
                self.status = WifiStatus::Scanning;
                log::info!("WiFi: scan started");
            }
            Err(e) => {
                self.status = WifiStatus::Failed("scan start failed");
                log::warn!("WiFi: start_scan failed: {}", e);
            }
        }
    }

    /// 每帧调用 — 推进 WiFi 状态机
    ///
    /// 应在主循环的每一帧调用，配合 `status()` 更新显示。
    pub fn tick(&mut self) {
        match &self.status {
            // ── 扫描中：轮询是否完成 ──
            WifiStatus::Scanning => {
                match self.wifi.is_scan_done() {
                    Ok(true) => {
                        // 扫描完成，取结果匹配
                        match self.wifi.get_scan_result() {
                            Ok(aps) => {
                                log::info!("WiFi: scan done, {} APs", aps.len());
                                let matched = self.match_and_connect(&aps);
                                if let Some(ssid) = matched {
                                    self.status = WifiStatus::Connecting(ssid);
                                } else {
                                    self.status = WifiStatus::Failed("no known wifi");
                                }
                            }
                            Err(e) => {
                                log::warn!("WiFi: get_scan_result failed: {}", e);
                                self.status = WifiStatus::Failed("scan result error");
                            }
                        }
                    }
                    Ok(false) => { /* 还在扫描，继续等 */ }
                    Err(e) => {
                        log::warn!("WiFi: is_scan_done error: {}", e);
                        self.status = WifiStatus::Failed("scan error");
                    }
                }
            }

            // ── 连接中：轮询是否已获取 IP ──
            WifiStatus::Connecting(ssid) => {
                if self.ip().map_or(false, |ip| !ip.is_unspecified()) {
                    log::info!("WiFi: connected to {} via {}", ssid, self.ip().unwrap());
                    self.status = WifiStatus::Connected(ssid.clone());
                }
                // 连接超时? 目前由调用方自行决定是否重试
            }

            // ── 空闲/已连接/失败：什么都不做 ──
            WifiStatus::Idle | WifiStatus::Connected(_) | WifiStatus::Failed(_) => {}
        }
    }

    /// 从 AP 列表匹配 NVS "wifi" 命名空间中的凭据并触发连接
    ///
    /// 返回匹配到的 SSID（如果触发连接成功），否则返回 None。
    fn match_and_connect(&mut self, aps: &[embedded_svc::wifi::AccessPointInfo]) -> Option<String> {
        for ap in aps {
            let ssid = ap.ssid.as_str();
            if ssid.is_empty() { continue; }
            log::info!("WiFi: AP '{}' rssi={}", ssid, ap.signal_strength);

            if let Ok(nvs) = Nvs::new("wifi") {
                if let Some(pass) = nvs.get_str(ssid) {
                    log::info!("WiFi: matched {}", ssid);
                    let _ = self.wifi.set_configuration(&Configuration::Client(ClientConfiguration {
                        ssid: ssid.try_into().unwrap(),
                        password: pass.as_str().try_into().unwrap(),
                        ..Default::default()
                    }));
                    // ⚠ 用原始 API 避免阻塞等待连接事件
                    unsafe { let _ = sys::esp_wifi_connect(); }
                    return Some(ssid.to_string());
                }
            }
        }
        None
    }

    /// 断开连接
    pub fn disconnect(&mut self) {
        let _ = self.wifi.disconnect();
        self.status = WifiStatus::Idle;
    }
}
