use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail, ensure};

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
        "Right now, building does not work outside a cargo build script (due to `cc` crate)",
    );
    eprintln!("Attempting to build mimalloc at {}", code.source_root.display());
    if !code.source_root.join("src/static.c").is_file() {
        println!(
            "cargo::error=The vendored mimalloc source code is missing src/static.c. Is the submodule initialized?"
        );
        bail!("Submodule not initialized");
    }
    assert!(debug || !debug_full, "cannot have `debug-full` without `debug`");

    // basic setup
    crate::version::verify_version(code)?;
    ctx.depend_file(&code.source_root);

    // Let cc handle compiler selection, Cargo's optimization/debug settings, and
    // link metadata. All mimalloc sources are included by this translation unit.
    let library_dir = PathBuf::from(std::env::var_os("OUT_DIR").context("OUT_DIR is not set")?);
    let target_os = std::env::var("CARGO_CFG_TARGET_OS")?;
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV")?;
    let target_vendor = std::env::var("CARGO_CFG_TARGET_VENDOR")?;
    let mut config = cc::Build::new();
    // Both versions use MI_DEBUG=2 for debug builds and =3 for full checks.
    // v3 renamed the CMake option MI_DEBUG_FULL to MI_DEBUG=FULL, but the C
    // definition is unchanged, so this mapping works for both versions.
    // https://github.com/microsoft/mimalloc/blob/v2.5.2/CMakeLists.txt#L312-L331
    // https://github.com/microsoft/mimalloc/blob/v3.6.1/CMakeLists.txt#L426-L468
    config
        .include(code.source_root.join("include"))
        .define("MI_STATIC_LIB", None)
        .define(
            "MI_DEBUG",
            if debug_full {
                "3"
            } else if debug {
                "2"
            } else {
                "0"
            },
        );
    if !debug {
        config.define("NDEBUG", None).define("MI_BUILD_RELEASE", None);
    }
    // TODO(v3): Match CMake's debug sampling defaults explicitly. v2's header
    // enables MI_GUARDED=1 in debug builds. v3's header also defaults to =1 and
    // MI_PROFILE=1, while v3's CMake build selects =2 for both in debug builds
    // (v3.6.1 disables guarded sampling on Emscripten/WASI).
    // This direct build currently uses the header defaults, so it lacks v3's
    // fine-grained debug sampling. Release profiling already defaults to =1.
    // https://github.com/microsoft/mimalloc/blob/v2.5.2/include/mimalloc/types.h#L79-L82
    // https://github.com/microsoft/mimalloc/blob/v3.6.1/include/mimalloc/types.h#L104-L123
    // https://github.com/microsoft/mimalloc/blob/v3.6.1/CMakeLists.txt#L470-L487
    // https://github.com/microsoft/mimalloc/blob/v3.6.1/CMakeLists.txt#L540-L557
    if target_env == "msvc" {
        // Match upstream: MSVC uses C++ atomics instead of C11 atomics.
        // https://github.com/microsoft/mimalloc/blob/v3.6.1/CMakeLists.txt#L259-L279
        config.cpp(true).std("c++17").flag("/Zc:__cplusplus");
    } else {
        config
            .std("c11")
            .flag_if_supported("-fvisibility=hidden")
            .flag_if_supported("-Wno-unknown-pragmas");
        if target_os != "haiku" {
            config.flag_if_supported(if target_env == "musl" {
                "-ftls-model=local-dynamic"
            } else {
                "-ftls-model=initial-exec"
            });
        }
    }
    if target_env == "musl" {
        config.define("MI_LIBC_MUSL", "1");
    }
    if target_os == "windows" && target_env == "gnu" {
        config.define("_WIN32_WINNT", "0x600");
    }
    // Upstream's ARM64 optimization flags differ: v2 selects ARMv8.1, while
    // v3 selects ARMv8.3 on Apple and ARMv8.1 elsewhere. This helper currently
    // leaves architecture flags to cc and user CFLAGS instead of matching those
    // upstream defaults.
    // https://github.com/microsoft/mimalloc/blob/v2.5.2/CMakeLists.txt#L493-L503
    // https://github.com/microsoft/mimalloc/blob/v3.6.1/CMakeLists.txt#L787-L826
    if secure {
        // Both versions map secure mode to MI_SECURE=4. CMake also defines
        // MI_FREE_IS_CHECKED=1 for v3. TODO(v3.5.3): Set it explicitly for
        // the vendored version; v3.6.1 derives it in types.h for MI_SECURE>=2.
        // https://github.com/microsoft/mimalloc/blob/v2.5.2/CMakeLists.txt#L233-L240
        // https://github.com/microsoft/mimalloc/blob/v3.6.1/CMakeLists.txt#L338-L352
        // https://github.com/microsoft/mimalloc/blob/v3.6.1/CMakeLists.txt#L499-L509
        // https://github.com/microsoft/mimalloc/blob/v3.6.1/include/mimalloc/types.h#L77-L81
        config.define("MI_SECURE", "4");
    }
    if visit_abandoned {
        // v2 uses this definition to enable its visit_abandoned runtime option
        // by default. v3 no longer reads it: abandoned blocks are visited through
        // a per-heap API, and the old runtime option is deprecated.
        // https://github.com/microsoft/mimalloc/blob/v2.5.2/src/options.c#L158-L162
        // https://github.com/microsoft/mimalloc/blob/v3.6.1/include/mimalloc.h#L328-L330
        // https://github.com/microsoft/mimalloc/blob/v3.6.1/include/mimalloc.h#L499
        config.define("MI_VISIT_ABANDONED", "1");
    }
    if override_malloc {
        config.define("MI_MALLOC_OVERRIDE", None);
        if target_env != "msvc" {
            config.flag_if_supported("-fno-builtin-malloc");
        }
        if target_vendor == "apple" {
            config.define("MI_OSX_ZONE", "1").define("MI_OSX_INTERPOSE", "1");
        }
    }
    // As in the upstream build, unavailable tracking headers disable tracking
    // with a warning. Probe with the target compiler, including user CFLAGS.
    // v3 consolidates the CMake switches into MI_TRACK=VALGRIND/ASAN, but still
    // emits the same MI_TRACK_VALGRIND/MI_TRACK_ASAN C definitions used here.
    // https://github.com/microsoft/mimalloc/blob/v3.6.1/CMakeLists.txt#L354-L391
    match alloc_tracker {
        Some(DebugAllocTracker::Valgrind) => {
            if has_tracking_headers(&config, &library_dir, &["valgrind/valgrind.h", "valgrind/memcheck.h"])? {
                config.define("MI_TRACK_VALGRIND", "1");
            } else {
                ctx.compile_warning("Valgrind headers not found; disabling Valgrind support");
            }
        }
        Some(DebugAllocTracker::AddressSanitizer) => {
            if target_vendor == "apple" && override_malloc {
                ctx.compile_warning(
                    "AddressSanitizer is incompatible with malloc overrides on Apple targets; disabling tracking",
                );
            } else if has_tracking_headers(&config, &library_dir, &["sanitizer/asan_interface.h"])? {
                config.define("MI_TRACK_ASAN", "1");
                config.flag(if target_env == "msvc" {
                    "/fsanitize=address"
                } else {
                    "-fsanitize=address"
                });
            } else {
                ctx.compile_warning("AddressSanitizer headers not found; disabling AddressSanitizer support");
            }
        }
        None => {}
    }
    let library_name = "mimalloc".to_owned();
    // v2 includes segment.c and segment-map.c; v3 uses page-map, thread-local
    // heap, sampling, and subprocess implementations. v3.6.1 organizes these
    // under arena/, alloc/, heap/, prim/, profile/, and util/. Each version
    // includes its own source set in static.c, so no version-specific file
    // list is needed here.
    // https://github.com/microsoft/mimalloc/blob/v2.5.2/src/static.c#L23-L41
    // https://github.com/microsoft/mimalloc/blob/v3.6.1/src/static.c#L29-L65
    config
        .file(code.source_root.join("src/static.c"))
        .try_compile(&library_name)
        .context("Failed to compile mimalloc src/static.c")?;

    // Native dependencies previously selected by CMake. Emit these after the
    // static archive so linkers can resolve mimalloc's external symbols.
    // https://github.com/microsoft/mimalloc/blob/v3.6.1/CMakeLists.txt#L885-L900
    if target_os == "windows" {
        for lib in ["psapi", "shell32", "user32", "advapi32", "bcrypt"] {
            println!("cargo::rustc-link-lib={lib}");
        }
    } else if std::env::var("CARGO_CFG_TARGET_FAMILY").is_ok_and(|families| families.split(',').any(|f| f == "unix")) {
        link_unix_libraries(&config, &library_dir)?;
    }
    Ok(BuiltLibrary {
        library_dir,
        library_name,
        header_file: code.source_root.join("include/mimalloc.h"),
        mimalloc_version: code.expected_version.clone(),
    })
}

/// Emit link directives for optional Unix libraries accepted by the target C compiler's linker.
///
/// This tests library availability, not whether mimalloc needs any particular symbol.
/// The executable is only linked, never run, so cross-compilation needs no target runner.
/// This assumes the C toolchain and Rust's final linker can find the same libraries.
/// Rust precedent: `libz-sys` also invokes the compiler selected by `cc` to link a
/// smoke program, then emits a link directive on success. Its probe additionally
/// references a zlib symbol; ours follows mimalloc's library-availability checks.
///
/// - <https://github.com/rust-lang/libz-sys/blob/1.1.31/build.rs#L238-L253>
/// - <https://github.com/rust-lang/libz-sys/blob/1.1.31/build.rs#L87-L98>
/// - <https://github.com/rust-lang/libz-sys/blob/1.1.31/src/smoke.c#L1-L5>
///
/// An alternative is to maintain target-specific link rules, as `libmimalloc-sys`
/// does for Linux ARM's atomic support. Here we retain mimalloc's upstream probes
/// to avoid assuming that a target always provides a particular library.
/// <https://github.com/purpleprotocol/mimalloc_rust/blob/v0.1.49/libmimalloc-sys/build.rs#L86-L93>
fn link_unix_libraries(config: &cc::Build, out_dir: &Path) -> anyhow::Result<()> {
    // Some targets provide these in libc, others need separate libraries.
    // Like upstream, probe by linking (never running) with the target compiler.
    // https://github.com/microsoft/mimalloc/blob/v3.6.1/CMakeLists.txt#L866-L883
    let probe = out_dir.join("mimalloc-link-libraries.c");
    std::fs::write(&probe, "int main(void) { return 0; }\n").context("Failed to write library probe")?;
    let compiler = config.try_get_compiler()?;
    for lib in ["pthread", "rt", "atomic"] {
        let output = compiler
            .to_command()
            .arg(&probe)
            .arg(format!("-l{lib}"))
            .arg("-o")
            .arg(out_dir.join("mimalloc-link-libraries"))
            .output()
            .with_context(|| format!("Failed to probe native library {lib}"))?;
        if output.status.success() {
            println!("cargo::rustc-link-lib={lib}");
        }
    }
    Ok(())
}

/// Check whether the target toolchain can compile the requested allocation-tracking headers.
///
/// Enabling Valgrind or ASan tracking makes mimalloc include external development
/// headers. A Cargo feature does not ensure those headers are installed. Mimalloc's
/// CMake build probes them and disables unavailable tracking with a warning; this
/// check preserves that fallback instead of failing the entire mimalloc compilation.
/// See <https://github.com/microsoft/mimalloc/blob/v3.6.1/CMakeLists.txt#L365-L391>.
///
/// Compiling with a clone of the configured `cc::Build` checks the target compiler,
/// sysroot, include paths, language, and user flags rather than host filesystem paths.
/// No target executable is linked or run. Probe metadata and compiler diagnostics
/// are suppressed so Cargo does not link the probe archive and callers can issue
/// one tracking-specific warning. A failed compile/archive returns `false`; writing
/// the probe can return an error. Success does not establish sanitizer runtime or
/// instrumentation support, which the later build and final link must provide.
fn has_tracking_headers(config: &cc::Build, out_dir: &Path, headers: &[&str]) -> anyhow::Result<bool> {
    let probe = out_dir.join("mimalloc-tracking-headers.c");
    let mut source = String::new();
    for header in headers {
        writeln!(source, "#include <{header}>")?;
    }
    source.push_str("int mimalloc_tracking_headers(void) { return 0; }\n");
    std::fs::write(&probe, source).context("Failed to write tracking header probe")?;
    Ok(config
        .clone()
        .cargo_metadata(false)
        .cargo_warnings(false)
        .file(probe)
        .try_compile("mimalloc-tracking-headers")
        .is_ok())
}
