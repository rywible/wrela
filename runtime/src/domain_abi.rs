use std::path::Path;

pub type DomainInitFn = unsafe extern "C" fn() -> i64;
pub type DomainTickFn =
    unsafe extern "C" fn(axis_x: f64, axis_y: f64, dt_ms: u32, collect_pressed: u32) -> i64;
pub type DomainSnapshotFn = unsafe extern "C" fn(buf_ptr: *mut u8, buf_len: usize) -> usize;
pub type DomainApplySnapshotFn = unsafe extern "C" fn(buf_ptr: *const u8, buf_len: usize) -> i64;

pub struct DomainAbi {
    pub init: DomainInitFn,
    pub tick: DomainTickFn,
    pub snapshot: DomainSnapshotFn,
    pub apply_snapshot: DomainApplySnapshotFn,
    _lib: libloading::Library,
}

impl std::fmt::Debug for DomainAbi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DomainAbi")
            .field("init", &(self.init as usize))
            .field("tick", &(self.tick as usize))
            .finish_non_exhaustive()
    }
}

impl DomainAbi {
    /// # Safety
    ///
    /// The shared library at `path` must export the domain ABI symbols with
    /// correct signatures. The caller is responsible for ensuring the library
    /// is compatible with the current runtime version.
    pub unsafe fn load(path: &Path) -> Result<Self, String> {
        let lib =
            unsafe { libloading::Library::new(path).map_err(|e| format!("dlopen failed: {e}"))? };
        let init: libloading::Symbol<DomainInitFn> = unsafe {
            lib.get(b"wrela_domain_init")
                .map_err(|e| format!("missing wrela_domain_init: {e}"))?
        };
        let tick: libloading::Symbol<DomainTickFn> = unsafe {
            lib.get(b"wrela_domain_tick")
                .map_err(|e| format!("missing wrela_domain_tick: {e}"))?
        };
        let snapshot: libloading::Symbol<DomainSnapshotFn> = unsafe {
            lib.get(b"wrela_domain_snapshot")
                .map_err(|e| format!("missing wrela_domain_snapshot: {e}"))?
        };
        let apply_snapshot: libloading::Symbol<DomainApplySnapshotFn> = unsafe {
            lib.get(b"wrela_domain_apply_snapshot")
                .map_err(|e| format!("missing wrela_domain_apply_snapshot: {e}"))?
        };

        Ok(Self {
            init: *init,
            tick: *tick,
            snapshot: *snapshot,
            apply_snapshot: *apply_snapshot,
            _lib: lib,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn load_returns_error_for_nonexistent_path() {
        let path = PathBuf::from("/tmp/wrela_nonexistent_domain_lib.dylib");
        let result = unsafe { DomainAbi::load(&path) };
        assert!(result.is_err());
        let err_msg = result.err().unwrap();
        assert!(err_msg.contains("dlopen failed"));
    }
}
