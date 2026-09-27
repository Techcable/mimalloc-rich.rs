use crate::context::CompilationContext;

/// Indicates under what condition debugging should be enabled.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum DebugCondition {
    Always,
    IfRustDebug,
    Never,
}
impl DebugCondition {
    fn eval(&self, ctx: &CompilationContext) -> bool {
        match self {
            DebugCondition::Always => true,
            DebugCondition::IfRustDebug => ctx.is_rust_debug(),
            DebugCondition::Never => false,
        }
    }
}

macro_rules! builder {
    ($v:vis struct $name:ident {
        $($field:ident: $tp:ty $(= $def:expr)?),+
        $(,)?
    }) => {
        #[derive(Debug)]
        #[allow(clippy::struct_excessive_bools)]
        $v struct $name {
            $($field: $tp,)*
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl $name {
            pub fn new() -> Self {
                macro_rules! maybe_def {
                    () => (Default::default());
                    ($d:expr) => ($d);
                }
                $name {
                    $($field: maybe_def!($($def)?),)*
                }
            }
            $(pub fn $field(&mut self, val: $tp) -> &mut Self {
                self.$field = val;
                self
            })*
        }
    };
}

builder!(
    pub struct OptionsBuilder {
    debug: DebugCondition = DebugCondition::IfRustDebug,
    debug_full: bool,
    enable_valgrind: bool,
    enable_address_sanitizer: bool,
    override_malloc: bool,
    secure: bool,
    visit_abandoned: bool,
}
);

impl OptionsBuilder {
    pub fn build(&self, ctx: &CompilationContext) -> anyhow::Result<Options> {
        let OptionsBuilder {
            ref debug,
            debug_full,
            enable_valgrind,
            enable_address_sanitizer,
            override_malloc,
            secure,
            visit_abandoned,
        } = *self;
        let debug = debug.eval(ctx);
        Ok(Options {
            debug,
            debug_full: debug && debug_full,
            visit_abandoned,
            secure,
            override_malloc,
            alloc_tracker: {
                match (enable_address_sanitizer, enable_valgrind) {
                    (true, true) => {
                        // cargo features need to be additive,
                        // so giving a warning is better than an error
                        ctx.compile_warning("Valgrind conflicts with AddressSanitizer, disabling valgrind");
                        Some(DebugAllocTracker::AddressSanitizer)
                    }
                    (true, false) => Some(DebugAllocTracker::AddressSanitizer),
                    (false, true) => Some(DebugAllocTracker::Valgrind),
                    (false, false) => {
                        if ctx.is_active_sanitizer("address") || ctx.is_active_sanitizer("hwaddress") {
                            // AddressSanitizer is enabled, so implicitly enable support
                            Some(DebugAllocTracker::AddressSanitizer)
                        } else {
                            None
                        }
                    }
                }
            },
        })
    }
}

/// A tool that tracks allocations, in order to debug memory-safety issues.
#[derive(Copy, Clone, Debug)]
pub(crate) enum DebugAllocTracker {
    Valgrind,
    AddressSanitizer,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct Options {
    pub(crate) debug: bool,
    pub(crate) debug_full: bool,
    pub(crate) secure: bool,
    pub(crate) override_malloc: bool,
    pub(crate) visit_abandoned: bool,
    pub(crate) alloc_tracker: Option<DebugAllocTracker>,
}
