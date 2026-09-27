#![expect(missing_docs, reason = "build script doesn't need docs")]
#![allow(
    clippy::std_instead_of_alloc,
    clippy::std_instead_of_core,
    reason = "build scripts can freely use stdlib"
)]

use std::path::PathBuf;

use anyhow::{Context, bail};
use mimalloc_rich_src_build::context::CompilationContext;
use mimalloc_rich_src_build::options::{DebugCondition, OptionsBuilder};

pub fn main() -> anyhow::Result<()> {
    //
    // build project
    //
    if !has_feature("vendored-mimalloc") {
        println!("cargo::error=The `vendored-mimalloc` feature is currently required");
        bail!("Missing required features")
    }

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

    let result = ({
        cfg_if::cfg_if! {
            if #[cfg(feature = "vendored-mimalloc")] {
                mimalloc_rich_src_v2::build(&options, &ctx)
            } else {
                unreachable!("no vendored version selected")
            }
        }
    })
    .context("Failed to build vendored mimalloc")?;

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
