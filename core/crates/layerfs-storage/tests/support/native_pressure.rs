//! Finite actual SQLite native allocation owners, not a replacement allocator.

use std::ffi::c_void;
use std::ptr::NonNull;

use rusqlite::ffi;

pub struct Allocation {
    pointer: NonNull<c_void>,
    pub bytes: u64,
}

impl Allocation {
    pub fn acquire(requested: u64) -> Option<Self> {
        let pointer = NonNull::new(unsafe { ffi::sqlite3_malloc64(requested) })?;
        let bytes = unsafe { ffi::sqlite3_msize(pointer.as_ptr()) };
        let owner = Self { pointer, bytes };
        assert!(bytes >= requested);
        for offset in (0..requested as usize).step_by(4096) {
            unsafe { pointer.as_ptr().cast::<u8>().add(offset).write(0xa5) };
        }
        unsafe {
            pointer
                .as_ptr()
                .cast::<u8>()
                .add(requested as usize - 1)
                .write(0x5a)
        };
        Some(owner)
    }
}

impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe { ffi::sqlite3_free(self.pointer.as_ptr()) };
    }
}

/// At most32 declared 1MiB requests; stop at the first actual native refusal.
pub fn until_refusal() -> Vec<Allocation> {
    let mut owners = Vec::with_capacity(32);
    for _ in 0..32 {
        match Allocation::acquire(1024 * 1024) {
            Some(owner) => owners.push(owner),
            None => return owners,
        }
    }
    panic!("expected actual refusal before32 one-MiB owners under32MiB guard");
}
