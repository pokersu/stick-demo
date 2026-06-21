fn main() {
    embuild::espidf::sysenv::output();

    // GCC 运行时库
    println!("cargo:rustc-link-lib=gcc");

    // ── Provision 数据 ──
    // 读取 workspace 根目录下的 provision.json，生成 PROVISION_DATA 常量
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = std::path::Path::new(&manifest_dir).parent().unwrap();
    let provision_path = workspace_root.join("provision.json");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = std::path::Path::new(&out_dir).join("provision_data.rs");

    if provision_path.exists() {
        let content = std::fs::read_to_string(&provision_path).unwrap();
        let map: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>> =
            serde_json::from_str(&content).expect("provision.json: invalid JSON format");

        let mut output = String::from("pub const PROVISION_DATA: &[(&str, &str, &str)] = &[\n");
        for (ns, keys) in &map {
            for (key, val) in keys {
                output.push_str(&format!("    ({:?}, {:?}, {:?}),\n", ns, key, val));
            }
        }
        output.push_str("];\n");
        std::fs::write(&out_path, output).unwrap();
        println!("cargo:rerun-if-changed={}", provision_path.display());
        println!("cargo:warning=Provision: loaded {} entries from provision.json", map.values().map(|m| m.len()).sum::<usize>());
    } else {
        // 文件不存在时生成空数据，这样编译总是通过
        std::fs::write(&out_path, "pub const PROVISION_DATA: &[(&str, &str, &str)] = &[];\n").unwrap();
    }
}
