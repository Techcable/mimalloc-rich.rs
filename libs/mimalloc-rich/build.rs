#![expect(missing_docs, reason = "fine for build scripts")]

pub fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(has_hint_cold_path)");
    if rustversion::cfg!(since(1.95)) {
        println!("cargo::rustc-cfg=has_hint_cold_path");
    }
}
