//! Code specific to mimalloc v1/v2.

use core::ffi::c_int;
use core::ops::ControlFlow;

use crate::heap::{HeapVisitorFunc, MiHeapAllocError, MiLocalHeap};
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
///
/// Since the [theaps](MiLocalHeap) for dead threads have been freed,
/// the `heap_ptr` parameter to the callback should always be null.
///
/// # Safety
/// Must not free or reallocate any of the memory that is being visited.
///
/// Since this is v1/v2, and there are no first-class heaps,
/// the other safety requirements of [`MiLocalHeap::visit_blocks`] aren't applicable,
#[inline]
#[cfg_attr(feature = "nightly-doc", doc(cfg(any(mimalloc1, mimalloc2))))]
pub unsafe fn mi_abandoned_visit_blocks<F: HeapVisitorFunc>(
    subproc_id: MiSubprocId,
    heap_tag: Option<HeapTag>,
    visit_blocks: bool,
    visitor: &mut F,
) -> ControlFlow<()> {
    use crate::heap::common::heap_visitor_trampoline;

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
