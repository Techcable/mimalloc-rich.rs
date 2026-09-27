#![expect(missing_docs, reason = "fine for build scripts")]

pub fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    let version = rustversion_detect::detect_version().unwrap();
    println!("cargo::rustc-check-cfg=cfg(has_hint_cold_path)");
    if version.is_since_minor_version(1, 95) {
        println!("cargo::rustc-cfg=has_hint_cold_path");
    }
}
