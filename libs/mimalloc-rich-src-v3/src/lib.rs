//! Contains the vendored source code for [mimalloc] v3.
//!
//! This library is meant to be used only in a build script.
//! It only provides the location and version of the vendored sources;
//! the build logic itself lives in `mimalloc-rich-sys`.
//!
//! [mimalloc]: https://github.com/microsoft/mimalloc

use std::path::Path;

/// The version of mimalloc that is vendored by this crate.
pub const MIMALLOC_VERSION: &str = "3.7.1";

/// The name of this crate, as seen by the compiler (`mimalloc_rich_src_v3`).
///
/// Used to verify that the crate version matches [`MIMALLOC_VERSION`].
pub const CRATE_NAME: &str = env!("CARGO_CRATE_NAME");

/// The version of this crate.
///
/// Used to verify that the crate version matches [`MIMALLOC_VERSION`].
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The root of the vendored mimalloc source tree (containing `CMakeLists.txt`).
pub fn vendored_source_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/mimalloc-v3"))
}

#[cfg(test)]
mod test {
    use mimalloc_rich_src_build::VendoredSource;

    #[test]
    fn verify_version() {
        let source = VendoredSource {
            source_root: super::vendored_source_root().to_path_buf(),
            vendored_crate_name: super::CRATE_NAME,
            vendored_crate_version: super::CRATE_VERSION,
            expected_version: super::MIMALLOC_VERSION.into(),
        };
        mimalloc_rich_src_build::version::verify_version(&source).unwrap();
    }
}
