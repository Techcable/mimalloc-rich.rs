//! Defines the [`MiLocalHeap`] type and associated functionality.
//!
//! This type is present on all mimalloc versions (v1, v2, and v3).

use core::marker::PhantomData;
use core::ptr::NonNull;

#[cfg(any(mimalloc1, mimalloc2))]
mod legacy;

#[cfg(any(mimalloc1, mimalloc2))]
pub use self::legacy::*;

/// Depending on the version, delegates to either `mi_theap_x` or `mi_heap_x`
#[allow(non_camel_case_types)]
mod theap {
    macro_rules! fallback_dispatch {
        ($($name:ident),+ $(,)?) => {
            cfg_if::cfg_if! {
                if #[cfg(any(mimalloc1, mimalloc2))] {
                    pastey::paste! {
                        $(pub use crate::sys::[<mi_heap_ $name>] as [<mi_theap_ $name>];)*
                    }
                } else if #[cfg(mimalloc3)] {
                    pastey::paste! {
                        $(pub use crate::sys::[<mi_theap_ $name>];)*
                    }
                } else {
                    compile_error!("Unknown mimalloc version");
                }
            }
        };
    }
    fallback_dispatch!(malloc_aligned, zalloc_aligned, collect);
    cfg_if::cfg_if! {
        if #[cfg(any(mimalloc1, mimalloc2))] {
            pub use sys::mi_heap_t as mi_theap_t;
        } else if #[cfg(mimalloc3)] {
            pub use sys::mi_theap_t;
        } else {
            compile_error!("Unknown version");
        }
    }

    // sadly no way to efficiently emulate these functions on mimalloc3 until PR mimalloc#1477 is released
    // As a workaround, we carry along the owning mi_heap_t* and use that.
    // This means we unconditionally use the mi_heap_...  functions
    // This also requires additional hacks in the common_code! macro
    pub use crate::sys::{mi_heap_realloc_aligned, mi_heap_rezalloc_aligned};
}

/// The native type of thread-local heaps.
///
/// On v2 this is an alias for [`sys::mi_heap_t`].
/// On v3, it is an alias for [`sys::mi_theap_t`].
#[expect(non_camel_case_types, reason = "mirrors C naming")]
pub type mi_theap_t = self::theap::mi_theap_t;

/// A thread-local heap.
///
/// In mimalloc v2 this is owned and the lifetime always 'static.
/// In mimalloc v3 this is bound to a particular [`crate::heap::MiSyncHeap`].
///
/// # Thread Safety
/// The heap is not thread safe to allocate from another thread, so is `!Sync`.
/// Memory allocated from a heap can be freed from any thread using [`crate::mi_free`].
/// It is unclear if the destructor can be called from another thread, so the type is `!Send`.
pub struct MiLocalHeap<'a> {
    marker: PhantomData<&'a ()>,
    ptr: NonNull<mi_theap_t>,
    /// Workaround until mimalloc#1477 is released.
    #[cfg(mimalloc3)]
    owning_heap: NonNull<sys::mi_heap_t>,
}
/// On mimalloc v3 this is a borrowed reference,
/// meaning it can be freely and cheaply duplicated.
///
/// The [`Copy`] trait is not implemented for better compatibility with v1/v2,
/// where the heap is [owned](crate::heap::MiOwnedHeap)
/// and duplication is not possible.
#[cfg(mimalloc3)]
impl Clone for MiLocalHeap<'_> {
    #[inline]
    fn clone(&self) -> Self {
        MiLocalHeap {
            marker: self.marker,
            ptr: self.ptr,
            owning_heap: self.owning_heap,
        }
    }
}
impl MiLocalHeap<'_> {
    #[inline]
    #[cfg(mimalloc3)]
    pub(crate) unsafe fn from_raw_parts(theap: NonNull<mi_theap_t>, heap: NonNull<sys::mi_heap_t>) -> Self {
        MiLocalHeap {
            marker: PhantomData,
            ptr: theap,
            owning_heap: heap,
        }
    }

    // TODO: Add this once v3 stops needing the PR mimalloc#1477 workaround
    #[cfg(any())]
    pub fn from_borrowed_ptr(theap: *mut mi_theap_t) -> Self {
        todo!()
    }

    /// Workaround for PR mimalloc#1477 is released
    #[inline]
    fn as_heap_ptr(&self) -> *mut sys::mi_heap_t {
        #[cfg(any(mimalloc1, mimalloc2))]
        {
            use crate::heap::MiHeap;
            // in v1 & v2, heap = theap
            self.as_ptr()
        }
        #[cfg(mimalloc3)]
        {
            self.owning_heap.as_ptr()
        }
    }
}
impl MiLocalHeap<'static> {
    /// Convert an owned pointer to a heap into a [`MiLocalHeap`] wrapper.
    ///
    /// The heap will be [deleted](Self::delete) when [`Drop`] is called.
    ///
    /// This function is not available on v3,
    /// as heaps on that version are never owned.
    ///
    /// # Safety
    /// The pointer must represent a valid heap
    /// that the caller has ownership of
    /// and the heap must be valid for use on this thread.
    #[inline]
    #[cfg(any(mimalloc1, mimalloc2))]
    #[cfg_attr(feature = "nightly-doc", doc(cfg(any(mimalloc1, mimalloc2))))]
    pub unsafe fn from_owned_ptr(theap: *mut mi_theap_t) -> Self {
        MiLocalHeap {
            marker: PhantomData,
            // SAFETY: Caller guarantees pointer is valid and not null
            ptr: unsafe { NonNull::new_unchecked(theap) },
        }
    }

    /// Using a borrowed pointer to the heap, execute the specified closure.
    ///
    /// This function is not currently available on v3 due to implementation limitations.
    ///
    /// # Safety
    /// The pointer must represent a valid heap,
    /// valid for the duration of the closure.
    #[inline]
    #[cfg(any(mimalloc1, mimalloc2))]
    pub unsafe fn with_borrowed_ptr<R>(theap: *mut mi_theap_t, func: impl FnOnce(&MiLocalHeap) -> R) -> R {
        use core::mem::ManuallyDrop;
        // SAFETY: The caller guarantees the validity of the pointer.
        // The ManuallyDrop ensures that the destructor is not executed,
        // and the callback only receives a & reference so cannot do anything bad
        let heap = unsafe { ManuallyDrop::new(Self::from_owned_ptr(theap)) };
        func(&heap)
    }
}
super::common::common_code!(impl MiLocalHeap<'_> {
    type Ptr = *mut mi_theap_t;
    type Owned = MiLocalHeap<'static>;
    ffi_mod = theap;
    ffi_prefix = mi_theap;
    realloc_aligned_ptr_method = as_heap_ptr;
    // use heap functions not theap functions until PR mimalloc#1477 is released
    realloc_aligned_ffi_prefix = mi_heap;
    owned_meta = {
        #[cfg(any(mimalloc1, mimalloc2))]
        #[cfg_attr(feature = "nightly-doc", doc(cfg(any(mimalloc1, mimalloc2))))]
    };
});
static_assertions::assert_not_impl_any!(MiLocalHeap<'static>: Send, Sync);
