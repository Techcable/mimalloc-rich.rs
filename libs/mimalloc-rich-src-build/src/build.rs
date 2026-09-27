use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow, bail, ensure};
use itertools::Itertools;

use crate::VendoredSource;
use crate::context::CompilationContext;
use crate::options::{DebugAllocTracker, Options};

#[derive(Debug)]
#[non_exhaustive]
pub struct BuiltLibrary {
    pub library_name: String,
    pub library_dir: PathBuf,
    pub header_file: PathBuf,
    pub mimalloc_version: String,
}

pub fn build(code: &VendoredSource, options: &Options, ctx: &CompilationContext) -> anyhow::Result<BuiltLibrary> {
    let Options {
        debug,
        debug_full,
        secure,
        override_malloc,
        alloc_tracker,
        visit_abandoned,
    } = options.clone();
    // basic sanity checks
    ensure!(
        ctx.is_cargo_build_script(),
        "Right now, building does not work outside a cargo build script (due to `cmake` crate)",
    );
    eprintln!("Attempting to build mimalloc at {}", code.source_root.display());
    if !code.source_root.join("CMakeLists.txt").is_file() {
        println!(
            "cargo::error=The vendored mimalloc source code is missing CMakeLists.txt. Is the submodule initialized?"
        );
        bail!("Submodule not initialized");
    }
    assert!(debug || !debug_full, "cannot have `debug-full` without `debug`");

    // basic setup
    crate::version::verify_version(code)?;
    ctx.depend_file(&code.source_root);

    // configure options
    let mut config = cmake::Config::new(&code.source_root);
    if debug {
        config.profile("Debug");
    } else {
        config.profile("Release");
    }
    // enables AddressSanitizer/valgrind
    // TODO: Failure to find AddressSanitizer/valgrind is not currently reported
    match alloc_tracker {
        Some(DebugAllocTracker::Valgrind) => {
            config.define("MI_TRACK_VALGRIND", "ON");
        }
        Some(DebugAllocTracker::AddressSanitizer) => {
            config.define("MI_TRACK_ASAN", "ON");
        }
        None => {}
    }
    if secure {
        config.define("MI_SECURE", "ON");
    }
    if debug_full {
        config.define("MI_DEBUG_FULL", "ON");
    }
    if visit_abandoned {
        // This isn't a cmake flag (at least not currently)
        config.cflag("-DMI_VISIT_ABANDONED");
    }
    config.define("MI_BUILD_SHARED", "OFF"); // not necessary
    config.define("MI_OVERRIDE", if override_malloc { "ON" } else { "OFF" });
    let resulting_build_dir = config.build();
    // TODO: Use pkg-config instead?
    let libs_dir = find_mimalloc_libs_dir(&resulting_build_dir).context("failed to find libs dir")?;
    let library_name = {
        let mut name_parts = vec!["mimalloc"];
        if matches!(alloc_tracker, Some(DebugAllocTracker::AddressSanitizer)) {
            name_parts.push("asan");
        }
        if debug {
            name_parts.push("debug");
        }
        name_parts.join("-")
    };
    Ok(BuiltLibrary {
        library_dir: libs_dir,
        library_name,
        header_file: code.source_root.join("include/mimalloc.h"),
        mimalloc_version: code.expected_version.clone(),
    })
}

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
