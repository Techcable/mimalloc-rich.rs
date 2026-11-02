//! Defines the [`MiSyncHeap`] type and associated functionality.
//!
//! This type is present only on mimalloc v3.

use core::ptr::NonNull;

use sys::{self, mi_heap_t};

use crate::heap::{MiHeapAllocError, MiLocalHeap};

/// A first-class heap, owning a group of allocations.
///
/// In v3 heaps can be used to allocate efficiently from any thread
/// without needing a [`MiLocalHeap`].
pub struct MiSyncHeap {
    ptr: NonNull<mi_heap_t>,
}
impl MiSyncHeap {
    /// Create a new heap.
    #[inline]
    pub fn new() -> Result<Self, MiHeapAllocError> {
        // SAFETY: This function is safe to call
        let raw_heap = unsafe { crate::sys::mi_heap_new() };
        if raw_heap.is_null() {
            Err(MiHeapAllocError(()))
        } else {
            // SAFETY: We know newly allocated heap is valid
            unsafe { Ok(Self::from_owned_ptr(raw_heap)) }
        }
    }

    /// Convert an owned pointer to a heap into a [`MiSyncHeap`] wrapper.
    ///
    /// The heap will be [deleted] when [`Drop`] is called.
    ///
    /// [deleted]: crate::heap::MiOwnedHeap::delete
    ///
    /// # Safety
    /// The pointer must represent a valid heap
    /// that the caller has ownership of.
    #[inline]
    pub unsafe fn from_owned_ptr(ptr: *mut mi_heap_t) -> Self {
        MiSyncHeap {
            // SAFETY: Caller guarantees validity, meaning this is not null
            ptr: unsafe { NonNull::new_unchecked(ptr) },
        }
    }

    /// Get the [`MiLocalHeap`] for the current thread.
    #[inline]
    pub fn local_heap(&self) -> MiLocalHeap<'_> {
        // SAFETY: We know our own pointer is valid
        let local_heap = unsafe { sys::mi_heap_theap(self.ptr.as_ptr()) };
        // Verify non-null with a runtime check
        // This is currently never NULL,
        // but that is not a public API guarantee (see PR#1483)
        let local_heap = NonNull::new(local_heap).expect("result `mi_heap_theap` is null");
        // SAFETY: Trust the result of mi_heap_theap to valid
        unsafe { MiLocalHeap::from_raw_parts(local_heap, self.ptr) }
    }
}
// SAFETY: The underlying type is thread-safe
unsafe impl Sync for MiSyncHeap {}
// SAFETY: The underlying type is thread-safe
unsafe impl Send for MiSyncHeap {}

super::common::common_code!(impl MiSyncHeap {
    type Ptr = *mut mi_heap_t;
    type Owned = MiSyncHeap;
    ffi_mod = sys;
    ffi_prefix = mi_heap;
    realloc_aligned_ptr_method = as_ptr;
    realloc_aligned_ffi_prefix = mi_heap;
    owned_meta = {};
});
