#![expect(missing_docs, reason = "fine for build scripts")]

pub fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(has_hint_cold_path)");
    println!("cargo::rustc-check-cfg=cfg(has_allocator_api)");
    if rustversion::cfg!(since(1.95)) {
        println!("cargo::rustc-cfg=has_hint_cold_path");
    }
    if rustversion::cfg!(since(1.100)) {
        println!("cargo::rustc-cfg=has_allocator_api");
    }

    // add mimalloc1, mimalloc2, mimalloc3 version configs
    // Note that mimalloc1 is not currently supported
    for x in 1..=3 {
        println!("cargo::rustc-check-cfg=cfg(mimalloc{x})");
    }
    {
        let major_version = std::env::var("DEP_MIMALLOC_MIMALLOC_MAJOR_VERSION")
            .expect("Missing version metadata from mimalloc-rich-sys");
        assert!(
            matches!(major_version.as_str(), "2" | "3"),
            "Unsupported mimalloc major version: {major_version:?}"
        );
        println!("cargo::rustc-cfg=mimalloc{major_version}");
    }
}
