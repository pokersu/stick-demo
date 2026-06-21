fn main() {
    embuild::espidf::sysenv::output();
    slint_build::compile("app.slint").unwrap();
}
