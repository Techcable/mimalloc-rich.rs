#[allow(unused_imports, reason = "used by docs")]
use allocator_api2::alloc::Allocator;

/// Implement common methods shared across allocator implementations.
///
/// Currently implements the following functions:
/// 1. [`Allocator::grow_zeroed`] because `mi_rezalloc_aligned` can't be used,
/// 2. [`Allocator::by_ref`] since this is required by `clippy::missing_trait_methods`
macro_rules! common_allocator_impl {
    (@default) => {
        common_allocator_impl!(@grow_zeroed);
        common_allocator_impl!(@by_ref);
    };
    (@grow_zeroed) => {
        /// Implement s`grow_zeroed` in terms of `grow` */
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
            // SAFETY: Successful grow will have valid memory
            unsafe {
                new_memory
                    .as_ptr()
                    .cast::<u8>()
                    .add(old_layout.size())
                    .write_bytes(0, additional_bytes)
            }
            Ok(new_memory)
        }
    };
    (@by_ref) => {
        // Only needed for `clippy::missing_trait_methods`
        // TODO: Is there a way to add an exception to the lint?
        #[inline(always)]
        fn by_ref(&self) -> &Self
        where
            Self: Sized,
        {
            self
        }
    }
}

/// Implement [`core::alloc::Allocator`] by delegating to [`allocator_api2::alloc::Allocator`].
macro_rules! delegate_impl_nightly_allocator {
    ($target:path) => {
        #[cfg(feature = "nightly-allocator-api")]
        mod nightly_impl {
            use super::*;
            use core::alloc::{AllocError, Layout};
            use core::ptr::NonNull;
            #[deny(clippy::missing_trait_methods)]
            // SAFETY: Delegates to allocator_api2, which has same requirements
            unsafe impl core::alloc::Allocator for $target {
                delegate_impl_nightly_allocator!(@delegate {
                    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError>;
                    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError>;
                    [unsafe] fn deallocate(&self, ptr: NonNull<u8>, layout: Layout);
                    [unsafe] fn shrink(&self, ptr: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> Result<NonNull<[u8]>, AllocError>;
                    [unsafe] fn grow(&self, ptr: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> Result<NonNull<[u8]>, AllocError>;
                    [unsafe] fn grow_zeroed(&self, ptr: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> Result<NonNull<[u8]>, AllocError>;
                });
                // NOTE: The nightly allocator API no longer has the by_ref method
                // common_allocator_impl!(@by_ref);
            }
        }
    };
    (@delegate {
        $($([$safety:ident])? fn $name:ident(&self, $($arg:ident: $arg_ty:ty),*) $( -> $res:ty)? ;)*
    }) => {
        $(
            #[inline]
            $($safety)* fn $name(&self, $($arg: $arg_ty),*) $(-> $res)* {
                let res = $($safety)* {
                    <Self as allocator_api2::alloc::Allocator>::$name(self, $($arg,)*)
                };
                $(let res: $res = {
                    match res {
                        Ok(pass) => Ok(pass),
                        Err(allocator_api2::alloc::AllocError) => Err(core::alloc::AllocError),
                    }
                };)*
                res
            }
        )*
    };
}
