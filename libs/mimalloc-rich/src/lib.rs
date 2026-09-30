//! Bindings to the [mimalloc] allocator.
//!
//! See the [official docs](https://microsoft.github.io/mimalloc/) for more details.
//!
//! This includes raw bindings under the [`sys` module](sys),
//! as well as higher-level bindings added as needed.
//!
//! [mimalloc]: https://github.com/microsoft/mimalloc
#![warn(missing_docs, future_incompatible)]
#![allow(clippy::inline_always, reason = "sometimes appropriate for low-level code")]
#![no_std]
#![allow(
    stable_features,
    reason = "allocator-api decl kept for compatibility with older 1.100 nightlies"
)]
#![cfg_attr(feature = "nightly-allocator-api", feature(allocator_api))]

use core::alloc::Layout;
use core::ffi::c_void;
use core::ptr::NonNull;

pub mod allocator;
pub mod heap;
pub(crate) mod internal_alloc_api;
pub mod options;
pub mod subproc;

pub use self::allocator::MiMalloc;

/// Raw bindings to mimalloc.
pub extern crate mimalloc_rich_sys as sys;

/// Indicates that a memory allocation failed due to running out of memory.
///
/// This is more specific than a [`core::alloc::AllocError`],
/// which can also be caused by invalid input or internal allocator problems.
#[derive(Debug, Copy, Clone, thiserror::Error)]
#[error("Allocation Failed: Out of Memory")]
pub struct OutOfMemoryError;
impl OutOfMemoryError {
    #[inline(always)]
    pub(crate) fn if_null<T>(ptr: *mut T) -> Result<NonNull<T>, OutOfMemoryError> {
        match NonNull::new(ptr) {
            Some(ptr) => Ok(ptr),
            None => {
                #[cfg(has_hint_cold_path)]
                #[allow(clippy::incompatible_msrv, reason = "appropriately guarded")]
                core::hint::cold_path();
                Err(OutOfMemoryError)
            }
        }
    }
}

/// Allocate `size` bytes.
///
/// If called with size zero, this will return a new unique pointer.
/// Returns an [`OutOfMemoryError`] if out-of-memory.
///
/// ## Safety
/// This function is safe to call.
#[inline]
pub fn mi_malloc(size: usize) -> Result<NonNull<c_void>, OutOfMemoryError> {
    // SAFETY: Safe to invoke
    unsafe { OutOfMemoryError::if_null(sys::mi_malloc(size)) }
}

/// Allocate a block of memory with the specified size and alignment.
///
/// If called with size zero, this will return a unique pointer.
///
/// ## Safety
/// This function is always safe to call.
#[inline]
pub fn mi_malloc_aligned(layout: Layout) -> Result<NonNull<c_void>, OutOfMemoryError> {
    // SAFETY: Safe to invoke
    unsafe { OutOfMemoryError::if_null(sys::mi_malloc_aligned(layout.size(), layout.align())) }
}

/// Allocate a zero-initialized block of memory with the specified size and alignment.
///
/// If called with size zero, this will return a unique pointer.
///
/// ## Safety
/// This function is always safe to call.
#[inline]
pub fn mi_zalloc_aligned(layout: Layout) -> Result<NonNull<c_void>, OutOfMemoryError> {
    // SAFETY: Safe to invoke
    unsafe { OutOfMemoryError::if_null(sys::mi_zalloc_aligned(layout.size(), layout.align())) }
}

/// Re-allocate a block of memory with the specified size.
///
/// If the pointer `p` is null, this behaves the same as [`mi_malloc_aligned`].
/// If newsize is larger than the original size allocated for p,
/// the bytes after size are uninitialized.
///
/// Returns null if out-of-memory.
/// If null is returned, the original memory is not freed.
///
/// ## Safety
/// Pointer must be previously allocated memory or null.
#[inline]
pub unsafe fn mi_realloc(ptr: *mut c_void, new_size: usize) -> Result<NonNull<c_void>, OutOfMemoryError> {
    // SAFETY: Caller guarantees old pointer is valid
    unsafe { OutOfMemoryError::if_null(sys::mi_realloc(ptr, new_size)) }
}

/// Re-allocate a block of memory with the specified size and alignment.
///
/// If the pointer `p` is null, this behaves the same as [`mi_malloc_aligned`].
/// If newsize is larger than the original size allocated for p,
/// the bytes after size are uninitialized.
///
/// Returns null if out-of-memory.
/// If null is returned, the original memory is not freed.
///
/// ## Safety
/// Pointer must be previously allocated memory or null.
#[inline]
pub unsafe fn mi_realloc_aligned(ptr: *mut c_void, layout: Layout) -> Result<NonNull<c_void>, OutOfMemoryError> {
    // SAFETY: Caller guarantees old pointer is valid
    unsafe { OutOfMemoryError::if_null(sys::mi_realloc_aligned(ptr, layout.size(), layout.align())) }
}

/// Return the available bytes in a memory block.
///
/// Returns 0 if the pointer is null.
///
/// ## Safety
/// Pointer must be previously allocated memory or null.
#[inline]
pub unsafe fn mi_usable_size(ptr: *mut c_void) -> usize {
    // SAFETY: Caller guarantees pointer is valid.
    unsafe { sys::mi_usable_size(ptr) }
}

/// An error that occurs calling [`mi_expand`].
///
/// Triggered by b failing to expand in-place,
/// which might happen even if there is otherwise sufficient mmeory to satisfy the request.
#[derive(Debug, thiserror::Error)]
#[error("Failed to expand allocation in-place")]
#[non_exhaustive]
pub struct ExpandError;

/// Try to re-allocate memory to `new_size` bytes in-place.
///
/// This supports expanding and shrinking memory.
///
/// Returns a pointer to the re-allocated memory of `new_size` bytes (always equal to `ptr`),
/// or [`AllocError`] if either out of memory or if the memory could not be expanded in place.
///
/// If an error is returned, the pointer p is not freed.
/// Otherwise, the original pointer is returned as the reallocated result.
///
/// If `new_size` is larger than the original size allocated for p,
/// the bytes after the original size are uninitialized.
///
/// Since the original pointer is never moved,
/// this necessarily preserves the original alignment.
///
/// [`AllocError`]: core::alloc::AllocError
///
/// ## Safety
/// Pointer must be previously allocated memory or null.
#[inline]
pub unsafe fn mi_expand(ptr: *mut c_void, new_size: usize) -> Result<NonNull<c_void>, ExpandError> {
    // SAFETY: Caller guarantees validity
    NonNull::new(unsafe { sys::mi_expand(ptr, new_size) }).ok_or(ExpandError)
}

/// Re-allocate a zero-initialized block of memory with the specified size and alignment.
///
/// If the pointer `p` is null, this behaves the same as [`mi_malloc_aligned`].
/// If newsize is larger than the original size allocated for p,
/// the bytes after size are uninitialized.
///
/// If an error is returned, the original memory is not freed.
///
/// *WARNING*: A zero-initialized re-allocations is only valid on memory
/// that was originally allocated with zero initialization too.
/// This means this function can not be used to implement [`core::alloc::Allocator::grow_zeroed`],
/// because that is well-defined to call even if the original memory was not zero-initialized.
///
/// ## Safety
/// The original pointer must be correctly allocated,
/// fulfilling the same requirements as [`mi_free`].
///
/// The documentation is unclear on the behavior when the original memory is not zero-initialized.
/// It might be undefined behavior.
#[inline]
pub unsafe fn mi_rezalloc_aligned(ptr: *mut c_void, layout: Layout) -> Result<NonNull<c_void>, OutOfMemoryError> {
    // SAFETY: Caller guarantees old pointer is invalid and was originally zero-initialized
    unsafe { OutOfMemoryError::if_null(sys::mi_rezalloc_aligned(ptr, layout.size(), layout.align())) }
}

/// Free the specified pointer, calling [`sys::mi_free`].
///
/// Does nothing if the pointer is NULL.
/// This can be used to free memory from any [heap](heap::MiHeap).
///
/// ## Safety
/// If non-null, the pointer must have been previously allocated through mimalloc.
/// Undefined behavior on double free.
#[inline(always)]
pub unsafe fn mi_free(ptr: *mut c_void) {
    // SAFETY: Guaranteed by the caller
    unsafe {
        sys::mi_free(ptr);
    }
}

/// Eagerly free emory.
///
/// Regular code should not have to call this function.
/// It can be beneficial in very narrow circumstances; in particular,
/// when a long running thread allocates a lot of blocks that are freed by other threads
/// it may improve resource usage by calling this every once in a while.
///
/// If force is true, this will aggressively return memory to the OS (can be expensive!)
///
/// ## Safety
/// This function is always safe to call.
#[inline]
pub fn mi_collect(force: bool) {
    // SAFETY: Safe to call
    unsafe { sys::mi_collect(force) }
}
