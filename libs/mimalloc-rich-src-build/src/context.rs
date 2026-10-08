use std::collections::BTreeSet;
use std::fmt::Display;
use std::path::Path;

fn var(s: &str) -> Option<String> {
    use std::env::VarError;
    match std::env::var(s) {
        Ok(s) => Some(s),
        Err(VarError::NotPresent) => None,
        Err(VarError::NotUnicode(invalid)) => {
            #[allow(clippy::unnecessary_debug_formatting, reason = "want OsStr quoted")]
            {
                panic!("Cargo var {s:?} is not unicode ({invalid:?})")
            }
        }
    }
}

pub struct CompilationContext {
    rust_debug: bool,
    active_sanitizers: BTreeSet<String>,
}
impl CompilationContext {
    pub(crate) fn is_cargo_build_script(&self) -> bool {
        let _ = self;
        true // only supported mode right now
    }

    pub(crate) fn is_rust_debug(&self) -> bool {
        self.rust_debug
    }

    pub(crate) fn is_active_sanitizer(&self, name: &str) -> bool {
        self.active_sanitizers.contains(name)
    }

    pub(crate) fn compile_warning(&self, s: impl Display) {
        let _ = self;
        for line in s.to_string().lines() {
            println!("cargo::warning={line}");
        }
    }

    pub(crate) fn depend_file(&self, s: impl AsRef<Path>) {
        let _ = self;
        println!("cargo:rerun-if-changed={}", s.as_ref().display());
    }

    pub fn cargo_build_script() -> Self {
        CompilationContext {
            rust_debug: is_debug_assertions_enabled(),
            active_sanitizers: match var("CARGO_CFG_SANITIZE") {
                Some(setting) => setting.split(',').map(String::from).collect(),
                None => BTreeSet::new(),
            },
        }
    }
}

/// Check if debug assertions are enabled for the current build.
///
/// # Fallback
/// Before Rust 1.93 (PR [rust-lang/cargo#16160]),
/// the `CARGO_CFG_DEBUG_ASSERTIONS` variable was never set in debug mode.
/// There was no way to detect debug mode with 100% accuracy,
/// but we implement a reasonable fallback on those versions.
///
/// [rust-lang/cargo#16160]: https://github.com/rust-lang/cargo/pull/16160
fn is_debug_assertions_enabled() -> bool {
    if rustversion::cfg!(since(1.93)) {
        var("CARGO_CFG_DEBUG_ASSERTIONS").is_some()
    } else {
        // resaonable but imperfect fallback on old versions of rust.
        var("PROFILE").as_deref() == Some("debug") || var("DEBUG").as_deref() == Some("true")
    }
}
