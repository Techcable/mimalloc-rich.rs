//! Automatically generated bindings to the [mimalloc] allocator.
//!
//! See [official docs](https://microsoft.github.io/mimalloc/) for more details.
//!
//! [mimalloc]: https://github.com/microsoft/mimalloc
#![cfg_attr(not(test), no_std)]
extern crate alloc;

use core::ffi::c_int;
use core::fmt::{Display, Formatter};

pub use self::generated::*;

mod generated {
    #![allow(non_upper_case_globals, non_camel_case_types, non_snake_case, missing_docs)]
    include!(concat!(env!("OUT_DIR"), "/mimalloc_bindings.rs"));
}

/// Parse [`mimalloc_version`] into a string-like value.
///
/// This function is unstable.
#[doc(hidden)]
pub fn mimalloc_version_str() -> impl Display {
    struct VersionInfo {
        version: c_int,
    }
    impl Display for VersionInfo {
        fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
            // https://github.com/microsoft/mimalloc/blob/v2.5.2/src/options.c#L207-L209
            let major_version = self.version / 10_000;
            let minor_version = (self.version % 10_000) / 100;
            let patch_version = self.version % 100;
            write!(f, "{major_version}.{minor_version}.{patch_version}")
        }
    }
    VersionInfo {
        // SAFETY: not actually unsafe
        version: unsafe { mi_version() },
    }
}

#[cfg(test)]
mod test {
    use alloc::string::ToString;

    #[test]
    fn vendored_version_matches() {
        const {
            assert!(cfg!(feature = "vendored"), "build should always be vendored");
        }
        let vendored_version = env!("MIMALLOC_VENDORED_VERSION");
        let runtime_version = super::mimalloc_version_str().to_string();
        assert_eq!(vendored_version, runtime_version);
    }
}
