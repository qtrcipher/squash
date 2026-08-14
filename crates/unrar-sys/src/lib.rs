//! Safe-ish Rust wrapper over the C ABI shim for the vendored RARLAB UnRAR
//! source (extraction only — the UnRAR license forbids RAR creation; see
//! vendor/unrar/README.squash.md).
//!
//! The shim speaks plain C types and UTF-8 strings (see `shim/shim.h`); the
//! `dll.hpp` structs never cross into Rust. Error values are the `dll.hpp`
//! `ERAR_*` codes wrapped in [`RarError`], plus [`RarError::ABORTED`] when the
//! caller's data callback aborts an entry.
//!
//! **Threading:** the UnRAR code keeps process-global state (the shared
//! `ErrHandler`, cleaned in `RAROpenArchiveEx`), so two live archives in one
//! process corrupt each other. [`RarArchive`] therefore holds a process-wide
//! mutex from open to close — one open RAR per process at a time.

use std::ffi::{c_char, c_int, c_void, CString};
use std::fmt;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// Serializes all UnRAR use (see module docs). Poisoning is ignored: a
/// panicking callback must not wedge every later archive.
static UNRAR_LOCK: Mutex<()> = Mutex::new(());

/// dll.hpp codes used by the wrapper (see shim.h for the full contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RarError(pub c_int);

impl RarError {
    pub const END_ARCHIVE: c_int = 10;
    pub const BAD_DATA: c_int = 12;
    pub const BAD_ARCHIVE: c_int = 13;
    pub const UNKNOWN_FORMAT: c_int = 14;
    pub const EOPEN: c_int = 15;
    pub const EREAD: c_int = 18;
    pub const EWRITE: c_int = 19;
    pub const MISSING_PASSWORD: c_int = 22;
    pub const BAD_PASSWORD: c_int = 24;
    pub const LARGE_DICT: c_int = 25;
    /// Shim-local: the data callback aborted the current entry.
    pub const ABORTED: c_int = 1000;

    pub fn is_password(&self) -> bool {
        matches!(self.0, Self::MISSING_PASSWORD | Self::BAD_PASSWORD)
    }
}

impl fmt::Display for RarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let desc = match self.0 {
            Self::END_ARCHIVE => "end of archive",
            Self::BAD_DATA => "bad data (CRC mismatch)",
            Self::BAD_ARCHIVE => "bad archive",
            Self::UNKNOWN_FORMAT => "unknown format",
            Self::EOPEN => "cannot open",
            Self::EREAD => "read error",
            Self::EWRITE => "write error",
            Self::MISSING_PASSWORD => "missing password",
            Self::BAD_PASSWORD => "bad password",
            Self::LARGE_DICT => "dictionary too large",
            Self::ABORTED => "aborted by callback",
            other => return write!(f, "unrar error {other}"),
        };
        write!(f, "unrar: {desc}")
    }
}

impl std::error::Error for RarError {}

/// Metadata for one archive entry (UTF-8 name, `\`-separators included).
#[derive(Debug, Clone)]
pub struct RarEntry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub is_encrypted: bool,
}

enum SquashRar {}

type DataCallback = Option<extern "C" fn(*const u8, usize, *mut c_void) -> c_int>;

extern "C" {
    fn squash_rar_open(path: *const c_char, list_only: c_int, out: *mut *mut SquashRar) -> c_int;
    fn squash_rar_next(
        arc: *mut SquashRar,
        name: *mut *const c_char,
        size: *mut u64,
        is_dir: *mut c_int,
        is_encrypted: *mut c_int,
    ) -> c_int;
    fn squash_rar_extract(arc: *mut SquashRar, cb: DataCallback, user: *mut c_void) -> c_int;
    fn squash_rar_skip(arc: *mut SquashRar) -> c_int;
    fn squash_rar_close(arc: *mut SquashRar);
}

/// An open RAR archive. Two usage passes, mirroring `tar_family`: open with
/// [`RarArchive::open_list`] for the metadata pass (layout decision), close,
/// reopen with [`RarArchive::open_extract`] for the byte pass.
pub struct RarArchive {
    handle: *mut SquashRar,
    /// Held for the archive's whole lifetime (see module docs).
    _guard: MutexGuard<'static, ()>,
}

impl RarArchive {
    /// Open for the metadata pass (RAR_OM_LIST).
    pub fn open_list(path: &Path) -> Result<Self, RarError> {
        Self::open_impl(path, true)
    }

    /// Open for the extraction pass (RAR_OM_EXTRACT).
    pub fn open_extract(path: &Path) -> Result<Self, RarError> {
        Self::open_impl(path, false)
    }

    fn open_impl(path: &Path, list_only: bool) -> Result<Self, RarError> {
        let guard = UNRAR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let c_path = path_to_cstring(path)?;
        let mut handle: *mut SquashRar = std::ptr::null_mut();
        let rc = unsafe { squash_rar_open(c_path.as_ptr(), c_int::from(list_only), &mut handle) };
        if rc != 0 {
            return Err(RarError(rc));
        }
        if handle.is_null() {
            return Err(RarError(RarError::EOPEN));
        }
        Ok(Self {
            handle,
            _guard: guard,
        })
    }

    /// Read the next entry header. `Ok(None)` at the end of the archive.
    pub fn next_entry(&mut self) -> Result<Option<RarEntry>, RarError> {
        let mut name: *const c_char = std::ptr::null();
        let mut size = 0u64;
        let mut is_dir = 0;
        let mut is_encrypted = 0;
        let rc = unsafe {
            squash_rar_next(
                self.handle,
                &mut name,
                &mut size,
                &mut is_dir,
                &mut is_encrypted,
            )
        };
        if rc == RarError::END_ARCHIVE {
            return Ok(None);
        }
        if rc != 0 {
            return Err(RarError(rc));
        }
        let name = unsafe { cstr_to_string(name) };
        Ok(Some(RarEntry {
            name,
            size,
            is_dir: is_dir != 0,
            is_encrypted: is_encrypted != 0,
        }))
    }

    /// Decode the current entry, streaming bytes to `sink`. The callback
    /// returns `false` to abort; that surfaces as [`RarError::ABORTED`] so the
    /// caller can raise its own (more specific) error.
    pub fn extract_current(&mut self, sink: &mut dyn FnMut(&[u8]) -> bool) -> Result<(), RarError> {
        // The trait-object pointer must outlive the FFI call, so it sits in a
        // local slot whose address we pass as user data.
        let mut slot: &mut dyn FnMut(&[u8]) -> bool = sink;
        let user = (&mut slot as *mut &mut dyn FnMut(&[u8]) -> bool).cast::<c_void>();
        let rc = unsafe { squash_rar_extract(self.handle, Some(data_trampoline), user) };
        if rc == 0 {
            Ok(())
        } else {
            Err(RarError(rc))
        }
    }

    /// Skip the current entry's payload.
    pub fn skip_current(&mut self) -> Result<(), RarError> {
        let rc = unsafe { squash_rar_skip(self.handle) };
        if rc == 0 {
            Ok(())
        } else {
            Err(RarError(rc))
        }
    }
}

impl Drop for RarArchive {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { squash_rar_close(self.handle) };
        }
    }
}

extern "C" fn data_trampoline(data: *const u8, size: usize, user: *mut c_void) -> c_int {
    let sink = unsafe { &mut *user.cast::<&mut dyn FnMut(&[u8]) -> bool>() };
    // UnRAR flushes with a null buffer when there is nothing to deliver
    // (ComprDataIO::UnpWrite with a zero count) — found by fuzzing. Rust
    // forbids `from_raw_parts` on a null pointer even at size 0, so skip the
    // sink and keep decoding. A null pointer with a non-zero size would be a
    // C-side bug: abort the decode instead of slicing it.
    if data.is_null() {
        return if size == 0 { 0 } else { 1 };
    }
    let chunk = unsafe { std::slice::from_raw_parts(data, size) };
    c_int::from(!sink(chunk))
}

/// The shim takes UTF-8 on every platform (it converts to wide chars for the
/// Windows API itself). Non-UTF-8 unix paths are passed through as raw bytes,
/// which is what unrar expects there.
fn path_to_cstring(path: &Path) -> Result<CString, RarError> {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    };
    #[cfg(not(unix))]
    let bytes = path.to_string_lossy().into_owned().into_bytes();
    CString::new(bytes).map_err(|_| RarError(RarError::EOPEN))
}

unsafe fn cstr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive the trampoline directly with a recording sink.
    fn call_trampoline(data: *const u8, size: usize, received: &mut Vec<Vec<u8>>) -> c_int {
        let mut sink = |chunk: &[u8]| -> bool {
            received.push(chunk.to_vec());
            true
        };
        let mut slot: &mut dyn FnMut(&[u8]) -> bool = &mut sink;
        let user = (&mut slot as *mut &mut dyn FnMut(&[u8]) -> bool).cast::<c_void>();
        data_trampoline(data, size, user)
    }

    #[test]
    fn trampoline_tolerates_null_zero_length_flush() {
        // Fuzz finding (Phase 5): UnRAR calls UCM_PROCESSDATA with a null
        // buffer and zero size on flush; `slice::from_raw_parts(null, 0)` is
        // UB. The chunk must be skipped, decoding continues (return 0).
        let mut received = Vec::new();
        assert_eq!(call_trampoline(std::ptr::null(), 0, &mut received), 0);
        assert!(received.is_empty());
    }

    #[test]
    fn trampoline_aborts_on_null_with_nonzero_size() {
        let mut received = Vec::new();
        assert_eq!(call_trampoline(std::ptr::null(), 64, &mut received), 1);
        assert!(received.is_empty());
    }

    #[test]
    fn trampoline_delivers_normal_chunks() {
        let bytes = [1u8, 2, 3, 4];
        let mut received = Vec::new();
        assert_eq!(call_trampoline(bytes.as_ptr(), 4, &mut received), 0);
        assert_eq!(received, vec![vec![1, 2, 3, 4]]);
    }
}
