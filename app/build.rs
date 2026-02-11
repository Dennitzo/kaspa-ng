#[cfg(windows)]
fn main() {
    use std::path::PathBuf;

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let icon_path = manifest_dir
        .join("..")
        .join("core")
        .join("resources")
        .join("icons")
        .join("favicon.ico");

    println!("cargo:rerun-if-changed={}", icon_path.display());

    if let Some(icon) = icon_path.to_str() {
        let mut res = winres::WindowsResource::new();
        res.set_icon(icon);
        let _ = res.compile();
    }
}

#[cfg(not(windows))]
fn main() {}
