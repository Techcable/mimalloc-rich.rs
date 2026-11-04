//! Tests for the high-level heap API.
//!
//! Tests for [`MiLocalHeap`] ownership only run on mimalloc v1/v2,
//! and tests for [`MiSyncHeap`] only run on mimalloc v3.

use core::alloc::Layout;
use core::ptr::NonNull;

use mimalloc_rich::heap::{MiHeap, MiOwnedHeap};

/// Allocate a block of `len` bytes from the heap and fill it with `fill`.
fn alloc_filled<H: MiHeap>(heap: &H, len: usize, fill: u8) -> NonNull<u8> {
    let layout = Layout::from_size_align(len, 16).unwrap();
    let ptr = heap.malloc_aligned(layout).expect("allocation failed").cast::<u8>();
    assert_eq!(ptr.as_ptr().addr() % 16, 0, "misaligned");
    // SAFETY: Just allocated `len` bytes
    unsafe { ptr.as_ptr().write_bytes(fill, len) };
    ptr
}

/// Check that the block at `ptr` has `len` bytes equal to `fill`.
///
/// # Safety
/// The pointer must be valid for reads of `len` bytes.
unsafe fn assert_filled(ptr: NonNull<u8>, len: usize, fill: u8) {
    // SAFETY: Guaranteed by caller
    let bytes = unsafe { core::slice::from_raw_parts(ptr.as_ptr(), len) };
    assert!(bytes.iter().all(|&b| b == fill), "memory was corrupted: {bytes:?}");
}

/// Exercise all the allocation methods of [`MiHeap`].
fn check_alloc_methods<H: MiHeap>(heap: &H) {
    // malloc + realloc preserves contents & alignment
    let ptr = alloc_filled(heap, 32, 0xAB);
    let big = Layout::from_size_align(4096, 256).unwrap();
    // SAFETY: Pointer was allocated from this heap
    let ptr = unsafe { heap.realloc_aligned(ptr.as_ptr().cast(), big) }
        .expect("realloc failed")
        .cast::<u8>();
    assert_eq!(ptr.as_ptr().addr() % 256, 0, "misaligned");
    // SAFETY: First 32 bytes are preserved by realloc
    unsafe { assert_filled(ptr, 32, 0xAB) };
    // SAFETY: Allocated from mimalloc
    unsafe { heap.free(ptr.as_ptr().cast()) };

    // zalloc + rezalloc keeps everything zeroed
    let small = Layout::from_size_align(64, 32).unwrap();
    let ptr = heap.zalloc_aligned(small).expect("zalloc failed").cast::<u8>();
    assert_eq!(ptr.as_ptr().addr() % 32, 0, "misaligned");
    // SAFETY: Just allocated
    unsafe { assert_filled(ptr, 64, 0) };
    let bigger = Layout::from_size_align(8192, 32).unwrap();
    // SAFETY: Allocated from this heap with zero-initialization
    let ptr = unsafe { heap.rezalloc_aligned(ptr.as_ptr().cast(), bigger) }
        .expect("rezalloc failed")
        .cast::<u8>();
    // SAFETY: rezalloc zero-initializes the new bytes
    unsafe { assert_filled(ptr, 8192, 0) };
    // SAFETY: Allocated from mimalloc
    unsafe { mimalloc_rich::mi_free(ptr.as_ptr().cast()) };

    heap.collect(false);
    heap.collect(true);
}

/// Allocate from a heap, delete it, then check the allocation is still usable.
fn check_delete_preserves_memory<H: MiOwnedHeap>(heap: H, delete: impl FnOnce(H)) {
    let ptr = alloc_filled(&heap, 1024, 0x5A);
    delete(heap);
    // On v1/v2, deleting a heap transfers its blocks to the backing heap,
    // while destroying it would free the pages.
    // Reading freed memory often still "works", so this catches a destroy reliably.
    // NOTE: Debug builds of v2 enable MI_GUARDED, where mi_heap_destroy behaves like
    // mi_heap_delete, so this only detects a destroy in release builds.
    #[cfg(any(mimalloc1, mimalloc2))]
    {
        use mimalloc_rich::sys;
        // SAFETY: Always safe to call
        let backing = unsafe { sys::mi_heap_get_backing() };
        // SAFETY: Backing heap is valid, block was allocated by mimalloc
        let owned = unsafe { sys::mi_heap_contains_block(backing, ptr.as_ptr().cast()) };
        assert!(owned, "block not transferred to backing heap (was the heap destroyed?)");
    }
    // SAFETY: Deleting a heap must preserve its allocations
    unsafe { assert_filled(ptr, 1024, 0x5A) };
    // SAFETY: Memory still allocated
    unsafe { mimalloc_rich::mi_free(ptr.as_ptr().cast()) };
}

#[cfg(any(mimalloc1, mimalloc2))]
mod local {
    use core::ffi::c_void;
    use core::ops::ControlFlow;

    use mimalloc_rich::heap::local::HeapTag;
    use mimalloc_rich::heap::{MiLocalHeap, MiOwnedHeap};

    use super::*;

    #[test]
    fn alloc_methods() {
        let heap = MiLocalHeap::new().unwrap();
        check_alloc_methods(&heap);
    }

    #[test]
    fn drop_preserves_memory() {
        check_delete_preserves_memory(MiLocalHeap::new().unwrap(), drop);
    }

    #[test]
    fn delete_preserves_memory() {
        check_delete_preserves_memory(MiLocalHeap::new().unwrap(), MiOwnedHeap::delete);
    }

    #[test]
    fn builder_destroy() {
        let heap = MiLocalHeap::builder()
            .allow_destroy(true)
            // NOTE: Non-default tags abort on mimalloc v2 debug builds
            // when the heap is dropped/destroyed with live allocations:
            // "page with tag 1 cannot be reclaimed by a heap with the same tag"
            .heap_tag(HeapTag::DEFAULT)
            .build()
            .unwrap();
        let _ = alloc_filled(&heap, 128, 1);
        let _ = alloc_filled(&heap, 128, 2);
        // SAFETY: Allocations are never used again
        unsafe { heap.destroy() };
    }

    #[test]
    fn visit_blocks() {
        const COUNT: usize = 5;
        const SIZE: usize = 48;
        let heap = MiLocalHeap::new().unwrap();
        let ptrs: [NonNull<u8>; COUNT] = core::array::from_fn(|i| alloc_filled(&heap, SIZE, u8::try_from(i).unwrap()));
        let mut found = 0usize;
        let res = heap.visit_blocks(true, &mut |_heap: &MiLocalHeap, _area, block: *mut c_void, size| {
            if !block.is_null() && ptrs.iter().any(|p| p.as_ptr().cast() == block) {
                assert!(size >= SIZE);
                found += 1;
            }
            ControlFlow::Continue(())
        });
        assert_eq!(res, ControlFlow::Continue(()));
        assert_eq!(found, COUNT);

        // Breaking stops early
        let mut calls = 0usize;
        let res = heap.visit_blocks(true, &mut |_heap: &MiLocalHeap, _area, _block, _size| {
            calls += 1;
            ControlFlow::Break(())
        });
        assert_eq!(res, ControlFlow::Break(()));
        assert_eq!(calls, 1);

        for ptr in ptrs {
            // SAFETY: Allocated from mimalloc and still valid
            unsafe { mimalloc_rich::mi_free(ptr.as_ptr().cast()) };
        }
    }
}

#[cfg(mimalloc3)]
mod sync {
    use mimalloc_rich::heap::{MiOwnedHeap, MiSyncHeap};

    use super::*;

    #[test]
    fn alloc_methods() {
        let heap = MiSyncHeap::new().unwrap();
        check_alloc_methods(&heap);
    }

    #[test]
    fn local_alloc_methods() {
        let heap = MiSyncHeap::new().unwrap();
        check_alloc_methods(&heap.local_heap());
        let local = heap.local_heap();
        check_alloc_methods(&local.clone());
    }

    #[test]
    fn drop_preserves_memory() {
        check_delete_preserves_memory(MiSyncHeap::new().unwrap(), drop);
    }

    #[test]
    fn delete_preserves_memory() {
        check_delete_preserves_memory(MiSyncHeap::new().unwrap(), MiOwnedHeap::delete);
    }

    #[test]
    fn local_alloc_survives_delete() {
        let heap = MiSyncHeap::new().unwrap();
        let ptr = alloc_filled(&heap.local_heap(), 256, 0x77);
        heap.delete();
        // SAFETY: Deleting a heap must preserve its allocations
        unsafe { assert_filled(ptr, 256, 0x77) };
        // SAFETY: Memory still allocated
        unsafe { mimalloc_rich::mi_free(ptr.as_ptr().cast()) };
    }

    #[test]
    fn destroy() {
        let heap = MiSyncHeap::new().unwrap();
        let _ = alloc_filled(&heap, 128, 1);
        let _ = alloc_filled(&heap.local_heap(), 128, 2);
        // SAFETY: Allocations are never used again
        unsafe { heap.destroy() };
    }

    #[test]
    fn multithreaded() {
        const THREADS: usize = 4;
        let heap = MiSyncHeap::new().unwrap();
        let ptrs = std::thread::scope(|s| {
            let handles: Vec<_> = (0..THREADS)
                .map(|i| {
                    let heap = &heap;
                    s.spawn(move || {
                        let a = alloc_filled(heap, 512, u8::try_from(i).unwrap());
                        let b = alloc_filled(&heap.local_heap(), 512, u8::try_from(i).unwrap());
                        // raw pointers are not Send
                        [a.as_ptr() as usize, b.as_ptr() as usize]
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect::<Vec<_>>()
        });
        drop(heap);
        for (i, pair) in ptrs.into_iter().enumerate() {
            for addr in pair {
                let ptr = NonNull::new(addr as *mut u8).unwrap();
                // SAFETY: Allocated above and preserved by delete
                unsafe { assert_filled(ptr, 512, u8::try_from(i).unwrap()) };
                // SAFETY: Free from a different thread is allowed
                unsafe { mimalloc_rich::mi_free(ptr.as_ptr().cast()) };
            }
        }
    }
}
