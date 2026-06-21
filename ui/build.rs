fn main() {
    embuild::espidf::sysenv::output();

    // 设置 LVGL 配置文件路径 — cargo 会转发为 DEP_LV_CONFIG_PATH 给 lvgl-sys
    let config_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    println!("cargo:config-path={}", config_dir.display());
}
