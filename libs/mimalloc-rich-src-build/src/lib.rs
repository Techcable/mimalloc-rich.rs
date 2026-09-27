//! Build code for vendored mimalloc,
//! hared across all versions.
//!
//! This crate is currently considered an implementation detail of the `mimalloc-sys` crate.
//! As such, its contents may change without notice.
#![allow(
    clippy::std_instead_of_core,
    clippy::std_instead_of_alloc,
    reason = "this only runs at build-time"
)]

use std::fmt::{Display, Formatter};
use std::path::PathBuf;

#[doc(hidden)]
pub mod build;
#[doc(hidden)]
pub mod context;
#[doc(hidden)]
pub mod options;
#[doc(hidden)]
pub mod version;

#[doc(hidden)]
pub struct VendoredSource {
    /// The root of the vendored mimalloc repository.
    pub source_root: PathBuf,
    pub vendored_crate_name: &'static str,
    pub vendored_crate_version: &'static str,
    /// The expected version of the mimalloc library.
    pub expected_version: String,
}
impl VendoredSource {
    pub(crate) fn crate_desc(&self) -> impl Display {
        struct Desc<'a>(&'a VendoredSource);
        impl Display for Desc<'_> {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}@{}", self.0.vendored_crate_name, self.0.vendored_crate_version)
            }
        }
        Desc(self)
    }
}
