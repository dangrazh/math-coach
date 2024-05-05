fn main() {
    slint_build::compile("ui/appwindow.slint").unwrap();
    
    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("math.ico"); // Replace this with the filename of your .ico file.
        res.compile().unwrap();
      }
}
