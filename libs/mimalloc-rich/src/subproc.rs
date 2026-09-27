//! The mimalloc subprocess API.
//!
//! Not much is implemented here,
//! only the minimum needed for [`crate::heap::mi_abandoned_visit_blocks`].

/// A mimalloc subprocess id,
/// used to ensure one process doesn't free the resources of another.
///
/// Wraps [`sys::mi_subproc_id_t`].
#[derive(Copy, Clone, Debug)]
pub struct MiSubprocId(sys::mi_subproc_id_t);
impl MiSubprocId {
    /// Create a `MiSubprocId` from a raw value.
    ///
    /// ## Safety
    /// Undefined behavior if the value is invalid.
    pub unsafe fn new_unchecked(raw: sys::mi_subproc_id_t) -> Self {
        MiSubprocId(raw)
    }

    /// Get the primary subprocess id,
    /// referring to the original process.
    ///
    /// Wraps [`sys::mi_subproc_main`].
    #[inline]
    pub fn main() -> MiSubprocId {
        // SAFETY: Requesting identifier should be safe
        let raw = unsafe { sys::mi_subproc_main() };
        // SAFETY: Main id is known to be valid
        unsafe { MiSubprocId::new_unchecked(raw) }
    }

    /// Convert this ID into a [`sys::mi_subproc_id_t`].
    #[inline]
    pub fn to_ffi(&self) -> sys::mi_subproc_id_t {
        self.0
    }
}
