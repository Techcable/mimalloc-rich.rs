//! Code specific to mimalloc v1/v2.

use core::ffi::{c_int, c_void};
use core::ops::ControlFlow;

use sys::mi_heap_t;

use crate::heap::{MiHeap, MiHeapAllocError, MiLocalHeap};
use crate::subproc::MiSubprocId;

#[cfg_attr(feature = "nightly-doc", doc(cfg(any(mimalloc1, mimalloc2))))]
impl MiLocalHeap<'static> {
    /// Create a builder to allocate a heap with custom options.
    ///
    /// See also [`Self::new`]
    #[inline]
    pub fn builder() -> MiLocalHeapBuilder {
        MiLocalHeapBuilder::default()
    }

    /// Create a new heap that can be used for allocation.
    ///
    /// See also [`Self::builder`].
    #[inline]
    pub fn new() -> Result<Self, MiHeapAllocError> {
        // SAFETY: This function is safe
        let res = unsafe { sys::mi_heap_new() };
        if res.is_null() {
            return Err(MiHeapAllocError(()));
        }
        // SAFETY: Newly created heap must be valid
        Ok(unsafe { Self::from_owned_ptr(res) })
    }
}

/// Visit all areas and blocks in abandoned heaps.
///
/// Requires that the [`crate::options::AllowVisitAbandoned`] runtime flag is enabled at the start of the program.
///
/// The `subproc_id` must match that of the abandoned heaps.
/// If `heap_tag` is specified, only visits those memory blocks with a matching heap tag.
/// If `visit_blocks` is true, visits all the allocated blocks;
/// otherwise it is only called on each heap arena.
///
/// Unless otherwise stated, functionality behaves the same as [`MiLocalHeap::visit_blocks`].
/// Wraps [`sys::mi_abandoned_visit_blocks`].
#[inline]
#[cfg_attr(feature = "nightly-doc", doc(cfg(any(mimalloc1, mimalloc2))))]
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

/// The visitor to the [`MiLocalHeap::visit_blocks`]  or [`mi_abandoned_visit_blocks`] function,
/// with signature `fn(heap: &MiHeap, area: &sys::mi_heap_area_t, block: *mut c_void, block_size: usize) -> ControlFlow<()>`
///
/// This function is always first called for every area with block as a NULL pointer.
/// If `visit_all_blocks` was true, the function is then called for every allocated block in that area.
///
/// Return [`ControlFlow::Break`] to stop visiting early.
#[cfg_attr(feature = "nightly-doc", doc(cfg(any(mimalloc1, mimalloc2))))]
pub trait HeapVisitorFunc: FnMut(&MiLocalHeap, &sys::mi_heap_area_t, *mut c_void, usize) -> ControlFlow<()> {}
impl<F: FnMut(&MiLocalHeap, &sys::mi_heap_area_t, *mut c_void, usize) -> ControlFlow<()>> HeapVisitorFunc for F {}

#[cfg_attr(feature = "nightly-doc", doc(cfg(any(mimalloc1, mimalloc2))))]
impl MiLocalHeap<'_> {
    /// Visit all areas and blocks in a heap.
    ///
    /// Only available on mimalloc v1/v2.
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
    /// #
    /// # fn do_visit_heap(heap: &MiLocalHeap) {
    /// use std::ops::ControlFlow;
    /// let mut callback = |heap: &MiLocalHeap, area: &sys::mi_heap_area_t, block: *mut c_void, block_size: usize| {
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
    // SAFETY: Caller guarantees that `heap` is valid
    #[expect(unused_unsafe, reason = "multiple unsafe ops")]
    unsafe {
        MiLocalHeap::with_borrowed_ptr(heap.cast_mut(), |heap| {
            func(
                heap,
                // SAFETY: Trust area to be non-null and type-accurate
                unsafe { &*area },
                block,
                block_size,
            )
            .is_continue()
        })
    }
}

/// Allocates a [`MiLocalHeap`] with custom options.
///
/// Only available on mimalloc v1/v2,
/// as mimalloc v3 creates heap at the [`MiSyncHeap`] level.
///
/// TODO: Support custom `arena_id`.
#[derive(Default)]
#[cfg_attr(feature = "nightly-doc", doc(cfg(any(mimalloc1, mimalloc2))))]
pub struct MiLocalHeapBuilder {
    heap_tag: HeapTag,
    allow_destroy: bool,
}
impl MiLocalHeapBuilder {
    /// Set if [`MiLocalHeap::destroy`] allowed?
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

    /// Allocate a new [`MiLocalHeap`] with the configured options,
    /// returning an error if allocation fails.
    ///
    /// See also [`MiLocalHeap::new`], which performs no special configuration.
    #[inline]
    pub fn build(&mut self) -> Result<MiLocalHeap<'static>, MiHeapAllocError> {
        // SAFETY: Options are valid
        let ptr = unsafe { sys::mi_heap_new_ex(self.heap_tag.0, self.allow_destroy, 0) };
        if ptr.is_null() {
            return Err(MiHeapAllocError(()));
        }
        // SAFETY: Newly created heap is valid
        Ok(unsafe { MiLocalHeap::from_owned_ptr(ptr) })
    }
}

/// A tag for a [`MiLocalHeap`],
/// to avoid mixing objects of different types.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct HeapTag(pub c_int);
impl HeapTag {
    /// The default heap tag, if none is specified or [`MiLocalHeap::new`] is used.
    pub const DEFAULT: HeapTag = HeapTag(0);
}
impl Default for HeapTag {
    fn default() -> Self {
        Self::DEFAULT
    }
}
