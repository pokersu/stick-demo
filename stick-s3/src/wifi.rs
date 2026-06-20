//! WiFi STA 模式驱动 — 完全非阻塞

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    nvs::EspDefaultNvsPartition,
    sys::{self, EspError},
    wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};

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

    pub fn ip(&self) -> Option<embedded_svc::ipv4::Ipv4Addr> {
        self.wifi.wifi().sta_netif().get_ip_info().ok().map(|i| i.ip)
    }

    /// 设置配置并发起连接（真正非阻塞，只调用 esp_wifi_connect）
    pub fn start_connect(&mut self, ssid: &str, password: &str) {
        if self.configured { return; }
        let _ = self.wifi.set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: ssid.try_into().unwrap(), password: password.try_into().unwrap(), ..Default::default()
        }));
        unsafe { let _ = sys::esp_wifi_connect(); }
        self.configured = true;
    }

    /// 断开连接
    pub fn disconnect(&mut self) {
        let _ = self.wifi.disconnect();
        self.configured = false;
    }
}
