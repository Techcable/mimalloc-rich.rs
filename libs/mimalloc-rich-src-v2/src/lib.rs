//! Contains the vendored source code for [mimalloc].
//!
//! This library is meant to be used only in a build script.
//!
//! [mimalloc]: https://github.com/microsoft/mimalloc

use std::path::Path;

use mimalloc_rich_src_build::VendoredSource;
use mimalloc_rich_src_build::build::BuiltLibrary;
use mimalloc_rich_src_build::context::CompilationContext;
use mimalloc_rich_src_build::options::Options;

const EXPECTED_MIMALLOC_VERSION: &str = "2.5.2";

fn vendored_source() -> VendoredSource {
    VendoredSource {
        expected_version: EXPECTED_MIMALLOC_VERSION.into(),
        vendored_crate_name: env!("CARGO_CRATE_NAME"),
        vendored_crate_version: env!("CARGO_PKG_VERSION"),
        source_root: Path::new(env!("CARGO_MANIFEST_DIR")).join("mimalloc-v2"),
    }
}

#[doc(hidden)]
pub fn build(options: &Options, ctx: &CompilationContext) -> anyhow::Result<BuiltLibrary> {
    mimalloc_rich_src_build::build::build(&vendored_source(), options, ctx)
}
