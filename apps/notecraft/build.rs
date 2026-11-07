//! Windows only: embed the app icon and version info (VERSIONINFO) into `notecraft.exe`.
//!
//! On every other target this does nothing. A missing icon or resource compiler is a warning,
//! so builds work before the icon exists and cross-compiles still link, unless
//! `NOTECRAFT_REQUIRE_WINRES=1` makes it an error (release builds).

fn main() {
    const ICON: &str = "../../assets/app-icon/notecraft.ico";
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ICON}");
    println!("cargo:rerun-if-env-changed=NOTECRAFT_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let required = std::env::var_os("NOTECRAFT_REQUIRE_WINRES").is_some();
    let mut res = winresource::WindowsResource::new();
    if std::path::Path::new(ICON).exists() {
        res.set_icon(ICON);
    } else if required {
        eprintln!("{ICON} is missing");
        std::process::exit(1);
    } else {
        println!("cargo:warning=notecraft.exe built without an icon: {ICON} is missing");
    }
    res.set("ProductName", "NoteCraft")
        .set("FileDescription", "NoteCraft notebook")
        .set("CompanyName", "Ideademic")
        .set("LegalCopyright", "Copyright (c) the NoteCraft authors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "notecraft.exe")
        .set("InternalName", "notecraft");
    if let Err(e) = res.compile() {
        if required {
            eprintln!("embedding Windows resources failed: {e}");
            std::process::exit(1);
        }
        println!("cargo:warning=notecraft.exe built without icon/version resources: {e}");
    }
}
