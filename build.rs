fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/app.ico")
            .set("ProductName", "AI Traffic Light")
            .set("FileDescription", "Compact AI activity indicator")
            .set("OriginalFilename", "AITrafficLight.exe")
            .set("LegalCopyright", "AI Traffic Light")
            .compile()
            .expect("failed to compile Windows resources");
    }
}
