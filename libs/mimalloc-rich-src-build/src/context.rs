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
            // No way to detect this with 100% accuracy, but this should be decently close
            rust_debug: var("PROFILE").as_deref() == Some("debug") || var("DEBUG").as_deref() == Some("true"),
            active_sanitizers: match var("CARGO_CFG_SANITIZE") {
                Some(setting) => setting.split(',').map(String::from).collect(),
                None => BTreeSet::new(),
            },
        }
    }
}
