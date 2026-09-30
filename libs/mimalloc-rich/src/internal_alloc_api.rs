//! Abstracts over the various supported allocator APIs.

use core::alloc::Layout;
use core::ptr::NonNull;

/// Stable version of [`core::alloc::Allocator`],
/// delegated to by traits implemented by [`impl_public_allocator_traits!`].
///
/// # Safety
/// Must uphold the expectations of [`core::alloc::Allocator`].
#[allow(unused, reason = "unused if allocator-api2 and nightly both disabled")]
pub unsafe trait Allocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError>;

    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError>;

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout);

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError>;

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError>;

    /// Implements `grow_zeroed` in terms of `grow`.
    #[inline]
    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        // NOTE: We cannot use the `mi_rezalloc_aligned` family of functions,
        // because original memory may not have been zero-initialized
        // The core::alloc::Allocator interface requires that the following code is well-defined:
        // ```
        // let old_layout = Layout::array::<u8>(12);
        // let ptr = MiMalloc.allocate().unwrap();
        // let new_layout = Layout::array::<u8>(24);
        // let ptr = MiMalloc.allocate_zeroed(ptr, new_layout).unwrap();
        // ```
        // We don't know whether mimalloc supports this use-case, so we emulate it.
        //
        // SAFETY: Caller guarantees validity of old pointer
        let new_memory = unsafe { self.grow(ptr, old_layout, new_layout) }?;
        // SAFETY: Caller guarantees new_layout is larger than old_layout
        let additional_bytes = unsafe { new_layout.size().unchecked_sub(old_layout.size()) };
        // SAFETY: We trust caller that old_layout is valid for the allocation,
        // we also know that new_layout.size >= old_layout.size
        let old_layout_end = unsafe { new_memory.as_ptr().cast::<u8>().add(old_layout.size()) };
        // SAFETY: Successful grow will have valid memory
        unsafe {
            old_layout_end.write_bytes(0, additional_bytes);
        }
        Ok(new_memory)
    }
}
/// Implement all supported allocator by delegating to [`Allocator`].
macro_rules! impl_public_allocator_traits {
    (impl Allocator for $target:ident) => {
        #[cfg(feature = "nightly-allocator-api")]
        #[cfg_attr(test, deny(clippy::missing_trait_methods))]
        // SAFETY: The internal Allocator trait has the same requirements as us
        unsafe impl core::alloc::Allocator for $target {
            $crate::internal_alloc_api::impl_public_allocator_traits!(@delegate_common {
                type Error = core::alloc::AllocError;
            });
        }
        #[cfg(feature = "allocator-api2-02")]
        #[cfg_attr(test, deny(clippy::missing_trait_methods))]
        // SAFETY: The internal Allocator trait has the same requirements as us
        unsafe impl allocator_api2_02::alloc::Allocator for $target {
            $crate::internal_alloc_api::impl_public_allocator_traits!(@delegate_common {
                type Error = allocator_api2_02::alloc::AllocError;
            });
            $crate::internal_alloc_api::impl_public_allocator_traits!(@by_ref);
        }
        #[cfg(feature = "allocator-api2-03")]
        #[cfg_attr(test, deny(clippy::missing_trait_methods))]
        // SAFETY: The internal Allocator trait has the same requirements as us
        unsafe impl allocator_api2_03::alloc::Allocator for $target {
            $crate::internal_alloc_api::impl_public_allocator_traits!(@delegate_common {
                type Error = allocator_api2_03::alloc::AllocError;
            });
            $crate::internal_alloc_api::impl_public_allocator_traits!(@by_ref);
        }
        #[cfg(feature = "allocator-api2-04")]
        #[cfg_attr(test, deny(clippy::missing_trait_methods))]
        // SAFETY: The internal Allocator trait has the same requirements as us
        unsafe impl allocator_api2_04::alloc::Allocator for $target {
            $crate::internal_alloc_api::impl_public_allocator_traits!(@delegate_common {
                type Error = allocator_api2_04::alloc::AllocError;
            });
            $crate::internal_alloc_api::impl_public_allocator_traits!(@by_ref);
        }
    };
    (@by_ref) => {
        #[inline]
        fn by_ref(&self) -> &Self where Self: Sized {
            self
        }
    };
    (@delegate_common {
        type Error = $error:ty;
    })  => {
        $crate::internal_alloc_api::impl_public_allocator_traits!(@delegate {
            fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, $error>;
            fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, $error>;
            [unsafe] fn deallocate(&self, ptr: NonNull<u8>, layout: Layout);
            [unsafe] fn shrink(&self, ptr: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> Result<NonNull<[u8]>, $error>;
            [unsafe] fn grow(&self, ptr: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> Result<NonNull<[u8]>, $error>;
            [unsafe] fn grow_zeroed(&self, ptr: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> Result<NonNull<[u8]>, $error>;
        });
    };
    (@delegate {
        $($([$safety:ident])? fn $name:ident(&self, $($arg:ident: $arg_ty:ty),*) $( -> $res:ty)? ;)*
    }) => {
        $(
            #[inline]
            $($safety)* fn $name(&self, $($arg: $arg_ty),*) $(-> $res)* {
                let res = $($safety)* {
                    <Self as crate::internal_alloc_api::Allocator>::$name(self, $($arg,)*)
                };
                $(let res: $res = {
                    match res {
                        Ok(pass) => Ok(pass),
                        Err(err @ crate::internal_alloc_api::AllocError) => {
                            #[cfg(has_hint_cold_path)]
                            #[allow(clippy::incompatible_msrv)]
                            core::hint::cold_path();
                            Err(err.into())
                        }
                    }
                };)*
                res
            }
        )*
    };
}

/// Counterpart for [`core::alloc::AllocError`] used with [`Allocator`]
///
/// Does not implement [`core::error::Error`],
/// as it should be immediately
#[derive(Clone, Debug)]
#[allow(unused, reason = "see Allocator trait")]
pub struct AllocError;
impl From<crate::OutOfMemoryError> for AllocError {
    #[inline(always)]
    fn from(_: crate::OutOfMemoryError) -> Self {
        AllocError
    }
}
macro_rules! alloc_error_conversions {
    ($(#[cfg($cond:meta)] $target:path),+ $(,)?) => {
        $(
        #[cfg($cond)]
        impl From<$target> for AllocError {
            #[inline]
            fn from(value: $target) -> Self {
                let $target {} = value;
                AllocError
            }
        }
        #[cfg($cond)]
        impl From<AllocError> for $target {
            #[inline]
            fn from(_value: AllocError) -> Self {
                $target
            }
        }
        #[cfg($cond)]
        impl From<crate::OutOfMemoryError> for $target {
            #[inline]
            fn from(value: crate::OutOfMemoryError) -> Self {
                let crate::OutOfMemoryError = value;
                $target
            }
        }
        #[cfg($cond)]
        impl From<crate::ExpandError> for $target {
            #[inline]
            fn from(value: crate::ExpandError) -> Self {
                let crate::ExpandError = value;
                $target
            }
        }

        )*
    }
}
alloc_error_conversions!(
    #[cfg(feature = "nightly-allocator-api")]
    core::alloc::AllocError,
    #[cfg(feature = "allocator-api2-02")]
    allocator_api2_02::alloc::AllocError,
    #[cfg(feature = "allocator-api2-03")]
    allocator_api2_03::alloc::AllocError,
    #[cfg(feature = "allocator-api2-04")]
    allocator_api2_04::alloc::AllocError,
);

pub(crate) use impl_public_allocator_traits;
