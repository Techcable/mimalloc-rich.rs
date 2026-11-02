pub trait Sealed {}

macro_rules! common_code {
    (impl $target:ident $(<$l:lifetime>)? {
        type Ptr = $ptr:ty;
        type Owned = $owned:ty;
        ffi_mod = $ffi_mod:ident;
        ffi_prefix = $ffi_prefix:ident;
        // HACK: Whether to call `as_ptr` or `as_heap_ptr` when implementing
        // realloc_aligned and rezalloc_aligned
        // Needed until PR mimalloc#1477 is released.
        realloc_aligned_ptr_method = $realloc_aligned_ptr_method:ident;
        // Custom `ffi_prefix` for realloc_aligned, rezalloc_aligned
        // Needed until PR mimalloc#1477 is released.
        realloc_aligned_ffi_prefix = $realloc_aligned_prefix:ident;
        owned_meta = {
            $(#[$owned_meta:meta])*
        };
    }) => {
        const _: () = {
            use core::alloc::Layout;
            use core::ffi::c_void;
            use core::fmt::{Debug, Formatter};
            use core::ptr::NonNull;
            use crate::OutOfMemoryError;
            use crate::heap::MiHeap;

            use crate::internal_alloc_api::AllocError;

            pastey::paste! {
                // SAFETY: Correct impl simply delegates
                unsafe impl MiHeap for $target $(<$l>)? {
                    type Ptr = $ptr;
                    #[inline]
                    fn as_ptr(&self) -> $ptr {
                        self.ptr.as_ptr()
                    }

                    #[inline]
                    fn malloc_aligned(&self, layout: Layout) -> Result<NonNull<c_void>, OutOfMemoryError> {
                        // SAFETY: Always safe to call
                        unsafe {
                            OutOfMemoryError::if_null($ffi_mod::[<$ffi_prefix _malloc_aligned>](
                                self.as_ptr(),
                                layout.size(),
                                layout.align(),
                            ))
                        }
                    }

                    #[inline]
                    fn zalloc_aligned(&self, layout: Layout) -> Result<NonNull<c_void>, OutOfMemoryError> {
                        // SAFETY: Always safe to call
                        unsafe {
                            OutOfMemoryError::if_null($ffi_mod::[<$ffi_prefix _zalloc_aligned>](
                                self.as_ptr(),
                                layout.size(),
                                layout.align(),
                            ))
                        }
                    }

                    #[inline]
                    unsafe fn realloc_aligned(
                        &self,
                        ptr: *mut c_void,
                        layout: Layout,
                    ) -> Result<NonNull<c_void>, OutOfMemoryError> {
                        // SAFETY: Caller guarantees this was allocated from this heap
                        unsafe {
                            OutOfMemoryError::if_null($ffi_mod::[<$realloc_aligned_prefix _realloc_aligned>](
                                self.$realloc_aligned_ptr_method(),
                                ptr,
                                layout.size(),
                                layout.align(),
                            ))
                        }
                    }

                    #[inline]
                    unsafe fn rezalloc_aligned(
                        &self,
                        ptr: *mut c_void,
                        layout: Layout,
                    ) -> Result<NonNull<c_void>, OutOfMemoryError> {
                        // SAFETY: Guaranteed by caller
                        unsafe {
                            OutOfMemoryError::if_null($ffi_mod::[<$realloc_aligned_prefix _rezalloc_aligned>](
                                self.$realloc_aligned_ptr_method(),
                                ptr,
                                layout.size(),
                                layout.align(),
                            ))
                        }
                    }

                    #[inline]
                    fn collect(&self, force: bool) {
                        // SAFETY: always safe to call
                        unsafe {
                            $ffi_mod::[<$ffi_prefix _collect>](self.as_ptr(), force)
                        }
                    }
                }
                impl super::common::Sealed for $target $(<$l>)? {}
                $(#[$owned_meta])*
                // SAFETY: Correct impl simply delegates
                unsafe impl crate::heap::MiOwnedHeap for $owned {
                    #[inline]
                    unsafe fn destroy(self) {
                        let ptr = self.into_owned_ptr();
                        // SAFETY: Caller is responsible for correctness,
                        // using into_owned_ptr avoids destructor
                        unsafe { crate::sys::mi_heap_destroy(ptr) }
                    }
                }
                $(#[$owned_meta])*
                impl Drop for $target$(<$l>)? {
                    fn drop(&mut self) {
                        // SAFETY: Heap no longer in use
                        unsafe { sys::mi_heap_delete(self.as_ptr()) }
                    }
                }

            }

            impl Debug for $target $(<$l>)? {
                fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
                    f.debug_struct(stringify!($target)).finish_non_exhaustive()
                }
            }
            /// Implements the allocator API
            ///
            /// Note that while allocation is specific to a particular heap and thread,
            /// deallocate uses the global [`crate::mi_free`] function and can be called from any thread.
            /// There is no need to remember the original heap it was allocated in.
            // SAFETY: Correctly delegates to heap
            unsafe impl crate::internal_alloc_api::Allocator for $target $(<$l>)? {
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
                /// There is no function for heap-specific frees.
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

            crate::internal_alloc_api::impl_public_allocator_traits!(impl Allocator for $target $(<$l>)?);
        };
    };
}

pub(crate) use common_code;
