fn main() {
    println!("cargo:rerun-if-changed=assets/SlideshowMaker.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/SlideshowMaker.ico")
            .set("ProductName", "SlideshowMaker")
            .set("FileDescription", "SlideshowMaker")
            .set("InternalName", "SlideshowMaker")
            .set(
                "OriginalFilename",
                &format!("SlideshowMaker-v{}.exe", env!("CARGO_PKG_VERSION")),
            )
            .set_language(0x0409)
            .compile_for("slideshow-maker")
            .expect("Compile the application icon and Windows version resources");
    }
}
