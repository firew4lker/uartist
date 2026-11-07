//! Embed the icon and version info in the Windows .exe.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/uartist.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/uartist.ico")
            .set("ProductName", "Uartist")
            .set("FileDescription", "Uartist serial terminal");
        if let Err(e) = res.compile() {
            // A missing resource compiler shouldn't stop the build.
            println!("cargo:warning=could not embed Windows resources: {e}");
        }
    }
}
