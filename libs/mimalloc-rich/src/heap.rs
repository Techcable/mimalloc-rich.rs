//! Defines a high-level wrapper for the mimalloc heap api.

mod common;
pub mod local;
#[cfg(mimalloc3)]
#[cfg_attr(feature = "nightly-doc", doc(cfg(mimalloc3)))]
pub mod sync;

use core::alloc::Layout;
use core::ffi::c_void;
use core::fmt::Debug;
use core::mem::ManuallyDrop;
use core::ptr::NonNull;

pub use self::local::MiLocalHeap;
#[cfg(mimalloc3)]
#[cfg_attr(feature = "nightly-doc", doc(cfg(mimalloc3)))]
pub use self::sync::MiSyncHeap;
use crate::OutOfMemoryError;

/// Indicates a failure to allocate a [`MiLocalHeap`].
#[derive(Debug, thiserror::Error)]
#[error("Failed to allocate a mimalloc heap")]
pub struct MiHeapAllocError(());

/// Functionality common to all mimalloc heaps.
///
/// # Freeing Memory
/// There is no method to free memory allocated from a specific heap.
/// Instead, the standard [`crate::mi_free`] method is used.
///
/// If this type is a [`MiOwnedHeap`],
/// it may release allocated memory when dropped.
///
/// # Safety
/// This trait is guaranteed to be implemented correctly,
/// and is sealed to only allow implementation inside this crate.
pub unsafe trait MiHeap: Debug + Sized + common::Sealed {
    /// The underlying C pointer type which this type wraps.
    type Ptr: Copy + 'static;

    /// Get a raw pointer to the underlying heap.
    ///
    /// When using an [owned heap](MiOwnedHeap),
    /// you may want to use [`MiOwnedHeap::into_owned_ptr`] instead.
    ///
    /// # Safety
    /// While this operation itself is safe,
    /// many operations on the heap can cause undefined behavior (especially for a [`MiOwnedHeap`]).
    ///
    /// In particular, calling [`MiOwnedHeap::delete`] or [`MiOwnedHeap::destroy`]
    /// could trigger a double-free when the type is dropped.
    fn as_ptr(&self) -> Self::Ptr;

    /// Allocate a block of memory with the specified layout in this heap.
    ///
    /// Returns [`AllocError`] on out of memory and correctly supports zero-sized allocations.
    ///
    /// Similar to [`crate::mi_malloc_aligned`], but specific to this heap.
    ///
    /// [`AllocError`]: core::alloc::AllocError
    ///
    /// # Safety
    /// Always safe to call.
    /// The allocated memory can be freed from any thread using [`crate::mi_free`].
    ///
    /// If this is a thread-local heap, this must only be invoked from the heap's original thread,
    /// but this is guaranteed by `!Send`.
    fn malloc_aligned(&self, layout: Layout) -> Result<NonNull<c_void>, OutOfMemoryError>;

    /// Allocate a zero-initialized block of memory with the specified layout in this heap.
    ///
    /// Similar to [`crate::mi_zalloc_aligned`], but specific to this heap.
    ///
    /// # Safety
    /// Always safe to call.
    /// The allocated memory can be freed from any thread using [`crate::mi_free`].
    ///
    /// If this is a thread-local heap, this must only be invoked from the heap's original thread,
    /// but this is guaranteed by `!Send`.
    fn zalloc_aligned(&self, layout: Layout) -> Result<NonNull<c_void>, OutOfMemoryError>;

    /// Re-allocate a block of memory with the specified layout in this heap.
    ///
    /// Similar to [`crate::mi_realloc_aligned`], but specific to this heap.
    ///
    /// # Safety
    /// The original memory must be allocated from this heap.
    ///
    /// Must only be invoked from the heap's original thread,
    /// but this is guaranteed by `!Send`.
    unsafe fn realloc_aligned(&self, ptr: *mut c_void, layout: Layout) -> Result<NonNull<c_void>, OutOfMemoryError>;

    /// Re-allocate a zero-initialized block of memory with the specified layout in this heap.
    ///
    /// Similar to [`crate::mi_zalloc_aligned`], but specific to this heap.
    ///
    /// # Safety
    /// The original memory must be allocated from this heap.
    /// Requires that the memory originally have been zero-initialized,
    /// just like with [`crate::mi_rezalloc_aligned`].
    ///
    /// Must come from the heap's original thread,
    /// but this is guaranteed by `!Send`.
    unsafe fn rezalloc_aligned(&self, ptr: *mut c_void, layout: Layout) -> Result<NonNull<c_void>, OutOfMemoryError>;

    /// Free memory allocated in this heap.
    ///
    /// This unconditionally delegates to [`crate::mi_free`],
    /// since that is the single interface to free all memory allocated through mimalloc,
    /// regardless of the original heap or thread.
    ///
    /// It exists only for consistency with the other allocation methods.
    ///
    /// # Safety
    /// Same requirements as [`crate::mi_free`].
    unsafe fn free(&self, ptr: *mut c_void) {
        // SAFETY: Caller guarantee
        unsafe { crate::mi_free(ptr) }
    }

    /// Collect outstanding resources in this heap.
    ///
    /// Similar to [`crate::mi_collect`], but specific to this heap.
    fn collect(&self, force: bool);
}
/// Functionality for owned mimalloc heaps.
///
/// On mimalloc v1/v2, a [`MiLocalHeap`] is owned.
/// On mimalloc v3, it is borrowed and [`MiSyncHeap`] is owned.
///
/// # Destroying the Heap
/// When this type is dropped, [`Self::delete`] is called ([`sys::mi_heap_delete`]).
/// This will free the resources associated with the heap
/// while preserving the memory allocated from it.
///
/// To free the heap and also all the allocated objects,
/// call [`Self::destroy`] instead.
///
/// # Safety
/// This trait is guaranteed to be implemented correctly,
/// and is sealed to only allow implementation inside this crate.
///
/// The design of the trait prevents use after free by safe code.
pub unsafe trait MiOwnedHeap: MiHeap + 'static {
    /// Consume ownership of this heap,
    /// returning a raw pointer to the underlying type.
    ///
    /// # Safety
    /// This method is safe to call,
    /// though many operations on the resulting pointer are not.
    ///
    /// This may leak memory if the heap is never freed,
    /// but that is still safe behavior.
    #[inline]
    fn into_owned_ptr(self) -> Self::Ptr {
        let this = ManuallyDrop::new(self);
        this.as_ptr()
    }

    /// Destroy the heap, freeing all allocated memory (calls [`sys::mi_heap_destroy`]).
    ///
    /// Prefer to use [`Self::delete`], which will preserve all allocated memory.
    /// Destroying or deleting the main heap for a process will be ignored.
    ///
    /// # Safety
    /// Undefined behavior if any of the memory in the heap is still being used.
    ///
    /// Freeing memory here can violate the guarantees of the [`core::alloc::Allocator`] trait.
    /// This means you need to be very careful of which allocations are still live.
    unsafe fn destroy(self);

    /// Free the resources associated with the heap,
    /// while preserving all allocated memory (calls [`sys::mi_heap_delete`]).
    ///
    /// The blocks of memory still live will be transferred to the default heap for the thread.
    /// If you want to free all the allocated memory, you can use [`Self::destroy`].
    /// Destroying or deleting the main heap for a process will be ignored.
    ///
    /// This is automatically run on [`Drop`], and exists as a separate method only for clarity.
    ///
    /// # Safety
    /// This method is safe to call because it takes ownership of the heap.
    #[inline]
    fn delete(self) {
        drop(self);
    }
}
