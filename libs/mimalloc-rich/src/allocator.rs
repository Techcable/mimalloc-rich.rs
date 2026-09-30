//! Declares the [`MiMalloc`] type,
//! which implements [`core::alloc::Allocator`],
//! [`core::alloc::GlobalAlloc`],
//! and [`allocator_api2::alloc::Allocator`].
//!
//! Implementing [`core::alloc::Allocator`] requires the `nightly-allocator-api` feature.
//! Implementing [`allocator_api2::alloc::Allocator`] requires `allocator-api2-all` feature,
//! which implements the trait for each supported version of `allocator-api2`.
//!
//! [`allocator_api2::alloc::Allocator`]: https://docs.rs/allocator-api2/latest/allocator_api2/alloc/trait.Allocator.html

use core::alloc::Layout;
use core::ffi::c_void;
use core::ptr::NonNull;

use crate::internal_alloc_api::{AllocError, impl_public_allocator_traits};

/// Sets mimalloc as the global allocator.
#[cfg(feature = "declare-global-allocator")]
#[global_allocator]
pub static GLOBAL: MiMalloc = self::MiMalloc;

/// The mimalloc global allocator.
#[derive(Default, Clone)]
pub struct MiMalloc;
impl MiMalloc {
    /// Create a new instance of the allocator.
    #[inline]
    pub const fn new() -> Self {
        MiMalloc
    }

    #[inline]
    #[allow(clippy::unused_self)]
    fn handle_alloc_res(&self, ptr: NonNull<c_void>, layout: Layout) -> NonNull<[u8]> {
        NonNull::slice_from_raw_parts(ptr.cast(), layout.size())
    }
}

#[cfg_attr(test, deny(clippy::missing_trait_methods))]
// SAFETY: This is a correct allocator
unsafe impl core::alloc::GlobalAlloc for MiMalloc {
    /// Allocate memory with the specified size and alignment
    ///
    /// This is always safe to use, and supports zero-sized allocations
    /// in the same way that [`crate::mi_malloc_aligned`] does.
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let res = crate::mi_malloc_aligned(layout);
        res.map_or(core::ptr::null_mut(), NonNull::as_ptr).cast()
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = layout;
        // SAFETY: Guaranteed by caller
        unsafe { crate::mi_free(ptr.cast()) }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        crate::mi_zalloc_aligned(layout)
            .map_or(core::ptr::null_mut(), NonNull::as_ptr)
            .cast()
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = layout; // not needed
        // SAFETY: Validity guaranteed by caller, alignment cannot change as we have only have a `new_size`
        let res = unsafe { crate::mi_realloc(ptr.cast(), new_size) };
        res.map_or(core::ptr::null_mut(), NonNull::as_ptr).cast()
    }
}
// SAFETY: This is a correct implementation
unsafe impl crate::internal_alloc_api::Allocator for MiMalloc {
    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let res = crate::mi_malloc_aligned(layout)?;
        Ok(self.handle_alloc_res(res, layout))
    }

    #[inline]
    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let res = crate::mi_zalloc_aligned(layout)?;
        Ok(self.handle_alloc_res(res, layout))
    }

    #[inline]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let _ = layout; // not needed
        // SAFETY: Caller guarantees correctness - there is only one mi_free function
        unsafe {
            crate::mi_free(ptr.as_ptr().cast());
        }
    }

    #[inline]
    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        let _ = old_layout;
        // SAFETY: Caller guarantees validity
        let res = unsafe { crate::mi_realloc_aligned(ptr.as_ptr().cast(), new_layout) }?;
        Ok(self.handle_alloc_res(res, new_layout))
    }

    #[inline]
    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        let _ = old_layout;
        // SAFETY: Caller guarantees validity
        let res = unsafe { crate::mi_realloc_aligned(ptr.as_ptr().cast(), new_layout) }?;
        Ok(self.handle_alloc_res(res, new_layout))
    }
}

impl_public_allocator_traits!(impl Allocator for MiMalloc);
