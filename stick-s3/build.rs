fn main() {
    embuild::espidf::sysenv::output();

    // cargo:rustc-link-arg 不传播给 build-std (std/alloc/core)。
    // 但 GCC 运行时库不会与 ESP-IDF 冲突，用 rustc-link-lib 传播。
    // 注意：不要加 -lc/-lm/-lpthread/-lstdc++ ——
    // ESP-IDF 提供自己的 newlib，加 -lc 会导致重复定义。
    println!("cargo:rustc-link-lib=gcc");
}
