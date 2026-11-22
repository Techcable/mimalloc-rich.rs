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
use core::ops::ControlFlow;
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
    /// Similar to [`crate::mi_rezalloc_aligned`], but specific to this heap.
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
    #[inline]
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
    /// Visit all areas and blocks in a heap.
    ///
    /// If `visit_all_blocks=true`, this visits all allocated blocks.
    /// Otherwise, the visitor is only called for every heap area.
    ///
    /// The callback arguments have the names `heap, area, block, block_size`,
    /// and return a value of [`ControlFlow`].
    ///
    /// This function is always first called for every area with `block` as a NULL pointer.
    /// If `visit_all_blocks` was true,
    /// the function is then called for every allocated block in that area.
    ///
    /// ## Example
    /// ```no_run
    /// # use std::ffi::c_void;
    /// # use mimalloc_rich::{sys, heap::MiLocalHeap};
    /// # use mimalloc_rich::heap::MiOwnedHeap;
    /// #
    /// # fn do_visit_heap(heap: &impl MiOwnedHeap) {
    /// use std::ops::ControlFlow;
    /// let mut callback = |heap_ptr: *const sys::mi_heap_t, area: &sys::mi_heap_area_t, block: *mut c_void, block_size: usize| {
    ///     if !block.is_null() && block_size >= 100 {
    ///         return ControlFlow::Break(());
    ///     }
    ///     ControlFlow::Continue(())
    /// };
    /// // SAFETY: The callback is well behaved (doesn't mutate anything)
    /// let _: ControlFlow<()> = unsafe { heap.visit_blocks(true, &mut callback) };
    /// # }
    /// ```
    ///
    /// # Safety
    /// While visiting a heap, operations that mutate the heap, allocate from it,
    /// or free its memory are undefined behavior.
    /// In v3, other threads may not allocate from or free into this heap.
    /// It is the caller's responsibility to ensure this.
    ///
    /// This fact is explicitly documented in mimalloc v3, but is true for v1/v2 as well.
    ///
    /// The visit function itself must be `unsafe` since allocation from a heap is safe.
    /// Safe abstractions like [`Box`] with a custom allocator (`Box<T, &MiLocalHeap>`) could also free memory.
    /// While a `&mut Self` reference might be able to enforce these requirements,
    /// it would likely require the caller to use a [`RefCell`] or [`UnsafeCell`].
    /// This would slow down every access or risk additional undefined behavior.
    ///
    /// [`Box`]: https://doc.rust-lang.org/std/boxed/struct.Box.html
    /// [`mi_free`]: crate::mi_free
    /// [`RefCell`]: core::cell::RefCell
    /// [`UnsafeCell`]: core::cell::UnsafeCell
    unsafe fn visit_blocks<F: HeapVisitorFunc>(&self, visit_all_blocks: bool, visitor: &mut F) -> ControlFlow<()>;

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

/// The visitor [closure](FnMut) passed to the [`MiOwnedHeap::visit_blocks`] function.
///
/// On v2, this is also used for the `mi_abandoned_visit_blocks` function.
/// On v3, this is also used for the [`MiSyncHeap::visit_abandoned_blocks`] function.
///
///
/// The closure signature matches that of the following function:
/// ```no_run
/// # use std::ffi::c_void;
/// # use std::ops::ControlFlow;
/// # use mimalloc_rich_sys as sys;
///
/// # trait Signature {
/// fn callback(
///     heap_ptr: *const sys::mi_heap_t,
///     area: &sys::mi_heap_area_t,
///     block: *mut c_void,
///     block_size: usize
/// ) -> ControlFlow<()>;
/// # }
/// ```
///
/// Return [`ControlFlow::Break`] to stop visiting.
///
/// # Safety
/// The callback is itself safe to invoke, but the visit functions that use it are not.
///
/// The callback only receives a raw [`sys::mi_heap_t`] pointer
/// instead of a reference because there are safety issues when mutating the heap.
/// Additionally, in v1/v2, when using `mi_abandoned_visit_blocks`, the heap pointer is always `NULL`.
///
/// The `area` parameter is known to be both non-null and valid,
/// so a reference can be used instead of a raw pointer.
pub trait HeapVisitorFunc:
    FnMut(*const sys::mi_heap_t, &sys::mi_heap_area_t, *mut c_void, usize) -> ControlFlow<()>
{
}
impl<F: FnMut(*const sys::mi_heap_t, &sys::mi_heap_area_t, *mut c_void, usize) -> ControlFlow<()>> HeapVisitorFunc
    for F
{
}
