//! Defines runtime options for mimalloc, wrapping a [`mi_option_t`].
//!
//! Instead of being an enumeration of possible options,
//! each option gets its own type.
//!
//! Not all options are currently supported.
//! The defaults in the documentation are copied from the docs for [`mi_option_t`] and may be outdated.
//!
//! # Safety
//! Changing certain options can invalidate the assumptions of other code.
//! It is the responsibility of the user to ensure changing the options don't break anything
//!
//! As an example, [DuckLogic](https://ducklogic.org) uses the [`AllowVisitAbandoned`] option to visit memory allocated by
//! dead threads.
//! If this option is disabled, the garbage collector will not be able to properly trace these objects,
//! violating memory safety.
//!
//! Other options like [`DestroyOnExit`] are dangerous because they could free memory still in use.
//!
//! [`mi_option_t`]: https://microsoft.github.io/mimalloc/group__options.html#gafebf7ed116adb38ae5218bc3ce06884c
/*
 * TODO: Simplify the type/trait nonsense
 *
 * We could potentially have a function per non-bool argument,
 * like `set_max_warnings` without any loss in flexibility.
 */

use core::ffi::c_long;
use core::fmt::Debug;

use sealed::{ConversionFromRawValueError, OptionKind};

use crate::options::sealed::{PrivateMiOption, RawOptionValue};

/// Represents a specific runtime option.
pub trait MiRuntimeOption: Copy + Debug + PrivateMiOption {
    /// Identify the option as a [`sys::mi_option_t`].
    fn ffi_id() -> sys::mi_option_t;
}
/// A [`MiRuntimeOption`] with a boolean value,
/// where `true` means the option is enabled.
pub trait MiFlagOption: MiRuntimeOption {
    /// Convert this flag to a boolean/
    fn to_bool(&self) -> bool;
}
macro_rules! declare_simple_flags {
    (
        $(
            $(#[$meta_attr:meta])*
            $name:ident as $ffi:ident
        ),+ $(,)?
    ) => {
        $(
            $(#[$meta_attr])*
            #[derive(Copy, Clone, Debug)]
            pub struct $name(pub bool);
            impl MiRuntimeOption for $name {
                #[inline(always)]
                fn ffi_id() -> sys::mi_option_t {
                    sys::mi_option_t::$ffi
                }
            }
            impl sealed::PrivateMiOption for $name {
                const NAME: &'static str = stringify!($name);
                const KIND: OptionKind = OptionKind::Flag;
                type RawValue = bool;
                #[inline]
                fn raw_value(&self) -> bool {
                    self.0
                }
                #[inline]
                fn from_raw_value(value: Self::RawValue) -> Result<Self, ConversionFromRawValueError> {
                    Ok($name(value))
                }
            }
            impl MiFlagOption for $name {
                #[inline]
                fn to_bool(&self) -> bool {
                    self.0
                }
            }
        )*
    }
}
declare_simple_flags!(
    /// Print error messages.
    ShowErrors as mi_option_show_errors,
    /// Print statistics on termination.
    ShowStats as mi_option_show_stats,
    /// Print verbose messages.
    Verbose as mi_option_verbose,
    /// Allow large (2 or 4 MiB) OS pages, implies eager commit.
    ///
    /// If false, also disables THP for the process.
    AllowLargeOsPages as mi_option_allow_large_os_pages,
    /// Should a memory purge decommit?.
    ///
    /// Set to `false` to use memory reset on a purge (instead of decommit).
    PurgeDecommits as mi_option_purge_decommits,
    /// Eagerly commit segments? (enabled by default).
    ///
    /// See also [`EagerCommitDelay`].
    EagerCommit as mi_option_eager_commit,
    /// Immediately purge delayed purges on thread termination.
    AbandonedPagePurge as mi_option_abandoned_page_purge,
    /// Do not use OS memory for allocation (but only programmatically reserved arenas)
    DisallowOsAlloc as mi_option_disallow_os_alloc,
    /// If enabled, do not use OS memory for allocation (but only pre-reserved arenas).
    LimitOsAlloc as mi_option_limit_os_alloc,
    /// Allow to reclaim an abandoned segment on a free (default true)
    AbandonedReclaimOnFree as mi_option_abandoned_reclaim_on_free,
    /// Extend purge delay on each subsequent delay (default true).
    PurgeExtendDelay as mi_option_purge_extend_delay,
    /// If true, do not use arena's for allocation (except if using specific arena id's)
    DisallowArenaAlloc as mi_option_disallow_arena_alloc,
    /// Allow visiting heap blocks from abandoned threads (default false).
    AllowVisitAbandoned as mi_option_visit_abandoned,
    /// If set, release all memory on exit.
    ///
    /// Sometimes used for dynamic unloading, but can be unsafe.
    DestroyOnExit as mi_option_destroy_on_exit,
);

macro_rules! declare_enum_options {
    (
        $(
            $(#[$primary_attr:meta])*

            $target:ident {
                $(
                    $(#[$variant_meta:meta])*
                    $variant:ident = $discriminant:literal,
                )*
            } as $ffi:ident
        ),+ $(,)?
    ) => {
        $(
            $(#[$primary_attr])*
            #[derive(Copy, Clone, Debug, Eq, PartialEq)]
            pub enum $target {
                $(
                    $(#[$variant_meta])*
                    $variant = $discriminant,
                )*
            }
            impl sealed::PrivateMiOption for $target {
                const NAME: &'static str = stringify!($target);
                const KIND: OptionKind = OptionKind::U32;
                type RawValue = u32;

                #[inline]
                fn from_raw_value(value: Self::RawValue) -> Result<Self, ConversionFromRawValueError> {
                    let res = match value {
                        $($discriminant => $target::$variant,)*
                        _ => return Err(ConversionFromRawValueError),
                    };
                    assert_eq!(res.raw_value(), value);
                    Ok(res)
                }

                #[inline]
                fn raw_value(&self) -> Self::RawValue {
                    match *self {
                        $($target::$variant => $discriminant,)*
                    }
                }
            }
            impl MiRuntimeOption for $target {
                #[inline]
                fn ffi_id() -> sys::mi_option_t {
                    sys::mi_option_t::$ffi
                }
            }
        )*
    }
}

declare_enum_options!(
    /// Whether to eager commit arenas.
    ArenaEagerCommit {
        /// Enable arena eager commit.
        Enabled = 1,
        /// Disable arena eager commit
        Disabled = 0,
        /// Only enable arena eager commit on overcommit systems (see [wikipedia]).
        ///
        /// [wikipedia]: https://en.wikipedia.org/wiki/Memory_overcommitment
        OnlyOnOvercommitSystems = 2,
    } as mi_option_arena_eager_commit,
);

macro_rules! declare_complex_options {
    (
        $(
            $(#[$meta_attr:meta])*
            $name:ident {
                $(#[$field_attr:meta])*
                pub $field:ident: $tp:ty,
                ffi => $ffi:ident,
                kind => $kind:expr,
            }
        ),+ $(,)?
    ) => {
        $(
            $(#[$meta_attr])*
            #[derive(Copy, Clone, Debug)]
            pub struct $name {
                $(#[$field_attr])*
                pub $field: $tp,
            }
            impl MiRuntimeOption for $name {
                #[inline]
                fn ffi_id() -> sys::mi_option_t {
                    sys::mi_option_t::$ffi
                }
            }
            impl sealed::PrivateMiOption for $name {
                const NAME: &'static str = stringify!($name);
                const KIND: OptionKind = $kind;
                type RawValue = $tp;
                #[inline]
                fn from_raw_value(value: Self::RawValue) -> Result<Self, sealed::ConversionFromRawValueError> {
                    Ok(Self { $field: value })
                }
                #[inline]
                fn raw_value(&self) -> Self::RawValue {
                    self.$field
                }
            }
        )*
    };
}
declare_complex_options!(
    /// Issue at most `limit` error messages.
    MaxErrors {
        /// The limit on error messages.
        pub limit: u32,
        ffi => mi_option_max_errors,
        kind => OptionKind::U32,
    },
    /// Issue at most `limit` warning messages.
    MaxWarnings {
        /// The limit on warning messages.
        pub limit: u32,
        ffi => mi_option_max_warnings,
        kind => OptionKind::U32,
    },
    /// Reserve `count` huge OS pages (1GiB pages) at startup
    ReserveHugeOsPages {
        /// Number of pages to reserve.
        pub count: u32,
        ffi => mi_option_reserve_huge_os_pages,
        kind => OptionKind::U32,
    },
    /// Reserve specified amount of OS memory in an arena at startup.
    ReserveOsMemory {
        /// Number of bytes to reserve.
        ///
        /// Will be implicitly rounded down to the nearest multiple of the internal size (currently 1 KiB).
        pub bytes: usize,
        ffi => mi_option_reserve_os_memory,
        kind => OptionKind::Size,
    },

    /// Set the initial memory size for arena reservation (default to 1 GiB when 64-bit).
    ArenaReserve {
        /// Number of bytes to reserve.
        ///
        /// Will be implicitly rounded down to the nearest multiple of the internal size (currently 1 KiB).
        pub bytes: usize,
        ffi => mi_option_arena_reserve,
        kind => OptionKind::Size,
    },
    /// Tag used for OS logging (macOS only for now) (default is 100).
    OsTag {
        /// The tag value.
        pub tag: u32,
        ffi => mi_option_os_tag,
        kind => OptionKind::U32,
    },
    /// Retry on out-of-memory for `retry` milliseconds, set to `0` to disable retries.
    ///
    /// Default is 400.
    /// This is only supported on windows.
    RetryOnOutOfMemory {
        /// Number of times to retry, or zero to disable.
        pub retry: u32,
        ffi => mi_option_retry_on_oom,
        kind => OptionKind::U32,
    },
    /// The first `segments` segments per thread are not eagerly committed,
    /// but per page in the segment on demand.
    EagerCommitDelay {
        /// Number of segments which should not be eagerly committed.
        pub segments: u32,
        ffi => mi_option_eager_commit_delay,
        kind => OptionKind::U32,
    },
    /// Delay memory purging by `delay` milliseconds.
    ///
    /// Use 0 for immediate purging or `None` for no purging at all. (default to 10)
    PurgeDelay {
        /// The dealy purging delay in milliseconds,
        /// or `None` if purging should not happen at all.
        ///
        /// Note that `0` and `None` have different meanings.
        pub delay: Option<u32>,
        ffi => mi_option_purge_delay,
        kind => OptionKind::Optional {
            inner: &OptionKind::U32,
            none_value: -1,
        },
    },
    /// If `limit=None` use all available numa nodes, otherwise use at most `limit` nodes.
    UseNumaNodes {
        /// The number of nodes to limit too, or `None` if there is no limit.
        ///
        /// Note that zero is not a valid limit and will cause a panic.
        pub limit: Option<u32>,
        ffi => mi_option_use_numa_nodes,
        kind => OptionKind::Optional {
            inner: &OptionKind::U32,
            none_value: 0,
        },
    },
    /// Maximum percentage of the abandoned segments can be reclaimed per try (default 10%)
    MaxSegmentReclaim {
        /// The percentage of segments to reclaim.
        ///
        /// Must be between 0 and 100.
        pub percent: u32,
        ffi => mi_option_max_segment_reclaim,
        kind => OptionKind::Percentage,
    },

    /// Multiplier for [`PurgeDelay`] option, which controls the purging of arenas.
    ///
    /// Default value is a multiplier of 10.
    ArenaPurgeMultiplier {
        /// The multiplier.
        pub multiplier: u32,
        ffi => mi_option_arena_purge_mult,
        kind => OptionKind::U32,
    },
);

/// Enable a mimalloc runtime flag.
///
/// ## Safety
/// Depending on the flag, this can trigger UB if this invalidates the assumptions of existing code.
/// See module documentation for more details.
#[inline]
pub unsafe fn mi_option_enable<F: MiFlagOption>() {
    // SAFETY: Responsibility of caller
    unsafe {
        sys::mi_option_enable(F::ffi_id());
    }
}

/// Disable a mimalloc runtime flag.
///
/// ## Safety
/// Depending on the flag, this can trigger UB if this invalidates the assumptions of existing code.
/// See module documentation for more details.
#[inline]
pub unsafe fn mi_option_disable<F: MiFlagOption>() {
    // SAFETY: Responsibility of caller
    unsafe {
        sys::mi_option_disable(F::ffi_id());
    }
}

///  Set whether a mimalloc runtime flag is enabled (`true`) or disabled (`false`).
///
/// ## Safety
/// Depending on the flag, this can trigger UB if this invalidates the assumptions of existing code.
/// See module documentation for more details.
#[inline]
pub unsafe fn mi_option_set_enabled<F: MiFlagOption>(enabled: bool) {
    // SAFETY: Responsibility of caller
    unsafe { sys::mi_option_set_enabled(F::ffi_id(), enabled) }
}

/// Set a mimalloc runtime flag.
///
/// ## Safety
/// Depending on the flag, this can trigger UB if this invalidates the assumptions of existing code.
/// See module documentation for more details.
///
/// ## Panics
/// Panics if the value passed is not valid.
/// For example, percentages must be between zero and 100.
/// This interface is not guaranteed to catch all errors,
/// but should hopefully catch most.
#[track_caller]
#[inline] // hopefully constant folded
pub unsafe fn mi_option_set<F: MiRuntimeOption>(option: F) {
    match option.raw_value().convert_to_raw_value(F::KIND) {
        Ok(result) => {
            // SAFETY: Responsibility of caller
            unsafe { sys::mi_option_set(F::ffi_id(), result) }
        }
        Err(cause) => {
            panic!("Invalid option {option:?}: {cause}")
        }
    }
}

/// Indicates that the value that would otherwise be return from [`mi_option_get`] is not valid.
#[derive(Debug, thiserror::Error)]
#[error("Value {raw_value} is not valid for {option_name}")]
pub struct InvalidOptionGetError {
    raw_value: c_long,
    option_name: &'static str,
}
/// Get the value of the specified [`MiRuntimeOption`].
///
/// ## Errors
/// Returns an error if the current value is not actually valid.
#[inline] // hopefully constant folded
pub fn mi_option_get<F: MiRuntimeOption>() -> Result<F, InvalidOptionGetError> {
    // SAFETY: Safe to call
    let raw_value: c_long = unsafe { sys::mi_option_get(F::ffi_id()) };
    <F::RawValue as RawOptionValue>::from_raw_value(raw_value, F::KIND)
        .and_then(F::from_raw_value)
        .map_err(|_| InvalidOptionGetError {
            raw_value,
            option_name: F::NAME,
        })
}

/// Adjustment for size options ([`OptionKind::Size`]).
const SIZE_ADJUSTMENT: usize = 1024;

mod sealed {
    use core::convert::Infallible;
    use core::ffi::c_long;
    use core::fmt::Debug;
    use core::num::TryFromIntError;

    use crate::options::SIZE_ADJUSTMENT;

    #[derive(Eq, PartialEq, Copy, Clone, Debug)]
    pub enum OptionKind {
        Size,
        Flag,
        Percentage,
        U32,
        Optional {
            inner: &'static OptionKind,
            none_value: c_long,
        },
    }

    pub trait PrivateMiOption: Sized {
        const NAME: &'static str;
        const KIND: OptionKind;
        type RawValue: RawOptionValue;
        fn from_raw_value(value: Self::RawValue) -> Result<Self, ConversionFromRawValueError>;
        fn raw_value(&self) -> Self::RawValue;
    }
    #[derive(Copy, Clone, Debug)]
    pub struct ConversionFromRawValueError;
    impl From<TryFromIntError> for ConversionFromRawValueError {
        #[inline]
        fn from(_: TryFromIntError) -> Self {
            ConversionFromRawValueError
        }
    }
    impl From<Infallible> for ConversionFromRawValueError {
        #[inline]
        fn from(x: Infallible) -> Self {
            match x {}
        }
    }
    #[derive(Debug, thiserror::Error)]
    pub enum RawValueIntoConversionError {
        #[error(
            "Integer overflowed a C `long` ({long_bits} bits)",
            long_bits = c_long::BITS
        )]
        IntegerOverflow,
        #[error("Value {value} is used to represent `None`")]
        ForbiddenOptionValue {
            value: c_long,
        },
        #[error("Not a valid percentage")]
        InvalidPercentage,
    }
    impl From<TryFromIntError> for RawValueIntoConversionError {
        #[inline]
        fn from(_: TryFromIntError) -> Self {
            Self::IntegerOverflow
        }
    }
    impl From<core::convert::Infallible> for RawValueIntoConversionError {
        #[inline]
        fn from(value: core::convert::Infallible) -> Self {
            match value {}
        }
    }
    pub trait RawOptionValue: Copy + Debug {
        fn from_raw_value(value: c_long, kind: OptionKind) -> Result<Self, ConversionFromRawValueError>;
        fn convert_to_raw_value(self, kind: OptionKind) -> Result<c_long, RawValueIntoConversionError>;
    }
    impl RawOptionValue for bool {
        #[inline]
        fn from_raw_value(value: c_long, kind: OptionKind) -> Result<Self, ConversionFromRawValueError> {
            assert_eq!(kind, OptionKind::Flag);
            match value {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(ConversionFromRawValueError),
            }
        }

        #[inline]
        fn convert_to_raw_value(self, kind: OptionKind) -> Result<c_long, RawValueIntoConversionError> {
            assert_eq!(kind, OptionKind::Flag);
            Ok(self as c_long)
        }
    }
    impl RawOptionValue for usize {
        #[inline]
        fn from_raw_value(value: c_long, kind: OptionKind) -> Result<Self, ConversionFromRawValueError> {
            assert_eq!(kind, OptionKind::Size);
            usize::try_from(value)
                .ok()
                .and_then(|x| x.checked_mul(SIZE_ADJUSTMENT))
                .ok_or(ConversionFromRawValueError)
        }

        #[inline]
        fn convert_to_raw_value(self, kind: OptionKind) -> Result<c_long, RawValueIntoConversionError> {
            assert_eq!(kind, OptionKind::Size);
            (self / SIZE_ADJUSTMENT)
                .try_into()
                .map_err(RawValueIntoConversionError::from)
        }
    }
    impl RawOptionValue for u32 {
        #[inline]
        fn from_raw_value(value: c_long, kind: OptionKind) -> Result<Self, ConversionFromRawValueError> {
            let value = u32::try_from(value)?;
            match kind {
                OptionKind::Percentage => {
                    if value <= 100 {
                        Ok(value)
                    } else {
                        Err(ConversionFromRawValueError)
                    }
                }
                OptionKind::U32 => Ok(value),
                _ => unreachable!("{kind:?}"),
            }
        }

        #[inline]
        #[allow(
            clippy::unnecessary_fallible_conversions,
            reason = "size of c_long is target specific"
        )]
        fn convert_to_raw_value(self, kind: OptionKind) -> Result<c_long, RawValueIntoConversionError> {
            match kind {
                OptionKind::Percentage => {
                    if self <= 100 {
                        Ok(self.try_into().expect("impossible"))
                    } else {
                        Err(RawValueIntoConversionError::InvalidPercentage)
                    }
                }
                OptionKind::U32 => self.try_into().map_err(RawValueIntoConversionError::from),
                _ => unreachable!("{kind:?}"),
            }
        }
    }
    impl<T: RawOptionValue> RawOptionValue for Option<T> {
        #[inline]
        fn from_raw_value(value: c_long, kind: OptionKind) -> Result<Self, ConversionFromRawValueError> {
            let OptionKind::Optional {
                none_value,
                inner: inner_kind,
            } = kind
            else {
                panic!("unexpected kind {kind:?}");
            };
            if value == none_value {
                Ok(None)
            } else {
                Ok(Some(T::from_raw_value(value, *inner_kind)?))
            }
        }

        #[inline]
        fn convert_to_raw_value(self, kind: OptionKind) -> Result<c_long, RawValueIntoConversionError> {
            let OptionKind::Optional {
                none_value,
                inner: inner_kind,
            } = kind
            else {
                panic!("unexpected kind {kind:?}");
            };
            assert!(!matches!(inner_kind, OptionKind::Optional { .. }), "{inner_kind:?}");
            match self {
                None => Ok(none_value),
                Some(inner_value) => {
                    let inner_raw = inner_value.convert_to_raw_value(*inner_kind)?;
                    if inner_raw == none_value {
                        Err(RawValueIntoConversionError::ForbiddenOptionValue { value: inner_raw })
                    } else {
                        Ok(inner_raw)
                    }
                }
            }
        }
    }
}
