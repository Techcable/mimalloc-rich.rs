//! Defines a high-level wrapper for the mimalloc heap api.
#![deprecated(note = "Will be changed significantly when upgrading to mimalloc v3")]

use core::alloc::Layout;
use core::ffi::{c_int, c_void};
use core::fmt::{Debug, Formatter};
use core::mem::ManuallyDrop;
use core::ops::ControlFlow;
use core::ptr::NonNull;

use crate::OutOfMemoryError;
use crate::internal_alloc_api::{AllocError, impl_public_allocator_traits};
use crate::subproc::MiSubprocId;
use crate::sys::{self, mi_heap_t};

/// Allocates a [`MiHeap`] with custom options.
///
/// TODO: Support custom `arena_id`.
#[derive(Default)]
pub struct MiHeapBuilder {
    heap_tag: HeapTag,
    allow_destroy: bool,
}
impl MiHeapBuilder {
    /// Set if [`MiHeap::destroy`] allowed?
    ///
    /// Not allowing this allows the heap to reclaim memory from terminated threads.
    /// This defaults to `false`.
    #[inline]
    pub fn allow_destroy(&mut self, allow: bool) -> &mut Self {
        self.allow_destroy = allow;
        self
    }

    /// Set a [heap tag](HeapTag), to prevent objects of different types from mixing.
    ///
    /// This defaults to [`HeapTag::DEFAULT`].
    #[inline]
    pub fn heap_tag(&mut self, heap_tag: HeapTag) -> &mut Self {
        self.heap_tag = heap_tag;
        self
    }

    /// Allocate a n ew [`MiHeap`] with the configured option,
    /// returning an error if allocation fails.
    ///
    /// See also [`MiHeap::new`], which performs no special configuration.
    #[inline]
    pub fn build(&mut self) -> Result<MiHeap, MiHeapAllocError> {
        // SAFETY: Options are valid
        let ptr = unsafe { sys::mi_heap_new_ex(self.heap_tag.0, self.allow_destroy, 0) };
        match NonNull::new(ptr) {
            Some(ptr) => Ok(MiHeap { ptr }),
            None => Err(MiHeapAllocError(())),
        }
    }
}

/// A tag for a [`MiHeap`],
/// to avoid mixing objects of different types.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct HeapTag(pub c_int);
impl HeapTag {
    /// The default heap tag, if none is specified or [`MiHeap::new`] is used.
    pub const DEFAULT: HeapTag = HeapTag(0);
}
impl Default for HeapTag {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Visit all areas and blocks in abandoned heaps.
///
/// Requires that the [`crate::options::AllowVisitAbandoned`] runtime flag is enabled at the start of the program.
///
/// The `subproc_id` must match that of the abandoned heaps.
/// If `heap_tag` is specified, only visit those memory with a matching heap tag.
/// If `visit_blocks` is true visits all the blocks,
/// otherwise the visitor
///
/// Unless otherwise stated, functionality behaves the same as [`MiHeap::visit_blocks`.
/// Wraps [`sys::mi_abandoned_visit_blocks`].
#[inline]
pub fn mi_abandoned_visit_blocks<F: HeapVisitorFunc>(
    subproc_id: MiSubprocId,
    heap_tag: Option<HeapTag>,
    visit_blocks: bool,
    visitor: &mut F,
) -> ControlFlow<()> {
    let heap_tag: c_int = match heap_tag {
        None => -1,
        Some(val @ HeapTag(-1)) => panic!("Heap tag is ambiguous: {val:?}"),
        Some(other) => other.0,
    };
    // SAFETY: Visitor is correct, rely on trampoline to do the rest
    let did_visit_all = unsafe {
        sys::mi_abandoned_visit_blocks(
            subproc_id.to_ffi(),
            heap_tag,
            visit_blocks,
            Some(heap_visitor_trampoline::<F>),
            core::ptr::from_mut::<F>(visitor).cast(),
        )
    };
    if did_visit_all {
        ControlFlow::Continue(())
    } else {
        ControlFlow::Break(())
    }
}
/// Indicates a failure to allocate a [`MiHeap`].
#[derive(Debug, thiserror::Error)]
#[error("Failed to allocate a mimalloc heap")]
pub struct MiHeapAllocError(());

/// A high-level wrapper for a [`sys::mi_heap_t`].
///
/// ## Freeing Memory
/// There is no method to free memory allocated from a specific heap.
/// Instead, the standard [`crate::mi_free`] method is used.
/// It can be safely
///
/// When this type is dropped, [`sys::mi_heap_delete`] is called.
/// This will free the resources associated with the heap, while preserving the memory allocated from it.
/// The blocks of memory still live will be transferred to the default heap for the thread (
///
/// To free the heap and destroy all objects in the heap at once,
/// call `Self::destroy` ([`sys::mi_heap_destroy`]).
///
/// ## Thread Safety
/// The heap is not thread safe to allocate from another thread, so is `!Sync`.
/// Memory allocated from a heap can be freed from any thread using [`crate::mi_free`].
/// It is unclear if the destructor can be called from another thread, so the type is `!Send`.
pub struct MiHeap {
    ptr: NonNull<mi_heap_t>,
}
impl MiHeap {
    /// Create a builder to allocate a heap with custom options.
    ///
    /// See also [`Self::new`]
    #[inline]
    pub fn builder() -> MiHeapBuilder {
        MiHeapBuilder::default()
    }

    /// Create a new heap that can be used for allocation.
    ///
    /// Sea also [`Self::builder`].
    #[inline]
    pub fn new() -> Result<Self, MiHeapAllocError> {
        // SAFETY: This function is safe
        let res = unsafe { sys::mi_heap_new() };
        let heap = NonNull::new(res).ok_or(MiHeapAllocError(()))?;
        Ok(Self { ptr: heap })
    }

    /// Get a raw pointer to the underlying heap.
    ///
    /// ## Safety
    /// While this operation itself is safe, many operations can cause undefined behavior.
    /// In particular, destroying or deleting the heap using a raw pointer
    /// could trigger a double-free when the [`MiHeap`] is dropped.
    #[inline]
    pub fn as_ptr(&self) -> *mut mi_heap_t {
        self.ptr.as_ptr()
    }

    /// Allocate a block of memory with the specified layout in this heap.
    ///
    /// Returns [`AllocError`] on out of memory and correctly supports zero-sized allocations.
    ///
    /// Similar to [`crate::mi_malloc_aligned`], but specific to this heap.
    ///
    /// [`AllocError`]: core::alloc::AllocError
    ///
    /// ## Safety
    /// Always safe to call.
    /// The allocated memory can be freed from any thread using [`crate::mi_free`].
    ///
    /// Must only be invoked from the heap's original thread,
    /// but this is guaranteed by `!Send`.
    #[inline]
    pub fn malloc_aligned(&self, layout: Layout) -> Result<NonNull<c_void>, OutOfMemoryError> {
        // SAFETY: Always safe to call
        unsafe {
            OutOfMemoryError::if_null(sys::mi_heap_malloc_aligned(
                self.as_ptr(),
                layout.size(),
                layout.align(),
            ))
        }
    }

    /// Allocate a zero-initialized block of memory with the specified layout in this heap.
    ///
    /// Similar to [`crate::mi_zalloc_aligned`], but specific to this heap.
    ///
    /// ## Safety
    /// Always safe to call.
    /// The allocated memory can be freed from any thread using [`crate::mi_free`].
    ///
    /// Must only be invoked from the heap's original thread,
    /// but this is guaranteed by `!Send`.
    #[inline]
    pub fn zalloc_aligned(&self, layout: Layout) -> Result<NonNull<c_void>, OutOfMemoryError> {
        // SAFETY: Always safe to call
        unsafe {
            OutOfMemoryError::if_null(sys::mi_heap_zalloc_aligned(
                self.as_ptr(),
                layout.size(),
                layout.align(),
            ))
        }
    }

    /// Re-allocate a block of memory with the specified layout in this heap.
    ///
    /// Similar to [`crate::mi_realloc_aligned`], but specific to this heap.
    ///
    /// ## Safety
    /// The original memory must be allocated from this heap.
    ///
    /// Must only be invoked from the heap's original thread,
    /// but this is guaranteed by `!Send`.
    #[inline]
    pub unsafe fn realloc_aligned(
        &self,
        ptr: *mut c_void,
        layout: Layout,
    ) -> Result<NonNull<c_void>, OutOfMemoryError> {
        // SAFETY: Caller guarantees this was allocated from this heap
        unsafe {
            OutOfMemoryError::if_null(sys::mi_heap_realloc_aligned(
                self.as_ptr(),
                ptr,
                layout.size(),
                layout.align(),
            ))
        }
    }

    /// Re-allocate a zero-initialized block of memory with the specified layout in this heap.
    ///
    /// Similar to [`crate::mi_zalloc_aligned`], but specific to this heap.
    ///
    /// ## Safety
    /// The original memory must be allocated from this heap.
    /// Requires that the memory originally have been zero-initialized,
    /// just like with [`crate::mi_rezalloc_aligned`].
    ///
    /// Must come from the heap's original thread,
    /// but this is guaranteed by `!Send`.
    #[inline]
    pub unsafe fn rezalloc_aligned(
        &self,
        ptr: *mut c_void,
        layout: Layout,
    ) -> Result<NonNull<c_void>, OutOfMemoryError> {
        // SAFETY: Guaranteed by caller
        unsafe {
            OutOfMemoryError::if_null(sys::mi_heap_rezalloc_aligned(
                self.as_ptr(),
                ptr,
                layout.size(),
                layout.align(),
            ))
        }
    }

    /// Free memory allocated in this heap.
    ///
    /// This unconditionally delegates to [`crate::mi_free`],
    /// since that is the single interface to free all memory allocated through mimalloc,
    /// regardless of the original heap or thread.
    ///
    /// It exists only for consistency, and you should prefer [`crate::mi_free`].
    ///
    /// ## Safety
    /// Same requirements as [`crate::mi_free`].
    #[inline]
    pub unsafe fn free(&self, ptr: *mut c_void) {
        // SAFETY: Caller guarantee
        unsafe { crate::mi_free(ptr) }
    }

    /// Collect outstanding resources in this heap.
    ///
    /// Similar to [`crate::mi_collect`], but specific to this heap.
    #[inline]
    pub fn collect(&self, force: bool) {
        // SAFETY: Valid heap and otherwise Safe to call
        unsafe { sys::mi_heap_collect(self.as_ptr(), force) }
    }

    /// Destroy the heap, freeing all allocated memory (calls [`sys::mi_heap_destroy`]).
    ///
    /// Prefer to use [`Self::delete`], which will preserve all allocated memory.
    ///
    /// ## Safety
    /// Undefined behavior if any of the memory in the heap is still being used.
    #[inline]
    pub unsafe fn destroy(self) {
        let heap = ManuallyDrop::new(self);
        // SAFETY: Caller is responsible for correctness, will not run destructor
        unsafe { sys::mi_heap_destroy(heap.as_ptr()) }
    }

    /// Free the resources associated with the heap, while preserving all allocated memory (calls [`sys::mi_heap_delete`]).
    ///
    /// Transfers still allocated blocks to the thread's default allocator.
    ///
    /// This is automatically run on [`Drop`], and exists as a separate method only for clarity.
    #[inline]
    pub fn delete(self) {
        drop(self);
    }

    /// Visit all areas and blocks in a heap.
    ///
    /// If `visit_all_blocks=true` this visits all allocated blocks.
    /// Otherwise, the visitor is only called for every heap area.
    ///
    /// The callback arguments have the names `heap, area, block, block_size`,
    /// and returns a value of [`ControlFlow`].
    ///
    /// This function is always first called for every area with `block` as a NULL pointer.
    /// If `visit_all_blocks` was true,
    /// the function is then called for every allocated block in that area.
    ///
    /// ## Example
    /// ```no_run
    /// # use std::ffi::c_void;
    /// # use mimalloc_rich::{sys, heap::MiHeap};
    /// #
    /// # fn do_visit_heap(heap: &MiHeap) {
    /// use std::ops::ControlFlow;
    /// let mut callback = |heap: &MiHeap, area: &sys::mi_heap_area_t, block: *mut c_void, block_size: usize| {
    ///     if !block.is_null() && block_size >= 100 {
    ///         return ControlFlow::Break(());
    ///     }
    ///     ControlFlow::Continue(())
    /// };
    /// let _: ControlFlow<()> = heap.visit_blocks(true, &mut callback);
    /// # }
    /// ```
    #[inline] // type-specialized
    pub fn visit_blocks<F: HeapVisitorFunc>(&self, visit_all_blocks: bool, visitor: &mut F) -> ControlFlow<()> {
        // SAFETY: Should be safe to visit, trampoline implemented correctly
        let all_visited = unsafe {
            sys::mi_heap_visit_blocks(
                self.as_ptr(),
                visit_all_blocks,
                Some(heap_visitor_trampoline::<F>),
                core::ptr::from_mut::<F>(visitor).cast(),
            )
        };
        if all_visited {
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break(())
        }
    }
}
/// The visitor to the [`MiHeap::visit_blocks`]  or [`mi_abandoned_visit_blocks`] function,
/// with signature `fn(heap: &MiHeap, area: &sys::mi_heap_area_t, block: *mut c_void, block_size: usize) -> ControlFlow<()>`
///
/// This function is always first called for every area with block as a NULL pointer.
/// If `visit_all_blocks` was true, the function is then called for every allocated block in that area.
///
/// Return [`ControlFlow::Break`] to stop visiting early.
pub trait HeapVisitorFunc: FnMut(&MiHeap, &sys::mi_heap_area_t, *mut c_void, usize) -> ControlFlow<()> {}
impl<F: FnMut(&MiHeap, &sys::mi_heap_area_t, *mut c_void, usize) -> ControlFlow<()>> HeapVisitorFunc for F {}
/// Convert a rust-style [`HeapVisitorFunc`] to a C-style [`sys::mi_block_visit_fun`].
///
/// ## Safety
/// All arguments to trampoline must be correct,
/// including `arg` which must point to a valid `&mut F`.
unsafe extern "C" fn heap_visitor_trampoline<F: HeapVisitorFunc>(
    heap: *const mi_heap_t,
    area: *const sys::mi_heap_area_t,
    block: *mut c_void,
    block_size: usize,
    arg: *mut c_void,
) -> bool {
    // SAFETY: Assume `arg` points to `F`
    let func = unsafe { &mut *arg.cast::<F>() };
    let heap = ManuallyDrop::new(MiHeap {
        // SAFETY: Mimalloc should only give a valid heap
        ptr: unsafe { NonNull::new_unchecked(heap.cast_mut()) },
    });
    func(
        &heap,
        // SAFETY: Trust area to be non-null and type-accurate
        unsafe { &*area },
        block,
        block_size,
    )
    .is_continue()
}
impl Drop for MiHeap {
    fn drop(&mut self) {
        // SAFETY: Heap no longer in use
        unsafe { sys::mi_heap_destroy(self.as_ptr()) }
    }
}
impl Debug for MiHeap {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MiHeap").finish_non_exhaustive()
    }
}
/// Implements the allocator API
///
/// Note that while allocation is specific to a particular heap and thread,
/// deallocate uses the global [`crate::mi_free`] function and can be called from any thread.
/// There is no need to remember the original heap it was allocated in.
// SAFETY: Correctly delegates to mi_heap_t
unsafe impl crate::internal_alloc_api::Allocator for MiHeap {
    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        Ok(NonNull::slice_from_raw_parts(
            self.malloc_aligned(layout)?.cast::<u8>(),
            layout.size(),
        ))
    }

    #[inline]
    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        Ok(NonNull::slice_from_raw_parts(
            self.zalloc_aligned(layout)?.cast::<u8>(),
            layout.size(),
        ))
    }

    /// Deallocate the specified block of memory by unconditionally delegating to [`crate::mi_free`].
    ///
    /// There is only a single free function in all of mimalloc.
    /// There is nno function for heap-specific frees,
    ///
    /// ## Safety
    /// Same safety requirements as [`crate::mi_free`].
    #[inline]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, _layout: Layout) {
        // SAFETY: Validity guaranteed by the caller
        unsafe { crate::mi_free(ptr.as_ptr().cast()) }
    }

    #[inline]
    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        let _ = old_layout; // ignored
        Ok(NonNull::slice_from_raw_parts(
            // SAFETY: Caller guarantees the original pointer is correct
            unsafe { self.realloc_aligned(ptr.as_ptr().cast(), new_layout)?.cast::<u8>() },
            new_layout.size(),
        ))
    }

    #[inline]
    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        let _ = old_layout; // ignored
        Ok(NonNull::slice_from_raw_parts(
            // SAFETY: Caller guarantees the original pointer is correct
            unsafe { self.realloc_aligned(ptr.as_ptr().cast(), new_layout)?.cast::<u8>() },
            new_layout.size(),
        ))
    }
}

impl_public_allocator_traits!(impl Allocator for MiHeap);
