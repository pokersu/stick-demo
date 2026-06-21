//! 首次启动时从编译时嵌入的 JSON 数据写入 NVS
//!
//! # 用法
//!
//! 在项目根目录创建 `provision.json`（已加入 `.gitignore`），
//! 格式为 `{"namespace": {"key": "value"}}`。
//! 编译时 `build.rs` 读取并嵌入为常量。
//! 首次启动时 `apply()` 遍历写入 NVS（仅当 key 不存在时写入）。
//!
//! 参考 `provision.example.json` 获取模板。

use crate::nvs::Nvs;

// 编译时生成的数据（来自 provision.json 或空列表）
include!(concat!(env!("OUT_DIR"), "/provision_data.rs"));

/// 将所有 provision 条目写入 NVS（仅当 key 不存在时）
pub fn apply() {
    if PROVISION_DATA.is_empty() {
        return;
    }

    let mut count = 0u32;
    for &(ns, key, val) in PROVISION_DATA {
        if let Ok(mut nvs) = Nvs::new(ns) {
            if nvs.get_str(key).is_none() {
                if nvs.set_str(key, val).is_ok() {
                    count += 1;
                    log::info!("Provision: {}.{} = {}", ns, key, val);
                }
            }
        }
    }
    if count > 0 {
        log::info!("Provision: {} entries written", count);
    }
}
