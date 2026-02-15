fn main() {
    println!("cargo:rustc-link-lib=framework=UserNotifications");
    tauri_build::build()
}
