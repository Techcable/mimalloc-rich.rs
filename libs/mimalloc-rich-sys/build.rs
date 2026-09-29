#![expect(missing_docs, reason = "build script doesn't need docs")]
#![allow(
    clippy::std_instead_of_alloc,
    clippy::std_instead_of_core,
    reason = "build scripts can freely use stdlib"
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow, bail};
use camino::Utf8PathBuf;
use itertools::Itertools;

pub fn main() -> anyhow::Result<()> {
    let vendor_dir = Utf8PathBuf::from("./vendored/mimalloc-v2");
    println!("cargo::rerun-if-changed={vendor_dir}");
    if !vendor_dir.join("CMakeLists.txt").is_file() {
        println!("cargo::error=The vendor/mimalloc submodule is not initialized - will fail to build");
        bail!("Submodule not initialized");
    }
    //
    // build project
    //
    if !has_feature("vendored-mimalloc") {
        println!("cargo::error=The `vendored-mimalloc` feature is currently required");
        bail!("Missing required features")
    }
    let rust_debug = var("PROFILE").as_deref() == Some("debug") || var("DEBUG").as_deref() == Some("true");
    let debug_mode = has_feature("debug") || (has_feature("debug-if-debug") && rust_debug);
    println!(
        "should debug {debug_mode}, rust debug {rust_debug}, debug var {:?}",
        var("DEBUG")
    );
    let mut config = cmake::Config::new(&vendor_dir);
    if debug_mode {
        config.profile("Debug");
    } else {
        config.profile("Release");
    }
    // enables AddressSanitizer/valgrind
    // TODO: Failure to find AddressSanitizer/valgrind is not currently reported
    let alloc_tracker = determine_alloc_trackers();
    match alloc_tracker {
        DebugAllocTracker::Valgrind => {
            config.define("MI_TRACK_VALGRIND", "ON");
        }
        DebugAllocTracker::AddressSanitizer => {
            config.define("MI_TRACK_ASAN", "ON");
        }
        DebugAllocTracker::Disabled => {}
    }
    if has_feature("secure") {
        config.define("MI_SECURE", "ON");
    }
    if has_feature("debug-full") {
        config.define("MI_DEBUG_FULL", "ON");
    }
    if has_feature("unstable-enable-visit-abandoned") {
        config.cflag("-DMI_VISIT_ABANDONED");
    }
    config.define("MI_BUILD_SHARED", "OFF"); // not necessary
    config.define("MI_OVERRIDE", if has_feature("override-libc-malloc") { "ON" } else { "OFF" });
    let result = config.build();
    // TODO: Use pkg-config instead?
    fn find_mimalloc_libs_dir(base: &Path) -> anyhow::Result<PathBuf> {
        // in a subdirectory lib/mimalloc-2.2
        // want to find this without hardcoding the version
        let libs_dir = ["lib", "lib64"]
            .into_iter()
            .map(|name| base.join(name))
            .find(|dir| dir.is_dir())
            .ok_or_else(|| anyhow!("Failed to find primary libs subdir"))?;
        // if lib/mimalloc exists (without suffix), prefer that
        let preferred_entry = libs_dir.join("mimalloc");
        if preferred_entry.is_dir() {
            return Ok(preferred_entry);
        }
        let libs_dir_entries = libs_dir.read_dir()?;
        libs_dir_entries.process_results(|entries| {
            entries
                .collect_vec()
                .into_iter()
                .filter(|entry| entry.file_name().to_str().is_some_and(|x| x.starts_with("mimalloc-")))
                .map(|entry| entry.path())
                .exactly_one()
                .with_context(|| {
                    format!(
                        "Expected to find a single `mimalloc-*` lib directory in {}",
                        libs_dir.display()
                    )
                })
        })?
    }
    let libs_dir = find_mimalloc_libs_dir(&result).context("failed to find libs dir")?;
    println!("cargo:rustc-link-search=native={}", libs_dir.display());
    let library_name = {
        let mut name_parts = vec!["mimalloc"];
        if matches!(alloc_tracker, DebugAllocTracker::AddressSanitizer) {
            name_parts.push("asan");
        }
        if debug_mode {
            name_parts.push("debug");
        }
        name_parts.join("-")
    };
    println!("cargo:rustc-link-lib=static={library_name}");

    //
    // generate bindings
    //
    println!("generating bindings");
    let bindings = bindgen::Builder::default()
        .header(vendor_dir.join("include/mimalloc.h"))
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

fn determine_alloc_trackers() -> DebugAllocTracker {
    let explicitly_sanitize = has_feature("sanitize-address");
    let explicit_valgrind = has_feature("valgrind");
    match (explicitly_sanitize, explicit_valgrind) {
        (true, true) => {
            println!("cargo::warning=Valgrind conflicts with AddressSanitizer, disabling valgrind");
            DebugAllocTracker::AddressSanitizer
        }
        (true, false) => DebugAllocTracker::AddressSanitizer,
        (false, true) => DebugAllocTracker::Valgrind,
        (false, false) => {
            let active_sanitizers = active_sanitizers();
            if active_sanitizers.contains("address") || active_sanitizers.contains("hwaddress") {
                // AddressSanitizer is enabled, so implicitly enable support
                DebugAllocTracker::AddressSanitizer
            } else {
                DebugAllocTracker::Disabled
            }
        }
    }
}
/// A special allocation tracker.
enum DebugAllocTracker {
    Valgrind,
    AddressSanitizer,
    Disabled,
}

fn active_sanitizers() -> BTreeSet<String> {
    match var("CARGO_CFG_SANITIZE") {
        Some(setting) => setting.split(',').map(String::from).collect(),
        None => BTreeSet::new(),
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
