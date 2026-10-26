fn main() {
    slint_build::compile("ui/appwindow.slint").unwrap();

    // Slint builds the window with all its screens on the main thread's stack; the
    // Windows default of 1 MB overflows in debug builds.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/STACK:8388608");
    }

    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("math.ico"); // Replace this with the filename of your .ico file.
        res.compile().unwrap();
      }
}
