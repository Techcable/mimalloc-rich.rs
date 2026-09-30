#![expect(missing_docs, reason = "build script doesn't need docs")]
#![allow(
    clippy::std_instead_of_alloc,
    clippy::std_instead_of_core,
    reason = "build scripts can freely use stdlib"
)]

use std::fmt::Display;
use std::path::PathBuf;

use anyhow::{Context, anyhow, bail};
use mimalloc_rich_src_build::build::BuiltLibrary;
use mimalloc_rich_src_build::context::CompilationContext;
use mimalloc_rich_src_build::options::{DebugCondition, Options, OptionsBuilder};

pub fn main() -> anyhow::Result<()> {
    //
    // build project
    //
    if !has_feature("vendored") {
        println!("cargo::error=The `vendored` feature is currently required");
        bail!("Missing required features")
    }
    let vendored_version: Version = Version::detect()?.unwrap_or_default();

    let ctx = CompilationContext::cargo_build_script();
    let options = {
        let mut builder = OptionsBuilder::new();
        builder.debug(if has_feature("debug") {
            DebugCondition::Always
        } else if has_feature("debug-if-debug") {
            DebugCondition::IfRustDebug
        } else {
            DebugCondition::Never
        });
        builder.secure(has_feature("secure"));
        builder.override_malloc(has_feature("override-libc-malloc"));
        builder.debug_full(has_feature("debug-full"));
        builder.visit_abandoned(has_feature("unstable-enable-visit-abandoned"));
        builder.enable_address_sanitizer(has_feature("sanitize-address"));
        builder.enable_valgrind(has_feature("valgrind"));
        builder
            .build(&ctx)
            .context("Failed to configure mimalloc build options")?
    };

    let result = vendored_version
        .build_vendored(&options, &ctx)
        .with_context(|| format!("Failed to build vendored mimalloc {vendored_version}"))?;

    println!("cargo::rustc-env=MIMALLOC_VENDORED_VERSION={}", result.mimalloc_version);
    println!("cargo::rustc-link-search=native={}", result.library_dir.display());
    println!("cargo::rustc-link-lib=static={}", result.library_name);

    //
    // generate bindings
    //
    println!("generating bindings");
    let bindings = bindgen::Builder::default()
        .header(result.header_file.to_string_lossy().into_owned())
        .allowlist_item("mi_.*")
        .allowlist_item("MI_.*")
        .rustified_non_exhaustive_enum("mi_option_e")
        .use_core()
        .generate()
        .context("Failed to generate bindings")?;
    let out_dir = PathBuf::from(var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_dir.join("mimalloc_bindings.rs"))
        .context("Failed to write bindings")?;
    Ok(())
}

#[derive(Copy, Clone, Debug, Default)]
enum Version {
    #[default]
    V2 = 2,
    V3 = 3,
}
macro_rules! version_info {
    ($($version:ident => {
        vendored_required_feature => $vendored_required_feature:literal,
        vendored_crate => $vendored_crate:ident,
    }),+ $(,)?) => {
        impl Version {
            const ALL: &[Version] = &[$(Version::$version),*];
            fn detect() -> anyhow::Result<Option<Self>> {
                fn force_feature(ver: Version) -> String {
                    format!("force-mimalloc-{ver}")
                }
                let mut matches: Vec<Version> = Vec::new();
                for &ver in Self::ALL {
                    if has_feature(&force_feature(ver)) {
                        matches.push(ver);
                    }
                }
                match matches.len() {
                    0 => Ok(None),
                    1 => Ok(Some(matches[0])),
                    _ => {
                        let flag_names = matches.iter()
                            .copied()
                            .map(force_feature)
                            .collect::<Vec<_>>();
                        Err(anyhow!(
                            "Conflicting feature flags: {}", flag_names.join(", ")
                        ))
                    }
                }
            }
            fn build_vendored(self, opts: &Options, ctx: &CompilationContext) -> anyhow::Result<BuiltLibrary> {
                match self {
                    $(Version::$version => {
                        cfg_if::cfg_if! {
                            if #[cfg(feature = $vendored_required_feature)] {
                                $vendored_crate::build(opts, ctx)
                            } else {
                                panic!(
                                    "Vendored mimalloc {self} not available unless `feature = {name}` is enabled",
                                    name = $vendored_required_feature,
                                )
                            }
                        }
                    },)*
                }
            }
        }
    }
}
version_info! {
    V2 => {
        vendored_required_feature => "vendored",
        vendored_crate => mimalloc_rich_src_v2,
    },
    V3 => {
        vendored_required_feature => "force-mimalloc-v3",
        vendored_crate => mimalloc_rich_src_v3,
    }
}
impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "v{}", *self as u32)
    }
}

fn var(name: impl AsRef<str>) -> Option<String> {
    let name = name.as_ref();
    match std::env::var(name) {
        Ok(success) => Some(success),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(value)) => {
            panic!("Variable {name:?} is not unicode: {}", value.to_string_lossy())
        }
    }
}
fn has_feature(name: &str) -> bool {
    let name = name.replace('-', "_").to_uppercase();
    var(format!("CARGO_FEATURE_{name}")).is_some()
}
